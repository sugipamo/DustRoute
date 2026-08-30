use std::collections::{BTreeMap, BTreeSet, VecDeque};

use dustroute_physical::{
    Block, BlockKind, ComponentId, Facing, GapEvidence, PhysicalBlockChange, PhysicalPatch,
    PhysicalScene, Pos, RepairImpact, RepairProposal, RepairReason, World,
};

#[must_use]
pub fn propose_scene_repairs(
    world: &World,
    scene: &PhysicalScene,
    max_gap: u32,
) -> Vec<RepairProposal> {
    propose_physical_repairs(world, scene, max_gap, None)
}

#[must_use]
pub fn propose_scene_repairs_near(
    world: &World,
    scene: &PhysicalScene,
    max_gap: u32,
    focus: Pos,
    max_distance: u32,
) -> Vec<RepairProposal> {
    propose_physical_repairs(world, scene, max_gap, Some((focus, max_distance)))
}

#[must_use]
pub fn propose_scene_component_removal(
    world: &World,
    scene: &PhysicalScene,
    pos: Pos,
) -> Option<RepairProposal> {
    propose_component_removal(world, scene, pos)
}

#[must_use]
fn propose_physical_repairs(
    world: &World,
    circuit: &PhysicalScene,
    max_gap: u32,
    focus: Option<(Pos, u32)>,
) -> Vec<RepairProposal> {
    let mut proposals = missing_wire_repairs(world, circuit, max_gap);
    proposals.extend(liveness_bridge_repairs(world, circuit));
    proposals.extend(missing_directional_component_repairs(
        world, circuit, max_gap,
    ));
    proposals.extend(missing_support_repairs(world, circuit));
    proposals.extend(direction_repairs(world, circuit));
    if let Some((focus, max_distance)) = focus {
        proposals.retain(|proposal| {
            proposal.patch.changes.iter().any(|change| {
                change.pos.x.abs_diff(focus.x)
                    + change.pos.y.abs_diff(focus.y)
                    + change.pos.z.abs_diff(focus.z)
                    <= max_distance
            })
        });
        proposals.sort_by_key(|proposal| {
            (
                repair_reason_priority(proposal.patch.reason),
                proposal.patch.changes[0].pos.x.abs_diff(focus.x)
                    + proposal.patch.changes[0].pos.y.abs_diff(focus.y)
                    + proposal.patch.changes[0].pos.z.abs_diff(focus.z),
                std::cmp::Reverse(proposal.patch.confidence_percent),
                proposal.patch.changes[0].pos,
            )
        });
        proposals.dedup_by(|left, right| {
            left.patch.reason == right.patch.reason && left.patch.changes == right.patch.changes
        });
        proposals.truncate(8);
    }
    for proposal in &mut proposals {
        proposal.impact = evaluate_repair(world, circuit, &proposal.patch);
    }
    proposals.retain(|proposal| {
        (focus.is_none() && proposal.patch.reason != RepairReason::ConnectMissingWire)
            || proposal.impact.is_some_and(RepairImpact::improves)
    });
    proposals.sort_by_key(|proposal| {
        let impact = proposal.impact;
        (
            std::cmp::Reverse(impact.is_some_and(RepairImpact::improves)),
            std::cmp::Reverse(impact.map_or(0, |impact| {
                impact
                    .undriven_required_inputs_before
                    .saturating_sub(impact.undriven_required_inputs_after)
            })),
            std::cmp::Reverse(impact.map_or(0, |impact| {
                impact
                    .drive_reachable_components_after
                    .saturating_sub(impact.drive_reachable_components_before)
            })),
            std::cmp::Reverse(proposal.patch.confidence_percent),
            focus.map_or(0, |(focus, _)| {
                proposal.patch.changes[0].pos.x.abs_diff(focus.x)
                    + proposal.patch.changes[0].pos.y.abs_diff(focus.y)
                    + proposal.patch.changes[0].pos.z.abs_diff(focus.z)
            }),
            proposal.patch.changes[0].pos,
        )
    });
    proposals
}

const fn repair_reason_priority(reason: RepairReason) -> u8 {
    match reason {
        RepairReason::RestoreComponentSupport => 0,
        RepairReason::ConnectMissingWire => 1,
        RepairReason::InsertDirectionalComponent | RepairReason::ReorientDirectionalComponent => 2,
        RepairReason::RemoveUnexpectedConnection | RepairReason::OptimizePlacement => 3,
    }
}

