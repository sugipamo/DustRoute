use std::collections::BTreeMap;

use dustroute_translate::cell_library::{CellLibrary, default_cell_library, verify_cell};
use dustroute_translate::cells::RotationY;
use dustroute_translate::logic::GateKind;
use dustroute_translate::physical::{CellId, Endpoint, PhysicalNode, PlacementCircuit, Route};
use dustroute_translate::world::Pos;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementWeights {
    pub wire_distance: f64,
    pub bounding_volume: f64,
    pub cell_block_count: f64,
    pub overlap_penalty: f64,
}
impl Default for PlacementWeights {
    fn default() -> Self {
        Self {
            wire_distance: 1.0,
            bounding_volume: 0.002,
            cell_block_count: 0.05,
            overlap_penalty: 1_000_000.0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementScore {
    pub total: f64,
    pub wire_distance: i32,
    pub bounding_volume: i32,
    pub cell_block_count: usize,
    pub overlaps: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationKind {
    Move,
    MovePair,
    Rotate,
    ReplaceCell,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementMutation {
    pub kind: MutationKind,
    pub cell_id: CellId,
    pub delta: Pos,
    pub rotation: Option<RotationY>,
    pub candidate_name: Option<String>,
    pub companion_move: Option<(CellId, Pos)>,
}
#[derive(Clone, Debug)]
pub struct PlacementOptimizationResult {
    pub circuit: PlacementCircuit,
    pub initial_score: PlacementScore,
    pub final_score: PlacementScore,
    pub accepted: Vec<PlacementMutation>,
}

fn refresh_endpoint(pc: &PlacementCircuit, ep: &Endpoint, output: bool) -> Endpoint {
    if let Some(id) = ep.cell {
        if output {
            pc.output_endpoint(id, &ep.port).unwrap()
        } else {
            pc.input_endpoint(id, &ep.port).unwrap()
        }
    } else {
        ep.clone()
    }
}
pub fn refresh_route_endpoints(pc: &mut PlacementCircuit) {
    let old = pc.routes.clone();
    pc.routes = old
        .into_iter()
        .map(|(id, r)| {
            (
                id,
                Route {
                    id,
                    source: refresh_endpoint(pc, &r.source, true),
                    sink: refresh_endpoint(pc, &r.sink, false),
                    path: vec![],
                    repeaters: vec![],
                },
            )
        })
        .collect();
    let old_terminals = pc.terminals.clone();
    pc.terminals = old_terminals
        .into_iter()
        .map(|(name, mut terminal)| {
            terminal.endpoint = refresh_endpoint(
                pc,
                &terminal.endpoint,
                terminal.direction == dustroute_translate::TerminalDirection::Output,
            );
            (name, terminal)
        })
        .collect();
}

#[must_use]
pub fn placement_score(pc: &PlacementCircuit, w: PlacementWeights) -> PlacementScore {
    let wire_distance = pc
        .routes
        .values()
        .map(|r| {
            (r.source.pos.x - r.sink.pos.x).abs()
                + (r.source.pos.y - r.sink.pos.y).abs()
                + (r.source.pos.z - r.sink.pos.z).abs()
        })
        .sum();
    let mut counts: BTreeMap<Pos, usize> = BTreeMap::new();
    for n in pc.cells.values() {
        for (p, _) in n.placed.blocks() {
            *counts.entry(p).or_default() += 1;
        }
    }
    let cell_block_count = counts.values().sum();
    let overlaps = counts.values().map(|n| n.saturating_sub(1)).sum();
    let bounding_volume = if counts.is_empty() {
        0
    } else {
        let xs = counts.keys().map(|p| p.x);
        let ys = counts.keys().map(|p| p.y);
        let zs = counts.keys().map(|p| p.z);
        (xs.clone().max().unwrap() - xs.min().unwrap() + 1)
            * (ys.clone().max().unwrap() - ys.min().unwrap() + 1)
            * (zs.clone().max().unwrap() - zs.min().unwrap() + 1)
    };
    PlacementScore {
        total: w.wire_distance * f64::from(wire_distance)
            + w.bounding_volume * f64::from(bounding_volume)
            + w.cell_block_count * cell_block_count as f64
            + w.overlap_penalty * overlaps as f64,
        wire_distance,
        bounding_volume,
        cell_block_count,
        overlaps,
    }
}

/// Counts cross-cell block pairs inside a one-block electrical keep-out.
/// This is deliberately conservative: exact redstone behavior is checked by
/// realization, while this inexpensive metric prevents geometry scoring from
/// preferring newly adjacent cell footprints in the first place.
pub(crate) fn electrical_keepout_contacts(pc: &PlacementCircuit) -> usize {
    let blocks = pc
        .cells
        .iter()
        .flat_map(|(cell, node)| {
            node.placed
                .blocks()
                .map(move |(position, block)| (*cell, position, block.kind))
        })
        .collect::<Vec<_>>();
    let mut contacts = 0;
    for (index, (first_cell, first_pos, first_kind)) in blocks.iter().enumerate() {
        for (second_cell, second_pos, second_kind) in &blocks[index + 1..] {
            if first_cell == second_cell
                || (!first_kind.is_redstone_related() && !second_kind.is_redstone_related())
            {
                continue;
            }
            let distance = first_pos.x.abs_diff(second_pos.x)
                + first_pos.y.abs_diff(second_pos.y)
                + first_pos.z.abs_diff(second_pos.z);
            if distance <= 1 {
                contacts += 1;
            }
        }
    }
    contacts
}
pub fn apply_mutation(
    pc: &PlacementCircuit,
    m: &PlacementMutation,
    lib: &CellLibrary,
) -> PlacementCircuit {
    let mut out = pc.clone();
    let node = &out.cells[&m.cell_id];
    let mut placed = node.placed.clone();
    match m.kind {
        MutationKind::Move | MutationKind::MovePair => {
            placed.origin = placed.origin.offset(m.delta.x, m.delta.y, m.delta.z);
        }
        MutationKind::Rotate => {
            placed.rotation = m.rotation.unwrap();
            placed.origin = placed.origin.offset(m.delta.x, m.delta.y, m.delta.z);
        }
        MutationKind::ReplaceCell => {
            placed.cell = lib
                .candidates_for(node.logical_kind)
                .iter()
                .find(|c| Some(c.name.as_str()) == m.candidate_name.as_deref())
                .unwrap()
                .clone()
        }
    };
    out.cells.insert(
        m.cell_id,
        PhysicalNode {
            id: m.cell_id,
            logical_kind: node.logical_kind,
            placed,
        },
    );
    if let Some((companion_id, delta)) = m.companion_move {
        let companion = out.cells.get_mut(&companion_id).unwrap();
        companion.placed.origin = companion.placed.origin.offset(delta.x, delta.y, delta.z);
    }
    refresh_route_endpoints(&mut out);
    out
}
#[must_use]
pub fn candidate_mutations(
    pc: &PlacementCircuit,
    lib: &CellLibrary,
    step: i32,
) -> Vec<PlacementMutation> {
    let mut out = vec![];
    for (&id, n) in &pc.cells {
        if !matches!(
            n.logical_kind,
            GateKind::Not | GateKind::And | GateKind::Or | GateKind::Xor | GateKind::Nand
        ) {
            continue;
        }
        for d in [
            Pos::new(step, 0, 0),
            Pos::new(-step, 0, 0),
            Pos::new(0, 0, step),
            Pos::new(0, 0, -step),
            Pos::new(step, 0, step),
            Pos::new(step, 0, -step),
            Pos::new(-step, 0, step),
            Pos::new(-step, 0, -step),
        ] {
            out.push(PlacementMutation {
                kind: MutationKind::Move,
                cell_id: id,
                delta: d,
                rotation: None,
                candidate_name: None,
                companion_move: None,
            });
        }
        let local_positions = n.placed.cell.world.positions().collect::<Vec<_>>();
        let center_sum = |rotation: RotationY| {
            let rotated = local_positions
                .iter()
                .map(|position| rotation.pos(*position))
                .collect::<Vec<_>>();
            let min_x = rotated.iter().map(|position| position.x).min().unwrap_or(0);
            let max_x = rotated.iter().map(|position| position.x).max().unwrap_or(0);
            let min_z = rotated.iter().map(|position| position.z).min().unwrap_or(0);
            let max_z = rotated.iter().map(|position| position.z).max().unwrap_or(0);
            (min_x + max_x, min_z + max_z)
        };
        let old_center = center_sum(n.placed.rotation);
        for rotation in [
            RotationY::R0,
            RotationY::R90,
            RotationY::R180,
            RotationY::R270,
        ] {
            if rotation != n.placed.rotation {
                let new_center = center_sum(rotation);
                let difference = (old_center.0 - new_center.0, old_center.1 - new_center.1);
                let x_floor = difference.0.div_euclid(2);
                let z_floor = difference.1.div_euclid(2);
                let x_offsets = [x_floor, x_floor + difference.0.rem_euclid(2)];
                let z_offsets = [z_floor, z_floor + difference.1.rem_euclid(2)];
                let mut deltas = Vec::new();
                for x in x_offsets {
                    for z in z_offsets {
                        let delta = Pos::new(x, 0, z);
                        if !deltas.contains(&delta) {
                            deltas.push(delta);
                        }
                    }
                }
                for delta in deltas {
                    out.push(PlacementMutation {
                        kind: MutationKind::Rotate,
                        cell_id: id,
                        delta,
                        rotation: Some(rotation),
                        candidate_name: None,
                        companion_move: None,
                    });
                }
            }
        }
        for c in lib.candidates_for(n.logical_kind) {
            if c.name != n.placed.cell.name && verify_cell(n.logical_kind, c).valid {
                out.push(PlacementMutation {
                    kind: MutationKind::ReplaceCell,
                    cell_id: id,
                    delta: Pos::new(0, 0, 0),
                    rotation: None,
                    candidate_name: Some(c.name.clone()),
                    companion_move: None,
                });
            }
        }
    }
    let connected_pairs = pc
        .routes
        .values()
        .filter_map(|route| Some((route.source.cell?, route.sink.cell?)))
        .map(|(first, second)| {
            if first < second {
                (first, second)
            } else {
                (second, first)
            }
        })
        .collect::<std::collections::BTreeSet<_>>();
    for (first, second) in connected_pairs {
        let first_origin = pc.cells[&first].placed.origin;
        let second_origin = pc.cells[&second].placed.origin;
        let toward = |difference: i32| difference.signum() * step;
        let first_delta = Pos::new(
            toward(second_origin.x - first_origin.x),
            0,
            toward(second_origin.z - first_origin.z),
        );
        let second_delta = Pos::new(-first_delta.x, 0, -first_delta.z);
        if first_delta != Pos::new(0, 0, 0) {
            out.push(PlacementMutation {
                kind: MutationKind::MovePair,
                cell_id: first,
                delta: first_delta,
                rotation: None,
                candidate_name: None,
                companion_move: Some((second, second_delta)),
            });
        }
    }
    out
}
#[must_use]
pub fn optimize_placement(
    pc: &PlacementCircuit,
    max_steps: usize,
    move_step: i32,
) -> PlacementOptimizationResult {
    let lib = default_cell_library();
    let w = PlacementWeights::default();
    let mut current = pc.clone();
    refresh_route_endpoints(&mut current);
    let initial = placement_score(&current, w);
    let mut score = initial;
    let mut accepted = vec![];
    for _ in 0..max_steps {
        let mut best = None;
        for m in candidate_mutations(&current, &lib, move_step) {
            let c = apply_mutation(&current, &m, &lib);
            let s = placement_score(&c, w);
            if s.total < score.total
                && best
                    .as_ref()
                    .is_none_or(|(_, _, bs): &(_, _, PlacementScore)| s.total < bs.total)
            {
                best = Some((m, c, s));
            }
        }
        if let Some((m, c, s)) = best {
            accepted.push(m);
            current = c;
            score = s
        } else {
            break;
        }
    }
    PlacementOptimizationResult {
        circuit: current,
        initial_score: initial,
        final_score: score,
        accepted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_translate::cells::{PlacedCell, PortKind, not_cell};
    #[test]
    fn optimizer_moves_cell_toward_nets() {
        let mut pc = PlacementCircuit::new();
        let id = pc.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(10, 2, 12),
                rotation: RotationY::R0,
            },
        );
        let src = PlacementCircuit::boundary("src", Pos::new(0, 2, 0), PortKind::Wire, None);
        let dst = PlacementCircuit::boundary("dst", Pos::new(22, 2, 0), PortKind::Wire, None);
        pc.add_route(src, pc.input_endpoint(id, "a").unwrap(), vec![], vec![]);
        pc.add_route(pc.output_endpoint(id, "out").unwrap(), dst, vec![], vec![]);
        let result = optimize_placement(&pc, 20, 2);
        assert!(result.final_score.total < result.initial_score.total);
        assert_eq!(result.circuit.cells[&id].placed.origin.z, 0);
    }
    #[test]
    fn replacement_candidates_are_verified() {
        let mut pc = PlacementCircuit::new();
        let id = pc.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(0, 0, 0),
                rotation: RotationY::R0,
            },
        );
        let lib = default_cell_library();
        let m = candidate_mutations(&pc, &lib, 2)
            .into_iter()
            .find(|m| m.candidate_name.as_deref() == Some("not_torch_top"))
            .unwrap();
        assert_eq!(
            apply_mutation(&pc, &m, &lib).cells[&id].placed.cell.name,
            "not_torch_top"
        );
    }

    #[test]
    fn rotations_are_proposed_for_guarded_validation() {
        let mut pc = PlacementCircuit::new();
        pc.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(0, 0, 0),
                rotation: RotationY::R0,
            },
        );
        let mutations = candidate_mutations(&pc, &default_cell_library(), 1);
        assert!(
            mutations
                .iter()
                .filter(|mutation| mutation.kind == MutationKind::Rotate)
                .count()
                >= 3
        );
        assert!(mutations.iter().any(|mutation| {
            mutation.kind == MutationKind::Rotate && mutation.delta != Pos::new(0, 0, 0)
        }));
    }

