use std::collections::BTreeSet;

use dustroute_translate::cell_library::default_cell_library;
use dustroute_translate::cells::PlacedCell;
use dustroute_translate::expr::{Expr, best_by_size};
use dustroute_translate::logic::GateKind;
use dustroute_translate::physical::{CellId, Endpoint, PhysicalCircuit, RouteId};
use dustroute_translate::port_realization::terminal_for_endpoint;
use dustroute_translate::routing::{RouterConfig, astar_route};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalRegion {
    pub cells: BTreeSet<CellId>,
    pub routes: BTreeSet<RouteId>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticFragment {
    pub expr: Expr,
    pub inputs: Vec<Endpoint>,
    pub outputs: Vec<Endpoint>,
    pub region: PhysicalRegion,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticRewrite {
    pub before: SemanticFragment,
    pub after_expr: Expr,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewriteReport {
    pub rule: String,
    pub removed_cells: Vec<CellId>,
    pub removed_routes: Vec<RouteId>,
    pub added_routes: Vec<RouteId>,
}

fn boundary(
    pc: &PhysicalCircuit,
    cells: &BTreeSet<CellId>,
) -> (
    Vec<dustroute_translate::physical::Route>,
    Vec<dustroute_translate::physical::Route>,
    Vec<dustroute_translate::physical::Route>,
) {
    let mut i = vec![];
    let mut o = vec![];
    let mut m = vec![];
    for r in pc.routes.values() {
        let a = r.source.cell.is_some_and(|x| cells.contains(&x));
        let b = r.sink.cell.is_some_and(|x| cells.contains(&x));
        match (a, b) {
            (false, true) => i.push(r.clone()),
            (true, false) => o.push(r.clone()),
            (true, true) => m.push(r.clone()),
            _ => {}
        }
    }
    (i, o, m)
}

#[must_use]
pub fn extract_linear_not_chain(
    pc: &PhysicalCircuit,
    start: CellId,
    max_len: usize,
) -> Option<SemanticFragment> {
    let mut cells = BTreeSet::new();
    let mut cur = start;
    while cells.len() < max_len {
        let n = pc.cells.get(&cur)?;
        if n.logical_kind != GateKind::Not
            || pc.incoming(cur).count() != 1
            || pc.outgoing(cur).count() != 1
        {
            break;
        }
        cells.insert(cur);
        let next = pc.outgoing(cur).next()?.sink.cell;
        if next.is_none_or(|x| {
            pc.cells
                .get(&x)
                .is_none_or(|n| n.logical_kind != GateKind::Not)
                || pc.incoming(x).count() != 1
        }) {
            break;
        }
        cur = next?;
    }
    if cells.is_empty() {
        return None;
    }
    let (i, o, m) = boundary(pc, &cells);
    if i.len() != 1 || o.len() != 1 {
        return None;
    }
    let mut e = Expr::Var("in0".into());
    for _ in &cells {
        e = Expr::Not(Box::new(e));
    }
    Some(SemanticFragment {
        expr: e,
        inputs: vec![i[0].source.clone()],
        outputs: vec![o[0].sink.clone()],
        region: PhysicalRegion {
            cells,
            routes: i.into_iter().chain(o).chain(m).map(|r| r.id).collect(),
        },
    })
}

#[must_use]
pub fn extract_and_then_not(pc: &PhysicalCircuit, id: CellId) -> Option<SemanticFragment> {
    if pc.cells.get(&id)?.logical_kind != GateKind::And {
        return None;
    }
    let incoming: Vec<_> = pc.incoming(id).cloned().collect();
    let outgoing: Vec<_> = pc.outgoing(id).cloned().collect();
    if incoming.len() != 2 || outgoing.len() != 1 {
        return None;
    }
    let not_id = outgoing[0].sink.cell?;
    if pc.cells.get(&not_id)?.logical_kind != GateKind::Not
        || pc.incoming(not_id).count() != 1
        || pc.outgoing(not_id).count() != 1
    {
        return None;
    }
    let cells = BTreeSet::from([id, not_id]);
    let (i, o, m) = boundary(pc, &cells);
    Some(SemanticFragment {
        expr: Expr::Not(Box::new(Expr::And(vec![
            Expr::Var("in0".into()),
            Expr::Var("in1".into()),
        ]))),
        inputs: i.iter().map(|r| r.source.clone()).collect(),
        outputs: o.iter().map(|r| r.sink.clone()).collect(),
        region: PhysicalRegion {
            cells,
            routes: i.into_iter().chain(o).chain(m).map(|r| r.id).collect(),
        },
    })
}
#[must_use]
pub fn simplify_fragment(f: &SemanticFragment) -> Option<SemanticRewrite> {
    let e = best_by_size(&f.expr, 4, 256);
    (e != f.expr).then(|| SemanticRewrite {
        before: f.clone(),
        after_expr: e,
    })
}

pub fn realize_identity_rewrite(
    pc: &mut PhysicalCircuit,
    r: &SemanticRewrite,
) -> Result<RewriteReport, String> {
    if r.after_expr != Expr::Var("in0".into())
        || r.before.inputs.len() != 1
        || r.before.outputs.len() != 1
    {
        return Err("rewrite is not a one-input identity".into());
    }
    let old = pc.clone();
    let removed_cells = r.before.region.cells.iter().copied().collect();
    let removed_routes = r.before.region.routes.iter().copied().collect();
    for id in &r.before.region.routes {
        pc.routes.remove(id);
    }
    for id in &r.before.region.cells {
        pc.cells.remove(id);
    }
    let source = r.before.inputs[0].clone();
    let sink = r.before.outputs[0].clone();
    let result = (|| {
        let world = pc.build_world().map_err(|e| e.to_string())?;
        let start = terminal_for_endpoint(&source).map_err(|e| e.to_string())?;
        let goal = terminal_for_endpoint(&sink).map_err(|e| e.to_string())?;
        let route =
            astar_route(&world, start, goal, RouterConfig::default()).map_err(|e| e.to_string())?;
        Ok::<_, String>(pc.add_route(source, sink, route.path, vec![]))
    })();
    match result {
        Ok(id) => Ok(RewriteReport {
            rule: "semantic-identity-realization".into(),
            removed_cells,
            removed_routes,
            added_routes: vec![id],
        }),
        Err(e) => {
            *pc = old;
            Err(e)
        }
    }
}

pub fn eliminate_double_not(pc: &mut PhysicalCircuit) -> Result<Option<RewriteReport>, String> {
    for id in pc.cells.keys().copied().collect::<Vec<_>>() {
        if let Some(f) = extract_linear_not_chain(pc, id, 8) {
            if let Some(r) = simplify_fragment(&f) {
                if r.after_expr == Expr::Var("in0".into()) {
                    return realize_identity_rewrite(pc, &r).map(Some);
                }
            }
        }
    }
    Ok(None)
}

pub fn realize_nand_rewrite(
    pc: &mut PhysicalCircuit,
    r: &SemanticRewrite,
) -> Result<RewriteReport, String> {
    if !matches!(r.after_expr, Expr::Nand(_))
        || r.before.inputs.len() != 2
        || r.before.outputs.len() != 1
    {
        return Err("rewrite is not a two-input NAND".into());
    }
    let old = pc.clone();
    let anchor = r
        .before
        .region
        .cells
        .iter()
        .find_map(|id| pc.cells.get(id).filter(|n| n.logical_kind == GateKind::And))
        .ok_or("NAND rewrite has no AND anchor")?
        .placed
        .clone();
    let nand = default_cell_library()
        .choose(GateKind::Nand)
        .ok_or("no verified NAND cell")?
        .clone();
    let removed_cells = r.before.region.cells.iter().copied().collect::<Vec<_>>();
    let removed_routes = r.before.region.routes.iter().copied().collect::<Vec<_>>();
    for id in &removed_routes {
        pc.routes.remove(id);
    }
    for id in &removed_cells {
        pc.cells.remove(id);
    }
    let nid = pc.add_cell(
        GateKind::Nand,
        PlacedCell {
            cell: nand,
            origin: anchor.origin,
            rotation: anchor.rotation,
        },
    );
    let result = (|| {
        let pairs = [
            (
                r.before.inputs[0].clone(),
                pc.input_endpoint(nid, "a").map_err(|e| e.to_string())?,
            ),
            (
                r.before.inputs[1].clone(),
                pc.input_endpoint(nid, "b").map_err(|e| e.to_string())?,
            ),
            (
                pc.output_endpoint(nid, "out").map_err(|e| e.to_string())?,
                r.before.outputs[0].clone(),
            ),
        ];
        let mut ids = vec![];
        for (source, sink) in pairs {
            // Include routes already realized in this rewrite so a later net
            // cannot silently cross and short them.
            let world = pc.build_world().map_err(|e| e.to_string())?;
            let start = terminal_for_endpoint(&source).map_err(|e| e.to_string())?;
            let goal = terminal_for_endpoint(&sink).map_err(|e| e.to_string())?;
            let rr = astar_route(&world, start, goal, RouterConfig::default())
                .map_err(|e| e.to_string())?;
            ids.push(pc.add_route(source, sink, rr.path, vec![]));
        }
        Ok::<_, String>(ids)
    })();
    match result {
        Ok(added_routes) => Ok(RewriteReport {
            rule: "semantic-nand-realization".into(),
            removed_cells,
            removed_routes,
            added_routes,
        }),
        Err(e) => {
            *pc = old;
            Err(e)
        }
    }
}

pub fn optimize_once_via_reverse(
    pc: &mut PhysicalCircuit,
) -> Result<Option<RewriteReport>, String> {
    for id in pc.cells.keys().copied().collect::<Vec<_>>() {
        if let Some(f) = extract_and_then_not(pc, id) {
            if let Some(r) = simplify_fragment(&f) {
                if matches!(r.after_expr, Expr::Nand(_)) {
                    return realize_nand_rewrite(pc, &r).map(Some);
                }
            }
        }
        if let Some(f) = extract_linear_not_chain(pc, id, 8) {
            if let Some(r) = simplify_fragment(&f) {
                if r.after_expr == Expr::Var("in0".into()) {
                    return realize_identity_rewrite(pc, &r).map(Some);
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_translate::cells::{PlacedCell, PortKind, RotationY, not_cell};
    use dustroute_translate::world::Pos;
    #[test]
    fn extracts_and_eliminates_double_not() {
        let mut pc = PhysicalCircuit::new();
        let a = pc.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(5, 1, 0),
                rotation: RotationY::R0,
            },
        );
        let b = pc.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(11, 1, 0),
                rotation: RotationY::R0,
            },
        );
        let src = PhysicalCircuit::boundary("in", Pos::new(0, 2, 0), PortKind::Wire, None);
        let dst = PhysicalCircuit::boundary("out", Pos::new(18, 2, 0), PortKind::Wire, None);
        pc.add_route(src, pc.input_endpoint(a, "a").unwrap(), vec![], vec![]);
        pc.add_route(
            pc.output_endpoint(a, "out").unwrap(),
            pc.input_endpoint(b, "a").unwrap(),
            vec![],
            vec![],
        );
        pc.add_route(pc.output_endpoint(b, "out").unwrap(), dst, vec![], vec![]);
        let f = extract_linear_not_chain(&pc, a, 8).unwrap();
        assert_eq!(best_by_size(&f.expr, 4, 256), Expr::Var("in0".into()));
        assert!(eliminate_double_not(&mut pc).unwrap().is_some());
        assert!(pc.cells.is_empty());
        assert_eq!(pc.routes.len(), 1);
        pc.build_world().unwrap().validate_supports().unwrap();
    }
}
