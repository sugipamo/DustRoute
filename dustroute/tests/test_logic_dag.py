from dustroute import *
from .common import settle


def test_logic_rewrite():
    x = Var('x')
    assert best_by_expr_size(Not(Not(x))) == x

def test_half_adder_dag_abstract_shape():
    dag = half_adder_dag()
    stats = dag_stats(dag)
    assert stats['nodes'] == 4
    assert stats['xor_nodes'] == 1
    assert dag.output_map.keys() == {'sum', 'carry'}
    assert evaluate_dag(dag, {'a': False, 'b': False}) == {'sum': False, 'carry': False}
    assert evaluate_dag(dag, {'a': False, 'b': True}) == {'sum': True, 'carry': False}
    assert evaluate_dag(dag, {'a': True, 'b': False}) == {'sum': True, 'carry': False}
    assert evaluate_dag(dag, {'a': True, 'b': True}) == {'sum': False, 'carry': True}

def test_xor_lowering_dag_preserves_semantics_and_sharing():
    abstract = half_adder_dag()
    primitive = lower_xor_dag(abstract)
    assert all((n.op is not GateKind.XOR for n in primitive.nodes))
    inputs = {n.name: n.id for n in primitive.nodes if n.op is GateKind.INPUT}
    fanout = primitive.fanout_counts()
    assert fanout[inputs['a']] == 3
    assert fanout[inputs['b']] == 3
    for a, b in ((False, False), (False, True), (True, False), (True, True)):
        env = {'a': a, 'b': b}
        assert evaluate_dag(abstract, env) == evaluate_dag(primitive, env)

def test_dag_builder_common_subexpression_sharing():
    b = DAGBuilder()
    a = b.input('a')
    c = b.input('b')
    x1 = b.op(GateKind.AND, a, c)
    x2 = b.op(GateKind.AND, a, c)
    assert x1 == x2
    dag = b.finish((('x', x1),))
    assert len(dag.nodes) == 3
    assert dag.fanout_counts()[x1] == 1

def test_dag_to_circuit_bridge_keeps_fanout_as_one_net():
    primitive = lower_xor_dag(half_adder_dag())
    bridge = dag_to_circuit_bridge(primitive)
    inputs = {n.name: n.id for n in primitive.nodes if n.op is GateKind.INPUT}
    a_gate = bridge.node_to_gate[inputs['a']]
    b_gate = bridge.node_to_gate[inputs['b']]
    a_nets = [n for n in bridge.circuit.nets if n.source.gate == a_gate]
    b_nets = [n for n in bridge.circuit.nets if n.source.gate == b_gate]
    assert len(a_nets) == 1
    assert len(b_nets) == 1
    assert len(a_nets[0].sinks) == 3
    assert len(b_nets[0].sinks) == 3

def test_mux_and_decoder_dag_truth_tables():
    mux = mux2_dag()
    for a in (False, True):
        for b in (False, True):
            for s in (False, True):
                got = evaluate_dag(mux, {'a': a, 'b': b, 's': s})['out']
                assert got == (b if s else a)
    dec = decoder1to2_dag()
    for en in (False, True):
        for s in (False, True):
            got = evaluate_dag(dec, {'en': en, 's': s})
            assert got['y0'] == (en and (not s))
            assert got['y1'] == (en and s)
