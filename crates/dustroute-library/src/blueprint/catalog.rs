//! Append-only catalog operations and source interface checks.
use super::validation::{invalid, validate_blocks, validate_function};
use super::{
    AssemblyRevisionId, BlueprintCatalog, BlueprintError, BlueprintPort, BlueprintPortKind,
    BlueprintRevision, BlueprintRevisionId, ClassificationRevision, ClassificationRevisionId,
    TypeContract, TypeRevision, TypeRevisionId,
};
use crate::PortDirection;
use dustroute_minecraft::{Block, Pos};

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
            TypeContract::SingleOperation { requirement } => {
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
                | TypeContract::SingleOperation { .. }
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
}
