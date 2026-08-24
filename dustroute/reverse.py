from __future__ import annotations
from dataclasses import dataclass

from .logic import Expr, Var, Not, And, Nand, best_by_expr_size
from .model import GateKind, Pos
from .physical import Endpoint, PhysicalCircuit, RewriteReport, _wire_terminal_for_endpoint
from .routing import RouterConfig, route_place_and_refresh
from .cells import PlacedCell
from .cell_library import default_cell_library


@dataclass(frozen=True)
class PhysicalRegion:
    """A connected physical subgraph chosen for semantic extraction."""
    cells: frozenset[int]
    routes: frozenset[int]


@dataclass(frozen=True)
class SemanticFragment:
    """
    Semantic meaning extracted from a physical region.

    Inputs/outputs are the stable physical boundary endpoints of the region.
    `expr` is expressed in terms of Var("in0"), Var("in1"), ...
    """
    expr: Expr
    inputs: tuple[Endpoint, ...]
    outputs: tuple[Endpoint, ...]
    region: PhysicalRegion


def _boundary_routes(pc: PhysicalCircuit, cells: set[int]):
    incoming = []
    outgoing = []
    internal = []
    for r in pc.routes.values():
        src_in = r.source.cell in cells if r.source.cell is not None else False
        sink_in = r.sink.cell in cells if r.sink.cell is not None else False

        if not src_in and sink_in:
            incoming.append(r)
        elif src_in and not sink_in:
            outgoing.append(r)
        elif src_in and sink_in:
            internal.append(r)
    return incoming, outgoing, internal


def extract_linear_not_chain(
    pc: PhysicalCircuit,
    start_cell: int,
    *,
    max_len: int = 8,
) -> SemanticFragment | None:
    """
    Recognize a linear chain of NOT cells from the physical graph.

    This is deliberately semantic-pattern extraction: it does not care about the
    exact torch layout once cells have already been identified as NOT physical
    implementations.
    """
    if start_cell not in pc.cells:
        return None

    chain = []
    cur = start_cell
    seen = set()

    while len(chain) < max_len:
        if cur in seen or cur not in pc.cells:
            break
        node = pc.cells[cur]
        if node.logical_kind is not GateKind.NOT:
            break

        incoming = pc.incoming(cur)
        outgoing = pc.outgoing(cur)
        if len(incoming) != 1 or len(outgoing) != 1:
            break

        chain.append(cur)
        seen.add(cur)

        nxt = outgoing[0].sink.cell
        if nxt is None or nxt not in pc.cells:
            break
        if pc.cells[nxt].logical_kind is not GateKind.NOT:
            break
        if len(pc.incoming(nxt)) != 1:
            break
        cur = nxt

    if not chain:
        return None

    region_cells = set(chain)
    incoming, outgoing, internal = _boundary_routes(pc, region_cells)

    if len(incoming) != 1 or len(outgoing) != 1:
        return None

    expr: Expr = Var("in0")
    for _ in chain:
        expr = Not(expr)

    route_ids = {r.id for r in incoming + outgoing + internal}

    return SemanticFragment(
        expr=expr,
        inputs=(incoming[0].source,),
        outputs=(outgoing[0].sink,),
        region=PhysicalRegion(
            frozenset(region_cells),
            frozenset(route_ids),
        ),
    )


def extract_all_not_chains(pc: PhysicalCircuit) -> tuple[SemanticFragment, ...]:
    out = []
    covered = set()
    for cid in sorted(pc.cells):
        if cid in covered:
            continue
        frag = extract_linear_not_chain(pc, cid)
        if frag is None:
            continue
        out.append(frag)
        covered.update(frag.region.cells)
    return tuple(out)


@dataclass(frozen=True)
class SemanticRewrite:
    before: SemanticFragment
    after_expr: Expr


def simplify_fragment(fragment: SemanticFragment) -> SemanticRewrite | None:
    """
    Run the existing logical rewrite engine over an extracted fragment.
    """
    best = best_by_expr_size(fragment.expr, max_steps=4, max_states=256)
    if best == fragment.expr:
        return None
    return SemanticRewrite(fragment, best)


