//! Finite placement search and source-layout resolution.
use super::routing::{build_routes, local_port, mapped_boundary};
use super::{
    ContextualVerificationState, MacroBoundaryDirection, MacroBoundaryPort, MacroRealizationError,
    MacroRealizationVerification, MacroReplacementPlan,
};
use crate::MacroReplacementCandidate;
use dustroute_library::blueprint::{BlueprintCatalog, BlueprintRevisionId};
use dustroute_physical::Pos;
use dustroute_translate::{PhysicalCell, PlacedCell, RotationY};
use std::collections::BTreeSet;
use std::sync::Arc;

pub fn resolve_builtin_layout(reference: &str) -> Result<PhysicalCell, MacroRealizationError> {
    resolve_blueprint_layout(
        reference,
        dustroute_library::builtin_blueprints::builtin_blueprints(),
    )
}

/// Resolves caller-supplied blueprint data, never executable generator names.
/// Historical built-in references are aliases for pinned, frozen revisions.
pub fn resolve_blueprint_layout(
    reference: &str,
    catalog: &dustroute_library::blueprint::BlueprintCatalog,
) -> Result<PhysicalCell, MacroRealizationError> {
    let id = layout_revision(reference)?;
    dustroute_translate::blueprint::blueprint_cell(catalog, &id)
        .map_err(|error| MacroRealizationError::UnsupportedLayoutReference(error.to_string()))
}

pub(crate) fn layout_revision(
    reference: &str,
) -> Result<BlueprintRevisionId, MacroRealizationError> {
    use dustroute_library::builtin_blueprints::{XOR_COMPACT_REVISION, XOR_REVISION};
    let revision = match reference {
        "dustroute-translate:compiled_xor_cell" => XOR_REVISION,
        "dustroute-translate:compact_compiled_xor_cell" => XOR_COMPACT_REVISION,
        _ => reference
            .strip_prefix("blueprint:")
            .ok_or_else(|| MacroRealizationError::UnsupportedLayoutReference(reference.into()))?,
    };
    BlueprintRevisionId::new(revision)
        .map_err(|error| MacroRealizationError::UnsupportedLayoutReference(error.into()))
}

/// Produces a read-only placement proposal. Candidate origins are derived by
/// aligning every candidate port with its corresponding fixed boundary port;
/// this keeps the search finite without imposing an arbitrary radius.
pub fn plan_macro_replacement(
    candidate: &MacroReplacementCandidate,
    boundary: &[MacroBoundaryPort],
) -> Result<MacroReplacementPlan, MacroRealizationError> {
    plan_macro_replacement_with_reserved(candidate, boundary, &BTreeSet::new())
}

pub fn plan_macro_replacement_with_reserved(
    candidate: &MacroReplacementCandidate,
    boundary: &[MacroBoundaryPort],
    reserved: &BTreeSet<Pos>,
) -> Result<MacroReplacementPlan, MacroRealizationError> {
    plan_macro_replacement_in_catalog(
        candidate,
        boundary,
        reserved,
        dustroute_library::builtin_blueprints::builtin_blueprints(),
    )
}

pub fn plan_macro_replacement_in_catalog(
    candidate: &MacroReplacementCandidate,
    boundary: &[MacroBoundaryPort],
    reserved: &BTreeSet<Pos>,
    catalog: &dustroute_library::blueprint::BlueprintCatalog,
) -> Result<MacroReplacementPlan, MacroRealizationError> {
    let revision = layout_revision(&candidate.layout_reference)?;
    let mut plan = plan_blueprint_replacement(
        catalog,
        &revision,
        &candidate.input_ports,
        &candidate.output_ports,
        boundary,
        reserved,
    )?;
    plan.component_id = candidate.component_id.as_str().into();
    Ok(plan)
}

