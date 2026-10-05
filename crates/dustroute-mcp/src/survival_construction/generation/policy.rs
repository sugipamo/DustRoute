//! Frontier ranking, pruning, shared budgets and completion selection.
//! A refused or exhausted search is not a proof of physical impossibility.
use super::super::{CheckedPrefix, ConstructionPlanningError, HypotheticalConstructionPlan, air};
use super::{
    GeneratedConstructionPlan, GenerationFailure, Node, Search, SearchProgress, SearchStop, center,
    distance, within,
};

impl<'a> Search<'a> {
    pub(super) fn note_best(&mut self, node: &Node<'_>) {
        let ledger = &node.checked.ledger;
        if ledger.remaining.len() < self.stats.best_remaining_permanent
            || (ledger.remaining.len() == self.stats.best_remaining_permanent
                && ledger.temporary.len() < self.stats.best_remaining_temporary.len())
        {
            self.stats.best_remaining_permanent = ledger.remaining.len();
            self.stats.best_remaining_targets = ledger.remaining.keys().copied().collect();
            self.stats.best_remaining_temporary = ledger.temporary.keys().copied().collect();
        }
    }
    pub(super) fn retain(&mut self, nodes: &mut Vec<Node<'a>>, node: Node<'a>) {
        nodes.push(node);
        nodes.sort_by(|a, b| self.score(a).total_cmp(&self.score(b)));
        self.stats.pruned += nodes.len().saturating_sub(self.limits.frontier);
        nodes.truncate(self.limits.frontier);
        self.stats.frontier_peak = self.stats.frontier_peak.max(nodes.len());
    }
    pub(super) fn exhausted(&self) -> bool {
        self.stats.candidate_checks >= self.limits.candidate_checks
    }
    pub(super) fn reject(&mut self, error: ConstructionPlanningError) {
        self.stats.rejected += 1;
        if self.stats.refusal_examples.len() < 16
            && !self
                .stats
                .refusal_examples
                .iter()
                .any(|e| e.code == error.code && e.detail == error.detail)
        {
            self.stats.refusal_examples.push(error);
        }
    }
    pub(super) fn work_distance(node: &Node<'_>, p: [f64; 3]) -> f64 {
        let ledger = &node.checked.ledger;
        if !ledger.remaining.is_empty() {
            // Pick the nearby pending work first, then rank approaches to it.
            // Taking the minimum across every target's work-distance can switch
            // to far-away low work whenever a nearby roof requires ascent.
            let from = node.checked.scenario.position();
            let t = ledger
                .remaining
                .keys()
                .min_by(|a, b| distance(from, center(**a)).total_cmp(&distance(from, center(**b))))
                .expect("nonempty remaining targets");
            let unsupported_elevated = t[1] >= ledger.site.baseline.min.y + 3
                && node
                    .checked
                    .scenario
                    .block([t[0], t[1] - 1, t[2]])
                    .is_ok_and(|s| s == air());
            let work = [
                f64::from(t[0]) + 0.5,
                f64::from(t[1]) + if unsupported_elevated { 1.0 } else { -1.0 },
                f64::from(t[2]) + 0.5,
            ];
            ((p[0] - work[0]).hypot(p[2] - work[2]) - 2.0).powi(2)
                + 16.0 * (p[1] - work[1]).min(0.0).powi(2)
        } else if !ledger.temporary.is_empty() {
            ledger
                .temporary
                .keys()
                .map(|t| distance(p, center(*t)))
                .fold(f64::INFINITY, f64::min)
        } else {
            let b = ledger.site.scope.retreat;
            distance(p, std::array::from_fn(|i| p[i].clamp(b.min[i], b.max[i])))
        }
    }

    pub(super) fn score(&self, node: &Node<'_>) -> f64 {
        let ledger = &node.checked.ledger;
        if node.cleaning {
            let height =
                node.checked.scenario.position()[1] - f64::from(ledger.site.baseline.min.y + 1);
            return 20.0 * ledger.temporary.len() as f64
                + 0.75 * height.max(0.0).powi(2)
                + 0.1 * Self::cleanup_distance(node, node.checked.scenario.position())
                + node.checked.steps.len() as f64 * 0.5;
        }
        let target_distance = Self::work_distance(node, node.checked.scenario.position());
        let temporary_cost = if ledger.remaining.is_empty() {
            20.0
        } else {
            0.2
        };
        1000.0 * ledger.remaining.len() as f64
            + temporary_cost * ledger.temporary.len() as f64
            + target_distance
            + node.checked.steps.len() as f64 * 0.5
    }

    pub(super) fn cleanup_distance(node: &Node<'_>, p: [f64; 3]) -> f64 {
        node.checked
            .ledger
            .temporary
            .keys()
            .map(|t| distance(p, center(*t)))
            .reduce(f64::min)
            .unwrap_or(0.0)
    }

