//! Directed signal-drive diagnostics layered on top of physical connectivity.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use dustroute_physical::{
    BlockKind, ComponentId, PhysicalScene, PortRef, PortRole, Pos, TransferKind,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveFailure {
    DisconnectedRequiredInput,
    NoReachableDriver,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalSourceKind {
    ControllableInput,
    IntrinsicSource,
    ObservationBoundary,
    InferredPrimaryInput,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignalSource {
    pub component: ComponentId,
    pub position: Pos,
    pub kind: SignalSourceKind,
    pub inferred: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredInputStatus {
    DrivenByKnownSource,
    AwaitingExternalInput,
    Disconnected,
    NoKnownSource,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequiredInputAssessment {
    pub device: ComponentId,
    pub position: Pos,
    pub block: BlockKind,
    pub input: PortRef,
    pub status: RequiredInputStatus,
    pub immediate_sources: BTreeSet<ComponentId>,
    pub inferred_primary_inputs: BTreeSet<ComponentId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UndrivenInput {
    pub device: ComponentId,
    pub position: Pos,
    pub block: BlockKind,
    pub input: PortRef,
    pub failure: DriveFailure,
    pub immediate_sources: BTreeSet<ComponentId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignalLivenessReport {
    /// Typed source evidence. Inferred primary inputs remain distinguishable
    /// from controllable or intrinsic sources.
    pub sources: Vec<SignalSource>,
    /// Components that can originate a signal without first receiving one.
    pub drive_sources: BTreeSet<ComponentId>,
    /// Components reachable from a drive source along directed signal edges.
    pub drive_reachable: BTreeSet<ComponentId>,
    /// Reachability when inferred primary inputs are allowed as hypothetical
    /// external drives.
    pub potential_drive_reachable: BTreeSet<ComponentId>,
    /// Required directional-device inputs that can never receive a signal.
    pub undriven_inputs: Vec<UndrivenInput>,
    pub required_input_assessments: Vec<RequiredInputAssessment>,
    /// Maximal groups where every component can reach every other component.
    /// Unlike physical traversal groups, these preserve signal direction.
    pub directed_regions: Vec<DirectedSignalRegion>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DirectedSignalRegion {
    pub id: usize,
    pub components: BTreeSet<ComponentId>,
    pub contains_drive_source: bool,
    pub cyclic: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RankedLivenessFinding {
    pub finding: UndrivenInput,
    pub manhattan_distance_from_focus: u32,
    pub downstream_component_count: usize,
    pub nearby_gap_candidate_count: usize,
    pub suspicion_score: u64,
}

struct SignalIndex {
    forward: BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    reverse: BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    incoming_by_port: BTreeMap<PortRef, BTreeSet<ComponentId>>,
    kinds: BTreeMap<ComponentId, BlockKind>,
}

impl SignalIndex {
    fn new(scene: &PhysicalScene) -> Self {
        let mut forward = BTreeMap::<_, BTreeSet<_>>::new();
        let mut reverse = BTreeMap::<_, BTreeSet<_>>::new();
        let mut incoming_by_port = BTreeMap::<_, BTreeSet<_>>::new();
        for connection in &scene.connections {
            if connection.transfer == TransferKind::StructuralSupport {
                continue;
            }
            let source = connection.source.component;
            let sink = connection.sink.component;
            forward.entry(source).or_default().insert(sink);
            reverse.entry(sink).or_default().insert(source);
            incoming_by_port
                .entry(connection.sink)
                .or_default()
                .insert(source);
        }
        Self {
            forward,
            reverse,
            incoming_by_port,
            kinds: scene
                .components
                .iter()
                .map(|component| (component.id, component.block.kind))
                .collect(),
        }
    }
}

#[must_use]
pub fn analyze_signal_liveness(scene: &PhysicalScene) -> SignalLivenessReport {
    let index = SignalIndex::new(scene);
    let directed_regions = directed_signal_regions(scene, &index);
    let mut sources = scene
        .components
        .iter()
        .filter_map(|component| {
            let kind = match component.block.kind {
                BlockKind::Lever | BlockKind::Button | BlockKind::PressurePlate => {
                    SignalSourceKind::ControllableInput
                }
                BlockKind::RedstoneBlock | BlockKind::RedstoneTorch => {
                    SignalSourceKind::IntrinsicSource
                }
                _ => return None,
            };
            Some(SignalSource {
                component: component.id,
                position: component.pos,
                kind,
                inferred: false,
            })
        })
        .chain(
            scene
                .open_frontier_components()
                .into_iter()
                .filter_map(|component| {
                    scene
                        .components
                        .iter()
                        .find(|candidate| candidate.id == component)
                        .map(|component| SignalSource {
                            component: component.id,
                            position: component.pos,
                            kind: SignalSourceKind::ObservationBoundary,
                            inferred: false,
                        })
                }),
        )
        .collect::<Vec<_>>();
    let drive_sources = sources
        .iter()
        .map(|source| source.component)
        .collect::<BTreeSet<_>>();
    let inferred_primary_inputs = infer_primary_inputs(&directed_regions, &drive_sources, &index);
    sources.extend(inferred_primary_inputs.iter().filter_map(|component| {
        scene
            .components
            .iter()
            .find(|candidate| candidate.id == *component)
            .map(|component| SignalSource {
                component: component.id,
                position: component.pos,
                kind: SignalSourceKind::InferredPrimaryInput,
                inferred: true,
            })
    }));
    sources.sort_by_key(|source| (source.position, source.component, source.kind as u8));
    let drive_reachable = reachable_from_sources(&index.forward, &drive_sources);
    let potential_sources = drive_sources
        .union(&inferred_primary_inputs)
        .copied()
        .collect::<BTreeSet<_>>();
    let potential_drive_reachable = reachable_from_sources(&index.forward, &potential_sources);

    let mut undriven_inputs = Vec::new();
    let mut required_input_assessments = Vec::new();
    for device in scene.components.iter().filter(|component| {
        matches!(
            component.block.kind,
            BlockKind::Repeater
                | BlockKind::Comparator
                | BlockKind::Piston
                | BlockKind::RedstoneTorch
        )
    }) {
        for port in device.ports.iter().filter(|port| {
            port.role == PortRole::Input
                || (device.block.kind == BlockKind::RedstoneTorch && port.role == PortRole::Control)
        }) {
            let input = PortRef {
                component: device.id,
                port: port.id,
            };
            let immediate_sources = index
                .incoming_by_port
                .get(&input)
                .cloned()
                .unwrap_or_default();
            let (status, failure) = if immediate_sources.is_empty() {
                (
                    RequiredInputStatus::Disconnected,
                    Some(DriveFailure::DisconnectedRequiredInput),
                )
            } else if immediate_sources
                .iter()
                .any(|source| drive_reachable.contains(source))
            {
                (RequiredInputStatus::DrivenByKnownSource, None)
            } else if immediate_sources
                .iter()
                .any(|source| potential_drive_reachable.contains(source))
            {
                (RequiredInputStatus::AwaitingExternalInput, None)
            } else if immediate_sources
                .iter()
                .all(|source| !drive_reachable.contains(source))
            {
                (
                    RequiredInputStatus::NoKnownSource,
                    Some(DriveFailure::NoReachableDriver),
                )
            } else {
                (RequiredInputStatus::DrivenByKnownSource, None)
            };
            let upstream = reachable_from_sources(&index.reverse, &immediate_sources);
            let assessment_inferred_inputs = upstream
                .intersection(&inferred_primary_inputs)
                .copied()
                .collect();
            required_input_assessments.push(RequiredInputAssessment {
                device: device.id,
                position: device.pos,
                block: device.block.kind,
                input,
                status,
                immediate_sources: immediate_sources.clone(),
                inferred_primary_inputs: assessment_inferred_inputs,
            });
            if let Some(failure) = failure {
                undriven_inputs.push(UndrivenInput {
                    device: device.id,
                    position: device.pos,
                    block: device.block.kind,
                    input,
                    failure,
                    immediate_sources,
                });
            }
        }
    }
    SignalLivenessReport {
        sources,
        drive_sources,
        drive_reachable,
        potential_drive_reachable,
        undriven_inputs,
        required_input_assessments,
        directed_regions,
    }
}

#[must_use]
pub fn rank_liveness_findings(
    scene: &PhysicalScene,
    report: &SignalLivenessReport,
    focus: Pos,
) -> Vec<RankedLivenessFinding> {
    let index = SignalIndex::new(scene);
    let gaps = scene.gap_candidates(2);
    let by_id = scene
        .components
        .iter()
        .map(|component| (component.id, component.pos))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut ranked = report
        .undriven_inputs
        .iter()
        .map(|finding| {
            let distance = manhattan(finding.position, focus);
            let downstream = reachable_from(&index.forward, finding.device);
            let nearby_gap_candidate_count = gaps
                .iter()
                .filter(|gap| {
                    gap.evidence.iter().any(|evidence| match evidence {
                        dustroute_physical::GapEvidence::Nearby { left, right, .. } => {
                            [left, right].iter().any(|component| {
                                by_id.get(component).is_some_and(|position| {
                                    manhattan(*position, finding.position) <= 4
                                })
                            })
                        }
                        dustroute_physical::GapEvidence::MissingInlineBlock { position }
                        | dustroute_physical::GapEvidence::InvalidSupport {
                            expected_support: position,
                            ..
                        } => manhattan(*position, finding.position) <= 4,
                        dustroute_physical::GapEvidence::DirectionMismatch {
                            component, ..
                        }
                        | dustroute_physical::GapEvidence::SuspectedUnexpectedConnection {
                            component,
                        } => by_id
                            .get(component)
                            .is_some_and(|position| manhattan(*position, finding.position) <= 4),
                    })
                })
                .count();
            let failure_weight = match finding.failure {
                DriveFailure::DisconnectedRequiredInput => 2_000,
                DriveFailure::NoReachableDriver => 1_000,
            };
            let suspicion_score = failure_weight
                + u64::try_from(downstream.len().min(100))
                    .unwrap_or(u64::MAX)
                    .saturating_mul(10)
                + u64::try_from(nearby_gap_candidate_count)
                    .unwrap_or(u64::MAX)
                    .saturating_mul(500)
                + 10_000_u64 / (u64::from(distance) + 1);
            RankedLivenessFinding {
                finding: finding.clone(),
                manhattan_distance_from_focus: distance,
                downstream_component_count: downstream.len(),
                nearby_gap_candidate_count,
                suspicion_score,
            }
        })
        .collect::<Vec<_>>();
    ranked.sort_by_key(|candidate| {
        (
            std::cmp::Reverse(candidate.suspicion_score),
            candidate.manhattan_distance_from_focus,
            candidate.finding.position,
        )
    });
    ranked
}

fn infer_primary_inputs(
    regions: &[DirectedSignalRegion],
    confirmed_sources: &BTreeSet<ComponentId>,
    index: &SignalIndex,
) -> BTreeSet<ComponentId> {
    let region_by_component = regions
        .iter()
        .flat_map(|region| {
            region
                .components
                .iter()
                .map(move |component| (*component, region.id))
        })
        .collect::<BTreeMap<_, _>>();
    let mut regions_with_external_incoming = BTreeSet::new();
    for (source_component, sinks) in &index.forward {
        let Some(source) = region_by_component.get(source_component) else {
            continue;
        };
        let Some(source_kind) = index.kinds.get(source_component) else {
            continue;
        };
        if matches!(source_kind, BlockKind::Solid | BlockKind::Transparent) {
            continue;
        }
        for sink_component in sinks {
            if let Some(sink) = region_by_component.get(sink_component)
                && source != sink
            {
                regions_with_external_incoming.insert(*sink);
            }
        }
    }
    regions
        .iter()
        .filter(|region| {
            !regions_with_external_incoming.contains(&region.id)
                && region.components.is_disjoint(confirmed_sources)
        })
        .flat_map(|region| {
            region.components.iter().filter(|component| {
                if index.kinds.get(component) != Some(&BlockKind::RedstoneWire) {
                    return false;
                }
                let neighbors = index
                    .forward
                    .get(component)
                    .into_iter()
                    .flatten()
                    .chain(index.reverse.get(component).into_iter().flatten())
                    .copied()
                    .filter(|neighbor| region.components.contains(neighbor))
                    .collect::<BTreeSet<_>>();
                neighbors.len() <= 1
            })
        })
        .copied()
        .collect()
}

fn reachable_from_sources(
    adjacency: &BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    sources: &BTreeSet<ComponentId>,
) -> BTreeSet<ComponentId> {
    let mut reachable = sources.clone();
    let mut queue = VecDeque::from_iter(sources.iter().copied());
    while let Some(source) = queue.pop_front() {
        for sink in adjacency.get(&source).into_iter().flatten().copied() {
            if reachable.insert(sink) {
                queue.push_back(sink);
            }
        }
    }
    reachable
}

fn directed_signal_regions(
    scene: &PhysicalScene,
    index: &SignalIndex,
) -> Vec<DirectedSignalRegion> {
    let nodes = scene
        .components
        .iter()
        .map(|component| component.id)
        .collect::<BTreeSet<_>>();
    let mut order = Vec::new();
    let mut visited = BTreeSet::new();
    for node in &nodes {
        visit_order(*node, &index.forward, &mut visited, &mut order);
    }
    visited.clear();
    let sources = scene
        .components
        .iter()
        .filter(|component| {
            matches!(
                component.block.kind,
                BlockKind::Lever
                    | BlockKind::Button
                    | BlockKind::PressurePlate
                    | BlockKind::RedstoneBlock
                    | BlockKind::RedstoneTorch
            )
        })
        .map(|component| component.id)
        .chain(scene.open_frontier_components())
        .collect::<BTreeSet<_>>();
    let mut regions = Vec::new();
    while let Some(node) = order.pop() {
        if visited.contains(&node) {
            continue;
        }
        let mut components = BTreeSet::new();
        collect_region(node, &index.reverse, &mut visited, &mut components);
        let cyclic = components.len() > 1
            || index
                .forward
                .get(&node)
                .is_some_and(|sinks| sinks.contains(&node));
        regions.push(DirectedSignalRegion {
            id: regions.len(),
            contains_drive_source: !components.is_disjoint(&sources),
            components,
            cyclic,
        });
    }
    regions
}

fn visit_order(
    node: ComponentId,
    adjacency: &BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    visited: &mut BTreeSet<ComponentId>,
    order: &mut Vec<ComponentId>,
) {
    if !visited.insert(node) {
        return;
    }
    for sink in adjacency.get(&node).into_iter().flatten().copied() {
        visit_order(sink, adjacency, visited, order);
    }
    order.push(node);
}

fn collect_region(
    node: ComponentId,
    adjacency: &BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    visited: &mut BTreeSet<ComponentId>,
    region: &mut BTreeSet<ComponentId>,
) {
    if !visited.insert(node) {
        return;
    }
    region.insert(node);
    for sink in adjacency.get(&node).into_iter().flatten().copied() {
        collect_region(sink, adjacency, visited, region);
    }
}

fn reachable_from(
    adjacency: &BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    start: ComponentId,
) -> BTreeSet<ComponentId> {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([start]);
    while let Some(source) = queue.pop_front() {
        if !seen.insert(source) {
            continue;
        }
        queue.extend(adjacency.get(&source).into_iter().flatten().copied());
    }
    seen
}

fn manhattan(left: Pos, right: Pos) -> u32 {
    left.x.abs_diff(right.x) + left.y.abs_diff(right.y) + left.z.abs_diff(right.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Facing, RegionBounds, World, analyze_world_region};

    #[test]
    fn reports_a_repeater_whose_required_input_is_missing() {
        let mut world = World::new();
        for x in 0..=2 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
        }
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::Lever));
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.facing = Some(Facing::East);
        world.set(Pos::new(2, 1, 0), repeater);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(3, 2, 1)),
        );
        let report = analyze_signal_liveness(&analysis.scene);
        assert!(report.undriven_inputs.iter().any(|finding| {
            finding.position == Pos::new(2, 1, 0)
                && finding.failure == DriveFailure::DisconnectedRequiredInput
        }));
    }

    #[test]
    fn distinguishes_an_inferred_external_input_from_a_fault() {
        let mut world = World::new();
        for x in 0..=2 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
        }
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::RedstoneWire));
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.facing = Some(Facing::East);
        world.set(Pos::new(2, 1, 0), repeater);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(3, 2, 1)),
        );
        let report = analyze_signal_liveness(&analysis.scene);
        assert!(
            report.required_input_assessments.iter().any(|assessment| {
                assessment.position == Pos::new(2, 1, 0)
                    && assessment.status == RequiredInputStatus::AwaitingExternalInput
                    && !assessment.inferred_primary_inputs.is_empty()
            }),
            "{report:#?}"
        );
        assert!(
            !report
                .undriven_inputs
                .iter()
                .any(|finding| finding.position == Pos::new(2, 1, 0))
        );
    }

    #[test]
    fn classifies_a_bare_bus_feeding_directional_devices_as_an_external_input() {
        let mut world = World::new();
        for z in 8..=25 {
            world.set(Pos::new(46, 0, z), Block::new(BlockKind::Solid));
            world.set(
                Pos::new(46, 1, z),
                if z == 20 {
                    let mut repeater = Block::new(BlockKind::Repeater);
                    repeater.facing = Some(Facing::North);
                    repeater
                } else {
                    Block::new(BlockKind::RedstoneWire)
                },
            );
        }
        world.set(Pos::new(45, 0, 8), Block::new(BlockKind::Solid));
        let mut branch_repeater = Block::new(BlockKind::Repeater);
        branch_repeater.facing = Some(Facing::West);
        world.set(Pos::new(45, 1, 8), branch_repeater);

        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(44, -1, 7), Pos::new(47, 2, 26)),
        );
        let report = analyze_signal_liveness(&analysis.scene);

        assert!(report.sources.iter().any(|source| {
            source.position == Pos::new(46, 1, 25)
                && source.kind == SignalSourceKind::InferredPrimaryInput
        }));
        assert!(report.required_input_assessments.iter().any(|assessment| {
            assessment.position == Pos::new(45, 1, 8)
                && assessment.status == RequiredInputStatus::AwaitingExternalInput
        }));
        assert!(
            !report
                .undriven_inputs
                .iter()
                .any(|finding| finding.position == Pos::new(45, 1, 8))
        );
    }

    #[test]
    fn directed_regions_partition_every_physical_component() {
        let mut world = World::new();
        for x in 0..=3 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
        }
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::Lever));
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::RedstoneWire));
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.facing = Some(Facing::East);
        world.set(Pos::new(2, 1, 0), repeater);
        world.set(Pos::new(3, 1, 0), Block::new(BlockKind::RedstoneWire));
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(4, 2, 1)),
        );
        let report = analyze_signal_liveness(&analysis.scene);
        let memberships = report
            .directed_regions
            .iter()
            .flat_map(|region| region.components.iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(memberships.len(), analysis.scene.components.len());
        assert_eq!(
            memberships.iter().copied().collect::<BTreeSet<_>>().len(),
            memberships.len()
        );
    }

    #[test]
    fn ranking_uses_the_focus_without_redefining_group_membership() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.facing = Some(Facing::East);
        world.set(Pos::new(0, 1, 0), repeater);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(1, 2, 1)),
        );
        let report = analyze_signal_liveness(&analysis.scene);
        let ranked = rank_liveness_findings(&analysis.scene, &report, Pos::new(0, 1, 0));
        assert_eq!(ranked[0].manhattan_distance_from_focus, 0);
        assert_eq!(
            ranked[0].finding.failure,
            DriveFailure::DisconnectedRequiredInput
        );
    }
}
