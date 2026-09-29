//! Versioned archive encoding and validated reconstruction. Saved claims are not proof.
use super::validation::invalid;
use super::{
    BehaviorBinding, BlueprintCatalog, BlueprintError, BlueprintPortKind, BlueprintRevision,
    ClassificationRevision, TypeContract, TypeRevision,
};
use serde::{Deserialize, Serialize};

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
    pub fn to_json(&self) -> Result<String, BlueprintError> {
        serde_json::to_string_pretty(&Archive {
            schema: if self
                .types
                .values()
                .any(|t| matches!(t.contract, TypeContract::SingleOperation { .. }))
            {
                "dustroute.blueprint-catalog.v12"
            } else if self
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
                | "dustroute.blueprint-catalog.v12"
        ) {
            return invalid("unsupported blueprint archive schema");
        }
        if archive.schema != "dustroute.blueprint-catalog.v12"
            && archive
                .types
                .iter()
                .any(|t| matches!(t.contract, TypeContract::SingleOperation { .. }))
        {
            return invalid("single-operation types require blueprint archive v12");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v11" | "dustroute.blueprint-catalog.v12"
        ) && archive
            .types
            .iter()
            .any(|t| matches!(t.contract, TypeContract::PistonDoor { .. }))
        {
            return invalid("completed-operation piston-door types require blueprint archive v11");
        }
        if !matches!(
            archive.schema.as_str(),
            "dustroute.blueprint-catalog.v10"
                | "dustroute.blueprint-catalog.v11"
                | "dustroute.blueprint-catalog.v12"
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
                | "dustroute.blueprint-catalog.v12"
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
                | "dustroute.blueprint-catalog.v12"
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
                | "dustroute.blueprint-catalog.v12"
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
                | "dustroute.blueprint-catalog.v12"
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
                | "dustroute.blueprint-catalog.v12"
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
                | "dustroute.blueprint-catalog.v12"
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
                | "dustroute.blueprint-catalog.v12"
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
