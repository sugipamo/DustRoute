//! Bounded caller policy over the common checked construction transitions.
//! Candidate pruning is deliberately incomplete; it never establishes impossibility.
use super::*;
use std::collections::BTreeSet;
use voxrig::checked_survival::SurvivalInput;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SearchLimits {
    /// Includes refused extensions. One check may call multiple native guards.
    pub candidate_checks: usize,
    pub expanded: usize,
    pub frontier: usize,
    pub actions: usize,
}
impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            candidate_checks: 100_000,
            expanded: 512,
            frontier: 16,
            actions: 256,
        }
    }
}
impl SearchLimits {
    fn validate(self) -> Result<()> {
        if !(1..=1_000_000).contains(&self.candidate_checks)
            || !(1..=4096).contains(&self.expanded)
            || !(1..=64).contains(&self.frontier)
            || !(1..=512).contains(&self.actions)
        {
            return Err(ConstructionPlanningError::new(
                "invalid_search_limits",
                "require 1..1000000 candidate checks, 1..4096 expansions, 1..64 frontier, 1..512 actions",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ConstructionSearch {
    pub candidate_checks: usize,
    pub expanded: usize,
    pub rejected: usize,
    pub pruned: usize,
    pub frontier_peak: usize,
    pub complete_checks: usize,
    pub action_limited_nodes: usize,
    pub cleanup_searches: usize,
    pub completed_access_cleanups: usize,
    pub best_remaining_permanent: usize,
    pub best_remaining_targets: Vec<[i32; 3]>,
    pub highest_hypothetical_feet: f64,
    pub best_remaining_temporary: Vec<[i32; 3]>,
    /// Bounded progress samples for diagnosing policy starvation and cycles.
    pub progress: Vec<SearchProgress>,
    /// Bounded samples, not an exhaustive explanation or a proof of impossibility.
    pub refusal_examples: Vec<ConstructionPlanningError>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SearchProgress {
    pub candidate_checks: usize,
    pub actions: usize,
    pub position: [f64; 3],
    pub remaining_permanent: usize,
    pub remaining_temporary: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GenerationFailure {
    PlanningTaskFailed {
        reason: String,
    },
    InvalidInput {
        error: ConstructionPlanningError,
    },
    UnsupportedTarget {
        position: [i32; 3],
        state: NativeBlockState,
        reason: String,
    },
    InitialSceneRefused {
        error: ConstructionPlanningError,
    },
    InsufficientMaterials {
        missing: BTreeMap<String, usize>,
    },
    NoCompletePlanWithinLimits {
        reason: SearchStop,
        search: Box<ConstructionSearch>,
    },
}

/// Async-client entry point. Only owned detached data enters the CPU worker;
/// live client I/O and action authority remain on their existing executor.
/// Dropping the awaiter discards the result, but an already started bounded
/// search continues read-only until its limits are reached.
pub async fn generate_construction_plan_async(
    scene: CapturedSurvivalScene,
    site: ConstructionSite,
    supplied: BTreeMap<String, usize>,
    temporary_material: String,
    limits: SearchLimits,
) -> std::result::Result<GeneratedConstructionPlan, GenerationFailure> {
    limits
        .validate()
        .map_err(|error| GenerationFailure::InvalidInput { error })?;
    tokio::task::spawn_blocking(move || {
        generate_construction_plan(&scene, &site, &supplied, &temporary_material, limits)
    })
    .await
    .map_err(|error| GenerationFailure::PlanningTaskFailed {
        reason: error.to_string(),
    })?
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchStop {
    CandidateBudget,
    ExpansionBudget,
    CandidateFrontierExhausted,
}

#[derive(Clone, Debug, Serialize)]
pub struct GeneratedConstructionPlan {
    pub plan: HypotheticalConstructionPlan,
    pub search: ConstructionSearch,
}

#[derive(Clone)]
struct Node<'a> {
    checked: CheckedPrefix<'a>,
    cleaning: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PoseKind {
    Center,
    Edge,
    Overhang,
}
#[derive(Clone, Copy)]
struct WorkPose {
    position: [f64; 3],
    kind: PoseKind,
}

struct Search<'a> {
    limits: SearchLimits,
    supplied: &'a BTreeMap<String, usize>,
    temporary_material: &'a str,
    temporary_cells: Vec<[i32; 3]>,
    stats: ConstructionSearch,
}

fn missing_materials(
    ledger: &Ledger<'_>,
    supplied: &BTreeMap<String, usize>,
) -> BTreeMap<String, usize> {
    // Reserve unbuilt permanent targets, including when temporary and permanent
    // materials coincide. Removing a temporary block never refunds consumption.
    let mut required = ledger.materials.required_supplied.clone();
    for state in ledger.remaining.values() {
        *required.entry(state.name.clone()).or_default() += 1;
    }
    required
        .into_iter()
        .filter_map(|(name, count)| {
            let missing = count.saturating_sub(supplied.get(&name).copied().unwrap_or(0));
            (missing > 0).then_some((name, missing))
        })
        .collect()
}

const FACES: [(BlockFace, [i32; 3]); 6] = [
    (BlockFace::Up, [0, 1, 0]),
    (BlockFace::North, [0, 0, -1]),
    (BlockFace::South, [0, 0, 1]),
    (BlockFace::West, [-1, 0, 0]),
    (BlockFace::East, [1, 0, 0]),
    (BlockFace::Down, [0, -1, 0]),
];

fn rotation(eye: [f64; 3], point: [f64; 3]) -> [f32; 2] {
    let d: [f64; 3] = std::array::from_fn(|i| point[i] - eye[i]);
    [
        (-d[0]).atan2(d[2]).to_degrees() as f32,
        (-d[1]).atan2(d[0].hypot(d[2])).to_degrees() as f32,
    ]
}
fn eye(node: &Node<'_>) -> [f64; 3] {
    let source = node.checked.scene.source();
    std::array::from_fn(|i| {
        node.checked.scenario.position()[i] + source.eye_position[i] - source.position[i]
    })
}
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>()
}
fn center(p: [i32; 3]) -> [f64; 3] {
    p.map(|n| f64::from(n) + 0.5)
}
fn within(bounds: TravelBounds, p: [f64; 3]) -> bool {
    (0..3).all(|i| bounds.min[i] <= p[i] && p[i] <= bounds.max[i])
}

impl<'a> Search<'a> {
    fn note_best(&mut self, node: &Node<'_>) {
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
    fn retain(&mut self, nodes: &mut Vec<Node<'a>>, node: Node<'a>) {
        nodes.push(node);
        nodes.sort_by(|a, b| self.score(a).total_cmp(&self.score(b)));
        self.stats.pruned += nodes.len().saturating_sub(self.limits.frontier);
        nodes.truncate(self.limits.frontier);
        self.stats.frontier_peak = self.stats.frontier_peak.max(nodes.len());
    }
    fn exhausted(&self) -> bool {
        self.stats.candidate_checks >= self.limits.candidate_checks
    }
    fn reject(&mut self, error: ConstructionPlanningError) {
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
    fn extend(&mut self, node: &Node<'a>, action: ConstructionAction) -> Option<Node<'a>> {
        if self.exhausted() || node.checked.steps.len() >= self.limits.actions {
            return None;
        }
        self.stats.candidate_checks += 1;
        match node.checked.clone().after(&action) {
            Ok(checked) => {
                let missing = missing_materials(&checked.ledger, self.supplied);
                if !missing.is_empty() {
                    let mut error = ConstructionPlanningError::new(
                        "insufficient_supplied_materials",
                        "candidate consumes reserved permanent or unavailable temporary materials",
                    );
                    error.missing_materials = missing;
                    self.reject(error);
                    None
                } else {
                    self.stats.highest_hypothetical_feet = self
                        .stats
                        .highest_hypothetical_feet
                        .max(checked.scenario.position()[1]);
                    let next = Node {
                        checked,
                        cleaning: node.cleaning,
                    };
                    self.note_best(&next);
                    Some(next)
                }
            }
            Err(error) => {
                self.reject(error);
                None
            }
        }
    }

    fn placement(
        &mut self,
        node: &Node<'a>,
        target: [i32; 3],
        material: &str,
        purpose: PlacementPurpose,
    ) -> Option<Node<'a>> {
        // This is only candidate filtering. Native checks remain authoritative.
        if distance(eye(node), center(target)) > 36.0 {
            return None;
        }
        for (face, delta) in FACES {
            let support = std::array::from_fn(|i| target[i] - delta[i]);
            if node
                .checked
                .scenario
                .block(support)
                .is_ok_and(|s| s != air())
            {
                let point = std::array::from_fn(|i| {
                    f64::from(support[i]) + 0.5 + f64::from(delta[i]) * 0.5
                });
                if let Some(next) = self.extend(
                    node,
                    ConstructionAction::Place {
                        purpose,
                        support,
                        face,
                        rotation: rotation(eye(node), point),
                        material: material.into(),
                    },
                ) {
                    return Some(next);
                }
            }
        }
        None
    }

    fn build_reachable(&mut self, mut node: Node<'a>) -> Node<'a> {
        // Greedy closure is a candidate policy, not a dependency theorem. Other
        // frontier branches can approach the same targets from different poses.
        loop {
            let mut targets: Vec<_> = node
                .checked
                .ledger
                .remaining
                .iter()
                .map(|(p, s)| (*p, s.name.clone()))
                .collect();
            targets.sort_by(|a, b| {
                a.0[1].cmp(&b.0[1]).then_with(|| {
                    distance(center(a.0), eye(&node)).total_cmp(&distance(center(b.0), eye(&node)))
                })
            });
            let mut changed = false;
            for (p, name) in targets {
                if let Some(next) = self.placement(&node, p, &name, PlacementPurpose::Permanent) {
                    node = next;
                    changed = true;
                }
                if self.exhausted() {
                    return node;
                }
            }
            if !changed {
                return node;
            }
        }
    }

    fn work_distance(node: &Node<'_>, p: [f64; 3]) -> f64 {
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

    fn score(&self, node: &Node<'_>) -> f64 {
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

    fn cleanup_distance(node: &Node<'_>, p: [f64; 3]) -> f64 {
        node.checked
            .ledger
            .temporary
            .keys()
            .map(|t| distance(p, center(*t)))
            .reduce(f64::min)
            .unwrap_or(0.0)
    }

    fn complete(&mut self, node: &Node<'a>) -> Option<HypotheticalConstructionPlan> {
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

    fn move_to(&mut self, node: &Node<'a>, target: [f64; 3], goal_radius: f64) -> Option<Node<'a>> {
        let from = node.checked.scenario.position();
        let yaw = rotation(from, target)[0];
        let jump = target[1] > from[1] + 0.1;
        let mut best: Option<(f64, Node<'a>)> = None;
        for active in 1..=32 {
            if self.exhausted() {
                break;
            }
            let controls = (0..active + 24)
                .map(|t| SurvivalControl {
                    yaw,
                    input: SurvivalInput {
                        forward: i8::from(t < active),
                        strafe: 0,
                        jump: jump && t == 0,
                    },
                })
                .collect();
            if let Some(next) = self.extend(node, ConstructionAction::Move { controls }) {
                let end = next.checked.scenario.position();
                let error = distance(target, end);
                // Restrict this candidate family to non-retracing motion
                // between edits. This prunes paths; it does not equate native
                // states or claim that a pruned route is physically invalid.
                let retraces = node
                    .checked
                    .steps
                    .iter()
                    .rev()
                    .take_while(|s| matches!(s, HypotheticalConstructionStep::Move { .. }))
                    .any(|s| match s {
                        HypotheticalConstructionStep::Move { prediction } => {
                            distance(end, prediction.initial_position) < 0.25_f64.powi(2)
                        }
                        _ => false,
                    });
                if (end[1] - target[1]).abs() < 0.01
                    && error < goal_radius.powi(2)
                    && !retraces
                    && best.as_ref().is_none_or(|(old, _)| error < *old)
                {
                    best = Some((error, next));
                    // First sufficiently centered admitted endpoint. Exact
                    // native prediction is retained; optimal control duration
                    // is not needed for this candidate policy.
                    if error <= goal_radius.min(0.13).powi(2) {
                        break;
                    }
                }
            }
        }
        best.map(|(_, next)| next)
    }

    fn access_successors(
        &mut self,
        node: &Node<'a>,
        target: [i32; 3],
        mut next: Node<'a>,
        successors: &mut Vec<Node<'a>>,
    ) {
        // A placement is useful access only together with a checked path onto it.
        // Grow a bounded column up to one block above current feet, using only
        // permitted cells and native placement transitions. This macro retains
        // every ordinary action; it is neither a shape template nor a teleport.
        let mut top = target;
        let max_top = node.checked.scenario.position()[1].floor() as i32;
        while top[1] <= max_top && !self.exhausted() {
            let feet = [
                f64::from(top[0]) + 0.5,
                f64::from(top[1]) + 1.0,
                f64::from(top[2]) + 0.5,
            ];
            if let Some(moved) = self.move_to(&next, feet, 0.23) {
                self.retain(successors, moved);
            } else {
                // Placement reach is longer than a useful step-up. Approach a
                // nearby ground/platform cell before trying to climb, checking
                // both legs against the edited scenario. This is a candidate
                // sequence, not an assumed route or a fixed access template.
                let mut approaches: Vec<_> = [[1, 0], [-1, 0], [0, 1], [0, -1]]
                    .into_iter()
                    .map(|[dx, dz]| {
                        [
                            feet[0] + f64::from(dx),
                            f64::from(top[1]),
                            feet[2] + f64::from(dz),
                        ]
                    })
                    .collect();
                approaches.sort_by(|a, b| {
                    distance(*a, next.checked.scenario.position())
                        .total_cmp(&distance(*b, next.checked.scenario.position()))
                });
                for approach in approaches {
                    if self.exhausted() {
                        break;
                    }
                    if let Some(walked) = self.move_to(&next, approach, 0.23) {
                        if let Some(climbed) = self.move_to(&walked, feet, 0.23) {
                            self.retain(successors, climbed);
                            break;
                        }
                    }
                }
            }
            top[1] += 1;
            if top[1] > max_top
                || self.temporary_cells.binary_search(&top).is_err()
                || !next.checked.scenario.block(top).is_ok_and(|s| s == air())
            {
                break;
            }
            let Some(higher) = self.placement(
                &next,
                top,
                self.temporary_material,
                PlacementPurpose::Temporary,
            ) else {
                break;
            };
            self.retain(successors, higher.clone());
            next = higher;
        }
    }

    fn moves(&mut self, node: &Node<'a>, successors: &mut Vec<Node<'a>>) {
        let from = node.checked.scenario.position();
        let base = from.map(|v| v.floor() as i32);
        let mut targets = Vec::new();
        // Surface centers and edge approaches, derived from the current scene.
        // Vertical offsets limit this candidate family to one-block transitions.
        for x in base[0] - 3..=base[0] + 3 {
            for z in base[2] - 3..=base[2] + 3 {
                for y in base[1] - 2..=base[1] {
                    if node
                        .checked
                        .scenario
                        .block([x, y, z])
                        .is_ok_and(|s| s != air())
                        && node
                            .checked
                            .scenario
                            .block([x, y + 1, z])
                            .is_ok_and(|s| s == air())
                        && node
                            .checked
                            .scenario
                            .block([x, y + 2, z])
                            .is_ok_and(|s| s == air())
                    {
                        for (kind, dx, dz) in [
                            (PoseKind::Center, 0.5, 0.5),
                            (PoseKind::Edge, 0.12, 0.5),
                            (PoseKind::Edge, 0.88, 0.5),
                            (PoseKind::Edge, 0.5, 0.12),
                            (PoseKind::Edge, 0.5, 0.88),
                            // Feet can overhang a cube while the conservative
                            // native body/support envelope remains admitted.
                            // Useful for reaching its outward face; no support
                            // authority follows from this candidate coordinate.
                            (PoseKind::Overhang, -0.12, 0.5),
                            (PoseKind::Overhang, 1.12, 0.5),
                            (PoseKind::Overhang, 0.5, -0.12),
                            (PoseKind::Overhang, 0.5, 1.12),
                        ] {
                            let target = [f64::from(x) + dx, f64::from(y + 1), f64::from(z) + dz];
                            if distance(from, target) > 0.04
                                && within(node.checked.ledger.site.scope.travel, target)
                            {
                                targets.push(WorkPose {
                                    position: target,
                                    kind,
                                });
                            }
                        }
                    }
                }
            }
        }
        // Avoid a coordinate scan consuming the entire budget before useful
        // directions. Distance to unfinished work is a heuristic, not admission.
        targets.sort_by(|a, b| {
            if node.cleaning {
                a.position[1].total_cmp(&b.position[1]).then_with(|| {
                    Self::cleanup_distance(node, a.position)
                        .total_cmp(&Self::cleanup_distance(node, b.position))
                })
            } else {
                Self::work_distance(node, a.position)
                    .total_cmp(&Self::work_distance(node, b.position))
            }
        });
        // Retain elevation alternatives separately: many samples on a nearby
        // floor must not crowd every possible scaffold ascent out of the family.
        let mut selected = Vec::new();
        for (height_class, limit) in [(-1, 2), (1, 2), (0, 4)] {
            for kind in [PoseKind::Center, PoseKind::Edge, PoseKind::Overhang] {
                let mut count = 0;
                for &target in &targets {
                    let class = if target.position[1] < from[1] - 0.1 {
                        -1
                    } else if target.position[1] > from[1] + 0.1 {
                        1
                    } else {
                        0
                    };
                    if class == height_class && target.kind == kind && count < limit {
                        selected.push(target);
                        count += 1;
                    }
                }
            }
        }
        self.stats.pruned += targets.len().saturating_sub(selected.len());
        let targets = selected;
        for target in targets {
            let radius = if target.kind == PoseKind::Overhang {
                0.10
            } else {
                0.23
            };
            if let Some(next) = self.move_to(node, target.position, radius) {
                self.retain(successors, next);
            }
            if self.exhausted() {
                return;
            }
        }
    }

    fn removal_successors(&mut self, node: &Node<'a>, successors: &mut Vec<Node<'a>>) {
        for &target in node.checked.ledger.temporary.keys() {
            // Retain lower tiers until descent. The common/native checks still
            // establish ownership, current support and the final escape path.
            if f64::from(target[1]) < node.checked.scenario.position()[1].floor() - 1.0 {
                self.stats.pruned += 1;
                continue;
            }
            for (face, delta) in FACES {
                let point =
                    std::array::from_fn(|i| f64::from(target[i]) + 0.5 + f64::from(delta[i]) * 0.5);
                if let Some(next) = self.extend(
                    node,
                    ConstructionAction::RemoveTemporary {
                        target,
                        face,
                        rotation: rotation(eye(node), point),
                    },
                ) {
                    self.retain(successors, next);
                    break;
                }
            }
        }
    }

    fn clear_access(&mut self, mut start: Node<'a>) -> Option<Node<'a>> {
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
}

/// Produce a complete hypothetical plan or bounded-search diagnostics. This is
/// read-only and grants no adoption, inventory receipt or execution authority.
/// This function is CPU-bound. Async clients must use the off-thread entry point
/// above so planning cannot starve keepalive and observation tasks.
pub fn generate_construction_plan(
    scene: &CapturedSurvivalScene,
    site: &ConstructionSite,
    supplied: &BTreeMap<String, usize>,
    temporary_material: &str,
    limits: SearchLimits,
) -> std::result::Result<GeneratedConstructionPlan, GenerationFailure> {
    limits
        .validate()
        .map_err(|error| GenerationFailure::InvalidInput { error })?;
    if !temporary_material.starts_with("minecraft:") || temporary_material.len() > 128 {
        return Err(GenerationFailure::InvalidInput {
            error: ConstructionPlanningError::new(
                "invalid_temporary_material",
                "use a canonical minecraft material name",
            ),
        });
    }
    // Native hypothetical placement currently synthesizes property-free cubes.
    // Do not spend a search budget attempting an exact state it cannot produce.
    if let Some((position, state)) = site
        .structure
        .iter()
        .find(|(_, s)| !s.properties.is_empty())
    {
        return Err(GenerationFailure::UnsupportedTarget {
            position: *position,
            state: state.clone(),
            reason: "native hypothetical cube placement produces property-free states".into(),
        });
    }
    let missing = missing_materials(&Ledger::new(site), supplied);
    if !missing.is_empty() {
        return Err(GenerationFailure::InsufficientMaterials { missing });
    }
    let checked = CheckedPrefix::new(scene, site)
        .map_err(|error| GenerationFailure::InitialSceneRefused { error })?;
    let temporary_cells = site
        .scope
        .temporary
        .iter()
        .flat_map(|r| cells(*r).map(xyz))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut search = Search {
        limits,
        supplied,
        temporary_material,
        temporary_cells,
        stats: ConstructionSearch {
            best_remaining_permanent: site.structure.len(),
            best_remaining_targets: site.structure.keys().copied().collect(),
            highest_hypothetical_feet: scene.source().position[1],
            frontier_peak: 1,
            ..Default::default()
        },
    };
    let mut frontier = vec![Node {
        checked,
        cleaning: false,
    }];
    while !frontier.is_empty() && search.stats.expanded < limits.expanded && !search.exhausted() {
        frontier.sort_by(|a, b| search.score(b).total_cmp(&search.score(a)));
        let initial = frontier.pop().unwrap();
        let before = initial.checked.ledger.remaining.len();
        let mut node = search.build_reachable(initial);
        let made_progress = node.checked.ledger.remaining.len() < before;
        search.stats.expanded += 1;
        if node.checked.ledger.remaining.is_empty() && !node.checked.ledger.temporary.is_empty() {
            let Some(cleaned) = search.clear_access(node) else {
                continue;
            };
            node = cleaned;
        }
        if search.stats.progress.len() < 64 {
            search.stats.progress.push(SearchProgress {
                candidate_checks: search.stats.candidate_checks,
                actions: node.checked.steps.len(),
                position: node.checked.scenario.position(),
                remaining_permanent: node.checked.ledger.remaining.len(),
                remaining_temporary: node.checked.ledger.temporary.len(),
            });
        }
        search.note_best(&node);
        if let Some(plan) = search.complete(&node) {
            return Ok(GeneratedConstructionPlan {
                plan,
                search: search.stats,
            });
        }
        if node.checked.steps.len() >= limits.actions {
            search.stats.action_limited_nodes += 1;
            continue;
        }
        let mut successors = Vec::new();
        if !node.checked.ledger.remaining.is_empty() {
            for target in search.temporary_cells.clone() {
                if node
                    .checked
                    .scenario
                    .block(target)
                    .is_ok_and(|s| s == air())
                {
                    if let Some(next) = search.placement(
                        &node,
                        target,
                        search.temporary_material,
                        PlacementPurpose::Temporary,
                    ) {
                        search.retain(&mut successors, next.clone());
                        search.access_successors(&node, target, next, &mut successors);
                    }
                }
                if search.exhausted() {
                    break;
                }
            }
        }
        search.moves(&node, &mut successors);
        let mut evaluated = Vec::new();
        for next in successors {
            let next = search.build_reachable(next);
            if let Some(plan) = search.complete(&next) {
                return Ok(GeneratedConstructionPlan {
                    plan,
                    search: search.stats,
                });
            }
            search.retain(&mut evaluated, next);
        }
        let stalled = !made_progress
            && !node.checked.ledger.temporary.is_empty()
            && evaluated.iter().all(|next| {
                next.checked.ledger.remaining.len() >= node.checked.ledger.remaining.len()
                    && Search::work_distance(next, next.checked.scenario.position())
                        >= Search::work_distance(&node, node.checked.scenario.position()) - 0.1
            });
        if stalled {
            if let Some(cleaned) = search.clear_access(node) {
                search.retain(&mut frontier, cleaned);
            }
        }
        for next in evaluated {
            search.retain(&mut frontier, next);
        }
    }
    Err(GenerationFailure::NoCompletePlanWithinLimits {
        reason: if search.exhausted() {
            SearchStop::CandidateBudget
        } else if search.stats.expanded >= limits.expanded {
            SearchStop::ExpansionBudget
        } else {
            SearchStop::CandidateFrontierExhausted
        },
        search: Box::new(search.stats),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_consumption_reserves_unbuilt_permanent_materials_and_never_refunds() {
        let site = ConstructionSite::from_grounded(
            &super::super::tests::design(),
            super::super::tests::scope(),
        )
        .unwrap();
        let mut ledger = Ledger::new(&site);
        let supplied = BTreeMap::from([("minecraft:cobblestone".into(), 49)]);
        assert!(missing_materials(&ledger, &supplied).is_empty());
        let edit = HypotheticalBlockEdit {
            position: [-3, 0, 2],
            before: air(),
            after: NativeBlockState {
                name: "minecraft:cobblestone".into(),
                properties: Default::default(),
            },
        };
        ledger.place(PlacementPurpose::Temporary, &edit).unwrap();
        assert_eq!(
            missing_materials(&ledger, &supplied)["minecraft:cobblestone"],
            1
        );
        ledger
            .remove(&HypotheticalBlockEdit {
                position: edit.position,
                before: edit.after,
                after: air(),
            })
            .unwrap();
        assert_eq!(
            missing_materials(&ledger, &supplied)["minecraft:cobblestone"],
            1
        );
        assert_eq!(ledger.remaining.len(), 49);
    }

    #[test]
    fn extreme_search_budgets_refuse_before_candidate_generation() {
        assert!(SearchLimits::default().validate().is_ok());
        for limits in [
            SearchLimits {
                candidate_checks: 0,
                ..Default::default()
            },
            SearchLimits {
                candidate_checks: usize::MAX,
                ..Default::default()
            },
            SearchLimits {
                expanded: usize::MAX,
                ..Default::default()
            },
            SearchLimits {
                frontier: 0,
                ..Default::default()
            },
            SearchLimits {
                frontier: 65,
                ..Default::default()
            },
            SearchLimits {
                actions: 513,
                ..Default::default()
            },
        ] {
            assert_eq!(limits.validate().unwrap_err().code, "invalid_search_limits");
        }
    }
}
