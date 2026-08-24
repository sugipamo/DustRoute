from dustroute import *
from .common import settle


def test_support():
    w = World()
    w.set(Pos(0, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, 0, 1, 0)
    w.validate_supports()

def test_wire_shapes():
    w = World()
    w.fill(Pos(0, 0, 0), Pos(2, 0, 2), Block(BlockKind.SOLID))
    for p in (Pos(1, 1, 0), Pos(1, 1, 1), Pos(2, 1, 1)):
        w.place(BlockKind.REDSTONE_WIRE, p.x, p.y, p.z)
    update_wire_shapes(w)
    assert wire_shape_name(w.get(Pos(1, 1, 1))) == 'corner'

def test_half_adder_compile():
    logical = half_adder()
    result = compile_circuit(logical)
    assert all((g.kind is not GateKind.XOR for g in result.logical.gates))
    assert len(result.physical.cells) == len(result.logical.gates)
    assert len(result.physical.routes) > 0
    assert len(result.physical.cell_world().positions()) > 0
    assert all((r.path for r in result.physical.routes.values()))

def test_raw_half_adder_records_dag_stages():
    raw = compile_raw_half_adder(spacing_x=16, spacing_z=12)
    assert any((n.op is GateKind.XOR for n in raw.abstract_dag.nodes))
    assert all((n.op is not GateKind.XOR for n in raw.primitive_dag.nodes))
    assert raw.primitive_dag.fanout_counts()[next((n.id for n in raw.primitive_dag.nodes if n.name == 'a'))] == 3

def test_dag_fanout_aware_half_adder_placement():
    raw = compile_raw_half_adder(spacing_x=12, spacing_z=8)
    by_kind = [(n.logical_kind, n.placed.origin) for n in raw.physical.cells.values()]
    assert max((p.x for _, p in by_kind)) <= 48
    assert max((p.z for _, p in by_kind)) <= 24
    assert min((p.z for _, p in by_kind)) >= 0

def test_raw_half_adder_observes_buffered_output_ports():
    raw = compile_raw_half_adder(spacing_x=12, spacing_z=8)
    primitive = raw.primitive_dag
    bridge = dag_to_circuit_bridge(primitive)
    sum_gate = bridge.output_to_gate['sum']
    carry_gate = bridge.output_to_gate['carry']
    assert raw.output_sum == raw.physical.output_ep(raw.gate_to_cell[sum_gate], 'out').pos
    assert raw.output_carry == raw.physical.output_ep(raw.gate_to_cell[carry_gate], 'out').pos
    assert raw.output_sum != raw.physical.input_ep(raw.gate_to_cell[sum_gate], 'in').pos
    assert raw.output_carry != raw.physical.input_ep(raw.gate_to_cell[carry_gate], 'in').pos

def test_generic_dag_baseline_compiles_mux_and_decoder():
    for dag in (mux2_dag(), decoder1to2_dag()):
        compiled = compile_baseline_dag(dag, spacing_x=12, lane_gap=8, allow_ripup=False)
        report = validate_routing_legality(compiled.physical, compiled.routing, compiled.world)
        assert report.valid
        assert validate_route_continuity(compiled.physical, compiled.routing, compiled.world).valid


def test_baseline_compiler_is_single_pipeline():
    result=BaselineCompiler(BaselineCompileConfig(
        spacing_x=12,
        lane_gap=8,
        allow_ripup=False,
    )).compile(half_adder_dag())
    assert result.input_positions.keys()=={"a","b"}
    assert result.output_positions.keys()=={"sum","carry"}
    assert validate_routing_legality(
        result.physical,result.routing,result.world
    ).valid
