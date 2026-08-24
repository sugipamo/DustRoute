from dustroute import *
from .common import settle


def test_connectivity_separate_dust_nets():
    w = World()
    w.fill(Pos(0, 0, 0), Pos(4, 0, 2), Block(BlockKind.SOLID))
    for x in range(1, 4):
        w.place(BlockKind.REDSTONE_WIRE, x, 1, 0)
    for x in range(1, 4):
        w.place(BlockKind.REDSTONE_WIRE, x, 1, 2)
    update_wire_shapes(w)
    g = extract_connectivity(w)
    ex = (ConnectivityExpectation(1, Pos(1, 1, 0), (Pos(3, 1, 0),)), ConnectivityExpectation(2, Pos(1, 1, 2), (Pos(3, 1, 2),)))
    v = validate_expected_nets(g, ex)
    assert v.valid

def test_connectivity_detects_accidental_short():
    w = World()
    w.fill(Pos(0, 0, 0), Pos(4, 0, 2), Block(BlockKind.SOLID))
    for x in range(1, 4):
        w.place(BlockKind.REDSTONE_WIRE, x, 1, 0)
    for x in range(1, 4):
        w.place(BlockKind.REDSTONE_WIRE, x, 1, 2)
    w.place(BlockKind.REDSTONE_WIRE, 2, 1, 1)
    update_wire_shapes(w)
    g = extract_connectivity(w)
    ex = (ConnectivityExpectation(1, Pos(1, 1, 0), (Pos(3, 1, 0),)), ConnectivityExpectation(2, Pos(1, 1, 2), (Pos(3, 1, 2),)))
    v = validate_expected_nets(g, ex)
    assert not v.valid
    assert (1, 2) in v.accidental_cross_net_connections

