//! Adapters between immutable blueprints and the existing physical compiler.
//!
//! This bridge deliberately does not certify type claims or bypass simulator /
//! placement checks. Legacy cells cannot represent every blueprint contract.

use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintError, BlueprintPort, BlueprintPortKind, BlueprintRevision,
    BlueprintRevisionId, ClassificationRevisionId, PositionedBlock,
};
use dustroute_library::{PortDirection, Provenance};

use crate::cells::{InputPort, OutputPort, PhysicalCell, PortKind};
use crate::world::BlockKind;

pub use dustroute_library::builtin_blueprints::{
    NOT_CLASSIFICATION_REVISION, NOT_SIDE_REVISION, NOT_TOP_REVISION, builtin_blueprints,
};

/// Compatibility accessor retained for callers of the first migration increment.
/// NOT and larger realizations now share the same frozen data catalog.
#[must_use]
pub fn builtin_not_blueprints() -> &'static BlueprintCatalog {
    builtin_blueprints()
}

/// Captures a proposed physical cell; supplied classifications remain claims.
#[must_use]
pub fn blueprint_from_cell(
    id: BlueprintRevisionId,
    cell: &PhysicalCell,
    classifications: Vec<ClassificationRevisionId>,
    provenance: Provenance,
) -> BlueprintRevision {
    let port_kind = |kind| match kind {
        PortKind::Wire => BlueprintPortKind::Wire,
        PortKind::BlockPower => BlueprintPortKind::BlockPower,
    };
    let ports = cell
        .inputs
        .iter()
        .map(|port| BlueprintPort {
            name: port.name.clone(),
            direction: PortDirection::Input,
            position: port.pos,
            kind: port_kind(port.kind),
            facing: port.facing,
            required_source_types: Vec::new(),
        })
        .chain(cell.outputs.iter().map(|port| BlueprintPort {
            name: port.name.clone(),
            direction: PortDirection::Output,
            position: port.pos,
            kind: port_kind(port.kind),
            facing: port.facing,
            required_source_types: Vec::new(),
        }))
        .collect();
    BlueprintRevision {
        required_laws: vec![],
        static_type_bindings: vec![],
        behavior_bindings: vec![],
        law: None,
        initial_layout: None,
        connections: vec![],
        port_bindings: vec![],
        id,
        parents: Vec::new(),
        name: cell.name.clone(),
        classifications,
        blocks: cell
            .world
            .iter()
            .map(|(position, block)| PositionedBlock {
                position: *position,
                block: block.clone(),
            })
            .collect(),
        inclusions: Vec::new(),
        ports,
        provenance,
    }
}

/// Expands a blueprint into the legacy compiler's mutable proposal format.
/// Requirements that format cannot retain are rejected, never discarded.
pub fn blueprint_cell(
    catalog: &BlueprintCatalog,
    id: &BlueprintRevisionId,
) -> Result<PhysicalCell, BlueprintError> {
    expand_cell(catalog, id, false)
}

/// Geometry projection for planning only. Consumers must retain this exact
/// source catalog/revision and review the resulting Assembly, including every
/// descendant, after routing. This is not a contract validation certificate.
pub fn blueprint_cell_for_routing(
    catalog: &BlueprintCatalog,
    id: &BlueprintRevisionId,
) -> Result<PhysicalCell, BlueprintError> {
    expand_cell(catalog, id, true)
}

fn expand_cell(
    catalog: &BlueprintCatalog,
    id: &BlueprintRevisionId,
    check_root_after_routing: bool,
) -> Result<PhysicalCell, BlueprintError> {
    let root = catalog
        .revision(id)
        .ok_or_else(|| BlueprintError::UnknownRevision(id.clone()))?;
    let expanded = catalog.expand(id)?;
    if !check_root_after_routing
        && expanded
            .source_claims
            .values()
            .any(|claims| claims.values().any(|block| block.kind == BlockKind::Air))
    {
        return Err(BlueprintError::Invalid(
            "legacy cells cannot retain explicit air requirements".into(),
        ));
    }
    for occurrence in &expanded.occurrences {
        let record = catalog
            .revision(&occurrence.revision)
            .expect("expanded revision exists");
        if record.law.is_some() {
            return Err(BlueprintError::Invalid(
                "legacy cell projection cannot execute or retain a law program".into(),
            ));
        }
        if !check_root_after_routing
            && record
                .ports
                .iter()
                .any(|port| !port.required_source_types.is_empty())
        {
            return Err(BlueprintError::Invalid(
                "legacy cells cannot certify consumer type requirements".into(),
            ));
        }
        if !check_root_after_routing && !record.connections.is_empty() {
            return Err(BlueprintError::Invalid(
                "legacy cells cannot retain internal connection contracts".into(),
            ));
        }
        if !check_root_after_routing && !record.static_type_bindings.is_empty() {
            return Err(BlueprintError::Invalid(
                "legacy cells cannot retain static type obligations".into(),
            ));
        }
        if !check_root_after_routing && !record.required_laws.is_empty() {
            return Err(BlueprintError::Invalid(
                "legacy cells cannot retain physical law requirements".into(),
            ));
        }
        if !check_root_after_routing && !record.behavior_bindings.is_empty() {
            return Err(BlueprintError::Invalid(
                "legacy cells cannot retain behavioral obligations".into(),
            ));
        }
        if record.ports.iter().any(|port| {
            matches!(
                port.kind,
                BlueprintPortKind::BlockState | BlueprintPortKind::DeviceOutput
            )
        }) {
            return Err(BlueprintError::Invalid(
                "legacy cells cannot retain block-state or direct-device interfaces".into(),
            ));
        }
    }
    let mut cell = PhysicalCell {
        source_revision: Some(id.clone()),
        name: root.name.clone(),
        world: expanded.proposed_world(),
        inputs: Vec::new(),
        outputs: Vec::new(),
    };
    for port in &root.ports {
        let kind = match port.kind {
            BlueprintPortKind::Wire => PortKind::Wire,
            BlueprintPortKind::BlockPower => PortKind::BlockPower,
            BlueprintPortKind::BlockState | BlueprintPortKind::DeviceOutput => {
                unreachable!("checked above")
            }
        };
        match port.direction {
            PortDirection::Input => cell.inputs.push(InputPort {
                name: port.name.clone(),
                pos: port.position,
                kind,
                facing: port.facing,
            }),
            PortDirection::Output => cell.outputs.push(OutputPort {
                name: port.name.clone(),
                pos: port.position,
                kind,
                facing: port.facing,
            }),
        }
    }
    Ok(cell)
}

pub fn builtin_cell_result(revision: &str) -> Result<PhysicalCell, BlueprintError> {
    let id = BlueprintRevisionId::new(revision)
        .map_err(|error| BlueprintError::Invalid(error.to_owned()))?;
    blueprint_cell(builtin_blueprints(), &id)
}

#[must_use]
pub(crate) fn builtin_cell(revision: &str) -> PhysicalCell {
    builtin_cell_result(revision).expect("built-in blueprint expands to a legacy cell")
}
