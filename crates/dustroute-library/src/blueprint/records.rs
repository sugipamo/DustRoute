//! Immutable source records and public identifiers. No catalog mutation.
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, RotationY, World};
use serde::{Deserialize, Serialize};

use crate::{ComponentId, LogicalSpec, PortDirection, Provenance};

macro_rules! identifier {
    ($name:ident) => {
        #[derive(
            Clone,
            Debug,
            Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
            schemars::JsonSchema,
        )]
        #[serde(transparent)]
        pub struct $name(ComponentId);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
                ComponentId::new(value).map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                Display::fmt(&self.0, f)
            }
        }
    };
}

identifier!(TypeRevisionId);
identifier!(ClassificationRevisionId);
identifier!(BlueprintRevisionId);
identifier!(AssemblyRevisionId);
identifier!(BlueprintUpdateId);
identifier!(InstanceId);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct PositionedBlock {
    pub position: Pos,
    /// Air is an explicit empty-space requirement, not an unspecified cell.
    pub block: Block,
}

/// Explicit requirements. Connection variants keep their original local meaning;
/// behavioral variants need execution evidence and cannot pass a snapshot check.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TypeContract {
    /// The physical interface required of the selected producer output.
    Signal { port_kind: BlueprintPortKind },
    /// Exact states relative to the bound terminal, not to its blueprint origin.
    /// Unspecified positions are unconstrained; absence is not evidence of air.
    BlockPattern { blocks: Vec<PositionedBlock> },
    /// Physical block identity at the bound terminal, independent of its
    /// current powered state. Does not assert a behavioral function.
    BlockKind { block_kind: BlockKind },
    /// Observable whole-realization behavior, not a classification-name lookup.
    RepeatedSettling {
        relation: crate::behavior_type::RepeatedSettling,
    },
    /// A fixed 3x3 aperture operated only between completed operations.
    PistonDoor {
        requirement: Box<crate::behavior_type::PistonDoor>,
    },
    /// A single false-to-true command from a completed initial state.
    SingleOperation {
        requirement: crate::behavior_type::SingleOperation,
    },
    /// Autonomous nonconstant recurrence after a finite startup.
    Periodic {
        requirement: crate::behavior_type::Periodic,
    },
    /// At least two falling edges followed by eventual permanent OFF.
    FiniteBurst {
        requirement: crate::behavior_type::FiniteBurst,
    },
}

