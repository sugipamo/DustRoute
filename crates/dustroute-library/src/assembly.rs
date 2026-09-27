//! Placed state and connections, independent of immutable source definitions.
//!
//! Source membership expresses interpretations, not equality to source blocks.
//! A single physical state is authoritative even when interpretations overlap.

use std::collections::{BTreeMap, BTreeSet};

use dustroute_minecraft::{Block, BlockKind, Pos, Region, World};
use serde::{Deserialize, Serialize};

use crate::blueprint::{
    AssemblyRevisionId, BlueprintCatalog, BlueprintError, BlueprintInclusion, BlueprintLayout,
    BlueprintOccurrence, BlueprintPort, BlueprintRevision, BlueprintRevisionId,
    ClassificationRevisionId, ExpansionLimits, InstanceId, InstancePath, PositionedBlock, invalid,
    rotated_block, transformed, unique, validate_blocks,
};
use crate::{PortDirection, Provenance};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct AssemblyRevision {
    pub id: AssemblyRevisionId,
    pub parents: Vec<AssemblyRevisionId>,
    pub assembly: Assembly,
}

/// Serializable data, not proof of physical validity or observation in Minecraft.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Assembly {
    pub name: String,
    /// Placed source occurrences. Their pinned nested inclusions are retained.
    pub instances: Vec<BlueprintInclusion>,
    /// Actual state, including unclassified routing and optional explicit air.
    pub blocks: Vec<PositionedBlock>,
    /// Only missing cells inside these regions are known air. Elsewhere a missing
    /// record is unknown. Regions do not claim live observation or completeness
    /// of a circuit's behavior.
    pub known_regions: Vec<Region>,
    pub connections: Vec<AssemblyConnection>,
    pub boundaries: Vec<AssemblyBoundary>,
}

