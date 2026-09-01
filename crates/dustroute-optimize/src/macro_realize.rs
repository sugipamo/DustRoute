use std::collections::BTreeSet;

use dustroute_physical::Pos;
use dustroute_translate::{FunctionalNetworkModel, PhysicalCell, PlacedCell, RotationY};

use crate::MacroReplacementCandidate;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MacroBoundaryDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroBoundaryPort {
    pub observed_index: usize,
    pub name: String,
    pub position: Pos,
    pub direction: MacroBoundaryDirection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroPortRoute {
    pub boundary: MacroBoundaryPort,
    pub candidate_port: String,
    pub candidate_position: Pos,
    /// Inclusive, axis-aligned route skeleton. It is deliberately not a block
    /// patch until support, strength and neighbouring-net checks have passed.
    pub path: Vec<Pos>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextualVerificationState {
    Pending,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroRealizationVerification {
    pub structural: ContextualVerificationState,
    pub steady_state: ContextualVerificationState,
    pub transitions: ContextualVerificationState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroReplacementPlan {
    pub component_id: String,
    pub placed: PlacedCell,
    pub routes: Vec<MacroPortRoute>,
    pub verification: MacroRealizationVerification,
    pub automatic_apply_allowed: bool,
    pub total_route_length: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MacroRealizationError {
    UnsupportedLayoutReference(String),
    MissingObservedPort {
        direction: MacroBoundaryDirection,
        index: usize,
    },
    MissingCandidatePort(String),
    NoPlacement,
}

/// Extracts the replaceable cell's externally visible contract. Support
/// blocks are intentionally absent: they remain physical realization detail.
#[must_use]
pub fn extract_cell_boundary(cell: &PhysicalCell) -> Vec<MacroBoundaryPort> {
    cell.inputs
        .iter()
        .enumerate()
        .map(|(observed_index, port)| MacroBoundaryPort {
            observed_index,
            name: port.name.clone(),
            position: port.pos,
            direction: MacroBoundaryDirection::Input,
        })
        .chain(
            cell.outputs
                .iter()
                .enumerate()
                .map(|(observed_index, port)| MacroBoundaryPort {
                    observed_index,
                    name: port.name.clone(),
                    position: port.pos,
                    direction: MacroBoundaryDirection::Output,
                }),
        )
        .collect()
}

/// Converts the terminals inferred from a physical observation into the same
/// boundary contract used by cell-library realizations.
#[must_use]
pub fn extract_model_boundary(model: &FunctionalNetworkModel) -> Vec<MacroBoundaryPort> {
    model
        .truth_table
        .inputs
        .iter()
        .enumerate()
        .map(|(observed_index, terminal)| MacroBoundaryPort {
            observed_index,
            name: format!("input_{observed_index}"),
            position: terminal.anchor,
            direction: MacroBoundaryDirection::Input,
        })
        .chain(
            model
                .truth_table
                .outputs
                .iter()
                .enumerate()
                .map(|(observed_index, terminal)| MacroBoundaryPort {
                    observed_index,
                    name: format!("output_{observed_index}"),
                    position: terminal.anchor,
                    direction: MacroBoundaryDirection::Output,
                }),
        )
        .collect()
}

pub fn resolve_builtin_layout(reference: &str) -> Result<PhysicalCell, MacroRealizationError> {
    match reference {
        "dustroute-translate:compiled_xor_cell" => dustroute_translate::compiled_xor_cell(),
        "dustroute-translate:compact_compiled_xor_cell" => {
            dustroute_translate::compact_compiled_xor_cell()
        }
        other => {
            return Err(MacroRealizationError::UnsupportedLayoutReference(
                other.into(),
            ));
        }
    }
    .map_err(MacroRealizationError::UnsupportedLayoutReference)
}

/// Produces a read-only placement proposal. Candidate origins are derived by
/// aligning every candidate port with its corresponding fixed boundary port;
/// this keeps the search finite without imposing an arbitrary radius.
pub fn plan_macro_replacement(
    candidate: &MacroReplacementCandidate,
    boundary: &[MacroBoundaryPort],
) -> Result<MacroReplacementPlan, MacroRealizationError> {
    let cell = resolve_builtin_layout(&candidate.layout_reference)?;
    let mappings = mapped_boundary(candidate, boundary)?;
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
            let routes = build_routes(&placed, &mappings)?;
            let total_route_length = routes
                .iter()
                .map(|route| route.path.len().saturating_sub(1))
                .sum();
            let plan = MacroReplacementPlan {
                component_id: candidate.component_id.as_str().into(),
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
                (
                    plan.total_route_length,
                    plan.placed.rotation as u8,
                    plan.placed.origin,
                ) < (
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

fn mapped_boundary<'a>(
    candidate: &'a MacroReplacementCandidate,
    boundary: &'a [MacroBoundaryPort],
) -> Result<Vec<(&'a MacroBoundaryPort, &'a str)>, MacroRealizationError> {
    let mut result = Vec::new();
    for (direction, names) in [
        (MacroBoundaryDirection::Input, &candidate.input_ports),
        (MacroBoundaryDirection::Output, &candidate.output_ports),
    ] {
        for (index, name) in names.iter().enumerate() {
            let port = boundary
                .iter()
                .find(|port| port.direction == direction && port.observed_index == index)
                .ok_or(MacroRealizationError::MissingObservedPort { direction, index })?;
            result.push((port, name.as_str()));
        }
    }
    Ok(result)
}

fn local_port(
    cell: &PhysicalCell,
    direction: MacroBoundaryDirection,
    name: &str,
) -> Result<Pos, MacroRealizationError> {
    match direction {
        MacroBoundaryDirection::Input => cell
            .inputs
            .iter()
            .find(|port| port.name == name)
            .map(|port| port.pos),
        MacroBoundaryDirection::Output => cell
            .outputs
            .iter()
            .find(|port| port.name == name)
            .map(|port| port.pos),
    }
    .ok_or_else(|| MacroRealizationError::MissingCandidatePort(name.into()))
}

fn build_routes(
    placed: &PlacedCell,
    mappings: &[(&MacroBoundaryPort, &str)],
) -> Result<Vec<MacroPortRoute>, MacroRealizationError> {
    mappings
        .iter()
        .map(|(boundary, name)| {
            let candidate_position = match boundary.direction {
                MacroBoundaryDirection::Input => placed.input_port(name).map(|port| port.pos),
                MacroBoundaryDirection::Output => placed.output_port(name).map(|port| port.pos),
            }
            .ok_or_else(|| MacroRealizationError::MissingCandidatePort((*name).into()))?;
            Ok(MacroPortRoute {
                boundary: (*boundary).clone(),
                candidate_port: (*name).into(),
                candidate_position,
                path: manhattan_path(candidate_position, boundary.position),
            })
        })
        .collect()
}

fn manhattan_path(from: Pos, to: Pos) -> Vec<Pos> {
    let mut result = vec![from];
    let mut cursor = from;
    for axis in 0..3 {
        while cursor != to && coordinate(cursor, axis) != coordinate(to, axis) {
            let delta = (coordinate(to, axis) - coordinate(cursor, axis)).signum();
            cursor = match axis {
                0 => Pos::new(cursor.x + delta, cursor.y, cursor.z),
                1 => Pos::new(cursor.x, cursor.y + delta, cursor.z),
                _ => Pos::new(cursor.x, cursor.y, cursor.z + delta),
            };
            result.push(cursor);
        }
    }
    debug_assert_eq!(result.last(), Some(&to));
    debug_assert_eq!(
        result.iter().copied().collect::<BTreeSet<_>>().len(),
        result.len()
    );
    result
}

const fn coordinate(pos: Pos, axis: usize) -> i32 {
    match axis {
        0 => pos.x,
        1 => pos.y,
        _ => pos.z,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObservedMacroMetrics, find_builtin_verified_macro_replacements};
    use dustroute_translate::{RegionBounds, analyze_world_region, derive_functional_network};

    #[test]
    fn plans_compact_xor_against_fixed_baseline_ports_without_authorizing_apply() {
        let baseline = dustroute_translate::compiled_xor_cell().unwrap();
        let (low, high) = baseline.world.bounds().unwrap();
        let analysis = analyze_world_region(&baseline.world, RegionBounds::new(low, high));
        let model = derive_functional_network(&baseline.world, &analysis, 8, 64).unwrap();
        let candidate = find_builtin_verified_macro_replacements(
            &model,
            "java",
            "1.21.11",
            ObservedMacroMetrics::from_world(&baseline.world),
        )
        .remove(0);

        let boundary = extract_cell_boundary(&baseline);
        let plan = plan_macro_replacement(&candidate, &boundary).unwrap();

        assert_eq!(plan.routes.len(), 3);
        assert!(
            plan.routes
                .iter()
                .all(|route| route.path.first() == Some(&route.candidate_position))
        );
        assert!(
            plan.routes
                .iter()
                .all(|route| route.path.last() == Some(&route.boundary.position))
        );
        assert!(!plan.automatic_apply_allowed);
        assert_eq!(
            plan.verification.transitions,
            ContextualVerificationState::Pending
        );
    }
}