/// An interpretation label, independent of connection types. Optional logical
/// claims support existing candidate discovery; they are never type evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ClassificationRevision {
    pub id: ClassificationRevisionId,
    pub name: String,
    pub logical_claim: Option<LogicalSpec>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct TypeRevision {
    pub id: TypeRevisionId,
    pub name: String,
    pub contract: TypeContract,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlueprintPortKind {
    Wire,
    BlockPower,
    /// Direct output of a supported signal-producing device, never dust.
    /// Output-only; actual propagation direction still requires route checks.
    DeviceOutput,
    BlockState,
}

impl BlueprintPortKind {
    /// Physical terminal presence only, not route or behavior evidence.
    pub fn matches_signal_block(self, block: &Block) -> bool {
        match self {
            Self::Wire => block.kind == BlockKind::RedstoneWire,
            Self::BlockPower => {
                let traits = block.redstone_traits();
                traits.conducts_weak_power || traits.conducts_strong_power
            }
            Self::DeviceOutput => matches!(
                block.kind,
                BlockKind::RedstoneTorch | BlockKind::Lever | BlockKind::RedstoneBlock
            ),
            Self::BlockState => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct BlueprintPort {
    pub name: String,
    pub direction: PortDirection,
    pub position: Pos,
    pub kind: BlueprintPortKind,
    pub facing: Option<Facing>,
    /// All requirements apply to the selected upstream output and its physical
    /// surroundings, never to the producer's classification or other ports.
    pub required_source_types: Vec<TypeRevisionId>,
}

/// An explicit obligation on this occurrence's actual terminal and surroundings,
/// independent of whether a consumer connects to it. Pattern offsets use the
/// terminal's local frame. Behavioral relations use BehaviorBinding instead.
#[derive(
    Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct StaticTypeBinding {
    pub type_revision: TypeRevisionId,
    pub port: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct BlueprintInclusion {
    pub instance: InstanceId,
    pub revision: BlueprintRevisionId,
    /// Rotation followed by translation in the parent's coordinate system.
    pub origin: Pos,
    #[serde(default, skip_serializing_if = "RotationY::is_identity")]
    pub rotation: RotationY,
}

/// A local occurrence path and one of its named physical interfaces. The empty
/// path denotes the source root; an assembly uses its top-level instance paths.
#[derive(
    Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema,
)]
pub struct BlueprintPortRef {
    pub instance: InstancePath,
    pub port: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct BlueprintConnection {
    pub source: BlueprintPortRef,
    pub sink: BlueprintPortRef,
    /// Directed proposed/actual route, including both physical terminals.
    pub path: Vec<Pos>,
}

/// Exposes a descendant's physical terminal under a parent port name. This is
/// an alias at the same position, direction, kind and facing, not a wire route.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct BlueprintPortBinding {
    pub name: String,
    pub port: BlueprintPortRef,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct BlueprintLayout {
    /// Complete captured geometry, independent of descendants' source defaults.
    pub blocks: Vec<PositionedBlock>,
    pub known_regions: Vec<Region>,
}

#[derive(
    Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema,
)]
#[serde(untagged, deny_unknown_fields)]
pub enum BehaviorBinding {
    /// The original single-output form for Periodic and FiniteBurst.
    Autonomous {
        behavior_type: TypeRevisionId,
        output_port: String,
    },
    /// Type port names mapped to this realization's physical port names.
    /// Actual input drivers belong to the execution context, not this source.
    RepeatedSettling {
        behavior_type: TypeRevisionId,
        inputs: BTreeMap<String, String>,
        outputs: BTreeMap<String, String>,
    },
    /// Explicit observation of each named terminal, including fixed locations.
    /// The existing Boolean behavior type keeps its meaning; the interpretation
    /// says which physical observation supplies each Boolean value.
    Observed {
        behavior_type: TypeRevisionId,
        observed_inputs: BTreeMap<String, crate::location_observation::ObservedPort>,
        observed_outputs: BTreeMap<String, crate::location_observation::ObservedPort>,
    },
}

impl BehaviorBinding {
    pub fn behavior_type(&self) -> &TypeRevisionId {
        match self {
            Self::Autonomous { behavior_type, .. }
            | Self::RepeatedSettling { behavior_type, .. }
            | Self::Observed { behavior_type, .. } => behavior_type,
        }
    }

    pub fn covers_output(&self, name: &str) -> bool {
        match self {
            Self::Autonomous { output_port, .. } => output_port == name,
            Self::RepeatedSettling { outputs, .. } => outputs.values().any(|port| port == name),
            Self::Observed {
                observed_outputs, ..
            } => observed_outputs
                .values()
                .any(|observation| observation.port() == name),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct BlueprintRevision {
    pub id: BlueprintRevisionId,
    /// Historical ancestry, independently of the physical containment graph.
    pub parents: Vec<BlueprintRevisionId>,
    pub name: String,
    /// Claims to be checked against this exact realization. Empty is allowed.
    pub classifications: Vec<ClassificationRevisionId>,
    /// Rechecked independently for every occurrence in its actual environment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behavior_bindings: Vec<BehaviorBinding>,
    /// Snapshot type obligations on this occurrence's actual named terminals,
    /// checked independently of whether any consumer is connected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub static_type_bindings: Vec<StaticTypeBinding>,
    /// Immutable laws required of the physical world or world simulator.
    /// Declaring a dependency never instantiates another physical execution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_laws: Vec<BlueprintRevisionId>,
    pub blocks: Vec<PositionedBlock>,
    /// When present, expansion uses this captured state instead of composing
    /// child geometry. Children remain immutable interpretations of that state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_layout: Option<BlueprintLayout>,
    /// Executable local physical law, independently of geometric interpretation.
    /// Loading checks its program structure, not Minecraft conformance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub law: Option<dustroute_minecraft::law::LawProgram>,
    pub inclusions: Vec<BlueprintInclusion>,
    pub ports: Vec<BlueprintPort>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<BlueprintConnection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub port_bindings: Vec<BlueprintPortBinding>,
    pub provenance: Provenance,
}

pub type InstancePath = Vec<InstanceId>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlueprintOccurrence {
    pub path: InstancePath,
    pub revision: BlueprintRevisionId,
    pub origin: Pos,
    pub rotation: RotationY,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExpandedBlueprint {
    /// Includes explicit air requirements; unlike World this does not drop air.
    pub blocks: BTreeMap<Pos, Block>,
    /// All containing occurrences, including ancestors, that claim each cell.
    /// The empty path denotes the root realization.
    pub membership: BTreeMap<Pos, BTreeSet<InstancePath>>,
    /// Each occurrence's own expanded source, before ancestor context applies.
    pub source_claims: BTreeMap<Pos, BTreeMap<InstancePath, Block>>,
    pub occurrences: Vec<BlueprintOccurrence>,
}

impl ExpandedBlueprint {
    /// Returns mutable proposal data, never a ValidatedWorld.
    #[must_use]
    pub fn proposed_world(&self) -> World {
        let mut world = World::new();
        for (position, block) in &self.blocks {
            world.set(*position, block.clone());
        }
        world
    }

    /// Returns interpretations touching the supplied positions. The caller must
    /// include the physical influence region, not just directly written cells.
    #[must_use]
    pub fn affected_occurrences(&self, positions: &BTreeSet<Pos>) -> BTreeSet<InstancePath> {
        positions
            .iter()
            .filter_map(|position| self.membership.get(position))
            .flat_map(|paths| paths.iter().cloned())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpansionLimits {
    pub maximum_depth: usize,
    pub maximum_occurrences: usize,
    pub maximum_block_claims: usize,
}

impl Default for ExpansionLimits {
    fn default() -> Self {
        Self {
            maximum_depth: 32,
            maximum_occurrences: 4096,
            maximum_block_claims: 65_536,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlueprintError {
    Invalid(String),
    UnknownType(TypeRevisionId),
    UnknownClassification(ClassificationRevisionId),
    UnknownRevision(BlueprintRevisionId),
    DuplicateType(TypeRevisionId),
    DuplicateClassification(ClassificationRevisionId),
    DuplicateRevision(BlueprintRevisionId),
    UnknownAssembly(AssemblyRevisionId),
    DuplicateAssembly(AssemblyRevisionId),
    AssemblyAncestryCycle(AssemblyRevisionId),
    ContainmentCycle(BlueprintRevisionId),
    AncestryCycle(BlueprintRevisionId),
    CoordinateOverflow,
    ConflictingBlocks {
        position: Pos,
        occurrence: InstancePath,
    },
    ExpansionLimit(&'static str),
    UnsatisfiedSourceType(TypeRevisionId),
    BehavioralEvidenceRequired(TypeRevisionId),
}

impl Display for BlueprintError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for BlueprintError {}
