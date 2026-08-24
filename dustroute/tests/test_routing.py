from dustroute import *
from .common import settle


def test_router():
    w = World()
    w.fill(Pos(0, 0, 0), Pos(40, 0, 0), Block(BlockKind.SOLID))
    r, reps = route_place_and_refresh(w, Pos(1, 1, 0), Pos(39, 1, 0))
    assert len(reps) >= 2
    w.set(Pos(0, 1, 0), Block(BlockKind.REDSTONE_BLOCK))
    update_wire_shapes(w)
    s = settle(w, 12)
    assert s.strength(Pos(39, 1, 0)) > 0

def test_multinet_shared_tree():
    terminal = make_terminal_cell()
    pc = PhysicalCircuit()
    s = pc.add_cell(GateKind.INPUT, PlacedCell(terminal, Pos(0, 0, 0)))
    a = pc.add_cell(GateKind.OUTPUT, PlacedCell(terminal, Pos(16, 0, -6)))
    b = pc.add_cell(GateKind.OUTPUT, PlacedCell(terminal, Pos(16, 0, 6)))
    source = pc.output_ep(s, 'out')
    sinks = (pc.input_ep(a, 'in'), pc.input_ep(b, 'in'))
    rn = route_net_tree(pc, 99, source, sinks)
    assert len(rn.branches) == 2
    shared = set(rn.branches[0]) & set(rn.branches[1])
    assert shared, 'fan-out branches should share the existing Net tree'
    assert rn.source == source and rn.sinks == sinks

def test_half_adder_multinet_compile():
    c, pc, routing, w = compile_circuit_multinet(half_adder())
    assert len(routing.nets) == len(c.nets)
    owners = {}
    for nid, rn in routing.nets.items():
        for pos in rn.occupied:
            assert pos not in owners or owners[pos] == nid
            owners[pos] = nid
    assert any((len(rn.sinks) > 1 for rn in routing.nets.values()))
    assert len(w.positions()) > 0

def test_ripup_reroute_control_flow():
    import dustroute.multinet as mn
    pc = PhysicalCircuit()
    nets = (Net(1, Pin(0, Direction.OUT, 0), (Pin(1, Direction.IN, 0),)), Net(2, Pin(2, Direction.OUT, 0), (Pin(3, Direction.IN, 0),)))
    mapping = {Pin(0, Direction.OUT, 0): pc.boundary('A0', Pos(0, 1, 0)), Pin(1, Direction.IN, 0): pc.boundary('A1', Pos(4, 1, 0)), Pin(2, Direction.OUT, 0): pc.boundary('B0', Pos(0, 1, 2)), Pin(3, Direction.IN, 0): pc.boundary('B1', Pos(4, 1, 2))}
    original = mn.route_net_tree
    calls = []

    def fake_route(pc_, nid, source, sinks, *, occupied_other=None, reserved_other_terminals=None, config=RouterConfig()):
        occupied = set(occupied_other or ())
        calls.append((nid, frozenset(occupied)))
        if nid == 2 and Pos(2, 1, 0) in occupied:
            raise RouteNotFound('synthetic ordering conflict')
        if nid == 1 and Pos(2, 1, 2) in occupied:
            path = (Pos(0, 2, 0), Pos(1, 2, 0), Pos(2, 2, 0), Pos(3, 2, 0), Pos(4, 2, 0))
        elif nid == 1:
            path = (Pos(0, 1, 0), Pos(1, 1, 0), Pos(2, 1, 0), Pos(3, 1, 0), Pos(4, 1, 0))
        else:
            path = (Pos(0, 1, 2), Pos(1, 1, 2), Pos(2, 1, 2), Pos(3, 1, 2), Pos(4, 1, 2))
        return RoutedNet(nid, source, sinks, (path,), frozenset(path), frozenset())
    mn.route_net_tree = fake_route
    try:
        result = mn.route_all_nets_ripup(pc, nets, lambda pin: mapping[pin], max_attempts=10, ripup_width=1)
    finally:
        mn.route_net_tree = original
    assert result.events
    assert result.events[0].failed_net == 2
    assert result.events[0].ripped_up == (1,)
    assert set(result.routing.nets) == {1, 2}
    assert not set(result.routing.nets[1].occupied) & set(result.routing.nets[2].occupied)

def test_half_adder_ripup_compile():
    c, pc, result, w = compile_circuit_ripup(half_adder(), max_attempts=64, ripup_width=2)
    assert len(result.routing.nets) == len(c.nets)
    owners = {}
    for nid, net in result.routing.nets.items():
        for pos in net.occupied:
            assert pos not in owners or owners[pos] == nid
            owners[pos] = nid
    assert len(w.positions()) > 0

def test_raw_half_adder_routing_is_legal_baseline():
    raw = compile_raw_half_adder(spacing_x=12, spacing_z=8)
    report = validate_routing_legality(raw.physical, raw.routing, raw.world)
    assert report.valid
    assert not report.cross_net_contacts
    assert not report.support_conflicts
    assert not report.over_budget_paths
    raw.world.validate_supports()

def test_raw_half_adder_tree_repeaters_respect_budget():
    raw = compile_raw_half_adder(spacing_x=12, spacing_z=8)
    report = validate_routing_legality(raw.physical, raw.routing, raw.world, max_wire_run=12)
    assert not report.over_budget_paths
    assert sum((len(n.repeaters) for n in raw.routing.nets.values())) > 0

def test_typed_sink_terminals_are_leaf_stubs():
    from dustroute.physical import _wire_terminal_for_endpoint
    from dustroute.multinet import _tree_adjacency, _endpoint_approach
    raw = compile_raw_half_adder(spacing_x=12, spacing_z=8)
    base = raw.physical.cell_world()
    for net in raw.routing.nets.values():
        adj = _tree_adjacency(net.branches)
        for sink in net.sinks:
            terminal = _wire_terminal_for_endpoint(base, sink)
            assert len(adj.get(terminal, set())) == 1
            approach = _endpoint_approach(sink, terminal)
            if sink.facing in (Facing.NORTH, Facing.EAST, Facing.SOUTH, Facing.WEST):
                assert approach in adj[terminal]
            if sink.kind is PortKind.BLOCK_POWER:
                wire = raw.world.get(terminal)
                assert wire.kind is BlockKind.REDSTONE_WIRE
                arms = {f for f, state in wire.wire_connections or () if state is not WireConnection.NONE}
                assert arms == {sink.facing}


def test_port_realization_contract_is_explicit():
    cell=make_not_top_cell()
    pc=PhysicalCircuit()
    cid=pc.add_cell(GateKind.NOT,PlacedCell(cell,Pos(10,2,3)))
    ep=pc.input_ep(cid,"a")
    world=pc.cell_world()

    realized=realize_sink_endpoint(world,ep)
    assert realized.leaf_required
    assert realized.terminal==Pos(9,2,3)
    assert realized.approach==Pos(8,2,3)
    assert realized.approach_facing is Facing.WEST


def test_routing_resources_keep_roles_separate():
    wire={Pos(3,2,4),Pos(4,2,4)}
    r=RoutingResources.from_conductors(
        wire,
        stair_clearance={Pos(3,3,4)},
        terminals={Pos(2,2,4)},
    )
    assert r.conductors==frozenset(wire)
    assert Pos(3,1,4) in r.supports
    assert Pos(3,2,3) in r.electrical_keepout
    assert Pos(3,3,4) in r.stair_clearance
    assert Pos(2,2,4) in r.terminals
    assert Pos(2,2,4) in r.blocked_conductors
