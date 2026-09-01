use std::collections::BTreeSet;

use dustroute_physical::{
    Block, BlockKind, Facing, PhysicalBlockChange, PhysicalPatch, PhysicalPatchReason, Pos, World,
};
use dustroute_translate::{
    FunctionalNetworkModel, InferredTruthTable, PhysicalCell, PlacedCell, RegionBounds, RotationY,
    TruthTableComparison, TruthTableRow,
};

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
    pub facing: Option<Facing>,
    pub driver_position: Option<Pos>,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MacroStructuralReport {
    pub candidate_collisions: Vec<Pos>,
    pub route_collisions: Vec<Pos>,
    pub route_cross_net_contacts: Vec<(usize, usize, Pos, Pos)>,
    pub candidate_support_issues: Vec<Pos>,
    /// Supports that a later materializer may add if their positions are free.
    pub required_route_supports: Vec<Pos>,
    pub blocked_route_supports: Vec<Pos>,
}

impl MacroStructuralReport {
    #[must_use]
    pub fn valid(&self) -> bool {
        self.candidate_collisions.is_empty()
            && self.route_collisions.is_empty()
            && self.route_cross_net_contacts.is_empty()
            && self.candidate_support_issues.is_empty()
            && self.blocked_route_supports.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedMacroReplacement {
    pub world: World,
    pub patch: PhysicalPatch,
    pub added_supports: Vec<Pos>,
    pub inserted_repeaters: Vec<Pos>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroSteadyStateReport {
    pub state: ContextualVerificationState,
    pub comparison: Option<TruthTableComparison>,
    /// Expected boundary index to newly inferred terminal index.
    pub input_mapping: Vec<usize>,
    pub output_mapping: Vec<usize>,
    pub differing_assignments: Vec<Vec<bool>>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroTransitionCase {
    pub from: Vec<bool>,
    pub to: Vec<bool>,
    pub original_outputs: Vec<Vec<bool>>,
    pub candidate_outputs: Vec<Vec<bool>>,
    pub equivalent: bool,
    pub first_difference_tick: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroTransitionReport {
    pub state: ContextualVerificationState,
    pub cases: Vec<MacroTransitionCase>,
    pub differing_cases: usize,
    pub reason: Option<String>,
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
    StructurallyInvalid(Box<MacroStructuralReport>),
    NoRepeaterSite {
        route: usize,
        after_steps: usize,
    },
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
            | dustroute_translate::InferredInputDriver::External(pos) => pos,
        };
        port.driver_position = Some(driver_position);
        port.facing = facing_between(port.position, driver_position);
    }
    boundary
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
    plan_macro_replacement_with_reserved(candidate, boundary, &BTreeSet::new())
}

pub fn plan_macro_replacement_with_reserved(
    candidate: &MacroReplacementCandidate,
    boundary: &[MacroBoundaryPort],
    reserved: &BTreeSet<Pos>,
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

/// Checks a proposal against the observed context without changing it.
/// `replaceable` is the exact ownership set of the focused implementation;
/// occupied blocks outside it are immutable obstacles.
#[must_use]
pub fn validate_macro_structure(
    plan: &MacroReplacementPlan,
    observed: &World,
    replaceable: &BTreeSet<Pos>,
) -> MacroStructuralReport {
    let mut report = MacroStructuralReport::default();
    let candidate_blocks = plan
        .placed
        .blocks()
        .collect::<std::collections::BTreeMap<_, _>>();
    let boundary_positions = plan
        .routes
        .iter()
        .map(|route| route.boundary.position)
        .collect::<BTreeSet<_>>();
    for (pos, block) in &candidate_blocks {
        let compatible_boundary = boundary_positions.contains(pos)
            && observed
                .get(*pos)
                .is_some_and(|actual| actual.kind == block.kind);
        if observed.get(*pos).is_some_and(|actual| actual != block)
            && !replaceable.contains(pos)
            && !compatible_boundary
        {
            report.candidate_collisions.push(*pos);
        }
    }
    let mut candidate_world = observed.clone();
    for pos in replaceable {
        candidate_world.remove(*pos);
    }
    for (pos, block) in &candidate_blocks {
        if boundary_positions.contains(pos)
            && observed
                .get(*pos)
                .is_some_and(|actual| actual.kind == block.kind)
        {
            continue;
        }
        candidate_world.set(*pos, block.clone());
    }
    report.candidate_support_issues = candidate_world
        .support_issues()
        .into_iter()
        .filter_map(|(pos, _, _)| candidate_blocks.contains_key(&pos).then_some(pos))
        .collect();

    for (index, route) in plan.routes.iter().enumerate() {
        for pos in route
            .path
            .iter()
            .copied()
            .skip(1)
            .take(route.path.len().saturating_sub(2))
        {
            if candidate_blocks.contains_key(&pos)
                || (observed.kind_at(pos) != BlockKind::Air && !replaceable.contains(&pos))
            {
                report.route_collisions.push(pos);
            }
            let support = pos.offset(0, -1, 0);
            if candidate_blocks
                .get(&support)
                .or_else(|| {
                    (!replaceable.contains(&support))
                        .then(|| observed.get(support))
                        .flatten()
                })
                .is_some_and(|block| block.redstone_traits().supports_dust_on_top)
            {
                continue;
            }
            if observed.kind_at(support) == BlockKind::Air || replaceable.contains(&support) {
                report.required_route_supports.push(support);
            } else {
                report.blocked_route_supports.push(support);
            }
        }
        for (other_index, other) in plan.routes.iter().enumerate().skip(index + 1) {
            for (first_index, first) in route.path.iter().enumerate() {
                for (second_index, second) in other.path.iter().enumerate() {
                    let dx = (first.x - second.x).abs();
                    let dy = (first.y - second.y).abs();
                    let dz = (first.z - second.z).abs();
                    let first_internal = first_index > 0 && first_index + 1 < route.path.len();
                    let second_internal = second_index > 0 && second_index + 1 < other.path.len();
                    if first == second
                        || (first_internal && second_internal && dx + dz == 1 && dy <= 1)
                    {
                        report
                            .route_cross_net_contacts
                            .push((index, other_index, *first, *second));
                    }
                }
            }
        }
    }
    report.candidate_collisions.sort_unstable();
    report.candidate_collisions.dedup();
    report.route_collisions.sort_unstable();
    report.route_collisions.dedup();
    report.candidate_support_issues.sort_unstable();
    report.candidate_support_issues.dedup();
    report.required_route_supports.sort_unstable();
    report.required_route_supports.dedup();
    report.blocked_route_supports.sort_unstable();
    report.blocked_route_supports.dedup();
    report
}

/// Converts a structurally valid skeleton into a virtual world and an exact
/// reversible patch. The observed world itself is never mutated.
pub fn materialize_macro_replacement(
    plan: &MacroReplacementPlan,
    observed: &World,
    replaceable: &BTreeSet<Pos>,
    max_wire_run: usize,
) -> Result<MaterializedMacroReplacement, MacroRealizationError> {
    let structural = validate_macro_structure(plan, observed, replaceable);
    if !structural.valid() {
        return Err(MacroRealizationError::StructurallyInvalid(Box::new(
            structural,
        )));
    }
    let mut world = observed.clone();
    for pos in replaceable {
        world.remove(*pos);
    }
    for (pos, block) in plan.placed.blocks() {
        let fixed_boundary = plan
            .routes
            .iter()
            .any(|route| route.boundary.position == pos)
            && observed
                .get(pos)
                .is_some_and(|actual| actual.kind == block.kind);
        if !fixed_boundary {
            world.set(pos, block);
        }
    }
    for support in &structural.required_route_supports {
        if world.kind_at(*support) == BlockKind::Air {
            world.set(*support, Block::new(BlockKind::Solid));
        }
    }

    let mut inserted_repeaters = Vec::new();
    for (route_index, route) in plan.routes.iter().enumerate() {
        let mut repeaters = repeater_sites(&route.path, max_wire_run).ok_or(
            MacroRealizationError::NoRepeaterSite {
                route: route_index,
                after_steps: max_wire_run,
            },
        )?;
        if route.boundary.direction == MacroBoundaryDirection::Input {
            for facing in repeaters.values_mut() {
                *facing = facing.opposite();
            }
        }
        for (index, pos) in route
            .path
            .iter()
            .copied()
            .enumerate()
            .skip(1)
            .take(route.path.len().saturating_sub(2))
        {
            if let Some(facing) = repeaters.get(&index) {
                let repeater = world.place(BlockKind::Repeater, pos);
                repeater.facing = Some(*facing);
                repeater.delay = Some(1);
                inserted_repeaters.push(pos);
            } else {
                world.place(BlockKind::RedstoneWire, pos);
            }
        }
    }
    dustroute_translate::update_wire_shapes(&mut world);
    let positions = observed
        .positions()
        .chain(world.positions())
        .collect::<BTreeSet<_>>();
    let changes = positions
        .into_iter()
        .filter_map(|pos| {
            let before = observed
                .get(pos)
                .cloned()
                .unwrap_or_else(|| Block::new(BlockKind::Air));
            let after = world
                .get(pos)
                .cloned()
                .unwrap_or_else(|| Block::new(BlockKind::Air));
            (before != after).then_some(PhysicalBlockChange { pos, before, after })
        })
        .collect();
    Ok(MaterializedMacroReplacement {
        world,
        patch: PhysicalPatch {
            reason: PhysicalPatchReason::OptimizePlacement,
            affected_fragments: Vec::new(),
            confidence_percent: 100,
            explanation: format!("replace focused implementation with {}", plan.component_id),
            changes,
        },
        added_supports: structural.required_route_supports,
        inserted_repeaters,
    })
}

/// Re-infers the materialized circuit, identifies its terminals by the fixed
/// boundary components, and compares rows in the original boundary order.
#[must_use]
pub fn verify_macro_steady_state(
    expected: &InferredTruthTable,
    original: &World,
    materialized: &World,
    max_inputs: usize,
    settle_ticks: usize,
) -> MacroSteadyStateReport {
    let Some((low, high)) = materialized.bounds() else {
        return unavailable_steady("materialized world is empty");
    };
    let analysis =
        dustroute_translate::analyze_world_region(materialized, RegionBounds::new(low, high));
    if expected.inputs.len() > max_inputs {
        return unavailable_steady("too many inputs for steady-state verification");
    }
    let Some((original_low, original_high)) = original.bounds() else {
        return unavailable_steady("original world is empty");
    };
    let original_analysis = dustroute_translate::analyze_world_region(
        original,
        RegionBounds::new(original_low, original_high),
    );
    let drivers = match expected
        .inputs
        .iter()
        .map(|terminal| {
            dustroute_translate::inferred_input_driver(original, &original_analysis, terminal)
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(drivers) => drivers,
        Err(error) => return unavailable_steady(&error.to_string()),
    };
    let actual = InferredTruthTable {
        inputs: analysis.inputs.clone(),
        outputs: analysis.outputs.clone(),
        rows: Vec::new(),
    };
    let input_mapping = boundary_terminal_mapping(
        expected.inputs.iter().map(|t| t.anchor),
        &actual,
        &analysis,
        true,
    )
    .unwrap_or_default();
    let output_mapping = boundary_terminal_mapping(
        expected.outputs.iter().map(|t| t.anchor),
        &actual,
        &analysis,
        false,
    )
    .unwrap_or_default();
    let mut rows = Vec::with_capacity(expected.rows.len());
    for expected_row in &expected.rows {
        let mut driven = materialized.clone();
        for (driver, powered) in drivers.iter().zip(&expected_row.inputs) {
            set_driver_in_world(&mut driven, *driver, *powered);
        }
        dustroute_translate::update_wire_shapes(&mut driven);
        let state = match dustroute_translate::RedstoneTickSimulator::new(driven)
            .and_then(|mut simulator| simulator.settle_ticks(settle_ticks))
        {
            Ok(state) => state,
            Err(error) => return unavailable_steady(&error.to_string()),
        };
        rows.push(TruthTableRow {
            inputs: expected_row.inputs.clone(),
            outputs: expected
                .outputs
                .iter()
                .map(|terminal| state.powered(terminal.anchor))
                .collect(),
        });
    }
    let normalized = InferredTruthTable {
        inputs: expected.inputs.clone(),
        outputs: expected.outputs.clone(),
        rows,
    };
    let comparison = dustroute_translate::compare_truth_tables(expected, &normalized);
    let differing_assignments = expected
        .rows
        .iter()
        .zip(&normalized.rows)
        .filter(|(expected, actual)| expected.outputs != actual.outputs)
        .map(|(expected, _)| expected.inputs.clone())
        .collect();
    MacroSteadyStateReport {
        state: if comparison.comparable && comparison.differing_bits == 0 {
            ContextualVerificationState::Passed
        } else {
            ContextualVerificationState::Failed
        },
        comparison: Some(comparison),
        input_mapping,
        output_mapping,
        differing_assignments,
        reason: None,
    }
}

fn boundary_terminal_mapping(
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

fn unavailable_steady(reason: &str) -> MacroSteadyStateReport {
    MacroSteadyStateReport {
        state: ContextualVerificationState::Pending,
        comparison: None,
        input_mapping: Vec::new(),
        output_mapping: Vec::new(),
        differing_assignments: Vec::new(),
        reason: Some(reason.into()),
    }
}

/// Exhaustively compares ordered input transitions for small Boolean cells.
/// Output samples include tick zero after the simultaneous input update.
#[must_use]
pub fn verify_macro_transitions(
    expected: &InferredTruthTable,
    original: &World,
    candidate: &World,
    settle_ticks: usize,
    observe_ticks: usize,
    max_inputs: usize,
) -> MacroTransitionReport {
    let original_context = match prepare_transition_context(original, expected, settle_ticks) {
        Ok(context) => context,
        Err(reason) => return unavailable_transitions(reason),
    };
    let candidate_context = MacroTransitionContext {
        world: candidate.clone(),
        drivers: original_context.drivers.clone(),
        outputs: original_context.outputs.clone(),
    };
    compare_transition_contexts(
        expected.inputs.len(),
        original_context,
        candidate_context,
        settle_ticks,
        observe_ticks,
        max_inputs,
    )
}

/// Compares independently inferred interfaces by terminal order. This is for
/// optimizations where an inferred terminal anchor may move even though the
/// Boolean interface remains comparable.
#[must_use]
pub fn verify_world_transitions(
    original: &World,
    original_truth: &InferredTruthTable,
    candidate: &World,
    candidate_truth: &InferredTruthTable,
    settle_ticks: usize,
    observe_ticks: usize,
    max_inputs: usize,
) -> MacroTransitionReport {
    if original_truth.inputs.len() != candidate_truth.inputs.len()
        || original_truth.outputs.len() != candidate_truth.outputs.len()
    {
        return unavailable_transitions(
            "original and candidate terminal counts are not comparable".to_owned(),
        );
    }
    let original_context = match prepare_transition_context(original, original_truth, settle_ticks)
    {
        Ok(context) => context,
        Err(reason) => return unavailable_transitions(reason),
    };
    let candidate_context =
        match prepare_transition_context(candidate, candidate_truth, settle_ticks) {
            Ok(context) => context,
            Err(reason) => return unavailable_transitions(reason),
        };
    compare_transition_contexts(
        original_truth.inputs.len(),
        original_context,
        candidate_context,
        settle_ticks,
        observe_ticks,
        max_inputs,
    )
}

fn compare_transition_contexts(
    input_count: usize,
    original_context: MacroTransitionContext,
    candidate_context: MacroTransitionContext,
    settle_ticks: usize,
    observe_ticks: usize,
    max_inputs: usize,
) -> MacroTransitionReport {
    if input_count > max_inputs || input_count >= usize::BITS as usize {
        return unavailable_transitions(format!(
            "cannot exhaustively enumerate {input_count} transition inputs"
        ));
    }
    let states = 1_usize << input_count;
    let original_states = match settled_transition_states(&original_context, states, settle_ticks) {
        Ok(states) => states,
        Err(reason) => return unavailable_transitions(reason),
    };
    let candidate_states = match settled_transition_states(&candidate_context, states, settle_ticks)
    {
        Ok(states) => states,
        Err(reason) => return unavailable_transitions(reason),
    };
    let mut cases = Vec::new();
    for from_bits in 0..states {
        for to_bits in 0..states {
            if from_bits == to_bits {
                continue;
            }
            let from = bits(from_bits, input_count);
            let to = bits(to_bits, input_count);
            let original_outputs = match simulate_boundary_transition(
                &original_context,
                &original_states[from_bits],
                &to,
                observe_ticks,
            ) {
                Ok(trace) => trace,
                Err(reason) => return unavailable_transitions(reason),
            };
            let candidate_outputs = match simulate_boundary_transition(
                &candidate_context,
                &candidate_states[from_bits],
                &to,
                observe_ticks,
            ) {
                Ok(trace) => trace,
                Err(reason) => return unavailable_transitions(reason),
            };
            let first_difference_tick = original_outputs
                .iter()
                .zip(&candidate_outputs)
                .position(|(original, candidate)| original != candidate);
            cases.push(MacroTransitionCase {
                from,
                to,
                equivalent: first_difference_tick.is_none()
                    && original_outputs.len() == candidate_outputs.len(),
                first_difference_tick,
                original_outputs,
                candidate_outputs,
            });
        }
    }
    let differing_cases = cases.iter().filter(|case| !case.equivalent).count();
    MacroTransitionReport {
        state: if differing_cases == 0 {
            ContextualVerificationState::Passed
        } else {
            ContextualVerificationState::Failed
        },
        cases,
        differing_cases,
        reason: None,
    }
}

struct MacroTransitionContext {
    world: World,
    drivers: Vec<dustroute_translate::InferredInputDriver>,
    outputs: Vec<Pos>,
}

fn prepare_transition_context(
    world: &World,
    expected: &InferredTruthTable,
    settle_ticks: usize,
) -> Result<MacroTransitionContext, String> {
    let (low, high) = world
        .bounds()
        .ok_or_else(|| "transition world is empty".to_owned())?;
    let analysis = dustroute_translate::analyze_world_region(world, RegionBounds::new(low, high));
    let inferred = dustroute_translate::infer_truth_table(
        world,
        &analysis,
        expected.inputs.len(),
        settle_ticks,
    )
    .map_err(|error| error.to_string())?;
    let mapping = boundary_terminal_mapping(
        expected.inputs.iter().map(|terminal| terminal.anchor),
        &inferred,
        &analysis,
        true,
    )
    .ok_or_else(|| "transition input boundary mapping is ambiguous".to_owned())?;
    let drivers = mapping
        .iter()
        .map(|index| {
            dustroute_translate::inferred_input_driver(world, &analysis, &inferred.inputs[*index])
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MacroTransitionContext {
        world: world.clone(),
        drivers,
        outputs: expected
            .outputs
            .iter()
            .map(|terminal| terminal.anchor)
            .collect(),
    })
}

fn simulate_boundary_transition(
    context: &MacroTransitionContext,
    settled: &dustroute_translate::RedstoneTickSimulator,
    to: &[bool],
    observe_ticks: usize,
) -> Result<Vec<Vec<bool>>, String> {
    let mut simulator = settled.clone();
    for (driver, powered) in context.drivers.iter().zip(to) {
        match driver {
            dustroute_translate::InferredInputDriver::Lever(pos) => simulator
                .set_powered(*pos, *powered)
                .map_err(|error| error.to_string())?,
            dustroute_translate::InferredInputDriver::External(pos) => simulator
                .set_external_powered(*pos, *powered)
                .map_err(|error| error.to_string())?,
        };
    }
    let observe = |state: &dustroute_translate::TickState| {
        context
            .outputs
            .iter()
            .map(|position| state.powered(*position))
            .collect::<Vec<_>>()
    };
    let mut trace = vec![observe(&simulator.snapshot())];
    for _ in 0..observe_ticks {
        let state = simulator
            .advance_tick()
            .map_err(|error| error.to_string())?;
        trace.push(observe(&state));
    }
    Ok(trace)
}

fn settled_transition_states(
    context: &MacroTransitionContext,
    state_count: usize,
    settle_ticks: usize,
) -> Result<Vec<dustroute_translate::RedstoneTickSimulator>, String> {
    (0..state_count)
        .map(|value| {
            let mut driven = context.world.clone();
            for (driver, powered) in context
                .drivers
                .iter()
                .zip(bits(value, context.drivers.len()))
            {
                set_driver_in_world(&mut driven, *driver, powered);
            }
            dustroute_translate::update_wire_shapes(&mut driven);
            let mut simulator = dustroute_translate::RedstoneTickSimulator::new(driven)
                .map_err(|error| error.to_string())?;
            simulator
                .settle_ticks(settle_ticks)
                .map_err(|error| error.to_string())?;
            Ok(simulator)
        })
        .collect()
}

fn set_driver_in_world(
    world: &mut World,
    driver: dustroute_translate::InferredInputDriver,
    powered: bool,
) {
    match driver {
        dustroute_translate::InferredInputDriver::Lever(pos) => {
            if let Some(mut block) = world.get(pos).cloned() {
                block.powered = Some(powered);
                world.set(pos, block);
            }
        }
        dustroute_translate::InferredInputDriver::External(pos) if powered => {
            world.set(pos, Block::new(BlockKind::RedstoneBlock));
        }
        dustroute_translate::InferredInputDriver::External(pos) => {
            world.remove(pos);
        }
    }
}

fn bits(value: usize, count: usize) -> Vec<bool> {
    (0..count).map(|index| value & (1 << index) != 0).collect()
}

fn unavailable_transitions(reason: String) -> MacroTransitionReport {
    MacroTransitionReport {
        state: ContextualVerificationState::Pending,
        cases: Vec::new(),
        differing_cases: 0,
        reason: Some(reason),
    }
}

fn repeater_sites(
    path: &[Pos],
    max_wire_run: usize,
) -> Option<std::collections::BTreeMap<usize, Facing>> {
    if max_wire_run == 0 {
        return None;
    }
    let mut result = std::collections::BTreeMap::new();
    let mut last_refresh = 0;
    while path.len().saturating_sub(1).saturating_sub(last_refresh) > max_wire_run {
        let upper = (last_refresh + max_wire_run).min(path.len().saturating_sub(2));
        let site = (last_refresh + 1..=upper).rev().find_map(|index| {
            facing_between(path[index - 1], path[index])
                .filter(|facing| facing_between(path[index], path[index + 1]) == Some(*facing))
                .map(|facing| (index, facing))
        })?;
        result.insert(site.0, site.1);
        last_refresh = site.0;
    }
    Some(result)
}

fn facing_between(from: Pos, to: Pos) -> Option<Facing> {
    match (to.x - from.x, to.y - from.y, to.z - from.z) {
        (1, 0, 0) => Some(Facing::East),
        (-1, 0, 0) => Some(Facing::West),
        (0, 0, 1) => Some(Facing::South),
        (0, 0, -1) => Some(Facing::North),
        _ => None,
    }
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
    reserved: &BTreeSet<Pos>,
) -> Result<Vec<MacroPortRoute>, MacroRealizationError> {
    let candidate_blocks = placed.blocks().map(|(pos, _)| pos).collect::<BTreeSet<_>>();
    let alternatives = mappings
        .iter()
        .map(|(boundary, name)| {
            let (candidate_position, facing) = match boundary.direction {
                MacroBoundaryDirection::Input => {
                    placed.input_port(name).map(|port| (port.pos, port.facing))
                }
                MacroBoundaryDirection::Output => {
                    placed.output_port(name).map(|port| (port.pos, port.facing))
                }
            }
            .ok_or_else(|| MacroRealizationError::MissingCandidatePort((*name).into()))?;
            if candidate_position == boundary.position {
                if boundary
                    .facing
                    .is_some_and(|expected| facing != Some(expected))
                {
                    return Ok(Vec::new());
                }
                return Ok(vec![MacroPortRoute {
                    boundary: (*boundary).clone(),
                    candidate_port: (*name).into(),
                    candidate_position,
                    path: vec![candidate_position],
                }]);
            }
            let route_start = facing
                .and_then(facing_offset)
                .map_or(candidate_position, |delta| {
                    candidate_position.offset(delta.x, delta.y, delta.z)
                });
            let mut paths = [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ]
            .into_iter()
            .map(|order| manhattan_path_order(route_start, boundary.position, order))
            .collect::<BTreeSet<_>>();
            for margin in [2, 4, 6, 8] {
                for sign in [-1, 1] {
                    let detour_z = if sign < 0 {
                        candidate_position.z.min(boundary.position.z) - margin
                    } else {
                        candidate_position.z.max(boundary.position.z) + margin
                    };
                    paths.insert(path_via(
                        route_start,
                        [
                            Pos::new(route_start.x, route_start.y, detour_z),
                            Pos::new(boundary.position.x, route_start.y, detour_z),
                            Pos::new(boundary.position.x, boundary.position.y, detour_z),
                        ],
                        boundary.position,
                    ));
                    let detour_x = if sign < 0 {
                        candidate_position.x.min(boundary.position.x) - margin
                    } else {
                        candidate_position.x.max(boundary.position.x) + margin
                    };
                    paths.insert(path_via(
                        route_start,
                        [
                            Pos::new(detour_x, route_start.y, route_start.z),
                            Pos::new(detour_x, route_start.y, boundary.position.z),
                            Pos::new(detour_x, boundary.position.y, boundary.position.z),
                        ],
                        boundary.position,
                    ));
                }
            }
            let mut paths = paths
                .into_iter()
                .map(|path| {
                    if route_start == candidate_position {
                        path
                    } else {
                        std::iter::once(candidate_position).chain(path).collect()
                    }
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(|path| MacroPortRoute {
                    boundary: (*boundary).clone(),
                    candidate_port: (*name).into(),
                    candidate_position,
                    path,
                })
                .filter(|route| {
                    route
                        .path
                        .iter()
                        .skip(1)
                        .take(route.path.len().saturating_sub(2))
                        .all(|pos| !reserved.contains(pos))
                })
                .collect::<Vec<_>>();
            paths.sort_by_key(|route| {
                (
                    route
                        .path
                        .iter()
                        .skip(1)
                        .take(route.path.len().saturating_sub(2))
                        .filter(|pos| candidate_blocks.contains(pos))
                        .count(),
                    route.path.len(),
                    route.path.clone(),
                )
            });
            Ok(paths)
        })
        .collect::<Result<Vec<_>, MacroRealizationError>>()?;
    let mut selected = Vec::new();
    select_non_contacting_routes(&alternatives, 0, &mut selected)
        .then_some(selected)
        .ok_or(MacroRealizationError::NoPlacement)
}

fn facing_offset(facing: Facing) -> Option<Pos> {
    match facing {
        Facing::North => Some(Pos::new(0, 0, -1)),
        Facing::East => Some(Pos::new(1, 0, 0)),
        Facing::South => Some(Pos::new(0, 0, 1)),
        Facing::West => Some(Pos::new(-1, 0, 0)),
        Facing::Up | Facing::Down => None,
    }
}

fn manhattan_path(from: Pos, to: Pos) -> Vec<Pos> {
    manhattan_path_order(from, to, [0, 1, 2])
}

fn manhattan_path_order(from: Pos, to: Pos, order: [usize; 3]) -> Vec<Pos> {
    let mut result = vec![from];
    let mut cursor = from;
    for axis in order {
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

fn select_non_contacting_routes(
    alternatives: &[Vec<MacroPortRoute>],
    index: usize,
    selected: &mut Vec<MacroPortRoute>,
) -> bool {
    if index == alternatives.len() {
        return true;
    }
    for candidate in &alternatives[index] {
        if selected
            .iter()
            .any(|existing| routes_make_contact(&existing.path, &candidate.path))
        {
            continue;
        }
        selected.push(candidate.clone());
        if select_non_contacting_routes(alternatives, index + 1, selected) {
            return true;
        }
        selected.pop();
    }
    false
}

fn routes_make_contact(first: &[Pos], second: &[Pos]) -> bool {
    first.iter().enumerate().any(|(first_index, a)| {
        second.iter().enumerate().any(|(second_index, b)| {
            if a == b {
                return true;
            }
            let first_internal = first_index > 0 && first_index + 1 < first.len();
            let second_internal = second_index > 0 && second_index + 1 < second.len();
            first_internal
                && second_internal
                && (a.x - b.x).abs() + (a.z - b.z).abs() == 1
                && (a.y - b.y).abs() <= 1
        })
    })
}

fn path_via<const N: usize>(from: Pos, waypoints: [Pos; N], to: Pos) -> Vec<Pos> {
    let mut path = vec![from];
    let mut cursor = from;
    for target in waypoints.into_iter().chain([to]) {
        let segment = manhattan_path(cursor, target);
        path.extend(segment.into_iter().skip(1));
        cursor = target;
    }
    path
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

        let boundary = extract_model_boundary_with_context(&model, &baseline.world, &analysis);
        let reserved = boundary
            .iter()
            .filter_map(|port| port.driver_position)
            .collect();
        let plan = plan_macro_replacement_with_reserved(&candidate, &boundary, &reserved).unwrap();

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

        let boundary_positions = boundary
            .iter()
            .map(|port| port.position)
            .collect::<BTreeSet<_>>();
        let replaceable = baseline
            .world
            .positions()
            .filter(|pos| !boundary_positions.contains(pos))
            .collect();
        let report = validate_macro_structure(&plan, &baseline.world, &replaceable);
        assert!(report.candidate_collisions.is_empty());
        assert!(report.blocked_route_supports.is_empty());
        let materialized =
            materialize_macro_replacement(&plan, &baseline.world, &replaceable, 14).unwrap();
        let steady = verify_macro_steady_state(
            &model.truth_table,
            &baseline.world,
            &materialized.world,
            8,
            64,
        );
        assert_eq!(
            steady.state,
            ContextualVerificationState::Passed,
            "{steady:#?}"
        );
        assert_eq!(steady.comparison.as_ref().unwrap().differing_rows, 0);
        assert!(steady.differing_assignments.is_empty());
        let transitions = verify_macro_transitions(
            &model.truth_table,
            &baseline.world,
            &materialized.world,
            64,
            16,
            4,
        );
        assert_eq!(transitions.state, ContextualVerificationState::Failed);
        assert_eq!(transitions.differing_cases, 12);
        assert!(
            transitions
                .cases
                .iter()
                .all(|case| { case.original_outputs.last() == case.candidate_outputs.last() })
        );
        let swap = transitions
            .cases
            .iter()
            .find(|case| case.from == [true, false] && case.to == [false, true])
            .unwrap();
        assert_eq!(swap.first_difference_tick, Some(10));
        assert_eq!(
            swap.original_outputs
                .iter()
                .filter(|value| value == &&vec![false])
                .count(),
            1
        );
        assert_eq!(
            swap.candidate_outputs
                .iter()
                .filter(|value| value == &&vec![false])
                .count(),
            2
        );
    }

    #[test]
    fn structural_validation_rejects_immutable_obstacles_and_cross_net_contacts() {
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
        let mut plan =
            plan_macro_replacement(&candidate, &extract_cell_boundary(&baseline)).unwrap();
        let (collision, candidate_block) = plan.placed.blocks().next().unwrap();
        let mut observed = World::new();
        let obstacle = if candidate_block.kind == BlockKind::Solid {
            BlockKind::Transparent
        } else {
            BlockKind::Solid
        };
        observed.set(collision, dustroute_physical::Block::new(obstacle));
        plan.routes[1].path = plan.routes[0].path.clone();

        let report = validate_macro_structure(&plan, &observed, &BTreeSet::new());
        assert!(report.candidate_collisions.contains(&collision));
        assert!(!report.route_cross_net_contacts.is_empty());
    }

    #[test]
    fn materializes_supported_wire_refreshes_strength_and_round_trips_patch() {
        let placed = PlacedCell {
            cell: dustroute_translate::terminal_cell("source"),
            origin: Pos::new(0, 0, 0),
            rotation: RotationY::R0,
        };
        let boundary = MacroBoundaryPort {
            observed_index: 0,
            name: "out".into(),
            position: Pos::new(20, 1, 0),
            direction: MacroBoundaryDirection::Output,
            facing: None,
            driver_position: None,
        };
        let plan = MacroReplacementPlan {
            component_id: "test.long-route".into(),
            placed,
            routes: vec![MacroPortRoute {
                boundary: boundary.clone(),
                candidate_port: "out".into(),
                candidate_position: Pos::new(0, 1, 0),
                path: manhattan_path(Pos::new(0, 1, 0), boundary.position),
            }],
            verification: MacroRealizationVerification {
                structural: ContextualVerificationState::Pending,
                steady_state: ContextualVerificationState::Pending,
                transitions: ContextualVerificationState::Pending,
            },
            automatic_apply_allowed: false,
            total_route_length: 20,
        };
        let mut observed = World::new();
        observed.set(Pos::new(20, 0, 0), Block::new(BlockKind::Solid));
        observed.place(BlockKind::RedstoneWire, boundary.position);

        let result = materialize_macro_replacement(&plan, &observed, &BTreeSet::new(), 14).unwrap();

        assert_eq!(result.inserted_repeaters, [Pos::new(14, 1, 0)]);
        assert_eq!(
            result.world.kind_at(Pos::new(14, 1, 0)),
            BlockKind::Repeater
        );
        assert!(result.world.support_issues().is_empty());
        let restored = result.patch.inverse().apply_virtual(&result.world).unwrap();
        assert_eq!(restored, observed);
    }

    #[test]
    fn steady_state_verification_rebinds_terminals_by_boundary_component() {
        let cell = dustroute_translate::compact_compiled_xor_cell().unwrap();
        let (low, high) = cell.world.bounds().unwrap();
        let analysis = analyze_world_region(&cell.world, RegionBounds::new(low, high));
        let expected = derive_functional_network(&cell.world, &analysis, 8, 64)
            .unwrap()
            .truth_table;

        let report = verify_macro_steady_state(&expected, &cell.world, &cell.world, 8, 64);

        assert_eq!(report.state, ContextualVerificationState::Passed);
        assert_eq!(report.comparison.unwrap().differing_bits, 0);
        assert_eq!(report.input_mapping.len(), 2);
        assert_eq!(report.output_mapping.len(), 1);
    }

    #[test]
    fn transition_verification_covers_single_and_multi_input_changes() {
        let cell = dustroute_translate::compact_compiled_xor_cell().unwrap();
        let (low, high) = cell.world.bounds().unwrap();
        let analysis = analyze_world_region(&cell.world, RegionBounds::new(low, high));
        let expected = derive_functional_network(&cell.world, &analysis, 8, 64)
            .unwrap()
            .truth_table;

        let report = verify_macro_transitions(&expected, &cell.world, &cell.world, 64, 16, 4);

        assert_eq!(report.state, ContextualVerificationState::Passed);
        assert_eq!(report.cases.len(), 12);
        assert_eq!(report.differing_cases, 0);
        assert!(
            report
                .cases
                .iter()
                .any(|case| { case.from == [true, false] && case.to == [false, true] })
        );
    }
}
