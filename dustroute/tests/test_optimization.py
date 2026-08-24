from dustroute import *
from .common import settle


def test_physical_rewrite():
    cell = make_not_cell()
    pc = PhysicalCircuit()
    a = pc.add_cell(GateKind.NOT, PlacedCell(cell, Pos(6, 1, 2)))
    b = pc.add_cell(GateKind.NOT, PlacedCell(cell, Pos(12, 1, 2)))
    src = pc.boundary('in', Pos(1, 2, 2))
    dst = pc.boundary('out', Pos(17, 2, 2))
    pc.add_route(src, pc.input_ep(a, 'a'), (src.pos,))
    pc.add_route(pc.output_ep(a, 'out'), pc.input_ep(b, 'a'), (pc.output_ep(a, 'out').pos,))
    pc.add_route(pc.output_ep(b, 'out'), dst, (dst.pos,))
    rep = eliminate_double_not(pc)
    assert rep and (not pc.cells) and (len(pc.routes) == 1)
    w = pc.build_world()
    w.set(Pos(0, 2, 2), Block(BlockKind.REDSTONE_BLOCK))
    update_wire_shapes(w)
    s = settle(w, 12)
    assert s.strength(dst.pos) > 0

def test_physical_rewrite_between_cells():
    not_cell = make_not_cell()
    terminal = make_terminal_cell()
    pc = PhysicalCircuit()
    src_cell = pc.add_cell(GateKind.INPUT, PlacedCell(terminal, Pos(0, 0, 0)))
    a = pc.add_cell(GateKind.NOT, PlacedCell(not_cell, Pos(5, 1, 0)))
    b = pc.add_cell(GateKind.NOT, PlacedCell(not_cell, Pos(11, 1, 0)))
    dst_cell = pc.add_cell(GateKind.OUTPUT, PlacedCell(terminal, Pos(18, 0, 0)))
    src = pc.output_ep(src_cell, 'out')
    dst = pc.input_ep(dst_cell, 'in')
    pc.add_route(src, pc.input_ep(a, 'a'), (src.pos,))
    pc.add_route(pc.output_ep(a, 'out'), pc.input_ep(b, 'a'), (pc.output_ep(a, 'out').pos,))
    pc.add_route(pc.output_ep(b, 'out'), dst, (dst.pos,))
    rep = eliminate_double_not(pc)
    assert rep is not None
    assert set(pc.cells) == {src_cell, dst_cell}
    assert len(pc.routes) == 1
    route = next(iter(pc.routes.values()))
    assert route.source.cell == src_cell and route.sink.cell == dst_cell
    w = pc.build_world()
    source_block = src.pos.offset(dx=-1)
    w.set(source_block, Block(BlockKind.REDSTONE_BLOCK))
    update_wire_shapes(w)
    s = settle(w, 16)
    assert s.strength(dst.pos) > 0

def test_reverse_extract_double_not():
    cell = make_not_cell()
    terminal = make_terminal_cell()
    pc = PhysicalCircuit()
    src = pc.add_cell(GateKind.INPUT, PlacedCell(terminal, Pos(0, 0, 0)))
    a = pc.add_cell(GateKind.NOT, PlacedCell(cell, Pos(6, 1, 0)))
    b = pc.add_cell(GateKind.NOT, PlacedCell(cell, Pos(12, 1, 0)))
    dst = pc.add_cell(GateKind.OUTPUT, PlacedCell(terminal, Pos(20, 0, 0)))
    pc.add_route(pc.output_ep(src, 'out'), pc.input_ep(a, 'a'), (pc.output_ep(src, 'out').pos,))
    pc.add_route(pc.output_ep(a, 'out'), pc.input_ep(b, 'a'), (pc.output_ep(a, 'out').pos,))
    pc.add_route(pc.output_ep(b, 'out'), pc.input_ep(dst, 'in'), (pc.output_ep(b, 'out').pos,))
    frag = extract_linear_not_chain(pc, a)
    assert frag is not None
    assert frag.region.cells == frozenset({a, b})
    assert frag.expr == Not(Not(Var('in0')))
    rw = simplify_fragment(frag)
    assert rw is not None and rw.after_expr == Var('in0')

def test_reverse_optimize_double_not():
    cell = make_not_cell()
    terminal = make_terminal_cell()
    pc = PhysicalCircuit()
    src = pc.add_cell(GateKind.INPUT, PlacedCell(terminal, Pos(0, 0, 0)))
    a = pc.add_cell(GateKind.NOT, PlacedCell(cell, Pos(6, 1, 0)))
    b = pc.add_cell(GateKind.NOT, PlacedCell(cell, Pos(12, 1, 0)))
    dst = pc.add_cell(GateKind.OUTPUT, PlacedCell(terminal, Pos(20, 0, 0)))
    pc.add_route(pc.output_ep(src, 'out'), pc.input_ep(a, 'a'), (pc.output_ep(src, 'out').pos,))
    pc.add_route(pc.output_ep(a, 'out'), pc.input_ep(b, 'a'), (pc.output_ep(a, 'out').pos,))
    pc.add_route(pc.output_ep(b, 'out'), pc.input_ep(dst, 'in'), (pc.output_ep(b, 'out').pos,))
    report = optimize_once_via_reverse(pc)
    assert report is not None
    assert report.rule == 'semantic-identity-realization'
    assert set(pc.cells) == {src, dst}
    assert len(pc.routes) == 1
    world = pc.build_world()
    source = pc.output_ep(src, 'out').pos.offset(dx=-1)
    world.set(source, Block(BlockKind.REDSTONE_BLOCK))
    update_wire_shapes(world)
    st = settle(world, 18)
    assert st.strength(pc.input_ep(dst, 'in').pos) > 0

