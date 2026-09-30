//! Virtual design authoring reuses immutable sources, review and construction.
use super::{BuildingVerification, component, design_geometry, sources, verify_candidate};
use crate::blueprint_update::BlueprintUpdateRequest;
use crate::snapshot::MinecraftSnapshot;
use dustroute_library::blueprint::{AssemblyRevisionId, BlueprintCatalog, BlueprintRecords};
use dustroute_library::building::BuildingDesignRequest;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_minecraft::{Pos, Region};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
pub struct BuildingDesignError {
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Pos>,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Box<crate::review_diagnostics::ReviewDiagnostics>>,
}
impl BuildingDesignError {
    pub(super) fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            item: None,
            position: None,
            detail: detail.into(),
            diagnostics: None,
        }
    }
    pub(super) fn at(mut self, name: &str, position: Option<Pos>) -> Self {
        self.item = Some(name.into());
        self.position = position;
        self
    }
}
impl From<String> for BuildingDesignError {
    fn from(detail: String) -> Self {
        Self::new("invalid_building", detail)
    }
}
impl From<&str> for BuildingDesignError {
    fn from(detail: &str) -> Self {
        Self::new("invalid_building", detail)
    }
}
impl std::fmt::Display for BuildingDesignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for BuildingDesignError {}

#[derive(Clone, Debug, Serialize)]
pub struct GeneratedBuildingDesign {
    pub specification: BuildingDesignRequest,
    pub records: BlueprintRecords,
    pub request: BlueprintUpdateRequest,
    pub context: RuntimeBehaviorContext,
    pub expected: MinecraftSnapshot,
    /// Claim counts may overlap for identical materials. unique_blocks is the
    /// physical write count, not the sum of independent part requirements.
    pub parts: BTreeMap<String, usize>,
    pub spaces: BTreeMap<String, Region>,
    pub unique_blocks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<AttachedDesignComponent>,
    pub verification: BuildingVerification,
}

#[derive(Clone, Debug, Serialize)]
pub struct AttachedDesignComponent {
    pub name: String,
    pub source_assembly_revision: AssemblyRevisionId,
    pub origin: Pos,
    pub reserved_space: Region,
    pub terminals: BTreeMap<String, Pos>,
}

/// The source, when requested, is immutable. MCP additionally checks unique
/// source adoption. An isolated source pass is never composition evidence.
pub fn generate_building_design(
    specification: BuildingDesignRequest,
    source: Option<(&BlueprintCatalog, &RuntimeBehaviorContext)>,
) -> Result<GeneratedBuildingDesign, BuildingDesignError> {
    let mut geometry = design_geometry::expand(&specification)?;
    let prepared = specification
        .component
        .as_ref()
        .map(|c| component::prepare(c, source, &specification, &geometry))
        .transpose()?;
    let context = prepared
        .as_ref()
        .map(|p| p.context.clone())
        .unwrap_or_else(|| RuntimeBehaviorContext::fresh_pistons(geometry.region, vec![]));
    if let Some(p) = &prepared {
        geometry.reserved.push(p.reserved);
    }
    let (mut records, mut request) = sources::build_layout(
        &specification.namespace,
        &specification.name,
        &geometry,
        &specification.spaces,
        &context,
    )
    .map_err(|e| BuildingDesignError::new("invalid_sources", e))?;
    let info = prepared
        .map(|p| {
            component::attach(
                &specification.namespace,
                p,
                &mut records,
                &mut request,
                &mut geometry,
            )
        })
        .transpose()?;
    let verification = verify_candidate(&records, &request, &context, &geometry.initial)?;
    Ok(GeneratedBuildingDesign {
        unique_blocks: geometry.initial.blocks.len(),
        parts: geometry
            .parts
            .into_iter()
            .map(|(n, b)| (n, b.len()))
            .collect(),
        spaces: specification
            .spaces
            .iter()
            .map(|s| (s.name.clone(), s.region))
            .collect(),
        specification,
        records,
        request,
        context,
        expected: geometry.initial,
        component: info,
        verification,
    })
}