fn liveness_bridge_repairs(world: &World, circuit: &PhysicalScene) -> Vec<RepairProposal> {
    let report = crate::analyze_signal_liveness(circuit);
    let by_id = circuit
        .components
        .iter()
        .map(|component| (component.id, component))
        .collect::<BTreeMap<_, _>>();
    let dead_input_roots = report
        .undriven_inputs
        .iter()
        .flat_map(|finding| finding.immediate_sources.iter().copied())
        .collect::<BTreeSet<_>>();
    let mut dead_region = BTreeSet::new();
    let mut queue = VecDeque::from_iter(dead_input_roots);
    while let Some(component) = queue.pop_front() {
        if report.drive_reachable.contains(&component) || !dead_region.insert(component) {
            continue;
        }
        queue.extend(
            circuit
                .connections
                .iter()
                .filter(|connection| {
                    connection.sink.component == component
                        && connection.transfer
                            != dustroute_physical::TransferKind::StructuralSupport
                })
                .map(|connection| connection.source.component),
        );
    }
    let horizontal = [
        Pos::new(1, 0, 0),
        Pos::new(-1, 0, 0),
        Pos::new(0, 0, 1),
        Pos::new(0, 0, -1),
    ];
    let mut candidates = BTreeMap::<Pos, (ComponentId, ComponentId)>::new();
    for dead in dead_region {
        let Some(dead_component) = by_id.get(&dead) else {
            continue;
        };
        for dy in -1..=1 {
            for offset in horizontal {
                let missing = dead_component.pos.offset(offset.x, dy, offset.z);
                if world.kind_at(missing) != BlockKind::Air
                    || !world
                        .get(missing.offset(0, -1, 0))
                        .is_some_and(|block| block.redstone_traits().supports_dust_on_top)
                {
                    continue;
                }
                let live = circuit.components.iter().find(|component| {
                    report.drive_reachable.contains(&component.id)
                        && (-1..=1).any(|live_dy| {
                            horizontal.iter().any(|other| {
                                missing.offset(other.x, live_dy, other.z) == component.pos
                            })
                        })
                });
                if let Some(live) = live {
                    candidates.entry(missing).or_insert((dead, live.id));
                }
            }
        }
    }
    candidates
        .into_iter()
        .map(|(missing, (dead, live))| {
            let mut wire = Block::new(BlockKind::RedstoneWire);
            wire.support_offset = Some(Pos::new(0, -1, 0));
            RepairProposal {
                impact: None,
                evidence: vec![
                    GapEvidence::Nearby {
                        left: dead,
                        right: live,
                        manhattan_distance: 2,
                    },
                    GapEvidence::MissingInlineBlock { position: missing },
                ],
                patch: PhysicalPatch {
                    reason: RepairReason::ConnectMissingWire,
                    affected_fragments: fragments_for(circuit, [dead, live]),
                    confidence_percent: 70,
                    explanation: format!(
                        "place redstone wire at ({}, {}, {}) to bridge an undriven causal region to a drive-reachable region",
                        missing.x, missing.y, missing.z
                    ),
                    changes: vec![PhysicalBlockChange {
                        pos: missing,
                        before: Block::new(BlockKind::Air),
                        after: wire,
                    }],
                },
            }
        })
        .collect()
}

#[must_use]
fn propose_component_removal(
    world: &World,
    circuit: &PhysicalScene,
    pos: Pos,
) -> Option<RepairProposal> {
    let component = circuit
        .components
        .iter()
        .find(|component| component.pos == pos && component.block.kind.is_redstone_related())?;
    let patch = PhysicalPatch {
        reason: RepairReason::RemoveUnexpectedConnection,
        affected_fragments: fragments_for(circuit, [component.id]),
        confidence_percent: 40,
        explanation: format!(
            "remove the explicitly identified {:?} at ({}, {}, {}); verify intended behavior afterward",
            component.block.kind, pos.x, pos.y, pos.z
        ),
        changes: vec![PhysicalBlockChange {
            pos,
            before: component.block.clone(),
            after: Block::new(BlockKind::Air),
        }],
    };
    Some(RepairProposal {
        impact: evaluate_repair(world, circuit, &patch),
        evidence: vec![GapEvidence::SuspectedUnexpectedConnection {
            component: component.id,
        }],
        patch,
    })
}