def test_placement_optimizer_moves_cell():
    pc = PhysicalCircuit()
    n = pc.add_cell(GateKind.NOT, PlacedCell(make_not_cell(), Pos(10, 2, 12)))
    src = pc.boundary('src', Pos(0, 2, 0))
    dst = pc.boundary('dst', Pos(22, 2, 0))
    pc.add_route(src, pc.input_ep(n, 'a'), ())
    pc.add_route(pc.output_ep(n, 'out'), dst, ())
    before = placement_score(pc)
    result = optimize_placement(pc, max_steps=20, move_step=2)
    assert result.final_score.total < before.total
    assert result.final_score.wire_distance < before.wire_distance
    assert result.circuit.cells[n].placed.origin.z == 0
    assert result.final_score.overlaps == 0

def test_placement_optimizer_cell_candidates():
    lib = default_cell_library()
    pc = PhysicalCircuit()
    n = pc.add_cell(GateKind.NOT, PlacedCell(make_not_cell(), Pos(0, 0, 0)))
    muts = candidate_mutations(pc, library=lib)
    replacements = [m for m in muts if m.kind is MutationKind.REPLACE_CELL]
    assert replacements
    assert any((m.candidate_name == 'not_top_torch' for m in replacements))
    swap = next((m for m in replacements if m.candidate_name == 'not_top_torch'))
    changed = apply_mutation(pc, swap, library=lib)
    assert changed.cells[n].placed.cell.name == 'not_torch_top'

def test_reverse_extract_and_not_as_nand():
    pc = PhysicalCircuit()
    term = make_terminal_cell()
    a_src = pc.add_cell(GateKind.INPUT, PlacedCell(term, Pos(0, 2, 0)))
    b_src = pc.add_cell(GateKind.INPUT, PlacedCell(term, Pos(0, 2, 10)))
    and_id = pc.add_cell(GateKind.AND, PlacedCell(make_and_cell(), Pos(12, 2, 2)))
    not_id = pc.add_cell(GateKind.NOT, PlacedCell(make_not_cell(), Pos(26, 2, 4)))
    out_id = pc.add_cell(GateKind.OUTPUT, PlacedCell(term, Pos(38, 2, 4)))
    pc.add_route(pc.output_ep(a_src, 'out'), pc.input_ep(and_id, 'a'), ())
    pc.add_route(pc.output_ep(b_src, 'out'), pc.input_ep(and_id, 'b'), ())
    pc.add_route(pc.output_ep(and_id, 'out'), pc.input_ep(not_id, 'a'), ())
    pc.add_route(pc.output_ep(not_id, 'out'), pc.input_ep(out_id, 'in'), ())
    frag = extract_and_then_not(pc, and_id)
    assert frag is not None
    assert frag.expr == Not(And(Var('in0'), Var('in1')))
    rw = simplify_fragment(frag)
    assert rw is not None
    assert isinstance(rw.after_expr, Nand)

def test_reverse_realize_nand():
    pc = PhysicalCircuit()
    term = make_terminal_cell()
    a_src = pc.add_cell(GateKind.INPUT, PlacedCell(term, Pos(0, 2, 0)))
    b_src = pc.add_cell(GateKind.INPUT, PlacedCell(term, Pos(0, 2, 10)))
    and_id = pc.add_cell(GateKind.AND, PlacedCell(make_and_cell(), Pos(12, 2, 2)))
    not_id = pc.add_cell(GateKind.NOT, PlacedCell(make_not_cell(), Pos(26, 2, 4)))
    out_id = pc.add_cell(GateKind.OUTPUT, PlacedCell(term, Pos(38, 2, 4)))
    pc.add_route(pc.output_ep(a_src, 'out'), pc.input_ep(and_id, 'a'), ())
    pc.add_route(pc.output_ep(b_src, 'out'), pc.input_ep(and_id, 'b'), ())
    pc.add_route(pc.output_ep(and_id, 'out'), pc.input_ep(not_id, 'a'), ())
    pc.add_route(pc.output_ep(not_id, 'out'), pc.input_ep(out_id, 'in'), ())
    report = optimize_once_via_reverse(pc)
    assert report is not None
    assert report.rule == 'semantic-nand-realization'
    assert any((n.logical_kind is GateKind.NAND for n in pc.cells.values()))
    assert not any((n.logical_kind is GateKind.AND for n in pc.cells.values()))
    assert not any((n.logical_kind is GateKind.NOT for n in pc.cells.values()))
    world = pc.build_world()
    world.validate_supports()