def realize_identity_rewrite(
    pc: PhysicalCircuit,
    rewrite: SemanticRewrite,
    *,
    config: RouterConfig = RouterConfig(),
) -> RewriteReport:
    """
    Realize a simplified fragment when it becomes identity Var("in0").

    This first realizer is intentionally tiny; more realizers can be registered
    later for NAND, NOR, XOR, etc.
    """
    if rewrite.after_expr != Var("in0"):
        raise NotImplementedError(
            f"No physical realizer yet for {rewrite.after_expr!r}"
        )

    frag = rewrite.before
    if len(frag.inputs) != 1 or len(frag.outputs) != 1:
        raise ValueError("Identity rewrite requires 1 input and 1 output")

    old_cells = {
        cid: pc.cells[cid]
        for cid in frag.region.cells
    }
    old_routes = {
        rid: pc.routes[rid]
        for rid in frag.region.routes
    }

    source = frag.inputs[0]
    sink = frag.outputs[0]

    for rid in frag.region.routes:
        pc.routes.pop(rid, None)
    for cid in frag.region.cells:
        pc.cells.pop(cid, None)

    try:
        w = pc.routing_world()
        start = _wire_terminal_for_endpoint(w, source)
        goal = _wire_terminal_for_endpoint(w, sink)
        rr, reps = route_place_and_refresh(w, start, goal, config)
        new_route = pc.add_route(source, sink, rr.path, reps)
    except Exception:
        pc.cells.update(old_cells)
        pc.routes.update(old_routes)
        raise

    return RewriteReport(
        "semantic-identity-realization",
        tuple(sorted(frag.region.cells)),
        tuple(sorted(frag.region.routes)),
        (new_route,),
    )


def optimize_once_via_reverse(
    pc: PhysicalCircuit,
    *,
    config: RouterConfig = RouterConfig(),
) -> RewriteReport | None:
    """
    One complete optimization step:
        physical -> semantic extraction -> logical rewrite -> physical realization
    """
    for fragment in extract_semantic_fragments(pc):
        rewrite = simplify_fragment(fragment)
        if rewrite is None:
            continue

        if rewrite.after_expr == Var("in0"):
            return realize_identity_rewrite(pc, rewrite, config=config)

        if isinstance(rewrite.after_expr, Nand):
            return realize_nand_rewrite(pc, rewrite, config=config)

    return None


def extract_and_then_not(
    pc: PhysicalCircuit,
    and_cell: int,
) -> SemanticFragment | None:
    """
    Recognize a two-cell semantic pattern:

        AND(a,b) -> NOT

    and extract it as:

        NOT(AND(in0,in1))

    The recognizer is topology-based at the PhysicalCircuit level. The actual
    physical implementation of AND/NOT may vary, as long as the cells are
    already identified semantically by their logical_kind.
    """
    if and_cell not in pc.cells:
        return None
    and_node = pc.cells[and_cell]
    if and_node.logical_kind is not GateKind.AND:
        return None

    incoming = pc.incoming(and_cell)
    outgoing = pc.outgoing(and_cell)
    if len(incoming) != 2 or len(outgoing) != 1:
        return None

    middle = outgoing[0]
    not_cell = middle.sink.cell
    if not_cell is None or not_cell not in pc.cells:
        return None
    if pc.cells[not_cell].logical_kind is not GateKind.NOT:
        return None

    not_in = pc.incoming(not_cell)
    not_out = pc.outgoing(not_cell)
    if len(not_in) != 1 or len(not_out) != 1:
        return None
    if not_in[0].id != middle.id:
        return None

    region_cells = {and_cell, not_cell}
    boundary_in, boundary_out, internal = _boundary_routes(pc, region_cells)
    if len(boundary_in) != 2 or len(boundary_out) != 1:
        return None

    # Stable input ordering: prefer sink port a,b if available, otherwise route id.
    boundary_in = sorted(
        boundary_in,
        key=lambda r: (
            0 if r.sink.port == "a" else 1 if r.sink.port == "b" else 2,
            r.id,
        ),
    )

    route_ids = {r.id for r in boundary_in + boundary_out + internal}

    return SemanticFragment(
        expr=Not(And(Var("in0"), Var("in1"))),
        inputs=tuple(r.source for r in boundary_in),
        outputs=(boundary_out[0].sink,),
        region=PhysicalRegion(
            frozenset(region_cells),
            frozenset(route_ids),
        ),
    )