fn missing_wire_repairs(
    world: &World,
    circuit: &PhysicalScene,
    max_gap: u32,
) -> Vec<RepairProposal> {
    let by_id: BTreeMap<_, _> = circuit
        .components
        .iter()
        .map(|component| (component.id, component))
        .collect();
    circuit
        .gap_candidates(max_gap)
        .into_iter()
        .filter_map(|candidate| {
            let GapEvidence::Nearby {
                left,
                right,
                manhattan_distance,
            } = candidate.evidence[0]
            else {
                return None;
            };
            if manhattan_distance != 2
                || by_id[&left].block.kind != BlockKind::RedstoneWire
                || by_id[&right].block.kind != BlockKind::RedstoneWire
            {
                return None;
            }
            let a = by_id[&left].pos;
            let b = by_id[&right].pos;
            if a.y != b.y || !((a.x == b.x) ^ (a.z == b.z)) {
                return None;
            }
            let missing = Pos::new((a.x + b.x) / 2, a.y, (a.z + b.z) / 2);
            let support = missing.offset(0, -1, 0);
            if world.kind_at(missing) != BlockKind::Air
                || !world
                    .get(support)
                    .is_some_and(|block| block.redstone_traits().supports_dust_on_top)
            {
                return None;
            }
            let mut wire = Block::new(BlockKind::RedstoneWire);
            wire.support_offset = Some(Pos::new(0, -1, 0));
            Some(RepairProposal {
                impact: None,
                evidence: vec![
                    GapEvidence::Nearby {
                        left,
                        right,
                        manhattan_distance,
                    },
                    GapEvidence::MissingInlineBlock { position: missing },
                ],
                patch: PhysicalPatch {
                    reason: RepairReason::ConnectMissingWire,
                    affected_fragments: vec![candidate.left, candidate.right],
                    confidence_percent: 75,
                    explanation: format!(
                        "place redstone wire at ({}, {}, {}) between two aligned wire fragments",
                        missing.x, missing.y, missing.z
                    ),
                    changes: vec![PhysicalBlockChange {
                        pos: missing,
                        before: Block::new(BlockKind::Air),
                        after: wire,
                    }],
                },
            })
        })
        .collect()
}

fn missing_directional_component_repairs(
    world: &World,
    circuit: &PhysicalScene,
    max_gap: u32,
) -> Vec<RepairProposal> {
    let by_id: BTreeMap<_, _> = circuit
        .components
        .iter()
        .map(|component| (component.id, component))
        .collect();
    let mut proposals = Vec::new();
    for candidate in circuit.gap_candidates(max_gap) {
        let GapEvidence::Nearby {
            left,
            right,
            manhattan_distance: 2,
        } = candidate.evidence[0]
        else {
            continue;
        };
        if by_id[&left].block.kind != BlockKind::RedstoneWire
            || by_id[&right].block.kind != BlockKind::RedstoneWire
        {
            continue;
        }
        let a = by_id[&left].pos;
        let b = by_id[&right].pos;
        let facing = match (b.x - a.x, b.z - a.z, b.y - a.y) {
            (2, 0, 0) => Facing::East,
            (-2, 0, 0) => Facing::West,
            (0, 2, 0) => Facing::South,
            (0, -2, 0) => Facing::North,
            _ => continue,
        };
        let missing = Pos::new((a.x + b.x) / 2, a.y, (a.z + b.z) / 2);
        if world.kind_at(missing) != BlockKind::Air
            || !world
                .get(missing.offset(0, -1, 0))
                .is_some_and(|block| block.redstone_traits().supports_dust_on_top)
        {
            continue;
        }
        for direction in [facing, facing.opposite()] {
            let mut repeater = Block::new(BlockKind::Repeater);
            repeater.facing = Some(direction);
            repeater.delay = Some(1);
            repeater.support_offset = Some(Pos::new(0, -1, 0));
            proposals.push(RepairProposal {
                impact: None,
                evidence: vec![
                    GapEvidence::Nearby {
                        left,
                        right,
                        manhattan_distance: 2,
                    },
                    GapEvidence::MissingInlineBlock { position: missing },
                ],
                patch: PhysicalPatch {
                    reason: RepairReason::InsertDirectionalComponent,
                    affected_fragments: vec![candidate.left, candidate.right],
                    confidence_percent: 55,
                    explanation: format!(
                        "insert an {:?}-facing repeater at ({}, {}, {}); choose direction from intended signal flow",
                        direction, missing.x, missing.y, missing.z
                    ),
                    changes: vec![PhysicalBlockChange {
                        pos: missing,
                        before: Block::new(BlockKind::Air),
                        after: repeater,
                    }],
                },
            });
        }
    }
    proposals
}

