use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{
    Block, BlockCapabilities, BlockKind, CapabilityLevel, ComponentId, ConnectionKind, Facing,
    FragmentId, GapCandidate, GapEvidence, PhysicalComponent, PhysicalFragment, PhysicalNet, Pos,
    VerifiedTopology, WireConnection,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStage {
    PhysicalClassification,
    Connectivity,
    SteadyState,
    Temporal,
    Repair,
    Placement,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockCapabilityGroup {
    pub observed_name: Option<String>,
    pub kind: BlockKind,
    pub count: usize,
    pub capabilities: BlockCapabilities,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CapabilityIssue {
    pub component: ComponentId,
    pub position: Pos,
    pub observed_name: Option<String>,
    pub kind: BlockKind,
    pub stage: CapabilityStage,
    pub level: CapabilityLevel,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SceneCapabilityReport {
    pub groups: Vec<BlockCapabilityGroup>,
    pub issues: Vec<CapabilityIssue>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Observation {
    pub dimension: String,
    pub regions: Vec<ObservedRegion>,
    pub frontier: Vec<ObservationFrontier>,
}

impl Observation {
    #[must_use]
    pub fn complete(dimension: impl Into<String>, bounds: SceneBounds) -> Self {
        Self {
            dimension: dimension.into(),
            regions: vec![ObservedRegion {
                bounds,
                completeness: RegionCompleteness::Complete,
            }],
            frontier: Vec::new(),
        }
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.frontier.is_empty()
            && self
                .regions
                .iter()
                .all(|region| region.completeness == RegionCompleteness::Complete)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SceneBounds {
    pub min: Pos,
    pub max: Pos,
}

impl SceneBounds {
    #[must_use]
    pub const fn new(a: Pos, b: Pos) -> Self {
        Self {
            min: Pos::new(min(a.x, b.x), min(a.y, b.y), min(a.z, b.z)),
            max: Pos::new(max(a.x, b.x), max(a.y, b.y), max(a.z, b.z)),
        }
    }

    #[must_use]
    pub const fn contains(self, pos: Pos) -> bool {
        pos.x >= self.min.x
            && pos.x <= self.max.x
            && pos.y >= self.min.y
            && pos.y <= self.max.y
            && pos.z >= self.min.z
            && pos.z <= self.max.z
    }
}

const fn min(a: i32, b: i32) -> i32 {
    if a < b { a } else { b }
}

const fn max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedRegion {
    pub bounds: SceneBounds,
    pub completeness: RegionCompleteness,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionCompleteness {
    Complete,
    OpenBoundary,
    MissingChunks,
    PartiallyUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservationFrontier {
    pub position: Pos,
    pub direction: Facing,
    pub reason: FrontierReason,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontierReason {
    ConnectedComponentContinues,
    ChunkUnavailable,
    ScanLimitReached,
    PolicyBoundary,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PortId(pub u8);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PortRef {
    pub component: ComponentId,
    pub port: PortId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalPort {
    pub id: PortId,
    pub role: PortRole,
    pub face: Facing,
    pub channel: PortChannel,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortRole {
    Input,
    Output,
    Bidirectional,
    Control,
    Support,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortChannel {
    RedstoneSignal,
    WeakPower,
    StrongPower,
    Observation,
    Mechanical,
    Structural,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SceneComponent {
    pub id: ComponentId,
    pub pos: Pos,
    pub block: Block,
    pub ports: Vec<PhysicalPort>,
    pub support: Option<SupportRelation>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SupportRelation {
    pub support_position: Pos,
    pub valid: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PortConnection {
    pub source: PortRef,
    pub sink: PortRef,
    pub transfer: TransferKind,
    pub confidence: Confidence,
    pub evidence: Vec<PhysicalEvidence>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    DustPropagation,
    DirectSignal,
    WeakPower,
    StrongPower,
    DirectionalDevice,
    SideControl,
    /// A block-state observation edge into an Observer. This is not a power
    /// transfer and must not be folded into an electrical net.
    Observation,
    StructuralSupport,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
    Certain,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhysicalEvidence {
    AdjacentBlocks {
        source: Pos,
        sink: Pos,
    },
    BlockFacing {
        component: ComponentId,
        facing: Facing,
    },
    WireShape {
        component: ComponentId,
        toward: Facing,
        shape: WireConnection,
    },
    Support {
        component: ComponentId,
        support: Pos,
    },
    MinecraftRule {
        rule: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhysicalDiagnostic {
    OpenObservationBoundary {
        frontier: ObservationFrontier,
    },
    InvalidSupport {
        component: ComponentId,
        expected: Pos,
    },
    AmbiguousConnection {
        components: [ComponentId; 2],
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalScene {
    pub observation: Observation,
    pub components: Vec<SceneComponent>,
    pub connections: Vec<PortConnection>,
    pub nets: Vec<PhysicalNet>,
    pub fragments: Vec<PhysicalFragment>,
    pub diagnostics: Vec<PhysicalDiagnostic>,
}

impl PhysicalScene {
    /// Undirected groups used to bound physical discovery. They are not
    /// directional signal regions or inferred logical circuits.
    #[must_use]
    pub fn physical_traversal_groups(&self) -> &[PhysicalFragment] {
        &self.fragments
    }

    #[must_use]
    pub fn from_unvalidated_topology(
        observation: Observation,
        topology: &VerifiedTopology,
    ) -> Self {
        Self::from_topology_and_optional_world(observation, topology, None)
    }

    /// Builds a physical observation while validating placement support against
    /// the observed world, before support-only blocks are removed from signal
    /// topology.
    #[must_use]
    pub fn from_topology_and_world(
        observation: Observation,
        topology: &VerifiedTopology,
        world: &crate::World,
    ) -> Self {
        Self::from_topology_and_optional_world(observation, topology, Some(world))
    }

    fn from_topology_and_optional_world(
        observation: Observation,
        topology: &VerifiedTopology,
        world: Option<&crate::World>,
    ) -> Self {
        let components = topology
            .components
            .iter()
            .map(|component| scene_component(component, topology, world))
            .collect::<Vec<_>>();
        let by_id = components
            .iter()
            .map(|component| (component.id, component))
            .collect::<BTreeMap<_, _>>();
        let connections = topology
            .connections
            .iter()
            .filter_map(|connection| port_connection(connection, &by_id))
            .collect();
        let mut diagnostics = observation
            .frontier
            .iter()
            .cloned()
            .map(|frontier| PhysicalDiagnostic::OpenObservationBoundary { frontier })
            .collect::<Vec<_>>();
        diagnostics.extend(components.iter().filter_map(|component| {
            component
                .support
                .filter(|support| !support.valid)
                .map(|support| PhysicalDiagnostic::InvalidSupport {
                    component: component.id,
                    expected: support.support_position,
                })
        }));
        Self {
            observation,
            components,
            connections,
            nets: topology.nets.clone(),
            fragments: topology.fragments.clone(),
            diagnostics,
        }
    }

    #[must_use]
    pub fn component_at(&self, pos: Pos) -> Option<&SceneComponent> {
        self.components
            .iter()
            .find(|component| component.pos == pos)
    }

    #[must_use]
    pub fn open_frontier_components(&self) -> BTreeSet<ComponentId> {
        self.observation
            .frontier
            .iter()
            .filter_map(|frontier| self.component_at(frontier.position).map(|item| item.id))
            .collect()
    }

    #[must_use]
    pub fn capability_report(&self) -> SceneCapabilityReport {
        let mut groups: Vec<BlockCapabilityGroup> = Vec::new();
        let mut issues = Vec::new();
        for component in &self.components {
            let capabilities = component.block.capabilities();
            let observed_name = component.block.observed_name.clone();
            if let Some(group) = groups.iter_mut().find(|group| {
                group.kind == component.block.kind && group.observed_name == observed_name
            }) {
                group.count += 1;
            } else {
                groups.push(BlockCapabilityGroup {
                    observed_name: observed_name.clone(),
                    kind: component.block.kind,
                    count: 1,
                    capabilities,
                });
            }
            for (stage, level) in [
                (
                    CapabilityStage::PhysicalClassification,
                    capabilities.physical_classification,
                ),
                (CapabilityStage::Connectivity, capabilities.connectivity),
                (CapabilityStage::SteadyState, capabilities.steady_state),
                (CapabilityStage::Temporal, capabilities.temporal),
                (CapabilityStage::Repair, capabilities.repair),
                (CapabilityStage::Placement, capabilities.placement),
            ] {
                if matches!(
                    level,
                    CapabilityLevel::Partial | CapabilityLevel::Unsupported
                ) {
                    issues.push(CapabilityIssue {
                        component: component.id,
                        position: component.pos,
                        observed_name: observed_name.clone(),
                        kind: component.block.kind,
                        stage,
                        level,
                    });
                }
            }
        }
        SceneCapabilityReport { groups, issues }
    }

    #[must_use]
    pub fn gap_candidates(&self, max_manhattan_distance: u32) -> Vec<GapCandidate> {
        let by_id: BTreeMap<_, _> = self
            .components
            .iter()
            .map(|component| (component.id, component))
            .collect();
        let mut candidates = Vec::new();
        for (left_index, left) in self.fragments.iter().enumerate() {
            for right in self.fragments.iter().skip(left_index + 1) {
                let nearest = left
                    .components
                    .iter()
                    .filter(|id| by_id[id].block.kind.is_redstone_related())
                    .flat_map(|left_id| {
                        right
                            .components
                            .iter()
                            .filter(|id| by_id[id].block.kind.is_redstone_related())
                            .map(|right_id| {
                                let a = by_id[left_id].pos;
                                let b = by_id[right_id].pos;
                                (
                                    *left_id,
                                    *right_id,
                                    a.x.abs_diff(b.x) + a.y.abs_diff(b.y) + a.z.abs_diff(b.z),
                                )
                            })
                    })
                    .min_by_key(|(_, _, distance)| *distance);
                if let Some((left_component, right_component, distance)) =
                    nearest.filter(|(_, _, distance)| *distance <= max_manhattan_distance)
                {
                    candidates.push(GapCandidate {
                        left: left.id,
                        right: right.id,
                        evidence: vec![GapEvidence::Nearby {
                            left: left_component,
                            right: right_component,
                            manhattan_distance: distance,
                        }],
                    });
                }
            }
        }
        candidates
    }

    #[must_use]
    pub fn discover_nearby_fragments(
        &self,
        seed: FragmentId,
        max_manhattan_distance: u32,
    ) -> BTreeSet<FragmentId> {
        let mut adjacency = BTreeMap::<FragmentId, Vec<FragmentId>>::new();
        for candidate in self.gap_candidates(max_manhattan_distance) {
            adjacency
                .entry(candidate.left)
                .or_default()
                .push(candidate.right);
            adjacency
                .entry(candidate.right)
                .or_default()
                .push(candidate.left);
        }
        let mut discovered = BTreeSet::from([seed]);
        let mut queue = VecDeque::from([seed]);
        while let Some(fragment) = queue.pop_front() {
            for next in adjacency.get(&fragment).into_iter().flatten() {
                if discovered.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        discovered
    }
}

fn scene_component(
    component: &PhysicalComponent,
    topology: &VerifiedTopology,
    world: Option<&crate::World>,
) -> SceneComponent {
    let support = component
        .block
        .support_pos(component.pos)
        .map(|support_position| {
            let valid = world.map_or_else(
                || {
                    topology.components.iter().any(|candidate| {
                        candidate.pos == support_position
                            && candidate.block.redstone_traits().supports_dust_on_top
                    })
                },
                |world| {
                    world
                        .get(support_position)
                        .is_some_and(|block| block.redstone_traits().supports_dust_on_top)
                },
            );
            SupportRelation {
                support_position,
                valid,
            }
        });
    SceneComponent {
        id: component.id,
        pos: component.pos,
        block: component.block.clone(),
        ports: ports_for(&component.block),
        support,
    }
}

fn ports_for(block: &Block) -> Vec<PhysicalPort> {
    let mut ports = Vec::new();
    let mut push = |role, face, channel| {
        ports.push(PhysicalPort {
            id: PortId(ports.len() as u8),
            role,
            face,
            channel,
        });
    };
    let horizontal = [Facing::North, Facing::East, Facing::South, Facing::West];
    if block.kind != BlockKind::Air {
        // Observation is a state relation, not an electrical conductor. Keep
        // a face-addressable port on every present block so an Observer's
        // input edge remains representable even for an otherwise inert solid.
        for face in [
            Facing::North,
            Facing::East,
            Facing::South,
            Facing::West,
            Facing::Up,
            Facing::Down,
        ] {
            push(PortRole::Output, face, PortChannel::Observation);
        }
    }
    match block.kind {
        BlockKind::RedstoneWire => {
            for face in horizontal {
                push(PortRole::Bidirectional, face, PortChannel::RedstoneSignal);
            }
            push(
                PortRole::Bidirectional,
                Facing::Up,
                PortChannel::RedstoneSignal,
            );
            push(
                PortRole::Bidirectional,
                Facing::Down,
                PortChannel::RedstoneSignal,
            );
            push(PortRole::Support, Facing::Down, PortChannel::Structural);
        }
        BlockKind::Repeater | BlockKind::Comparator => {
            let facing = block.facing.unwrap_or(Facing::North);
            push(PortRole::Output, facing, PortChannel::RedstoneSignal);
            push(
                PortRole::Input,
                facing.opposite(),
                PortChannel::RedstoneSignal,
            );
            for face in horizontal
                .into_iter()
                .filter(|face| *face != facing && *face != facing.opposite())
            {
                push(PortRole::Control, face, PortChannel::RedstoneSignal);
            }
            push(PortRole::Support, Facing::Down, PortChannel::Structural);
        }
        BlockKind::RedstoneTorch => {
            for face in horizontal {
                push(PortRole::Output, face, PortChannel::RedstoneSignal);
            }
            let control_face = match block.support_offset {
                Some(Pos { x: 1, y: 0, z: 0 }) => Facing::East,
                Some(Pos { x: -1, y: 0, z: 0 }) => Facing::West,
                Some(Pos { x: 0, y: 1, z: 0 }) => Facing::Up,
                Some(Pos { x: 0, y: 0, z: 1 }) => Facing::South,
                Some(Pos { x: 0, y: 0, z: -1 }) => Facing::North,
                _ => Facing::Down,
            };
            push(PortRole::Control, control_face, PortChannel::RedstoneSignal);
        }
        BlockKind::Lever
        | BlockKind::Button
        | BlockKind::PressurePlate
        | BlockKind::RedstoneBlock => {
            for face in horizontal {
                push(PortRole::Output, face, PortChannel::StrongPower);
            }
            if let Some(face) = support_facing(block.support_offset)
                && !horizontal.contains(&face)
            {
                push(PortRole::Output, face, PortChannel::StrongPower);
            }
        }
        BlockKind::Piston => {
            for face in horizontal {
                push(PortRole::Input, face, PortChannel::RedstoneSignal);
            }
            push(
                PortRole::Output,
                block.facing.unwrap_or(Facing::North),
                PortChannel::Mechanical,
            );
        }
        BlockKind::Observer => {
            let output = block.facing.unwrap_or(Facing::North);
            push(PortRole::Output, output, PortChannel::StrongPower);
            push(PortRole::Input, output.opposite(), PortChannel::Observation);
        }
        BlockKind::Solid | BlockKind::Transparent | BlockKind::RedstoneLamp => {
            if !block.redstone_traits().conducts_weak_power {
                return ports;
            }
            for face in [
                Facing::North,
                Facing::East,
                Facing::South,
                Facing::West,
                Facing::Up,
                Facing::Down,
            ] {
                push(PortRole::Bidirectional, face, PortChannel::WeakPower);
            }
        }
        BlockKind::Air => {}
    }
    ports
}

fn support_facing(offset: Option<Pos>) -> Option<Facing> {
    match offset? {
        Pos { x: 1, y: 0, z: 0 } => Some(Facing::East),
        Pos { x: -1, y: 0, z: 0 } => Some(Facing::West),
        Pos { x: 0, y: 1, z: 0 } => Some(Facing::Up),
        Pos { x: 0, y: -1, z: 0 } => Some(Facing::Down),
        Pos { x: 0, y: 0, z: 1 } => Some(Facing::South),
        Pos { x: 0, y: 0, z: -1 } => Some(Facing::North),
        _ => None,
    }
}

fn port_connection(
    connection: &crate::PhysicalConnection,
    components: &BTreeMap<ComponentId, &SceneComponent>,
) -> Option<PortConnection> {
    let source = components.get(&connection.source)?;
    let sink = components.get(&connection.sink)?;
    let source_face = facing_between(source.pos, sink.pos)?;
    let sink_face = source_face.opposite();
    let source_port = select_port(source, source_face, true, connection.kind)?;
    let sink_port = select_port(sink, sink_face, false, connection.kind)?;
    let mut evidence = vec![PhysicalEvidence::AdjacentBlocks {
        source: source.pos,
        sink: sink.pos,
    }];
    if let Some(facing) = source.block.facing {
        evidence.push(PhysicalEvidence::BlockFacing {
            component: source.id,
            facing,
        });
    }
    if let Some(shape) = source
        .block
        .wire_connections
        .as_ref()
        .and_then(|connections| connections.get(&source_face))
        .copied()
    {
        evidence.push(PhysicalEvidence::WireShape {
            component: source.id,
            toward: source_face,
            shape,
        });
    }
    if let Some(shape) = sink
        .block
        .wire_connections
        .as_ref()
        .and_then(|connections| connections.get(&sink_face))
        .copied()
    {
        evidence.push(PhysicalEvidence::WireShape {
            component: sink.id,
            toward: sink_face,
            shape,
        });
    }
    evidence.push(PhysicalEvidence::MinecraftRule {
        rule: match connection.kind {
            ConnectionKind::Dust => "java.redstone.dust_connection.horizontal",
            ConnectionKind::DustRise => "java.redstone.dust_connection.vertical_rise",
            ConnectionKind::DustFallThroughConductor => {
                "java.redstone.dust_connection.vertical_fall_through_conductor"
            }
            ConnectionKind::WeakPower => "java.redstone.weak_power",
            ConnectionKind::StrongPower => "java.redstone.strong_power",
            ConnectionKind::DirectionalInput => "java.redstone.directional_input",
            ConnectionKind::DirectionalOutput => "java.redstone.directional_output",
            ConnectionKind::DirectSource => "java.redstone.direct_source",
            ConnectionKind::Control => "java.redstone.side_or_support_control",
            ConnectionKind::ObserverInput => "java.redstone.observer.block_state_observation",
            ConnectionKind::ObserverOutput => "java.redstone.observer.strong_pulse",
            ConnectionKind::Support => "java.block.structural_support",
        }
        .to_owned(),
    });
    Some(PortConnection {
        source: PortRef {
            component: source.id,
            port: source_port.id,
        },
        sink: PortRef {
            component: sink.id,
            port: sink_port.id,
        },
        transfer: transfer_kind(connection.kind),
        confidence: Confidence::Certain,
        evidence,
    })
}

fn select_port(
    component: &SceneComponent,
    face: Facing,
    outgoing: bool,
    kind: ConnectionKind,
) -> Option<&PhysicalPort> {
    component.ports.iter().find(|port| {
        port.face == face
            && match kind {
                ConnectionKind::Support => port.channel == PortChannel::Structural,
                ConnectionKind::Control if outgoing => {
                    matches!(port.role, PortRole::Output | PortRole::Bidirectional)
                }
                ConnectionKind::Control => port.role == PortRole::Control,
                ConnectionKind::ObserverInput if outgoing => {
                    port.channel == PortChannel::Observation && port.role == PortRole::Output
                }
                ConnectionKind::ObserverInput => {
                    port.channel == PortChannel::Observation && port.role == PortRole::Input
                }
                ConnectionKind::ObserverOutput if outgoing => {
                    port.channel == PortChannel::StrongPower && port.role == PortRole::Output
                }
                ConnectionKind::ObserverOutput => {
                    matches!(
                        port.channel,
                        PortChannel::RedstoneSignal
                            | PortChannel::WeakPower
                            | PortChannel::StrongPower
                    ) && matches!(port.role, PortRole::Input | PortRole::Bidirectional)
                }
                _ if outgoing => matches!(port.role, PortRole::Output | PortRole::Bidirectional),
                _ => matches!(
                    port.role,
                    PortRole::Input | PortRole::Bidirectional | PortRole::Control
                ),
            }
    })
}

const fn transfer_kind(kind: ConnectionKind) -> TransferKind {
    match kind {
        ConnectionKind::Dust
        | ConnectionKind::DustRise
        | ConnectionKind::DustFallThroughConductor => TransferKind::DustPropagation,
        ConnectionKind::WeakPower => TransferKind::WeakPower,
        ConnectionKind::StrongPower => TransferKind::StrongPower,
        ConnectionKind::DirectionalInput | ConnectionKind::DirectionalOutput => {
            TransferKind::DirectionalDevice
        }
        ConnectionKind::DirectSource => TransferKind::DirectSignal,
        ConnectionKind::Control => TransferKind::SideControl,
        ConnectionKind::ObserverInput => TransferKind::Observation,
        ConnectionKind::ObserverOutput => TransferKind::StrongPower,
        ConnectionKind::Support => TransferKind::StructuralSupport,
    }
}

fn facing_between(source: Pos, sink: Pos) -> Option<Facing> {
    match (sink.x - source.x, sink.y - source.y, sink.z - source.z) {
        (1, 0, 0) => Some(Facing::East),
        (1, 1 | -1, 0) => Some(Facing::East),
        (-1, 0, 0) => Some(Facing::West),
        (-1, 1 | -1, 0) => Some(Facing::West),
        (0, 1, 0) => Some(Facing::Up),
        (0, -1, 0) => Some(Facing::Down),
        (0, 0, 1) => Some(Facing::South),
        (0, 1 | -1, 1) => Some(Facing::South),
        (0, 0, -1) => Some(Facing::North),
        (0, 1 | -1, -1) => Some(Facing::North),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PhysicalComponent, PhysicalConnection};

    #[test]
    fn world_support_is_valid_even_when_support_is_not_a_signal_component() {
        let mut wire = Block::new(BlockKind::RedstoneWire);
        wire.support_offset = Some(Pos::new(0, -1, 0));
        let topology = VerifiedTopology::from_parts(
            vec![PhysicalComponent {
                id: ComponentId(0),
                pos: Pos::new(0, 1, 0),
                block: wire.clone(),
            }],
            [],
        );
        let mut world = crate::World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Transparent));
        world.set(Pos::new(0, 1, 0), wire);
        let scene = PhysicalScene::from_topology_and_world(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(Pos::new(0, 0, 0), Pos::new(0, 1, 0)),
            ),
            &topology,
            &world,
        );
        assert_eq!(scene.components.len(), 1);
        assert_eq!(
            scene.components[0].support.map(|support| support.valid),
            Some(true)
        );
        assert!(scene.diagnostics.is_empty());
    }

    #[test]
    fn repeater_ports_preserve_direction_and_physical_origin() {
        let wire = PhysicalComponent {
            id: ComponentId(0),
            pos: Pos::new(0, 1, 0),
            block: Block::new(BlockKind::RedstoneWire),
        };
        let mut repeater_block = Block::new(BlockKind::Repeater);
        repeater_block.facing = Some(Facing::East);
        let repeater = PhysicalComponent {
            id: ComponentId(1),
            pos: Pos::new(1, 1, 0),
            block: repeater_block,
        };
        let topology = VerifiedTopology::from_parts(
            vec![wire, repeater],
            [PhysicalConnection {
                source: ComponentId(0),
                sink: ComponentId(1),
                kind: ConnectionKind::DirectionalInput,
            }],
        );
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(Pos::new(0, 0, 0), Pos::new(2, 2, 0)),
            ),
            &topology,
        );
        assert_eq!(scene.connections.len(), 1);
        let repeater = scene.component_at(Pos::new(1, 1, 0)).unwrap();
        assert!(
            repeater
                .ports
                .iter()
                .any(|port| port.role == PortRole::Input && port.face == Facing::West)
        );
        assert!(
            repeater
                .ports
                .iter()
                .any(|port| port.role == PortRole::Output && port.face == Facing::East)
        );
    }

    #[test]
    fn observer_ports_keep_front_observation_separate_from_back_power() {
        let mut observer_block = Block::new(BlockKind::Observer);
        observer_block.facing = Some(Facing::East);
        let topology = VerifiedTopology::from_parts(
            vec![
                PhysicalComponent {
                    id: ComponentId(0),
                    pos: Pos::new(0, 1, 0),
                    block: Block::new(BlockKind::Solid),
                },
                PhysicalComponent {
                    id: ComponentId(1),
                    pos: Pos::new(1, 1, 0),
                    block: observer_block,
                },
                PhysicalComponent {
                    id: ComponentId(2),
                    pos: Pos::new(2, 1, 0),
                    block: Block::new(BlockKind::RedstoneWire),
                },
            ],
            [
                PhysicalConnection {
                    source: ComponentId(0),
                    sink: ComponentId(1),
                    kind: ConnectionKind::ObserverInput,
                },
                PhysicalConnection {
                    source: ComponentId(1),
                    sink: ComponentId(2),
                    kind: ConnectionKind::ObserverOutput,
                },
            ],
        );
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(Pos::new(0, 0, 0), Pos::new(2, 2, 0)),
            ),
            &topology,
        );
        assert_eq!(scene.connections.len(), 2);
        assert!(
            scene
                .connections
                .iter()
                .any(|connection| connection.transfer == TransferKind::Observation)
        );
        assert!(
            scene
                .connections
                .iter()
                .any(|connection| connection.transfer == TransferKind::StrongPower)
        );
        let observer = scene.component_at(Pos::new(1, 1, 0)).unwrap();
        assert!(observer.ports.iter().any(|port| {
            port.role == PortRole::Input
                && port.face == Facing::West
                && port.channel == PortChannel::Observation
        }));
        assert!(observer.ports.iter().any(|port| {
            port.role == PortRole::Output
                && port.face == Facing::East
                && port.channel == PortChannel::StrongPower
        }));
    }

    #[test]
    fn floor_button_and_pressure_plate_keep_their_downward_power_port() {
        for kind in [BlockKind::Button, BlockKind::PressurePlate] {
            let mut block = Block::new(kind);
            block.support_offset = Some(Pos::new(0, -1, 0));
            let topology = VerifiedTopology::from_parts(
                vec![
                    PhysicalComponent {
                        id: ComponentId(0),
                        pos: Pos::new(0, 1, 0),
                        block,
                    },
                    PhysicalComponent {
                        id: ComponentId(1),
                        pos: Pos::new(0, 0, 0),
                        block: Block::new(BlockKind::Solid),
                    },
                ],
                [PhysicalConnection {
                    source: ComponentId(0),
                    sink: ComponentId(1),
                    kind: ConnectionKind::DirectSource,
                }],
            );
            let scene = PhysicalScene::from_unvalidated_topology(
                Observation::complete(
                    "minecraft:overworld",
                    SceneBounds::new(Pos::new(0, 0, 0), Pos::new(0, 1, 0)),
                ),
                &topology,
            );
            assert_eq!(scene.connections.len(), 1);
            let source = scene.component_at(Pos::new(0, 1, 0)).unwrap();
            let downward = source
                .ports
                .iter()
                .find(|port| port.face == Facing::Down)
                .expect("floor input has a downward source port");
            assert_eq!(scene.connections[0].source.port, downward.id);
            assert_eq!(scene.connections[0].transfer, TransferKind::DirectSignal);
        }
    }

    #[test]
    fn vertical_dust_connection_preserves_its_minecraft_rule() {
        let topology = VerifiedTopology::from_parts(
            vec![
                PhysicalComponent {
                    id: ComponentId(0),
                    pos: Pos::new(0, 1, 0),
                    block: Block::new(BlockKind::RedstoneWire),
                },
                PhysicalComponent {
                    id: ComponentId(1),
                    pos: Pos::new(1, 2, 0),
                    block: Block::new(BlockKind::RedstoneWire),
                },
            ],
            [PhysicalConnection {
                source: ComponentId(0),
                sink: ComponentId(1),
                kind: ConnectionKind::DustRise,
            }],
        );
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(Pos::new(0, 1, 0), Pos::new(1, 2, 0)),
            ),
            &topology,
        );
        assert_eq!(scene.connections[0].confidence, Confidence::Certain);
        assert!(scene.connections[0].evidence.iter().any(|evidence| {
            matches!(
                evidence,
                PhysicalEvidence::MinecraftRule { rule }
                    if rule == "java.redstone.dust_connection.vertical_rise"
            )
        }));
    }

    #[test]
    fn open_boundary_is_not_reported_as_a_complete_scene() {
        let frontier = ObservationFrontier {
            position: Pos::new(0, 1, 0),
            direction: Facing::West,
            reason: FrontierReason::ScanLimitReached,
        };
        let observation = Observation {
            dimension: "minecraft:overworld".to_owned(),
            regions: vec![ObservedRegion {
                bounds: SceneBounds::new(Pos::new(0, 0, 0), Pos::new(2, 2, 2)),
                completeness: RegionCompleteness::OpenBoundary,
            }],
            frontier: vec![frontier],
        };
        let scene =
            PhysicalScene::from_unvalidated_topology(observation, &VerifiedTopology::default());
        assert!(!scene.observation.is_complete());
        assert!(matches!(
            scene.diagnostics[0],
            PhysicalDiagnostic::OpenObservationBoundary { .. }
        ));
    }
}
