//! Immutable blueprint records and overlapping physical interpretations.
//!
//! Catalog membership is a claim, not evidence of signal compatibility. Expanded
//! blocks are proposals and still need the existing physical/placement checks.

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

/// Stored definitions can only be read or appended; IDs can never be rebound.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BlueprintCatalog {
    types: BTreeMap<TypeRevisionId, TypeRevision>,
    classifications: BTreeMap<ClassificationRevisionId, ClassificationRevision>,
    revisions: BTreeMap<BlueprintRevisionId, BlueprintRevision>,
    assemblies: BTreeMap<AssemblyRevisionId, crate::assembly::AssemblyRevision>,
}

#[derive(Deserialize, Serialize)]
struct Archive {
    schema: String,
    types: Vec<TypeRevision>,
    classifications: Vec<ClassificationRevision>,
    revisions: Vec<BlueprintRevision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    assemblies: Vec<crate::assembly::AssemblyRevision>,
}

impl BlueprintCatalog {
    /// Saves actual state separately from source blueprints. Only structural
    /// integrity is checked here; loaded state never acquires placement proof.
    pub fn insert_assembly(
        &mut self,
        revision: crate::assembly::AssemblyRevision,
    ) -> Result<(), BlueprintError> {
        let id = revision.id.clone();
        if self.assemblies.contains_key(&id) {
            return Err(BlueprintError::DuplicateAssembly(id));
        }
        self.assemblies.insert(id.clone(), revision);
        if let Err(error) = self.validate_assembly_revision(&id) {
            self.assemblies.remove(&id);
            return Err(error);
        }
        Ok(())
    }

    #[must_use]
    pub fn assembly(&self, id: &AssemblyRevisionId) -> Option<&crate::assembly::AssemblyRevision> {
        self.assemblies.get(id)
    }

    pub fn assemblies(&self) -> impl Iterator<Item = &crate::assembly::AssemblyRevision> {
        self.assemblies.values()
    }

