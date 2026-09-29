//! Bounded route geometry and repeater sites.
use super::{MacroBoundaryDirection, MacroBoundaryPort, MacroPortRoute, MacroRealizationError};
use dustroute_physical::{Facing, Pos};
use dustroute_translate::{PhysicalCell, PlacedCell};
use std::collections::BTreeSet;

pub(super) fn repeater_sites(
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

pub(super) fn facing_between(from: Pos, to: Pos) -> Option<Facing> {
    match (to.x - from.x, to.y - from.y, to.z - from.z) {
        (1, 0, 0) => Some(Facing::East),
        (-1, 0, 0) => Some(Facing::West),
        (0, 0, 1) => Some(Facing::South),
        (0, 0, -1) => Some(Facing::North),
        _ => None,
    }
}

pub(super) fn mapped_boundary<'a>(
    input_ports: &'a [String],
    output_ports: &'a [String],
    boundary: &'a [MacroBoundaryPort],
) -> Result<Vec<(&'a MacroBoundaryPort, &'a str)>, MacroRealizationError> {
    let mut result = Vec::new();
    for (direction, names) in [
        (MacroBoundaryDirection::Input, input_ports),
        (MacroBoundaryDirection::Output, output_ports),
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

pub(super) fn local_port(
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

pub(super) fn build_routes(
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

pub(super) fn manhattan_path(from: Pos, to: Pos) -> Vec<Pos> {
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
