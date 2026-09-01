use std::collections::BTreeSet;

use dustroute_physical::{
    Block, BlockKind, Facing, PhysicalBlockChange, PhysicalPatch, PhysicalPatchReason, Pos, World,
};
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
    for (pos, block) in &candidate_blocks {
        if observed.get(*pos).is_some_and(|actual| actual != block) && !replaceable.contains(pos) {
            report.candidate_collisions.push(*pos);
        }
    }
    let mut candidate_world = observed.clone();
    for pos in replaceable {
        candidate_world.remove(*pos);
    }
    for (pos, block) in &candidate_blocks {
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
            for first in &route.path {
                for second in &other.path {
                    let dx = (first.x - second.x).abs();
                    let dy = (first.y - second.y).abs();
                    let dz = (first.z - second.z).abs();
                    if first == second || (dx + dz == 1 && dy <= 1) {
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
        world.set(pos, block);
    }
    for support in &structural.required_route_supports {
        if world.kind_at(*support) == BlockKind::Air {
            world.set(*support, Block::new(BlockKind::Solid));
        }
    }

    let mut inserted_repeaters = Vec::new();
    for (route_index, route) in plan.routes.iter().enumerate() {
        let repeaters = repeater_sites(&route.path, max_wire_run).ok_or(
            MacroRealizationError::NoRepeaterSite {
                route: route_index,
                after_steps: max_wire_run,
            },
        )?;
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

        let replaceable = baseline.world.positions().collect();
        let report = validate_macro_structure(&plan, &baseline.world, &replaceable);
        assert!(report.candidate_collisions.is_empty());
        assert!(report.blocked_route_supports.is_empty());
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
}