    fn validate_assembly_revision(&self, id: &AssemblyRevisionId) -> Result<(), BlueprintError> {
        let record = self
            .assembly(id)
            .ok_or_else(|| BlueprintError::UnknownAssembly(id.clone()))?;
        unique(record.parents.iter(), "duplicate assembly parent")?;
        let mut pending = record.parents.clone();
        let mut seen = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            if &parent == id {
                return Err(BlueprintError::AssemblyAncestryCycle(id.clone()));
            }
            if seen.insert(parent.clone()) {
                let record = self
                    .assembly(&parent)
                    .ok_or(BlueprintError::UnknownAssembly(parent))?;
                pending.extend(record.parents.iter().cloned());
            }
        }
        record.assembly.inspect(self)?;
        Ok(())
    }

    pub fn insert_type(&mut self, definition: TypeRevision) -> Result<(), BlueprintError> {
        if self.types.contains_key(&definition.id) {
            return Err(BlueprintError::DuplicateType(definition.id));
        }
        if definition.name.trim().is_empty() {
            return invalid("type name is empty");
        }
        match &definition.contract {
            TypeContract::Signal {
                port_kind: BlueprintPortKind::BlockState,
            } => {
                return invalid("a block-state interface is not a redstone signal");
            }
            TypeContract::Signal { .. } | TypeContract::BlockKind { .. } => {}
            TypeContract::BlockPattern { blocks } => {
                if blocks.is_empty() {
                    return invalid("block pattern is empty");
                }
                validate_blocks(blocks)?;
            }
            TypeContract::RepeatedSettling { relation } => {
                relation
                    .validate()
                    .map_err(|message| BlueprintError::Invalid(message.into()))?;
            }
            TypeContract::PistonDoor { requirement } => {
                requirement
                    .validate()
                    .map_err(|message| BlueprintError::Invalid(message.into()))?;
            }
            TypeContract::Periodic { requirement } => {
                requirement
                    .validate()
                    .map_err(|message| BlueprintError::Invalid(message.into()))?;
            }
            TypeContract::FiniteBurst { requirement } => {
                requirement
                    .validate()
                    .map_err(|message| BlueprintError::Invalid(message.into()))?;
            }
        }
        self.types.insert(definition.id.clone(), definition);
        Ok(())
    }

    pub fn insert_classification(
        &mut self,
        definition: ClassificationRevision,
    ) -> Result<(), BlueprintError> {
        if self.classifications.contains_key(&definition.id) {
            return Err(BlueprintError::DuplicateClassification(definition.id));
        }
        if definition.name.trim().is_empty() {
            return invalid("classification name is empty");
        }
        if let Some(specification) = &definition.logical_claim {
            validate_function(specification)?;
        }
        self.classifications
            .insert(definition.id.clone(), definition);
        Ok(())
    }

    #[must_use]
    pub fn classification(&self, id: &ClassificationRevisionId) -> Option<&ClassificationRevision> {
        self.classifications.get(id)
    }

    pub fn classifications(&self) -> impl Iterator<Item = &ClassificationRevision> {
        self.classifications.values()
    }

    /// Checks source conditions without inspecting any classification or truth
    /// table. `block_at` must describe the same physical snapshot in coordinates
    /// relative to the selected output in its local orientation. Unknown
    /// positions must return None.
    /// Passing does not prove a route exists or that a realization behaves as
    /// claimed. Those are separate physical and behavioral validation stages.
    pub fn check_source_requirements(
        &self,
        source: &BlueprintPort,
        consumer: &BlueprintPort,
        block_at: impl Fn(Pos) -> Option<Block>,
    ) -> Result<(), BlueprintError> {
        if source.direction != PortDirection::Output || consumer.direction != PortDirection::Input {
            return invalid("connections require an output producer and an input consumer");
        }
        if (source.kind == BlueprintPortKind::BlockState)
            != (consumer.kind == BlueprintPortKind::BlockState)
        {
            return invalid(
                "redstone and block-state interfaces need an explicit physical adapter",
            );
        }
        self.check_port_types(source, &consumer.required_source_types, block_at)
    }

    /// Snapshot obligations at a terminal, also usable without a connection.
    /// Coordinates are relative to the port in its source frame. Unknown is
    /// never proof; callers with partial snapshots distinguish it in review.
    pub fn check_port_types(
        &self,
        source: &BlueprintPort,
        requirements: &[TypeRevisionId],
        block_at: impl Fn(Pos) -> Option<Block>,
    ) -> Result<(), BlueprintError> {
        // An interface label is not proof that its terminal physically exists.
        if source.kind != BlueprintPortKind::BlockState {
            let present = block_at(Pos::default())
                .is_some_and(|block| source.kind.matches_signal_block(&block));
            if !present {
                return invalid("producer terminal does not match its physical interface");
            }
        }
        for required in requirements {
            let definition = self
                .type_revision(required)
                .ok_or_else(|| BlueprintError::UnknownType(required.clone()))?;
            let satisfied = match &definition.contract {
                TypeContract::Signal { port_kind } => source.kind == *port_kind,
                TypeContract::BlockKind { block_kind } => {
                    block_at(Pos::default()).is_some_and(|block| block.kind == *block_kind)
                }
                TypeContract::BlockPattern { blocks } => blocks
                    .iter()
                    .all(|record| block_at(record.position).as_ref() == Some(&record.block)),
                TypeContract::RepeatedSettling { .. }
                | TypeContract::PistonDoor { .. }
                | TypeContract::Periodic { .. }
                | TypeContract::FiniteBurst { .. } => {
                    return Err(BlueprintError::BehavioralEvidenceRequired(required.clone()));
                }
            };
            if !satisfied {
                return Err(BlueprintError::UnsatisfiedSourceType(required.clone()));
            }
        }
        Ok(())
    }

    /// Inserts a concrete proposal atomically after referential/geometry checks.
    /// Classification claims and consumer requirements are not certified here.
    pub fn insert_revision(&mut self, revision: BlueprintRevision) -> Result<(), BlueprintError> {
        if self.revisions.contains_key(&revision.id) {
            return Err(BlueprintError::DuplicateRevision(revision.id));
        }
        let id = revision.id.clone();
        self.revisions.insert(id.clone(), revision);
        if let Err(error) = self.validate_revision(&id) {
            self.revisions.remove(&id);
            return Err(error);
        }
        Ok(())
    }

    /// Appends a set of mutually referring proposals atomically, independent
    /// of input order. This is structural validation, not promotion evidence.
    pub fn insert_revisions(
        &mut self,
        revisions: Vec<BlueprintRevision>,
    ) -> Result<(), BlueprintError> {
        let mut proposed = self.clone();
        let mut ids = Vec::new();
        for revision in revisions {
            let id = revision.id.clone();
            if proposed.revisions.contains_key(&id) {
                return Err(BlueprintError::DuplicateRevision(id));
            }
            ids.push(id.clone());
            proposed.revisions.insert(id, revision);
        }
        for id in ids {
            proposed.validate_revision(&id)?;
        }
        *self = proposed;
        Ok(())
    }

    #[must_use]
    pub fn type_revision(&self, id: &TypeRevisionId) -> Option<&TypeRevision> {
        self.types.get(id)
    }

    #[must_use]
    pub fn revision(&self, id: &BlueprintRevisionId) -> Option<&BlueprintRevision> {
        self.revisions.get(id)
    }

    pub fn revisions(&self) -> impl Iterator<Item = &BlueprintRevision> {
        self.revisions.values()
    }

    pub fn type_revisions(&self) -> impl Iterator<Item = &TypeRevision> {
        self.types.values()
    }

    #[must_use]
    pub fn candidates(&self, classification: &ClassificationRevisionId) -> Vec<&BlueprintRevision> {
        self.revisions
            .values()
            .filter(|revision| revision.classifications.contains(classification))
            .collect()
    }

    pub fn expand(&self, id: &BlueprintRevisionId) -> Result<ExpandedBlueprint, BlueprintError> {
        self.expand_with_limits(id, ExpansionLimits::default())
    }

    /// Resolves transparent parent/child aliases without changing the physical
    /// producer's local type frame. All aliased consumer requirements apply.
    pub fn resolve_port(
        &self,
        id: &BlueprintRevisionId,
        reference: &BlueprintPortRef,
    ) -> Result<(BlueprintPort, RotationY), BlueprintError> {
        let occurrences = self
            .expand(id)?
            .occurrences
            .into_iter()
            .map(|occurrence| (occurrence.path.clone(), occurrence))
            .collect();
        crate::interfaces::InterfaceIndex::new(self, &occurrences)?.resolved_port(reference)
    }

    pub fn expand_with_limits(
        &self,
        id: &BlueprintRevisionId,
        limits: ExpansionLimits,
    ) -> Result<ExpandedBlueprint, BlueprintError> {
        let mut expansion = Expansion {
            catalog: self,
            limits,
            result: ExpandedBlueprint::default(),
            active: BTreeSet::new(),
            block_claims: 0,
        };
        expansion.result.blocks =
            expansion.visit(id, Pos::default(), RotationY::R0, &mut Vec::new())?;
        Ok(expansion.result)
    }

    pub fn to_json(&self) -> Result<String, BlueprintError> {
        serde_json::to_string_pretty(&Archive {
            schema: if self
                .types
                .values()
                .any(|t| matches!(t.contract, TypeContract::PistonDoor { .. }))
            {
                "dustroute.blueprint-catalog.v11"
            } else if self.revisions.values().any(|r| {
                r.behavior_bindings
                    .iter()
                    .any(|b| matches!(b, BehaviorBinding::Observed { .. }))
            }) {
                "dustroute.blueprint-catalog.v10"
            } else if self
                .revisions
                .values()
                .any(|revision| !revision.required_laws.is_empty())
            {
                "dustroute.blueprint-catalog.v9"
            } else if self
                .revisions
                .values()
                .any(|revision| !revision.static_type_bindings.is_empty())
            {
                "dustroute.blueprint-catalog.v8"
            } else if needs_device_schema(self.types.values(), self.revisions.values()) {
                "dustroute.blueprint-catalog.v7"
            } else if self.revisions.values().any(|revision| {
                revision
                    .behavior_bindings
                    .iter()
                    .any(|binding| matches!(binding, BehaviorBinding::RepeatedSettling { .. }))
            }) {
                "dustroute.blueprint-catalog.v6"
            } else if self
                .types
                .values()
                .any(|definition| matches!(definition.contract, TypeContract::FiniteBurst { .. }))
            {
                "dustroute.blueprint-catalog.v5"
            } else if self
                .types
                .values()
                .any(|definition| matches!(definition.contract, TypeContract::Periodic { .. }))
            {
                "dustroute.blueprint-catalog.v4"
            } else if self
                .revisions
                .values()
                .any(|revision| revision.law.is_some())
                || self.types.values().any(|definition| {
                    matches!(definition.contract, TypeContract::RepeatedSettling { .. })
                })
            {
                "dustroute.blueprint-catalog.v3"
            } else if self
                .revisions
                .values()
                .any(|revision| revision.initial_layout.is_some())
            {
                "dustroute.blueprint-catalog.v2"
            } else {
                "dustroute.blueprint-catalog.v1"
            }
            .to_owned(),
            types: self.types.values().cloned().collect(),
            classifications: self.classifications.values().cloned().collect(),
            revisions: self.revisions.values().cloned().collect(),
            assemblies: self.assemblies.values().cloned().collect(),
        })
        .map_err(|error| BlueprintError::Invalid(error.to_string()))
    }

    /// Archives are self-contained and may list children after parents. No
    /// filesystem paths, function names or network references are executed.
    pub fn from_json(input: &str) -> Result<Self, BlueprintError> {
        let archive: Archive = serde_json::from_str(input)
            .map_err(|error| BlueprintError::Invalid(error.to_string()))?;
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v1"
                | "dustroute.blueprint-catalog.v2"
                | "dustroute.blueprint-catalog.v3"
                | "dustroute.blueprint-catalog.v4"
                | "dustroute.blueprint-catalog.v5"
                | "dustroute.blueprint-catalog.v6"
                | "dustroute.blueprint-catalog.v7"
                | "dustroute.blueprint-catalog.v8"
                | "dustroute.blueprint-catalog.v9"
                | "dustroute.blueprint-catalog.v10"
                | "dustroute.blueprint-catalog.v11"
        ) {
            return invalid("unsupported blueprint archive schema");
        }
        if archive.schema != "dustroute.blueprint-catalog.v11"
            && archive
                .types
                .iter()
                .any(|t| matches!(t.contract, TypeContract::PistonDoor { .. }))
        {
            return invalid("completed-operation piston-door types require blueprint archive v11");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v10" | "dustroute.blueprint-catalog.v11"
        ) && archive.revisions.iter().any(|r| {
            r.behavior_bindings
                .iter()
                .any(|b| matches!(b, BehaviorBinding::Observed { .. }))
        }) {
            return invalid("explicit observation bindings require blueprint archive v10");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v9"
                | "dustroute.blueprint-catalog.v10"
                | "dustroute.blueprint-catalog.v11"
        ) && archive
            .revisions
            .iter()
            .any(|revision| !revision.required_laws.is_empty())
        {
            return invalid("physical law requirements require blueprint archive v9");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v8"
                | "dustroute.blueprint-catalog.v9"
                | "dustroute.blueprint-catalog.v10"
                | "dustroute.blueprint-catalog.v11"
        ) && archive
            .revisions
            .iter()
            .any(|revision| !revision.static_type_bindings.is_empty())
        {
            return invalid("static type bindings require blueprint archive v8");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v7"
                | "dustroute.blueprint-catalog.v8"
                | "dustroute.blueprint-catalog.v9"
                | "dustroute.blueprint-catalog.v10"
                | "dustroute.blueprint-catalog.v11"
        ) && needs_device_schema(archive.types.iter(), archive.revisions.iter())
        {
            return invalid("device terminals and block-kind types require blueprint archive v7");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v6"
                | "dustroute.blueprint-catalog.v7"
                | "dustroute.blueprint-catalog.v8"
                | "dustroute.blueprint-catalog.v9"
                | "dustroute.blueprint-catalog.v10"
                | "dustroute.blueprint-catalog.v11"
        ) && archive.revisions.iter().any(|revision| {
            revision
                .behavior_bindings
                .iter()
                .any(|binding| matches!(binding, BehaviorBinding::RepeatedSettling { .. }))
        }) {
            return invalid("repeated-settling bindings require blueprint archive v6");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v5"
                | "dustroute.blueprint-catalog.v6"
                | "dustroute.blueprint-catalog.v7"
                | "dustroute.blueprint-catalog.v8"
                | "dustroute.blueprint-catalog.v9"
                | "dustroute.blueprint-catalog.v10"
                | "dustroute.blueprint-catalog.v11"
        ) && archive
            .types
            .iter()
            .any(|definition| matches!(definition.contract, TypeContract::FiniteBurst { .. }))
        {
            return invalid("finite-burst types require blueprint archive v5");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v4"
                | "dustroute.blueprint-catalog.v5"
                | "dustroute.blueprint-catalog.v6"
                | "dustroute.blueprint-catalog.v7"
                | "dustroute.blueprint-catalog.v8"
                | "dustroute.blueprint-catalog.v9"
                | "dustroute.blueprint-catalog.v10"
                | "dustroute.blueprint-catalog.v11"
        ) && archive
            .types
            .iter()
            .any(|definition| matches!(definition.contract, TypeContract::Periodic { .. }))
        {
            return invalid("periodic types require blueprint archive v4");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v3"
                | "dustroute.blueprint-catalog.v4"
                | "dustroute.blueprint-catalog.v5"
                | "dustroute.blueprint-catalog.v6"
                | "dustroute.blueprint-catalog.v7"
                | "dustroute.blueprint-catalog.v8"
                | "dustroute.blueprint-catalog.v9"
                | "dustroute.blueprint-catalog.v10"
                | "dustroute.blueprint-catalog.v11"
        ) && (archive
            .revisions
            .iter()
            .any(|revision| revision.law.is_some())
            || archive.types.iter().any(|definition| {
                matches!(definition.contract, TypeContract::RepeatedSettling { .. })
            }))
        {
            return invalid("behavior types and executable laws require blueprint archive v3");
        }
        if archive.schema == "dustroute.blueprint-catalog.v1"
            && archive
                .revisions
                .iter()
                .any(|revision| revision.initial_layout.is_some())
        {
            return invalid("captured initial layouts require blueprint archive v2");
        }
        let mut catalog = Self::default();
        for definition in archive.types {
            catalog.insert_type(definition)?;
        }
        for definition in archive.classifications {
            catalog.insert_classification(definition)?;
        }
        for revision in archive.revisions {
            let id = revision.id.clone();
            if catalog.revisions.insert(id.clone(), revision).is_some() {
                return Err(BlueprintError::DuplicateRevision(id));
            }
        }
        for id in catalog.revisions.keys() {
            catalog.validate_revision(id)?;
        }
        for revision in archive.assemblies {
            let id = revision.id.clone();
            if catalog.assemblies.insert(id.clone(), revision).is_some() {
                return Err(BlueprintError::DuplicateAssembly(id));
            }
        }
        for id in catalog.assemblies.keys() {
            catalog.validate_assembly_revision(id)?;
        }
        Ok(catalog)
    }

    fn validate_revision(&self, id: &BlueprintRevisionId) -> Result<(), BlueprintError> {
        let revision = self
            .revision(id)
            .ok_or_else(|| BlueprintError::UnknownRevision(id.clone()))?;
        if revision.name.trim().is_empty() {
            return invalid("blueprint name is empty");
        }
        validate_blocks(&revision.blocks)?;
        if let Some(law) = &revision.law {
            law.compile().map_err(BlueprintError::Invalid)?;
        }
        unique(
            revision.required_laws.iter(),
            "duplicate physical law requirement",
        )?;
        for law in &revision.required_laws {
            let source = self
                .revision(law)
                .ok_or_else(|| BlueprintError::UnknownRevision(law.clone()))?;
            if source.law.is_none() {
                return invalid(
                    "a required physical law must reference an executable law revision",
                );
            }
        }
        if let Some(layout) = &revision.initial_layout {
            if !revision.blocks.is_empty() {
                return invalid("captured layout and compositional blocks are mutually exclusive");
            }
            validate_blocks(&layout.blocks)?;
            if layout.known_regions.len() > ExpansionLimits::default().maximum_occurrences {
                return Err(BlueprintError::ExpansionLimit("layout regions"));
            }
            for region in &layout.known_regions {
                if region.min.x > region.max.x
                    || region.min.y > region.max.y
                    || region.min.z > region.max.z
                {
                    return invalid("invalid layout region");
                }
            }
        }
        unique(revision.parents.iter(), "duplicate revision parent")?;
        unique(revision.classifications.iter(), "duplicate classification")?;
        unique(
            revision.inclusions.iter().map(|child| &child.instance),
            "duplicate instance",
        )?;
        unique(
            revision.ports.iter().map(|port| &port.name),
            "duplicate port",
        )?;
        for classification in &revision.classifications {
            if !self.classifications.contains_key(classification) {
                return Err(BlueprintError::UnknownClassification(
                    classification.clone(),
                ));
            }
        }
        for port in &revision.ports {
            if port.name.trim().is_empty() {
                return invalid("port name is empty");
            }
            if port.direction != PortDirection::Input && !port.required_source_types.is_empty() {
                return invalid("only a consumer input may require source types");
            }
            if port.kind == BlueprintPortKind::DeviceOutput
                && port.direction != PortDirection::Output
            {
                return invalid("device-output ports are output-only");
            }
            unique(
                port.required_source_types.iter(),
                "duplicate source requirement",
            )?;
            for required in &port.required_source_types {
                self.require_type(required)?;
            }
        }
        unique(
            revision.static_type_bindings.iter(),
            "duplicate static type binding",
        )?;
        for binding in &revision.static_type_bindings {
            let definition = self
                .type_revision(&binding.type_revision)
                .ok_or_else(|| BlueprintError::UnknownType(binding.type_revision.clone()))?;
            if !matches!(
                definition.contract,
                TypeContract::Signal { .. }
                    | TypeContract::BlockKind { .. }
                    | TypeContract::BlockPattern { .. }
            ) {
                return invalid(
                    "static type bindings require snapshot contracts, not behavioral relations",
                );
            }
            if !revision.ports.iter().any(|port| port.name == binding.port) {
                return invalid("static type binding requires a named physical port");
            }
        }
        unique(
            revision.behavior_bindings.iter(),
            "duplicate behavior binding",
        )?;
        for binding in &revision.behavior_bindings {
            let definition = self
                .type_revision(binding.behavior_type())
                .ok_or_else(|| BlueprintError::UnknownType(binding.behavior_type().clone()))?;
            let valid_port = |name: &str, direction| {
                revision.ports.iter().any(|port| {
                    port.name == name
                        && port.direction == direction
                        && port.kind != BlueprintPortKind::BlockState
                })
            };
            match (binding, &definition.contract) {
                (
                    BehaviorBinding::Observed {
                        observed_inputs,
                        observed_outputs,
                        ..
                    },
                    contract,
                ) => {
                    use crate::location_observation::ObservedPort;
                    let (inputs, outputs): (Vec<&str>, Vec<&str>) = match contract {
                        TypeContract::PistonDoor { requirement } => (
                            vec![requirement.closed_input.as_str()],
                            requirement
                                .aperture
                                .iter()
                                .flatten()
                                .flat_map(|cell| [cell.air.as_str(), cell.solid.as_str()])
                                .collect(),
                        ),
                        TypeContract::RepeatedSettling { relation } => (
                            relation.inputs.iter().map(String::as_str).collect(),
                            relation.outputs.iter().map(String::as_str).collect(),
                        ),
                        TypeContract::Periodic { requirement } => {
                            (vec![], vec![requirement.output.as_str()])
                        }
                        TypeContract::FiniteBurst { requirement } => {
                            (vec![], vec![requirement.output.as_str()])
                        }
                        _ => return invalid("observed binding requires a behavioral type"),
                    };
                    if observed_inputs
                        .keys()
                        .map(String::as_str)
                        .collect::<BTreeSet<_>>()
                        != inputs.into_iter().collect()
                        || observed_outputs
                            .keys()
                            .map(String::as_str)
                            .collect::<BTreeSet<_>>()
                            != outputs.into_iter().collect()
                    {
                        return invalid(
                            "observed binding must map every named type input and output exactly",
                        );
                    }
                    unique(
                        observed_inputs.values().map(|o| o.port()),
                        "independent behavior inputs require distinct physical ports",
                    )?;
                    for (observations, direction) in [
                        (observed_inputs, PortDirection::Input),
                        (observed_outputs, PortDirection::Output),
                    ] {
                        for observation in observations.values() {
                            let Some(port) = revision
                                .ports
                                .iter()
                                .find(|p| p.name == observation.port() && p.direction == direction)
                            else {
                                return invalid(
                                    "observed binding requires a named port with matching direction",
                                );
                            };
                            match observation {
                                ObservedPort::Signal { .. }
                                    if port.kind == BlueprintPortKind::BlockState =>
                                {
                                    return invalid(
                                        "signal observation cannot reinterpret a block-state port",
                                    );
                                }
                                ObservedPort::Location { predicate, .. } => {
                                    if port.kind != BlueprintPortKind::BlockState {
                                        return invalid(
                                            "location observation requires an explicit block-state port",
                                        );
                                    }
                                    predicate
                                        .validate()
                                        .map_err(|reason| BlueprintError::Invalid(reason.into()))?;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                (
                    BehaviorBinding::Autonomous { output_port, .. },
                    TypeContract::Periodic { .. } | TypeContract::FiniteBurst { .. },
                ) => {
                    if !valid_port(output_port, PortDirection::Output) {
                        return invalid("behavior binding requires a named signal output port");
                    }
                }
                (
                    BehaviorBinding::RepeatedSettling {
                        inputs, outputs, ..
                    },
                    TypeContract::RepeatedSettling { relation },
                ) => {
                    if inputs.keys().collect::<BTreeSet<_>>()
                        != relation.inputs.iter().collect::<BTreeSet<_>>()
                        || outputs.keys().collect::<BTreeSet<_>>()
                            != relation.outputs.iter().collect::<BTreeSet<_>>()
                    {
                        return invalid(
                            "behavior binding must map every named type input and output exactly",
                        );
                    }
                    if inputs
                        .values()
                        .any(|name| !valid_port(name, PortDirection::Input))
                        || outputs
                            .values()
                            .any(|name| !valid_port(name, PortDirection::Output))
                    {
                        return invalid(
                            "behavior binding requires named signal ports with matching directions",
                        );
                    }
                    unique(
                        inputs.values(),
                        "independent behavior inputs require distinct physical ports",
                    )?;
                }
                _ => return invalid("behavior binding shape does not match its type contract"),
            }
        }
        // Ancestry and containment may share references, but their cycles mean
        // different things and are checked independently.
        let mut pending = revision.parents.clone();
        let mut seen = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            if &parent == id {
                return Err(BlueprintError::AncestryCycle(id.clone()));
            }
            if seen.insert(parent.clone()) {
                let record = self
                    .revision(&parent)
                    .ok_or_else(|| BlueprintError::UnknownRevision(parent.clone()))?;
                pending.extend(record.parents.iter().cloned());
            }
        }
        let occurrences = self
            .expand(id)?
            .occurrences
            .into_iter()
            .map(|occurrence| (occurrence.path.clone(), occurrence))
            .collect();
        crate::interfaces::InterfaceIndex::new(self, &occurrences)?;
        Ok(())
    }

    fn require_type(&self, id: &TypeRevisionId) -> Result<(), BlueprintError> {
        self.types
            .contains_key(id)
            .then_some(())
            .ok_or_else(|| BlueprintError::UnknownType(id.clone()))
    }
}

struct Expansion<'a> {
    catalog: &'a BlueprintCatalog,
    limits: ExpansionLimits,
    result: ExpandedBlueprint,
    active: BTreeSet<BlueprintRevisionId>,
    block_claims: usize,
}

impl Expansion<'_> {
    fn visit(
        &mut self,
        id: &BlueprintRevisionId,
        origin: Pos,
        rotation: RotationY,
        path: &mut InstancePath,
    ) -> Result<BTreeMap<Pos, Block>, BlueprintError> {
        if path.len() > self.limits.maximum_depth {
            return Err(BlueprintError::ExpansionLimit("depth"));
        }
        if self.result.occurrences.len() >= self.limits.maximum_occurrences {
            return Err(BlueprintError::ExpansionLimit("occurrences"));
        }
        if !self.active.insert(id.clone()) {
            return Err(BlueprintError::ContainmentCycle(id.clone()));
        }
        let revision = self
            .catalog
            .revision(id)
            .ok_or_else(|| BlueprintError::UnknownRevision(id.clone()))?;
        self.result.occurrences.push(BlueprintOccurrence {
            path: path.clone(),
            revision: id.clone(),
            origin,
            rotation,
        });
        for port in &revision.ports {
            transformed(port.position, origin, rotation)?;
        }
        let records = revision
            .initial_layout
            .as_ref()
            .map_or(&revision.blocks, |layout| &layout.blocks);
        if records.len() > self.limits.maximum_block_claims - self.block_claims {
            return Err(BlueprintError::ExpansionLimit("block claims"));
        }
        let mut blocks = BTreeMap::new();
        if let Some(layout) = &revision.initial_layout {
            for region in &layout.known_regions {
                transformed(region.min, origin, rotation)?;
                transformed(region.max, origin, rotation)?;
            }
        }
        for record in records {
            let position = transformed(record.position, origin, rotation)?;
            let block = rotated_block(&record.block, rotation)?;
            blocks.insert(position, block);
        }
        for child in &revision.inclusions {
            path.push(child.instance.clone());
            let child_blocks = self.visit(
                &child.revision,
                transformed(child.origin, origin, rotation)?,
                rotation.then(child.rotation),
                path,
            )?;
            if revision.initial_layout.is_none() {
                for (position, block) in child_blocks {
                    if blocks
                        .get(&position)
                        .is_some_and(|existing| existing != &block)
                    {
                        return Err(BlueprintError::ConflictingBlocks {
                            position,
                            occurrence: path.clone(),
                        });
                    }
                    blocks.insert(position, block);
                }
            }
            path.pop();
        }
        if blocks.len() > self.limits.maximum_block_claims - self.block_claims {
            return Err(BlueprintError::ExpansionLimit("block claims"));
        }
        self.block_claims += blocks.len();
        for (position, block) in &blocks {
            self.result
                .source_claims
                .entry(*position)
                .or_default()
                .insert(path.clone(), block.clone());
            let membership = self.result.membership.entry(*position).or_default();
            for length in 0..=path.len() {
                membership.insert(path[..length].to_vec());
            }
        }
        self.active.remove(id);
        Ok(blocks)
    }
}

pub(crate) fn translated(position: Pos, origin: Pos) -> Result<Pos, BlueprintError> {
    Ok(Pos::new(
        position
            .x
            .checked_add(origin.x)
            .ok_or(BlueprintError::CoordinateOverflow)?,
        position
            .y
            .checked_add(origin.y)
            .ok_or(BlueprintError::CoordinateOverflow)?,
        position
            .z
            .checked_add(origin.z)
            .ok_or(BlueprintError::CoordinateOverflow)?,
    ))
}

pub(crate) fn transformed(
    position: Pos,
    origin: Pos,
    rotation: RotationY,
) -> Result<Pos, BlueprintError> {
    translated(
        rotation
            .checked_pos(position)
            .ok_or(BlueprintError::CoordinateOverflow)?,
        origin,
    )
}

pub(crate) fn rotated_block(block: &Block, rotation: RotationY) -> Result<Block, BlueprintError> {
    if !rotation.is_identity() && block.piston_entity.is_some() {
        return invalid("rotating block entities is outside the blueprint scope");
    }
    rotation
        .checked_block(block)
        .ok_or(BlueprintError::CoordinateOverflow)
}

pub(crate) fn validate_blocks(blocks: &[PositionedBlock]) -> Result<(), BlueprintError> {
    unique(
        blocks.iter().map(|block| block.position),
        "duplicate local block position",
    )
}

pub(crate) fn unique<T: Ord>(
    values: impl IntoIterator<Item = T>,
    message: &str,
) -> Result<(), BlueprintError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return invalid(message);
        }
    }
    Ok(())
}

