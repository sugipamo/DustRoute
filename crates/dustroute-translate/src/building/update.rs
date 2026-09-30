//! Structured edits are ordinary immutable proposals rooted in the real base.
use super::design::{PreparedDesign, prepare_design};
use super::{BuildingDesignError, GeneratedBuildingDesign};
use crate::blueprint_update::{BlueprintUpdateDiff, BlueprintUpdates};
use dustroute_library::assembly::Assembly;
use dustroute_library::blueprint::*;
use dustroute_library::building::BuildingDesignUpdateRequest;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
pub struct GeneratedBuildingDesignUpdate {
    pub base_assembly_revision_id: AssemblyRevisionId,
    #[serde(flatten)]
    pub design: GeneratedBuildingDesign,
    pub diff: BlueprintUpdateDiff,
    pub retained_revisions: BTreeMap<String, BlueprintRevisionId>,
    pub placed_instances_modified: bool,
}

/// The previous input must describe every base occurrence and obligation.
/// MCP also requires unique adoption and fresh review of all selected sources.
pub fn generate_building_design_update(
    specification: BuildingDesignUpdateRequest,
    catalog: &BlueprintCatalog,
    previous_source: Option<&RuntimeBehaviorContext>,
    next_source: Option<&RuntimeBehaviorContext>,
) -> Result<GeneratedBuildingDesignUpdate, BuildingDesignError> {
    if specification.previous.namespace == specification.design.namespace {
        return Err(BuildingDesignError::new(
            "immutable_namespace",
            "use a new namespace for the changed design",
        ));
    }
    let base = catalog
        .assembly(&specification.base_assembly_revision_id)
        .ok_or_else(|| {
            BuildingDesignError::new("unknown_base", "selected base Assembly is missing")
        })?;
    let mut previous_input = specification.previous;
    // Regeneration is comparison data, never a proposed update or replacement
    // for an existing ID. Use a fresh internal namespace even after many edits.
    previous_input.namespace = (0..128)
        .map(|i| format!("dustroute.comparison.n{i}"))
        .find(|prefix| {
            !catalog
                .revisions()
                .any(|r| r.id.as_str().starts_with(&format!("{prefix}.")))
        })
        .ok_or_else(|| {
            BuildingDesignError::new("comparison_budget", "comparison namespaces are exhausted")
        })?;
    let previous = prepare_design(previous_input, previous_source.map(|c| (catalog, c)))?;
    let mut before_catalog = previous.records.catalog().map_err(|e| e.to_string())?;
    before_catalog
        .insert_revisions(previous.request.revisions)
        .map_err(|e| e.to_string())?;
    ensure_same_design(
        catalog,
        &base.assembly,
        &before_catalog,
        &previous.request.candidate_state.assembly,
    )?;
    let root = &base.assembly.instances[0];
    let parent = catalog.revision(&root.revision).expect("inspected parent");
    let body_pin = &parent.inclusions[0].revision;
    let body = catalog.revision(body_pin).expect("inspected body");
    let mut next = prepare_design(specification.design, next_source.map(|c| (catalog, c)))?;
    let mut next_catalog = next.records.catalog().map_err(|e| e.to_string())?;
    next_catalog
        .insert_revisions(next.request.revisions.clone())
        .map_err(|e| e.to_string())?;
    for r in &next.request.revisions {
        if catalog.revision(&r.id).is_some() {
            return Err(BuildingDesignError::new(
                "immutable_namespace",
                "candidate namespace already contains a definition",
            ));
        }
    }
    let new_body = next
        .request
        .revisions
        .iter()
        .find(|r| r.id == next.request.next_child)
        .expect("generated body");
    let mut retained_revisions = BTreeMap::new();
    let mut reused = BTreeMap::new();
    let mut ancestry = BTreeMap::new();
    for inclusion in &new_body.inclusions {
        if let Some(old) = body
            .inclusions
            .iter()
            .find(|i| i.instance == inclusion.instance)
        {
            let old_def = catalog.revision(&old.revision).expect("base child");
            let new_def = next_catalog
                .revision(&inclusion.revision)
                .expect("candidate child");
            if same_definition(catalog, old_def, &next_catalog, new_def, false)? {
                reused.insert(inclusion.revision.clone(), old.revision.clone());
                retained_revisions.insert(inclusion.instance.as_str().into(), old.revision.clone());
            } else {
                ancestry.insert(inclusion.revision.clone(), old.revision.clone());
            }
        }
    }
    let request = &mut next.request;
    request.base_state = base.id.clone();
    request.base_parent = root.revision.clone();
    request.previous_child = body_pin.clone();
    request.candidate_state.parents = vec![base.id.clone()];
    request.description="Explicit structured design update; unchanged child pins retained. Review/adoption never modifies a placed instance or grants a site-edit capability.".into();
    request.revisions.retain(|r| !reused.contains_key(&r.id));
    for revision in &mut request.revisions {
        if revision.id == request.next_child {
            revision.parents = vec![body_pin.clone()];
        } else if revision.id == request.candidate_parent {
            revision.parents = vec![root.revision.clone()];
        } else if let Some(old) = ancestry.get(&revision.id) {
            revision.parents = vec![old.clone()];
        }
        for inclusion in &mut revision.inclusions {
            if let Some(old) = reused.get(&inclusion.revision) {
                inclusion.revision = old.clone();
            }
        }
    }
    next.records = base_records(catalog, &next)?;
    let mut design = next.finish()?;
    let mut updates = BlueprintUpdates::new(design.records.catalog().map_err(|e| e.to_string())?);
    updates
        .create(design.request.clone())
        .map_err(|e| e.to_string())?;
    let diff = updates
        .diff(&design.request.id)
        .map_err(|e| e.to_string())?;
    // This is an update against a catalog that already owns the adopted base.
    // Sending all historical sources again can make a valid bounded update
    // exceed the public import budget after only a few revisions.
    let referenced_types = design
        .request
        .revisions
        .iter()
        .flat_map(|r| {
            r.static_type_bindings
                .iter()
                .map(|b| &b.type_revision)
                .chain(r.behavior_bindings.iter().map(|b| b.behavior_type()))
                .chain(r.ports.iter().flat_map(|p| p.required_source_types.iter()))
        })
        .collect::<BTreeSet<_>>();
    design
        .records
        .types
        .retain(|t| catalog.type_revision(&t.id).is_none() && referenced_types.contains(&t.id));
    design.records.classifications.clear();
    design.records.revisions.clear();
    design.records.assemblies.clear();
    Ok(GeneratedBuildingDesignUpdate {
        base_assembly_revision_id: base.id.clone(),
        design,
        diff,
        retained_revisions,
        placed_instances_modified: false,
    })
}

