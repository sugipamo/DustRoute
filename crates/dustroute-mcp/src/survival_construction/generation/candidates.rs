//! Ordered candidate families. Native checked transitions admit every action;
//! pose generation and family limits remain deliberately incomplete caller policy.
use super::super::{
    ConstructionAction, ConstructionPlanningError, HypotheticalConstructionStep, PlacementPurpose,
    air,
};
use super::{
    FACES, Node, PoseKind, Search, WorkPose, center, distance, eye, missing_materials, rotation,
    within,
};
use crate::survival_error::SurvivalErrorCode;
use voxrig::checked_survival::{SurvivalControl, SurvivalInput};

impl<'a> Search<'a> {
    pub(super) fn extend(
        &mut self,
        node: &Node<'a>,
        action: ConstructionAction,
    ) -> Option<Node<'a>> {
        if self.exhausted() || node.checked.steps.len() >= self.limits.actions {
            return None;
        }
        self.stats.candidate_checks += 1;
        match node.checked.clone().after(&action) {
            Ok(checked) => {
                let missing = missing_materials(&checked.ledger, self.supplied);
                if !missing.is_empty() {
                    let mut error = ConstructionPlanningError::new(
                        SurvivalErrorCode::InsufficientSuppliedMaterials,
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

    pub(super) fn placement(
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

    pub(super) fn build_reachable(&mut self, mut node: Node<'a>) -> Node<'a> {
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

    pub(super) fn move_to(
        &mut self,
        node: &Node<'a>,
        target: [f64; 3],
        goal_radius: f64,
    ) -> Option<Node<'a>> {
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

    pub(super) fn access_successors(
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

    pub(super) fn moves(&mut self, node: &Node<'a>, successors: &mut Vec<Node<'a>>) {
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

    pub(super) fn removal_successors(&mut self, node: &Node<'a>, successors: &mut Vec<Node<'a>>) {
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

    pub(super) fn work_successors(&mut self, node: &Node<'a>) -> Vec<Node<'a>> {
        let mut successors = Vec::new();
        if !node.checked.ledger.remaining.is_empty() {
            for target in self.temporary_cells.clone() {
                if node
                    .checked
                    .scenario
                    .block(target)
                    .is_ok_and(|s| s == air())
                {
                    if let Some(next) = self.placement(
                        node,
                        target,
                        self.temporary_material,
                        PlacementPurpose::Temporary,
                    ) {
                        self.retain(&mut successors, next.clone());
                        self.access_successors(node, target, next, &mut successors);
                    }
                }
                if self.exhausted() {
                    break;
                }
            }
        }
        self.moves(node, &mut successors);
        successors
    }
}
