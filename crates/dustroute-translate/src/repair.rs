use std::collections::{BTreeMap, BTreeSet};

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
    propose_physical_repairs(world, scene, max_gap)
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
) -> Vec<RepairProposal> {
    let mut proposals = missing_wire_repairs(world, circuit, max_gap);
    proposals.extend(missing_directional_component_repairs(
        world, circuit, max_gap,
    ));
    proposals.extend(missing_support_repairs(world, circuit));
    proposals.extend(direction_repairs(world, circuit));
    for proposal in &mut proposals {
        proposal.impact = evaluate_repair(world, circuit, &proposal.patch);
    }
    proposals.sort_by_key(|proposal| {
        (
            std::cmp::Reverse(proposal.impact.is_some_and(RepairImpact::improves)),
            std::cmp::Reverse(proposal.patch.confidence_percent),
            proposal.patch.changes[0].pos,
        )
    });
    proposals
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
                || !world.kind_at(support).properties().supports_components
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
                .kind_at(missing.offset(0, -1, 0))
                .properties()
                .supports_components
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
    let (min, max) = repaired.bounds()?;
    let analysis = crate::analyze_world_region(&repaired, crate::RegionBounds::new(min, max));
    Some(RepairImpact {
        fragments_before: circuit.fragments.len(),
        fragments_after: analysis.scene.fragments.len(),
        invalid_supports_before: world.support_issues().len(),
        invalid_supports_after: repaired.support_issues().len(),
    })
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
            RegionBounds::new(Pos::new(0, 0, 0), Pos::new(4, 2, 0)),
        );
        let proposals = propose_scene_repairs(&world, &analysis.scene, 2);
        assert_eq!(proposals.len(), 3);
        assert_eq!(proposals[0].patch.changes[0].pos, Pos::new(2, 1, 0));
        assert_eq!(proposals[0].patch.reason, RepairReason::ConnectMissingWire);
        assert!(proposals[0].impact.unwrap().improves());
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
