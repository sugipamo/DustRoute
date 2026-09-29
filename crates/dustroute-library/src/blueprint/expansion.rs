//! Bounded containment expansion and coordinate transforms.
use super::validation::invalid;
use super::{
    BlueprintCatalog, BlueprintError, BlueprintOccurrence, BlueprintPort, BlueprintPortRef,
    BlueprintRevisionId, ExpandedBlueprint, ExpansionLimits, InstancePath,
};
use dustroute_minecraft::{Block, Pos, RotationY};
use std::collections::{BTreeMap, BTreeSet};

impl BlueprintCatalog {
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