def extract_semantic_fragments(
    pc: PhysicalCircuit,
) -> tuple[SemanticFragment, ...]:
    """
    General semantic extractor registry.

    More specific multi-cell patterns run before simpler NOT chains so larger
    meaningful fragments are preferred.
    """
    out: list[SemanticFragment] = []
    covered: set[int] = set()

    # Specific AND->NOT pattern first.
    for cid in sorted(pc.cells):
        if cid in covered:
            continue
        frag = extract_and_then_not(pc, cid)
        if frag is not None:
            out.append(frag)
            covered.update(frag.region.cells)

    # Then remaining NOT chains.
    for cid in sorted(pc.cells):
        if cid in covered:
            continue
        frag = extract_linear_not_chain(pc, cid)
        if frag is not None and not (set(frag.region.cells) & covered):
            out.append(frag)
            covered.update(frag.region.cells)

    return tuple(out)


def realize_nand_rewrite(
    pc: PhysicalCircuit,
    rewrite: SemanticRewrite,
    *,
    config: RouterConfig = RouterConfig(),
) -> RewriteReport:
    """
    Realize NOT(AND(a,b)) -> NAND(a,b) as one verified NAND physical cell.
    """
    if not isinstance(rewrite.after_expr, Nand):
        raise ValueError("NAND realizer requires Nand expression")

    frag = rewrite.before
    if len(frag.inputs) != 2 or len(frag.outputs) != 1:
        raise ValueError("NAND rewrite requires 2 inputs and 1 output")

    old_cells = {cid: pc.cells[cid] for cid in frag.region.cells}
    old_routes = {rid: pc.routes[rid] for rid in frag.region.routes}
    old_nc, old_nr = pc._nc, pc._nr

    # Anchor replacement at the original AND cell position.
    and_nodes = [
        pc.cells[cid]
        for cid in frag.region.cells
        if pc.cells[cid].logical_kind is GateKind.AND
    ]
    if len(and_nodes) != 1:
        raise ValueError("Expected exactly one AND cell in NAND fragment")
    anchor = and_nodes[0].placed.origin

    for rid in frag.region.routes:
        pc.routes.pop(rid, None)
    for cid in frag.region.cells:
        pc.cells.pop(cid, None)

    added_routes = []
    try:
        cell = default_cell_library().choose(GateKind.NAND)
        nand_id = pc.add_cell(
            GateKind.NAND,
            PlacedCell(cell, anchor),
        )

        # Route each stable fragment input to the corresponding NAND input.
        for name, source in zip(("a", "b"), frag.inputs):
            sink = pc.input_ep(nand_id, name)
            w = pc.routing_world()
            start = _wire_terminal_for_endpoint(w, source)
            goal = _wire_terminal_for_endpoint(w, sink)
            rr, reps = route_place_and_refresh(w, start, goal, config)
            added_routes.append(pc.add_route(source, sink, rr.path, reps))

        # Route NAND output to the stable fragment output boundary.
        source = pc.output_ep(nand_id, "out")
        sink = frag.outputs[0]
        w = pc.routing_world()
        start = _wire_terminal_for_endpoint(w, source)
        goal = _wire_terminal_for_endpoint(w, sink)
        rr, reps = route_place_and_refresh(w, start, goal, config)
        added_routes.append(pc.add_route(source, sink, rr.path, reps))

    except Exception:
        # Remove anything added and restore the original local subgraph.
        for cid in tuple(pc.cells):
            if cid >= old_nc:
                pc.cells.pop(cid, None)
        for rid in tuple(pc.routes):
            if rid >= old_nr:
                pc.routes.pop(rid, None)
        pc.cells.update(old_cells)
        pc.routes.update(old_routes)
        pc._nc, pc._nr = old_nc, old_nr
        raise

    return RewriteReport(
        "semantic-nand-realization",
        tuple(sorted(frag.region.cells)),
        tuple(sorted(frag.region.routes)),
        tuple(added_routes),
    )