/// Plans any explicitly selected concrete revision, including multiple inputs
/// and outputs. Port order is an explicit binding to observed boundary indices;
/// no GateKind, classification meaning or logical equivalence is inferred.
pub fn plan_blueprint_replacement(
    catalog: &BlueprintCatalog,
    revision: &BlueprintRevisionId,
    input_ports: &[String],
    output_ports: &[String],
    boundary: &[MacroBoundaryPort],
    reserved: &BTreeSet<Pos>,
) -> Result<MacroReplacementPlan, MacroRealizationError> {
    let cell = dustroute_translate::blueprint::blueprint_cell_for_routing(catalog, revision)
        .map_err(|error| MacroRealizationError::Blueprint(error.to_string()))?;
    let source_catalog = Arc::new(catalog.clone());
    for (names, actual) in [
        (
            input_ports,
            cell.inputs
                .iter()
                .map(|p| p.name.as_str())
                .collect::<BTreeSet<_>>(),
        ),
        (
            output_ports,
            cell.outputs.iter().map(|p| p.name.as_str()).collect(),
        ),
    ] {
        if names.len() != actual.len()
            || names.iter().map(String::as_str).collect::<BTreeSet<_>>() != actual
        {
            return Err(MacroRealizationError::Blueprint(
                "port bindings must cover each selected interface exactly once".into(),
            ));
        }
    }
    if boundary.len() != input_ports.len() + output_ports.len() {
        return Err(MacroRealizationError::Blueprint(
            "boundary and selected interface counts differ".into(),
        ));
    }
    let mappings = mapped_boundary(input_ports, output_ports, boundary)?;
    let rotations = [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ];
    let mut best: Option<MacroReplacementPlan> = None;

    for rotation in rotations {
        for (boundary_port, candidate_name) in &mappings {
            let local = local_port(&cell, boundary_port.direction, candidate_name)?;
            let rotated = rotation.pos(local);
            let origin = Pos::new(
                boundary_port.position.x - rotated.x,
                boundary_port.position.y - rotated.y,
                boundary_port.position.z - rotated.z,
            );
            let placed = PlacedCell {
                cell: cell.clone(),
                origin,
                rotation,
            };
            let aligned_boundary_ports = mappings
                .iter()
                .filter_map(|(boundary, name)| {
                    let (position, facing) = match boundary.direction {
                        MacroBoundaryDirection::Input => {
                            placed.input_port(name).map(|port| (port.pos, port.facing))
                        }
                        MacroBoundaryDirection::Output => {
                            placed.output_port(name).map(|port| (port.pos, port.facing))
                        }
                    }?;
                    (position == boundary.position
                        && boundary
                            .facing
                            .is_none_or(|expected| facing == Some(expected)))
                    .then_some(position)
                })
                .collect::<BTreeSet<_>>();
            if placed.blocks().any(|(pos, _)| {
                reserved.contains(&pos)
                    || (boundary.iter().any(|port| port.position == pos)
                        && !aligned_boundary_ports.contains(&pos))
            }) {
                continue;
            }
            if mappings.iter().any(|(boundary, name)| {
                let placed_port = match boundary.direction {
                    MacroBoundaryDirection::Input => {
                        placed.input_port(name).map(|port| (port.pos, port.facing))
                    }
                    MacroBoundaryDirection::Output => {
                        placed.output_port(name).map(|port| (port.pos, port.facing))
                    }
                };
                placed_port.is_some_and(|(position, facing)| {
                    position == boundary.position
                        && boundary
                            .facing
                            .is_some_and(|expected| facing != Some(expected))
                })
            }) {
                continue;
            }
            let routes = match build_routes(&placed, &mappings, reserved) {
                Ok(routes) => routes,
                Err(MacroRealizationError::NoPlacement) => continue,
                Err(error) => return Err(error),
            };
            let total_route_length = routes
                .iter()
                .map(|route| route.path.len().saturating_sub(1))
                .sum();
            let candidate_positions = placed.blocks().map(|(pos, _)| pos).collect::<BTreeSet<_>>();
            let route_collision_count = routes
                .iter()
                .flat_map(|route| {
                    route
                        .path
                        .iter()
                        .skip(1)
                        .take(route.path.len().saturating_sub(2))
                })
                .filter(|pos| candidate_positions.contains(pos))
                .count();
            let plan = MacroReplacementPlan {
                source_catalog: source_catalog.clone(),
                component_id: revision.as_str().into(),
                placed,
                routes,
                verification: MacroRealizationVerification {
                    structural: ContextualVerificationState::Pending,
                    steady_state: ContextualVerificationState::Pending,
                    transitions: ContextualVerificationState::Pending,
                },
                automatic_apply_allowed: false,
                total_route_length,
            };
            if best.as_ref().is_none_or(|current| {
                let current_positions = current
                    .placed
                    .blocks()
                    .map(|(pos, _)| pos)
                    .collect::<BTreeSet<_>>();
                let current_collision_count = current
                    .routes
                    .iter()
                    .flat_map(|route| {
                        route
                            .path
                            .iter()
                            .skip(1)
                            .take(route.path.len().saturating_sub(2))
                    })
                    .filter(|pos| current_positions.contains(pos))
                    .count();
                (
                    route_collision_count,
                    plan.total_route_length,
                    plan.placed.rotation as u8,
                    plan.placed.origin,
                ) < (
                    current_collision_count,
                    current.total_route_length,
                    current.placed.rotation as u8,
                    current.placed.origin,
                )
            }) {
                best = Some(plan);
            }
        }
    }
    best.ok_or(MacroRealizationError::NoPlacement)
}