fn missing_support_repairs(world: &World, circuit: &PhysicalScene) -> Vec<RepairProposal> {
    let by_pos: BTreeMap<_, _> = circuit
        .components
        .iter()
        .map(|component| (component.pos, component.id))
        .collect();
    world
        .support_issues()
        .into_iter()
        .filter_map(|(pos, _, support)| {
            let component = *by_pos.get(&pos)?;
            let support = support?;
            (world.kind_at(support) == BlockKind::Air).then(|| RepairProposal {
                impact: None,
                evidence: vec![GapEvidence::InvalidSupport {
                    component,
                    expected_support: support,
                }],
                patch: PhysicalPatch {
                    reason: RepairReason::RestoreComponentSupport,
                    affected_fragments: fragments_for(circuit, [component]),
                    confidence_percent: 85,
                    explanation: format!(
                        "restore a solid support at ({}, {}, {})",
                        support.x, support.y, support.z
                    ),
                    changes: vec![PhysicalBlockChange {
                        pos: support,
                        before: Block::new(BlockKind::Air),
                        after: Block::new(BlockKind::Solid),
                    }],
                },
            })
        })
        .collect()
}

fn direction_repairs(world: &World, circuit: &PhysicalScene) -> Vec<RepairProposal> {
    let by_pos: BTreeMap<_, _> = circuit
        .components
        .iter()
        .map(|component| (component.pos, component.id))
        .collect();
    let mut proposals = Vec::new();
    for component in circuit.components.iter().filter(|component| {
        matches!(
            component.block.kind,
            BlockKind::Repeater | BlockKind::Comparator
        )
    }) {
        let horizontal = [
            (Facing::North, Pos::new(0, 0, -1)),
            (Facing::East, Pos::new(1, 0, 0)),
            (Facing::South, Pos::new(0, 0, 1)),
            (Facing::West, Pos::new(-1, 0, 0)),
        ];
        let neighbors: Vec<_> = horizontal
            .iter()
            .filter_map(|(facing, offset)| {
                let pos = component.pos.offset(offset.x, 0, offset.z);
                if world.kind_at(pos) == BlockKind::RedstoneWire {
                    by_pos.get(&pos).map(|id| (*facing, *id))
                } else {
                    None
                }
            })
            .collect();
        if neighbors.len() != 2 || neighbors[0].0.opposite() != neighbors[1].0 {
            continue;
        }
        for (facing, neighbor) in &neighbors {
            if component.block.facing == Some(*facing) {
                continue;
            }
            let mut after = component.block.clone();
            after.facing = Some(*facing);
            proposals.push(RepairProposal {
                impact: None,
                evidence: vec![GapEvidence::DirectionMismatch {
                    component: component.id,
                    toward: *neighbor,
                }],
                patch: PhysicalPatch {
                    reason: RepairReason::ReorientDirectionalComponent,
                    affected_fragments: fragments_for(circuit, [component.id, *neighbor]),
                    confidence_percent: 60,
                    explanation: format!(
                        "orient {:?} at ({}, {}, {}) toward {:?}; signal intent is ambiguous",
                        component.block.kind,
                        component.pos.x,
                        component.pos.y,
                        component.pos.z,
                        facing
                    ),
                    changes: vec![PhysicalBlockChange {
                        pos: component.pos,
                        before: component.block.clone(),
                        after,
                    }],
                },
            });
        }
    }
    proposals
}