def test_connectivity_repeater_direction():
    w = World()
    w.fill(Pos(0, 0, 0), Pos(4, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, 0, 1, 0)
    w.place(BlockKind.REPEATER, 1, 1, 0, facing=Facing.EAST, delay=1)
    w.place(BlockKind.REDSTONE_WIRE, 2, 1, 0)
    update_wire_shapes(w)
    g = extract_connectivity(w)
    assert g.can_reach(Pos(0, 1, 0), Pos(2, 1, 0))
    assert not g.can_reach(Pos(2, 1, 0), Pos(0, 1, 0))

def test_connectivity_conditions_distinguish_weak_from_strong():
    w = World()
    w.set(Pos(-1, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, -1, 1, 0, wire_connections=((Facing.EAST, WireConnection.SIDE),))
    w.set(Pos(0, 1, 0), Block(BlockKind.SOLID))
    w.set(Pos(1, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, 1, 1, 0, wire_connections=((Facing.WEST, WireConnection.SIDE),))
    w.set(Pos(-2, 1, 0), Block(BlockKind.REDSTONE_BLOCK))
    state = RedstoneTickSimulator(w).snapshot()
    graph = extract_connectivity(w)
    assert graph.can_potentially_reach(Pos(-1, 1, 0), Pos(1, 1, 0))
    assert not graph.can_actively_reach(Pos(-1, 1, 0), Pos(1, 1, 0), state)
    conditional = [edge for edge in graph.edges if edge.kind is EdgeKind.BLOCK_TO_DUST_STRONG]
    assert conditional
    assert all((edge.requirement is EdgeRequirement.STRONG_BLOCK_POWER for edge in conditional))

def test_connectivity_short_detection_uses_dust_components_only():
    w = World()
    w.set(Pos(-1, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, -1, 1, 0, wire_connections=((Facing.EAST, WireConnection.SIDE),))
    w.set(Pos(0, 1, 0), Block(BlockKind.SOLID))
    w.set(Pos(1, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, 1, 1, 0, wire_connections=((Facing.WEST, WireConnection.SIDE),))
    graph = extract_connectivity(w)
    validation = validate_expected_nets(graph, (ConnectivityExpectation(1, Pos(-1, 1, 0), (Pos(-1, 1, 0),)), ConnectivityExpectation(2, Pos(1, 1, 0), (Pos(1, 1, 0),))))
    assert validation.valid
    assert len(graph.conductive_components()) == 0

def test_physical_step_connected_primitives():
    w = World()
    _supports = lambda ps: [w.set(p, Block(BlockKind.SOLID)) for p in ps]
    _supports((Pos(0, 0, 0), Pos(1, 0, 0), Pos(1, 0, 1)))
    for p in (Pos(0, 1, 0), Pos(1, 1, 0), Pos(1, 1, 1)):
        w.place(BlockKind.REDSTONE_WIRE, p.x, p.y, p.z)
    update_wire_shapes(w)
    assert physical_step_connected(w, Pos(0, 1, 0), Pos(1, 1, 0))
    assert physical_step_connected(w, Pos(1, 1, 0), Pos(1, 1, 1))
    w2 = World()
    for x in range(3):
        w2.set(Pos(x, 0, 0), Block(BlockKind.SOLID))
    w2.place(BlockKind.REDSTONE_WIRE, 0, 1, 0)
    w2.place(BlockKind.REPEATER, 1, 1, 0, facing=Facing.EAST, delay=1)
    w2.place(BlockKind.REDSTONE_WIRE, 2, 1, 0)
    update_wire_shapes(w2)
    assert physical_step_connected(w2, Pos(0, 1, 0), Pos(1, 1, 0))
    assert physical_step_connected(w2, Pos(1, 1, 0), Pos(2, 1, 0))
    assert not physical_step_connected(w2, Pos(2, 1, 0), Pos(1, 1, 0))
    assert not physical_step_connected(w2, Pos(1, 1, 0), Pos(0, 1, 0))

def test_physical_step_connected_stairs():
    w = World()
    w.set(Pos(0, 0, 0), Block(BlockKind.SOLID))
    w.set(Pos(1, 1, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, 0, 1, 0)
    w.place(BlockKind.REDSTONE_WIRE, 1, 2, 0)
    update_wire_shapes(w)
    assert physical_step_connected(w, Pos(0, 1, 0), Pos(1, 2, 0))
    assert physical_step_connected(w, Pos(1, 2, 0), Pos(0, 1, 0))
    w2 = World()
    w2.set(Pos(0, 1, 0), Block(BlockKind.SOLID))
    w2.set(Pos(1, 0, 0), Block(BlockKind.SOLID))
    w2.place(BlockKind.REDSTONE_WIRE, 0, 2, 0)
    w2.place(BlockKind.REDSTONE_WIRE, 1, 1, 0)
    update_wire_shapes(w2)
    assert physical_step_connected(w2, Pos(0, 2, 0), Pos(1, 1, 0))

def test_physical_step_connected_block_power_boundary():
    w = World()
    w.set(Pos(0, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, 0, 1, 0)
    w.set(Pos(1, 1, 0), Block(BlockKind.SOLID))
    update_wire_shapes(w)
    assert physical_step_connected(w, Pos(0, 1, 0), Pos(1, 1, 0))
    w2 = World()
    w2.set(Pos(0, 0, 0), Block(BlockKind.SOLID))
    w2.place(BlockKind.REPEATER, 0, 1, 0, facing=Facing.EAST, delay=1)
    w2.set(Pos(1, 1, 0), Block(BlockKind.SOLID))
    assert physical_step_connected(w2, Pos(0, 1, 0), Pos(1, 1, 0))

def test_route_continuity_detects_bad_repeater_direction():
    terminal = make_terminal_cell()
    pc = PhysicalCircuit()
    s = pc.add_cell(GateKind.INPUT, PlacedCell(terminal, Pos(0, 0, 0)))
    d = pc.add_cell(GateKind.OUTPUT, PlacedCell(terminal, Pos(4, 0, 0)))
    source = pc.output_ep(s, 'out')
    sink = pc.input_ep(d, 'in')
    path = (source.pos, Pos(1, 1, 0), Pos(2, 1, 0), Pos(3, 1, 0), sink.pos)
    rn = RoutedNet(77, source, (sink,), (path,), frozenset(path), frozenset({Pos(2, 1, 0)}))
    routing = MultiNetRouting({77: rn})
    world = materialize_multinet(pc, routing)
    good = validate_route_continuity(pc, routing, world)
    assert good.valid
    world.set(Pos(2, 1, 0), Block(BlockKind.REPEATER, facing=Facing.WEST, delay=1))
    broken = validate_route_continuity(pc, routing, world)
    assert not broken.valid
    assert any((x.src == Pos(1, 1, 0) and x.dst == Pos(2, 1, 0) for x in broken.broken))
    assert any((x.src == Pos(2, 1, 0) and x.dst == Pos(3, 1, 0) for x in broken.broken))

def test_half_adder_route_continuity_is_explicitly_validated():
    raw = compile_raw_half_adder(spacing_x=12, spacing_z=8)
    continuity = validate_route_continuity(raw.physical, raw.routing, raw.world)
    assert continuity.valid
    legality = validate_routing_legality(raw.physical, raw.routing, raw.world)
    assert not legality.broken_steps