pub use crate::blueprint::{
    BlueprintConnection as AssemblyConnection, BlueprintPortBinding as AssemblyBoundary,
    BlueprintPortRef as AssemblyPortRef,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceStateDifference {
    pub instance: InstancePath,
    pub position: Pos,
    pub source: Block,
    /// None means unknown, not air. Known air is an explicit BlockKind::Air.
    pub actual: Option<Block>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssemblyChangeImpact {
    pub changed_positions: BTreeSet<Pos>,
    /// Union of the old and new interpretations. This is invalidation data, not
    /// a claim that unaffected physics can be skipped during final validation.
    pub affected_occurrences: BTreeSet<InstancePath>,
}

pub struct BlueprintGrouping {
    pub id: BlueprintRevisionId,
    pub name: String,
    pub classifications: Vec<ClassificationRevisionId>,
    pub provenance: Provenance,
}

/// A checked index of records, not a physical validation certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssemblyView {
    blocks: BTreeMap<Pos, Block>,
    known_regions: Vec<Region>,
    pub occurrences: BTreeMap<InstancePath, BlueprintOccurrence>,
    pub membership: BTreeMap<Pos, BTreeSet<InstancePath>>,
    /// Multiple source states may describe the same cell. They never overwrite
    /// its actual state, and disagreements remain available for revalidation.
    pub source_claims: BTreeMap<Pos, BTreeMap<InstancePath, Block>>,
    interfaces: crate::interfaces::InterfaceIndex,
}

impl Assembly {
    /// Materializes inherited route proposals as explicit assembly data. An
    /// already supplied actual route wins for its terminal pair. Conflicting
    /// source proposals need an explicit actual route; iteration order never
    /// chooses one. The returned data still needs physical validation.
    pub fn with_source_connections(
        &self,
        catalog: &BlueprintCatalog,
    ) -> Result<Self, BlueprintError> {
        let view = self.inspect(catalog)?;
        let actual = self
            .connections
            .iter()
            .map(|edge| view.connection_key(edge))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let mut proposals: BTreeMap<_, AssemblyConnection> = BTreeMap::new();
        for edge in view.source_connections() {
            let key = view.connection_key(edge)?;
            if actual.contains(&key) {
                continue;
            }
            if let Some(previous) = proposals.get(&key) {
                if previous.path != edge.path {
                    return invalid("conflicting source routes need an explicit actual connection");
                }
            } else {
                proposals.insert(key, edge.clone());
            }
        }
        let mut result = self.clone();
        result.connections.extend(proposals.into_values());
        result.inspect(catalog)?;
        Ok(result)
    }

    /// Prepares a source candidate with a captured initial layout and pinned
    /// children. This checks structure only: it does not validate or adopt a
    /// promotion, and never mutates the supplied catalog.
    pub fn group_as_blueprint(
        &self,
        catalog: &BlueprintCatalog,
        grouping: BlueprintGrouping,
    ) -> Result<(BlueprintRevision, Assembly), BlueprintError> {
        let view = self.inspect(catalog)?;
        let mut ports = Vec::new();
        for boundary in &self.boundaries {
            let (mut port, _) = view.resolved_port(&boundary.port)?;
            port.name = boundary.name.clone();
            ports.push(port);
        }
        let blueprint = BlueprintRevision {
            required_laws: vec![],
            static_type_bindings: vec![],
            behavior_bindings: vec![],
            id: grouping.id,
            parents: vec![],
            name: grouping.name,
            classifications: grouping.classifications,
            blocks: vec![],
            law: None,
            initial_layout: Some(BlueprintLayout {
                blocks: self.blocks.clone(),
                known_regions: self.known_regions.clone(),
            }),
            inclusions: self.instances.clone(),
            ports,
            connections: self.connections.clone(),
            port_bindings: self.boundaries.clone(),
            provenance: grouping.provenance,
        };
        let root = InstanceId::new("root").expect("valid instance ID");
        let mut grouped = self.clone();
        grouped.instances = vec![BlueprintInclusion {
            instance: root.clone(),
            revision: blueprint.id.clone(),
            origin: Pos::default(),
            rotation: Default::default(),
        }];
        let prefix = |reference: &mut AssemblyPortRef| reference.instance.insert(0, root.clone());
        for edge in &mut grouped.connections {
            prefix(&mut edge.source);
            prefix(&mut edge.sink);
        }
        for boundary in &mut grouped.boundaries {
            prefix(&mut boundary.port);
        }
        // Check the whole prospective pair without partially publishing it.
        let mut proposed = catalog.clone();
        proposed.insert_revision(blueprint.clone())?;
        grouped.inspect(&proposed)?;
        Ok((blueprint, grouped))
    }

    pub fn inspect(&self, catalog: &BlueprintCatalog) -> Result<AssemblyView, BlueprintError> {
        let limits = ExpansionLimits::default();
        if self.name.trim().is_empty() {
            return invalid("assembly name is empty");
        }
        if self.blocks.len() > limits.maximum_block_claims
            || self.known_regions.len() > limits.maximum_occurrences
            || self
                .connections
                .iter()
                .try_fold(0_usize, |sum, edge| sum.checked_add(edge.path.len()))
                .is_none_or(|sum| sum > limits.maximum_block_claims)
        {
            return Err(BlueprintError::ExpansionLimit("assembly records"));
        }
        validate_blocks(&self.blocks)?;
        unique(
            self.instances.iter().map(|instance| &instance.instance),
            "duplicate assembly instance",
        )?;
        unique(
            self.boundaries.iter().map(|boundary| &boundary.name),
            "duplicate assembly boundary",
        )?;
        unique(
            self.connections
                .iter()
                .map(|edge| (&edge.source, &edge.sink)),
            "duplicate assembly connection",
        )?;
        for region in &self.known_regions {
            if region.min.x > region.max.x
                || region.min.y > region.max.y
                || region.min.z > region.max.z
            {
                return invalid("invalid known region");
            }
        }
        let mut view = AssemblyView {
            blocks: self
                .blocks
                .iter()
                .map(|record| (record.position, record.block.clone()))
                .collect(),
            known_regions: self.known_regions.clone(),
            occurrences: BTreeMap::new(),
            membership: BTreeMap::new(),
            source_claims: BTreeMap::new(),
            interfaces: Default::default(),
        };
        let mut claims = 0;
        for instance in &self.instances {
            let expanded = catalog.expand(&instance.revision)?;
            if expanded.occurrences.len() > limits.maximum_occurrences - view.occurrences.len() {
                return Err(BlueprintError::ExpansionLimit("assembly occurrences"));
            }
            let prefix = |path: &InstancePath| {
                std::iter::once(instance.instance.clone())
                    .chain(path.iter().cloned())
                    .collect::<InstancePath>()
            };
            for occurrence in expanded.occurrences {
                let path = prefix(&occurrence.path);
                let origin = transformed(occurrence.origin, instance.origin, instance.rotation)?;
                let rotation = instance.rotation.then(occurrence.rotation);
                let record = catalog
                    .revision(&occurrence.revision)
                    .expect("expanded source exists");
                for port in &record.ports {
                    transformed(port.position, origin, rotation)?;
                }
                view.occurrences.insert(
                    path.clone(),
                    BlueprintOccurrence {
                        path,
                        origin,
                        rotation,
                        ..occurrence
                    },
                );
            }
            for (position, paths) in expanded.membership {
                if paths.len() > limits.maximum_block_claims - claims {
                    return Err(BlueprintError::ExpansionLimit("assembly source claims"));
                }
                claims += paths.len();
                let position = transformed(position, instance.origin, instance.rotation)?;
                for path in paths {
                    let path = prefix(&path);
                    view.membership.entry(position).or_default().insert(path);
                }
            }
            for (position, sources) in expanded.source_claims {
                let position = transformed(position, instance.origin, instance.rotation)?;
                for (path, block) in sources {
                    view.source_claims
                        .entry(position)
                        .or_default()
                        .insert(prefix(&path), rotated_block(&block, instance.rotation)?);
                }
            }
        }
        view.interfaces = crate::interfaces::InterfaceIndex::new(catalog, &view.occurrences)?;
        for edge in &self.connections {
            let source = view.port(catalog, &edge.source)?;
            let sink = view.port(catalog, &edge.sink)?;
            if source.direction != PortDirection::Output || sink.direction != PortDirection::Input {
                return invalid("assembly connections require output to input");
            }
            if edge.path.first() != Some(&source.position)
                || edge.path.last() != Some(&sink.position)
            {
                return invalid("assembly connection endpoints do not match their selected ports");
            }
        }
        for boundary in &self.boundaries {
            if boundary.name.trim().is_empty() {
                return invalid("assembly boundary name is empty");
            }
            view.port(catalog, &boundary.port)?;
        }
        Ok(view)
    }
}

impl AssemblyView {
    /// Resolves aliases and combines every requirement on the same terminal.
    /// The rotation belongs to the physical producer, even behind parent ports.
    pub fn resolved_port(
        &self,
        reference: &AssemblyPortRef,
    ) -> Result<(BlueprintPort, dustroute_minecraft::RotationY), BlueprintError> {
        self.interfaces.resolved_port(reference)
    }

    pub fn canonical_port_ref(
        &self,
        reference: &AssemblyPortRef,
    ) -> Result<&AssemblyPortRef, BlueprintError> {
        self.interfaces.canonical(reference)
    }

    pub fn connection_key(
        &self,
        edge: &AssemblyConnection,
    ) -> Result<(AssemblyPortRef, AssemblyPortRef), BlueprintError> {
        Ok((
            self.canonical_port_ref(&edge.source)?.clone(),
            self.canonical_port_ref(&edge.sink)?.clone(),
        ))
    }

    #[must_use]
    pub fn source_connections(&self) -> &[AssemblyConnection] {
        &self.interfaces.source_connections
    }

    #[must_use]
    pub fn changes_from(&self, previous: &Self) -> AssemblyChangeImpact {
        let positions: BTreeSet<_> = self
            .blocks
            .keys()
            .chain(previous.blocks.keys())
            .chain(self.source_claims.keys())
            .chain(previous.source_claims.keys())
            .copied()
            .collect();
        let changed_positions: BTreeSet<_> = positions
            .into_iter()
            .filter(|position| self.block_at(*position) != previous.block_at(*position))
            .collect();
        let mut affected_occurrences = self.affected_occurrences(&changed_positions);
        affected_occurrences.extend(previous.affected_occurrences(&changed_positions));
        for path in self.occurrences.keys().chain(previous.occurrences.keys()) {
            if self.occurrences.get(path) != previous.occurrences.get(path)
                || self.known_regions != previous.known_regions
            {
                affected_occurrences.insert(path.clone());
            }
        }
        AssemblyChangeImpact {
            changed_positions,
            affected_occurrences,
        }
    }

    #[must_use]
    pub fn block_at(&self, position: Pos) -> Option<Block> {
        self.blocks.get(&position).cloned().or_else(|| {
            self.known_regions
                .iter()
                .any(|region| region.contains(position))
                .then(|| Block::new(BlockKind::Air))
        })
    }

    /// Reconstructs actual data, never the source expansion or a ValidatedWorld.
    #[must_use]
    pub fn proposed_world(&self) -> World {
        let mut world = World::new();
        for (position, block) in &self.blocks {
            world.set(*position, block.clone());
        }
        world
    }

    pub fn port(
        &self,
        catalog: &BlueprintCatalog,
        reference: &AssemblyPortRef,
    ) -> Result<BlueprintPort, BlueprintError> {
        let occurrence = self
            .occurrences
            .get(&reference.instance)
            .ok_or_else(|| BlueprintError::Invalid("unknown assembly instance path".into()))?;
        let revision = catalog
            .revision(&occurrence.revision)
            .ok_or_else(|| BlueprintError::UnknownRevision(occurrence.revision.clone()))?;
        let mut port = revision
            .ports
            .iter()
            .find(|port| port.name == reference.port)
            .ok_or_else(|| BlueprintError::Invalid("unknown assembly port".into()))?
            .clone();
        port.position = transformed(port.position, occurrence.origin, occurrence.rotation)?;
        port.facing = port.facing.map(|facing| occurrence.rotation.facing(facing));
        Ok(port)
    }

    #[must_use]
    pub fn source_differences(&self) -> Vec<SourceStateDifference> {
        let mut differences = Vec::new();
        for (position, claims) in &self.source_claims {
            let actual = self.block_at(*position);
            for (instance, source) in claims {
                if actual.as_ref() != Some(source) {
                    differences.push(SourceStateDifference {
                        instance: instance.clone(),
                        position: *position,
                        source: source.clone(),
                        actual: actual.clone(),
                    });
                }
            }
        }
        differences
    }

    /// Includes all shared interpretations and their ancestors. Callers supply
    /// the affected physical region, including indirect influence when needed.
    #[must_use]
    pub fn affected_occurrences(&self, positions: &BTreeSet<Pos>) -> BTreeSet<InstancePath> {
        positions
            .iter()
            .filter_map(|position| self.membership.get(position))
            .flat_map(|paths| paths.iter().cloned())
            .collect()
    }
}