fn evaluate_repair(
    world: &World,
    circuit: &PhysicalScene,
    patch: &PhysicalPatch,
) -> Option<RepairImpact> {
    let mut repaired = patch.apply_virtual(world).ok()?;
    crate::update_wire_shapes(&mut repaired);
    let bounds = circuit
        .observation
        .regions
        .iter()
        .map(|region| region.bounds)
        .reduce(|left, right| {
            dustroute_physical::SceneBounds::new(
                Pos::new(
                    left.min.x.min(right.min.x),
                    left.min.y.min(right.min.y),
                    left.min.z.min(right.min.z),
                ),
                Pos::new(
                    left.max.x.max(right.max.x),
                    left.max.y.max(right.max.y),
                    left.max.z.max(right.max.z),
                ),
            )
        })
        .map(|bounds| crate::RegionBounds::new(bounds.min, bounds.max))
        .or_else(|| {
            repaired
                .bounds()
                .map(|(min, max)| crate::RegionBounds::new(min, max))
        })?;
    let analysis = crate::analyze_world_region(&repaired, bounds);
    let before_liveness = crate::analyze_signal_liveness(circuit);
    let after_liveness = crate::analyze_signal_liveness(&analysis.scene);
    let before_electrical =
        crate::solve_instantaneous(world, &crate::DeviceOutputState::initially_lit(world), 128);
    let after_electrical = crate::solve_instantaneous(
        &repaired,
        &crate::DeviceOutputState::initially_lit(&repaired),
        128,
    );
    let temporal = analysis.scene.temporal_assessment();
    Some(RepairImpact {
        fragments_before: circuit.fragments.len(),
        fragments_after: analysis.scene.fragments.len(),
        invalid_supports_before: world.support_issues().len(),
        invalid_supports_after: repaired.support_issues().len(),
        undriven_required_inputs_before: before_liveness.undriven_inputs.len(),
        undriven_required_inputs_after: after_liveness.undriven_inputs.len(),
        external_input_waiting_before: before_liveness
            .required_input_assessments
            .iter()
            .filter(|assessment| {
                assessment.status == crate::RequiredInputStatus::AwaitingExternalInput
            })
            .count(),
        external_input_waiting_after: after_liveness
            .required_input_assessments
            .iter()
            .filter(|assessment| {
                assessment.status == crate::RequiredInputStatus::AwaitingExternalInput
            })
            .count(),
        drive_reachable_components_before: before_liveness.drive_reachable.len(),
        drive_reachable_components_after: after_liveness.drive_reachable.len(),
        instantaneous_solve_converged_before: before_electrical.is_ok(),
        instantaneous_solve_converged_after: after_electrical.is_ok(),
        energized_positions_before: before_electrical
            .as_ref()
            .map_or(0, energized_position_count),
        energized_positions_after: after_electrical
            .as_ref()
            .map_or(0, energized_position_count),
        temporal_requirement_after: temporal.requirement,
        requires_temporal_validation: temporal.requirement
            > dustroute_physical::TemporalRequirement::OrderedUpdates,
    })
}

fn energized_position_count(state: &crate::InstantaneousElectricalState) -> usize {
    state
        .signal_levels
        .iter()
        .filter(|(_, level)| **level > 0)
        .map(|(pos, _)| *pos)
        .chain(
            state
                .block_power
                .iter()
                .filter(|(_, power)| power.powered())
                .map(|(pos, _)| *pos),
        )
        .collect::<BTreeSet<_>>()
        .len()
}