// Compare all physical data and retained obligations. Only revision ancestry
// and generated static-type IDs are normalized; contracts are compared directly.
fn same_definition(
    a_catalog: &BlueprintCatalog,
    a: &BlueprintRevision,
    b_catalog: &BlueprintCatalog,
    b: &BlueprintRevision,
    ignore_child_pins: bool,
) -> Result<bool, BuildingDesignError> {
    let normalize = |catalog: &BlueprintCatalog, definition: &BlueprintRevision| {
        let mut definition = definition.clone();
        definition.id = BlueprintRevisionId::new("compare.v1").expect("literal ID");
        definition.parents.clear();
        if ignore_child_pins {
            for inclusion in &mut definition.inclusions {
                inclusion.revision = definition.id.clone();
            }
        }
        let mut contracts = Vec::new();
        for binding in &mut definition.static_type_bindings {
            contracts.push(
                catalog
                    .type_revision(&binding.type_revision)
                    .ok_or_else(|| {
                        BuildingDesignError::new("unknown_type", "retained static type is missing")
                    })?
                    .contract
                    .clone(),
            );
            binding.type_revision = TypeRevisionId::new("compare.v1").expect("literal ID");
        }
        Ok::<_, BuildingDesignError>((definition, contracts))
    };
    Ok(normalize(a_catalog, a)? == normalize(b_catalog, b)?)
}

fn ensure_same_design(
    catalog: &BlueprintCatalog,
    base: &Assembly,
    expected_catalog: &BlueprintCatalog,
    expected: &Assembly,
) -> Result<(), BuildingDesignError> {
    let failure = || {
        BuildingDesignError::new(
            "base_design_mismatch",
            "previous structured input does not describe the selected Assembly and every retained requirement; retrieve the correct input or use an explicit general Blueprint proposal",
        )
    };
    let normalize = |a: &Assembly| {
        let mut a = a.clone();
        for i in &mut a.instances {
            i.revision = BlueprintRevisionId::new("compare.v1").expect("literal ID");
        }
        a
    };
    if normalize(base) != normalize(expected) {
        return Err(failure());
    }
    let a = base.inspect(catalog).map_err(|e| e.to_string())?;
    let b = expected
        .inspect(expected_catalog)
        .map_err(|e| e.to_string())?;
    if a.occurrences.keys().collect::<BTreeSet<_>>() != b.occurrences.keys().collect() {
        return Err(failure());
    }
    for (path, occurrence) in &a.occurrences {
        let other = &b.occurrences[path];
        if occurrence.origin != other.origin
            || occurrence.rotation != other.rotation
            || !same_definition(
                catalog,
                catalog
                    .revision(&occurrence.revision)
                    .expect("inspected base"),
                expected_catalog,
                expected_catalog
                    .revision(&other.revision)
                    .expect("inspected expected"),
                true,
            )?
        {
            return Err(failure().at(
                &path
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join("/"),
                None,
            ));
        }
    }
    Ok(())
}

fn base_records(
    catalog: &BlueprintCatalog,
    next: &PreparedDesign,
) -> Result<BlueprintRecords, BuildingDesignError> {
    let mut merged = catalog.clone();
    for t in &next.records.types {
        match merged.type_revision(&t.id) {
            Some(old) if old == t => {}
            Some(_) => {
                return Err(BuildingDesignError::new(
                    "immutable_namespace",
                    "candidate type collides with an existing definition",
                ));
            }
            None => merged.insert_type(t.clone()).map_err(|e| e.to_string())?,
        }
    }
    // The artificial empty site from new-design authoring is not the update
    // base. Original sources and assemblies already exist in the base catalog.
    Ok(BlueprintRecords {
        types: merged.type_revisions().cloned().collect(),
        classifications: merged.classifications().cloned().collect(),
        revisions: merged.revisions().cloned().collect(),
        assemblies: merged.assemblies().cloned().collect(),
    })
}
