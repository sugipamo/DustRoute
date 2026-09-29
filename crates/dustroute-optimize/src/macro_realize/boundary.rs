//! Observed interface extraction.
use super::routing::facing_between;
use super::{MacroBoundaryDirection, MacroBoundaryPort};
use dustroute_physical::{Pos, World};
use dustroute_translate::{FunctionalNetworkModel, InferredTruthTable, PhysicalCell};

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
            facing: port.facing,
            driver_position: None,
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
                    facing: port.facing,
                    driver_position: None,
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
            facing: None,
            driver_position: None,
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
                    facing: None,
                    driver_position: None,
                }),
        )
        .collect()
}

#[must_use]
pub fn extract_model_boundary_with_context(
    model: &FunctionalNetworkModel,
    world: &World,
    analysis: &dustroute_translate::RegionAnalysis,
) -> Vec<MacroBoundaryPort> {
    let mut boundary = extract_model_boundary(model);
    for port in boundary
        .iter_mut()
        .filter(|port| port.direction == MacroBoundaryDirection::Input)
    {
        let Some(terminal) = model.truth_table.inputs.get(port.observed_index) else {
            continue;
        };
        let Ok(driver) = dustroute_translate::inferred_input_driver(world, analysis, terminal)
        else {
            continue;
        };
        let driver_position = match driver {
            dustroute_translate::InferredInputDriver::Lever(pos)
            | dustroute_translate::InferredInputDriver::Button(pos)
            | dustroute_translate::InferredInputDriver::PressurePlate(pos)
            | dustroute_translate::InferredInputDriver::External(pos) => pos,
        };
        port.driver_position = Some(driver_position);
        port.facing = facing_between(port.position, driver_position);
    }
    boundary
}

pub(super) fn boundary_terminal_mapping(
    anchors: impl Iterator<Item = Pos>,
    actual: &InferredTruthTable,
    analysis: &dustroute_translate::RegionAnalysis,
    inputs: bool,
) -> Option<Vec<usize>> {
    let terminals = if inputs {
        &actual.inputs
    } else {
        &actual.outputs
    };
    anchors
        .map(|anchor| {
            let component = analysis
                .components
                .iter()
                .find(|component| component.positions.contains(&anchor))?
                .id;
            let matches = terminals
                .iter()
                .enumerate()
                .filter(|(_, terminal)| terminal.component == component)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if matches.len() == 1 {
                Some(matches[0])
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn set_driver_in_world(
    world: &mut World,
    driver: dustroute_translate::InferredInputDriver,
    powered: bool,
) -> Result<(), String> {
    dustroute_translate::apply_inferred_input_driver(world, driver, powered)
        .map_err(|error| error.to_string())
}