fn fragments_for(
    circuit: &PhysicalScene,
    components: impl IntoIterator<Item = ComponentId>,
) -> Vec<dustroute_physical::FragmentId> {
    let components: BTreeSet<_> = components.into_iter().collect();
    circuit
        .fragments
        .iter()
        .filter(|fragment| !fragment.components.is_disjoint(&components))
        .map(|fragment| fragment.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{RegionBounds, analyze_world_region, update_wire_shapes};

    use super::*;

    #[test]
    fn proposes_one_wire_for_a_one_block_break() {
        let mut world = World::new();
        for x in 0..=4 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
        }
        for x in [0, 1, 3, 4] {
            world.place(BlockKind::RedstoneWire, Pos::new(x, 1, 0));
        }
        update_wire_shapes(&mut world);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(5, 3, 1)),
        );
        let proposals = propose_scene_repairs(&world, &analysis.scene, 2);
        assert_eq!(proposals.len(), 3);
        assert_eq!(proposals[0].patch.changes[0].pos, Pos::new(2, 1, 0));
        assert_eq!(proposals[0].patch.reason, RepairReason::ConnectMissingWire);
        assert!(proposals[0].impact.unwrap().improves());
    }

    #[test]
    fn virtual_repair_evaluates_liveness_electrical_and_temporal_risk() {
        let mut world = World::new();
        for x in 0..=4 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
        }
        world.place(BlockKind::RedstoneBlock, Pos::new(0, 1, 0));
        for x in [1, 3] {
            world.place(BlockKind::RedstoneWire, Pos::new(x, 1, 0));
        }
        let repeater = world.place(BlockKind::Repeater, Pos::new(4, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(1);
        repeater.support_offset = Some(Pos::new(0, -1, 0));
        update_wire_shapes(&mut world);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(5, 3, 1)),
        );
        let proposal = propose_scene_repairs(&world, &analysis.scene, 2)
            .into_iter()
            .find(|proposal| proposal.patch.reason == RepairReason::ConnectMissingWire)
            .unwrap();
        let impact = proposal.impact.unwrap();
        assert!(impact.external_input_waiting_after < impact.external_input_waiting_before);
        assert!(impact.drive_reachable_components_after > impact.drive_reachable_components_before);
        assert!(impact.instantaneous_solve_converged_before);
        assert!(impact.instantaneous_solve_converged_after);
        assert!(impact.energized_positions_after > impact.energized_positions_before);
        assert!(impact.requires_temporal_validation);
    }

    #[test]
    fn inferred_external_input_is_not_automatically_bridged_inside_one_traversal_group() {
        let mut world = World::new();
        for x in 0..=5 {
            for z in 0..=2 {
                world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
            }
        }
        world.place(BlockKind::RedstoneBlock, Pos::new(0, 1, 0));
        for pos in [
            Pos::new(1, 1, 0),
            Pos::new(3, 1, 0),
            Pos::new(1, 1, 1),
            Pos::new(1, 1, 2),
            Pos::new(2, 1, 2),
            Pos::new(3, 1, 2),
            Pos::new(4, 1, 2),
            Pos::new(5, 1, 2),
            Pos::new(5, 1, 1),
            Pos::new(5, 1, 0),
        ] {
            world.place(BlockKind::RedstoneWire, pos);
        }
        let repeater = world.place(BlockKind::Repeater, Pos::new(4, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(1);
        repeater.support_offset = Some(Pos::new(0, -1, 0));
        update_wire_shapes(&mut world);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(6, 3, 3)),
        );
        assert_eq!(analysis.scene.physical_traversal_groups().len(), 1);
        assert!(analysis.scene.gap_candidates(2).is_empty());
        let proposal = propose_scene_repairs(&world, &analysis.scene, 2)
            .into_iter()
            .find(|proposal| {
                proposal.patch.reason == RepairReason::ConnectMissingWire
                    && proposal.patch.changes[0].pos == Pos::new(2, 1, 0)
            });
        assert!(
            proposal.is_none(),
            "an inferred input must not be shorted automatically"
        );
    }

    #[test]
    fn proposes_both_directions_when_repeater_intent_is_ambiguous() {
        let mut world = World::new();
        for x in 0..=2 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
        }
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
        repeater.facing = Some(Facing::North);
        world.place(BlockKind::RedstoneWire, Pos::new(2, 1, 0));
        update_wire_shapes(&mut world);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(0, 0, 0), Pos::new(2, 2, 0)),
        );
        let proposals = propose_scene_repairs(&world, &analysis.scene, 2);
        assert_eq!(
            proposals
                .iter()
                .filter(|proposal| {
                    proposal.patch.reason == RepairReason::ReorientDirectionalComponent
                })
                .count(),
            2
        );
    }

    #[test]
    fn explicit_short_removal_is_low_confidence_and_reversible() {
        let mut world = World::new();
        for x in 0..=2 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
            world.place(BlockKind::RedstoneWire, Pos::new(x, 1, 0));
        }
        update_wire_shapes(&mut world);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(0, 0, 0), Pos::new(2, 2, 0)),
        );
        let proposal =
            propose_scene_component_removal(&world, &analysis.scene, Pos::new(1, 1, 0)).unwrap();
        assert_eq!(proposal.patch.confidence_percent, 40);
        assert_eq!(
            proposal.patch.inverse().changes[0].after.kind,
            BlockKind::RedstoneWire
        );
        assert!(
            proposal.impact.unwrap().fragments_after > proposal.impact.unwrap().fragments_before
        );
    }

    #[test]
    fn restores_missing_support_candidate() {
        let mut world = World::new();
        let repeater = world.place(BlockKind::Repeater, Pos::new(0, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.support_offset = Some(Pos::new(0, -1, 0));
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(0, 0, 0), Pos::new(0, 2, 0)),
        );
        let proposals = propose_scene_repairs(&world, &analysis.scene, 2);
        assert!(proposals.iter().any(|proposal| {
            proposal.patch.reason == RepairReason::RestoreComponentSupport
                && proposal.patch.changes[0].pos == Pos::new(0, 0, 0)
        }));
    }
}