    #[test]
    fn electrical_keepout_detects_new_cross_cell_adjacency() {
        let mut pc = PlacementCircuit::new();
        let first = pc.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(0, 0, 0),
                rotation: RotationY::R0,
            },
        );
        let second = pc.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(8, 0, 0),
                rotation: RotationY::R0,
            },
        );
        assert_eq!(electrical_keepout_contacts(&pc), 0);
        pc.cells.get_mut(&second).unwrap().placed.origin = Pos::new(3, 0, 0);
        assert!(electrical_keepout_contacts(&pc) > 0);
        assert_ne!(first, second);
    }

    #[test]
    fn connected_cells_receive_an_atomic_pair_compression_candidate() {
        let mut pc = PlacementCircuit::new();
        let first = pc.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(0, 0, 0),
                rotation: RotationY::R0,
            },
        );
        let second = pc.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(12, 0, 4),
                rotation: RotationY::R0,
            },
        );
        pc.add_route(
            pc.output_endpoint(first, "out").unwrap(),
            pc.input_endpoint(second, "a").unwrap(),
            vec![],
            vec![],
        );
        let library = default_cell_library();
        let mutation = candidate_mutations(&pc, &library, 1)
            .into_iter()
            .find(|mutation| mutation.kind == MutationKind::MovePair)
            .unwrap();
        let candidate = apply_mutation(&pc, &mutation, &library);
        assert_eq!(candidate.cells[&first].placed.origin, Pos::new(1, 0, 1));
        assert_eq!(candidate.cells[&second].placed.origin, Pos::new(11, 0, 3));
    }
}