fn validate_function(spec: &LogicalSpec) -> Result<(), BlueprintError> {
    if spec.stateful {
        return invalid("BooleanFunction does not describe stateful behavior");
    }
    if spec
        .ports
        .iter()
        .any(|port| port.name.trim().is_empty() || port.bit_width != 1)
    {
        return invalid("BooleanFunction requires named single-bit ports");
    }
    unique(
        spec.ports.iter().map(|port| &port.name),
        "duplicate function port",
    )?;
    let inputs = spec.input_names().len();
    if inputs == 0 || spec.output_names().is_empty() || inputs >= usize::BITS as usize {
        return invalid("unsupported BooleanFunction arity");
    }
    if spec.truth_table.len() != 1_usize << inputs {
        return invalid("BooleanFunction needs every input assignment");
    }
    let mut assignments = BTreeSet::new();
    for row in &spec.truth_table {
        if row.len() != spec.ports.len() || !assignments.insert(&row[..inputs]) {
            return invalid("invalid or repeated BooleanFunction row");
        }
    }
    Ok(())
}

pub(crate) fn invalid<T>(message: &str) -> Result<T, BlueprintError> {
    Err(BlueprintError::Invalid(message.to_owned()))
}

fn needs_device_schema<'a>(
    types: impl Iterator<Item = &'a TypeRevision>,
    revisions: impl Iterator<Item = &'a BlueprintRevision>,
) -> bool {
    types.into_iter().any(|t| {
        matches!(
            t.contract,
            TypeContract::BlockKind { .. }
                | TypeContract::Signal {
                    port_kind: BlueprintPortKind::DeviceOutput
                }
        )
    }) || revisions.into_iter().any(|r| {
        r.ports
            .iter()
            .any(|p| p.kind == BlueprintPortKind::DeviceOutput)
    })
}