    pub(super) fn complete(&mut self, node: &Node<'a>) -> Option<HypotheticalConstructionPlan> {
        if !node.checked.ledger.remaining.is_empty()
            || !node.checked.ledger.temporary.is_empty()
            || !within(
                node.checked.ledger.site.scope.retreat,
                node.checked.scenario.position(),
            )
        {
            return None;
        }
        self.stats.complete_checks += 1;
        match node.checked.clone().finish(self.supplied) {
            Ok(plan) => Some(plan),
            Err(error) => {
                self.reject(error);
                None
            }
        }
    }

    pub(super) fn clear_access(&mut self, mut start: Node<'a>) -> Option<Node<'a>> {
        self.stats.cleanup_searches += 1;
        start.cleaning = true;
        let mut frontier = vec![start];
        // Separate bounded subgoal: no new temporary placement while clearing.
        // All actual transitions remain in the same checked sequence, and all
        // attempts consume the global budgets. A failed cleanup is only a failed
        // candidate; no partial plan is dispatched or treated as a safe escape.
        for _ in 0..128 {
            if frontier.is_empty()
                || self.exhausted()
                || self.stats.expanded >= self.limits.expanded
            {
                break;
            }
            frontier.sort_by(|a, b| self.score(b).total_cmp(&self.score(a)));
            let mut node = frontier.pop().unwrap();
            self.stats.expanded += 1;
            if node.checked.ledger.temporary.is_empty() {
                node.cleaning = false;
                self.stats.completed_access_cleanups += 1;
                return Some(node);
            }
            if node.checked.steps.len() >= self.limits.actions {
                self.stats.action_limited_nodes += 1;
                continue;
            }
            let mut next = Vec::new();
            self.removal_successors(&node, &mut next);
            self.moves(&node, &mut next);
            for successor in next {
                self.retain(&mut frontier, successor);
            }
        }
        None
    }
    pub(super) fn run(
        mut self,
        checked: CheckedPrefix<'a>,
    ) -> std::result::Result<GeneratedConstructionPlan, GenerationFailure> {
        let mut frontier = vec![Node {
            checked,
            cleaning: false,
        }];
        while !frontier.is_empty()
            && self.stats.expanded < self.limits.expanded
            && !self.exhausted()
        {
            frontier.sort_by(|a, b| self.score(b).total_cmp(&self.score(a)));
            let initial = frontier.pop().unwrap();
            let before = initial.checked.ledger.remaining.len();
            let mut node = self.build_reachable(initial);
            let made_progress = node.checked.ledger.remaining.len() < before;
            self.stats.expanded += 1;
            if node.checked.ledger.remaining.is_empty() && !node.checked.ledger.temporary.is_empty()
            {
                let Some(cleaned) = self.clear_access(node) else {
                    continue;
                };
                node = cleaned;
            }
            if self.stats.progress.len() < 64 {
                self.stats.progress.push(SearchProgress {
                    candidate_checks: self.stats.candidate_checks,
                    actions: node.checked.steps.len(),
                    position: node.checked.scenario.position(),
                    remaining_permanent: node.checked.ledger.remaining.len(),
                    remaining_temporary: node.checked.ledger.temporary.len(),
                });
            }
            self.note_best(&node);
            if let Some(plan) = self.complete(&node) {
                return Ok(GeneratedConstructionPlan {
                    plan,
                    search: self.stats,
                });
            }
            if node.checked.steps.len() >= self.limits.actions {
                self.stats.action_limited_nodes += 1;
                continue;
            }
            let successors = self.work_successors(&node);
            let mut evaluated = Vec::new();
            for next in successors {
                let next = self.build_reachable(next);
                if let Some(plan) = self.complete(&next) {
                    return Ok(GeneratedConstructionPlan {
                        plan,
                        search: self.stats,
                    });
                }
                self.retain(&mut evaluated, next);
            }
            let stalled = !made_progress
                && !node.checked.ledger.temporary.is_empty()
                && evaluated.iter().all(|next| {
                    next.checked.ledger.remaining.len() >= node.checked.ledger.remaining.len()
                        && Search::work_distance(next, next.checked.scenario.position())
                            >= Search::work_distance(&node, node.checked.scenario.position()) - 0.1
                });
            if stalled {
                if let Some(cleaned) = self.clear_access(node) {
                    self.retain(&mut frontier, cleaned);
                }
            }
            for next in evaluated {
                self.retain(&mut frontier, next);
            }
        }
        Err(GenerationFailure::NoCompletePlanWithinLimits {
            reason: if self.exhausted() {
                SearchStop::CandidateBudget
            } else if self.stats.expanded >= self.limits.expanded {
                SearchStop::ExpansionBudget
            } else {
                SearchStop::CandidateFrontierExhausted
            },
            limits: self.limits,
            search: Box::new(self.stats),
        })
    }
}
