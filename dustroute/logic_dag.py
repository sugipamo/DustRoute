from __future__ import annotations

from dataclasses import dataclass
from collections import defaultdict
import heapq
from typing import Iterable, Mapping

from .logic import Circuit, Direction, Gate, Net, Pin
from .model import GateKind


NodeId = int


@dataclass(frozen=True)
class LogicNode:
    """
    Pure logical DAG node.

    `inputs` references producer node IDs. There are deliberately no physical
    concepts here: no coordinates, ports, wire shapes, support blocks, or
    routing Nets.
    """
    id: NodeId
    op: GateKind
    inputs: tuple[NodeId, ...] = ()
    name: str | None = None


@dataclass(frozen=True)
class LogicDAG:
    nodes: tuple[LogicNode, ...]
    outputs: tuple[tuple[str, NodeId], ...]

    def __post_init__(self):
        by_id={n.id:n for n in self.nodes}
        if len(by_id) != len(self.nodes):
            raise ValueError("duplicate DAG node id")

        for n in self.nodes:
            for src in n.inputs:
                if src not in by_id:
                    raise ValueError(f"node {n.id} references missing input {src}")

        for name,nid in self.outputs:
            if nid not in by_id:
                raise ValueError(f"output {name!r} references missing node {nid}")

        # Validate acyclic structure immediately.
        self.topological_order()

    @property
    def by_id(self) -> dict[NodeId, LogicNode]:
        return {n.id:n for n in self.nodes}

    @property
    def output_map(self) -> dict[str, NodeId]:
        return dict(self.outputs)

    def topological_order(self) -> tuple[NodeId, ...]:
        by_id=self.by_id
        indegree={nid:0 for nid in by_id}
        users:dict[NodeId,list[NodeId]]=defaultdict(list)

        for n in self.nodes:
            indegree[n.id]=len(n.inputs)
            for src in n.inputs:
                users[src].append(n.id)

        # Always choose the smallest ready node ID. Builders assign IDs in
        # creation order, so this preserves a stable topological insertion
        # order and avoids changing downstream naive placement merely because
        # the graph was converted to/from DAG IR.
        ready=[nid for nid,d in indegree.items() if d==0]
        heapq.heapify(ready)
        out=[]
        while ready:
            nid=heapq.heappop(ready)
            out.append(nid)
            for user in users.get(nid,()):
                indegree[user]-=1
                if indegree[user]==0:
                    heapq.heappush(ready,user)

        if len(out) != len(by_id):
            raise ValueError("LogicDAG contains a cycle")
        return tuple(out)

    def users(self) -> dict[NodeId, tuple[NodeId, ...]]:
        users:dict[NodeId,list[NodeId]]=defaultdict(list)
        for n in self.nodes:
            for src in n.inputs:
                users[src].append(n.id)
        return {nid:tuple(xs) for nid,xs in users.items()}

    def fanout_counts(self, *, include_outputs: bool = True) -> dict[NodeId, int]:
        counts={n.id:0 for n in self.nodes}
        for n in self.nodes:
            for src in n.inputs:
                counts[src]+=1
        if include_outputs:
            for _,nid in self.outputs:
                counts[nid]+=1
        return counts

    def logic_depths(self) -> dict[NodeId, int]:
        by_id=self.by_id
        depth:dict[NodeId,int]={}
        for nid in self.topological_order():
            n=by_id[nid]
            depth[nid]=0 if not n.inputs else 1+max(depth[x] for x in n.inputs)
        return depth


class DAGBuilder:
    """Small deterministic builder with optional structural sharing."""

    def __init__(self):
        self._nodes:list[LogicNode]=[]
        self._next_id=0
        self._intern:dict[tuple,NodeId]={}

    def input(self, name: str) -> NodeId:
        key=(GateKind.INPUT,(),name)
        if key in self._intern:
            return self._intern[key]
        nid=self._next_id; self._next_id+=1
        self._nodes.append(LogicNode(nid,GateKind.INPUT,(),name))
        self._intern[key]=nid
        return nid

    def op(
        self,
        op: GateKind,
        *inputs: NodeId,
        name: str | None = None,
        share: bool = True,
    ) -> NodeId:
        if op in (GateKind.INPUT,GateKind.OUTPUT):
            raise ValueError("use input()/DAG outputs instead")
        key=(op,tuple(inputs),name)
        if share and key in self._intern:
            return self._intern[key]
        nid=self._next_id; self._next_id+=1
        self._nodes.append(LogicNode(nid,op,tuple(inputs),name))
        if share:
            self._intern[key]=nid
        return nid

    def finish(self, outputs: Mapping[str,NodeId] | Iterable[tuple[str,NodeId]]) -> LogicDAG:
        items=tuple(outputs.items()) if isinstance(outputs,Mapping) else tuple(outputs)
        return LogicDAG(tuple(self._nodes),items)


def half_adder_dag() -> LogicDAG:
    """Abstract half adder before XOR lowering."""
    b=DAGBuilder()
    a=b.input("a")
    c=b.input("b")
    sum_=b.op(GateKind.XOR,a,c,name="sum_xor")
    carry=b.op(GateKind.AND,a,c,name="carry_and")
    return b.finish((("sum",sum_),("carry",carry)))


def circuit_to_dag(
    circuit: Circuit,
    *,
    input_names: Mapping[int,str] | None = None,
    output_names: Mapping[int,str] | None = None,
) -> LogicDAG:
    """
    Convert the existing Gate/Pin/Net graph into a pure DAG.

    OUTPUT gates are represented as DAG output labels rather than computational
    nodes. This removes physical-Net notions from the lowering stage.
    """
    input_names=dict(input_names or {})
    output_names=dict(output_names or {})
    gate_by_id={g.id:g for g in circuit.gates}

    incoming:dict[tuple[int,int],int]={}
    for net in circuit.nets:
        for sink in net.sinks:
            incoming[(sink.gate,sink.index)]=net.source.gate

    b=DAGBuilder()
    node_for_gate:dict[int,NodeId]={}

    unresolved={g.id for g in circuit.gates if g.kind is not GateKind.OUTPUT}
    while unresolved:
        progress=False
        for gid in sorted(tuple(unresolved)):
            g=gate_by_id[gid]
            if g.kind is GateKind.INPUT:
                node_for_gate[gid]=b.input(input_names.get(gid,f"in{gid}"))
                unresolved.remove(gid);progress=True
                continue

            src_gates=[]
            ready=True
            for i in range(g.input_count):
                src_gate=incoming.get((gid,i))
                if src_gate is None or src_gate not in node_for_gate:
                    ready=False;break
                src_gates.append(node_for_gate[src_gate])
            if not ready:
                continue

            node_for_gate[gid]=b.op(g.kind,*src_gates,name=f"g{gid}",share=False)
            unresolved.remove(gid);progress=True

        if not progress:
            raise ValueError("Circuit is cyclic or has unresolved inputs")

    outputs=[]
    output_index=0
    for g in circuit.gates:
        if g.kind is not GateKind.OUTPUT:
            continue
        src_gate=incoming.get((g.id,0))
        if src_gate is None or src_gate not in node_for_gate:
            raise ValueError(f"OUTPUT gate {g.id} has no resolved input")
        name=output_names.get(g.id,f"out{output_index}")
        outputs.append((name,node_for_gate[src_gate]))
        output_index+=1

    return b.finish(outputs)


def lower_xor_dag(dag: LogicDAG, *, strategy: str = "sop") -> LogicDAG:
    """
    Lower XOR nodes while preserving DAG sharing.

    strategy="sop":
        XOR(a,b) = OR(AND(a,NOT(b)), AND(NOT(a),b))

    Existing producers are reused. NOT(a)/NOT(b) are structurally interned, so
    multiple lowered XORs can share identical primitive subexpressions.
    """
    if strategy != "sop":
        raise ValueError(f"unsupported XOR lowering strategy: {strategy}")

    src=dag.by_id
    b=DAGBuilder()
    mapped:dict[NodeId,NodeId]={}

    for nid in dag.topological_order():
        n=src[nid]
        if n.op is GateKind.INPUT:
            mapped[nid]=b.input(n.name or f"in{nid}")
            continue

        ins=tuple(mapped[x] for x in n.inputs)

        if n.op is GateKind.XOR:
            if len(ins) != 2:
                raise ValueError("current XOR lowering supports binary XOR only")
            a,c=ins
            # Keep the historical primitive creation order used by the
            # pre-DAG XOR lowering: NOT(second), NOT(first), left term,
            # right term, OR. Logical semantics do not depend on this order,
            # but the current naive placement still uses stable gate order.
            nc=b.op(GateKind.NOT,c)
            na=b.op(GateKind.NOT,a)
            left=b.op(GateKind.AND,a,nc)
            right=b.op(GateKind.AND,na,c)
            mapped[nid]=b.op(GateKind.OR,left,right,name=n.name)
        else:
            mapped[nid]=b.op(n.op,*ins,name=n.name)

    return b.finish((name,mapped[nid]) for name,nid in dag.outputs)


@dataclass(frozen=True)
class DAGCircuitBridge:
    circuit: Circuit
    node_to_gate: dict[NodeId, int]
    output_to_gate: dict[str, int]


def dag_to_circuit_bridge(dag: LogicDAG) -> DAGCircuitBridge:
    """
    Bridge pure logical DAG back to the existing Circuit IR while retaining the
    node/gate correspondence needed by later compiler stages.
    """
    by_id=dag.by_id
    order=dag.topological_order()

    gate_id_for_node={nid:i for i,nid in enumerate(order)}
    gates=[]

    for nid in order:
        n=by_id[nid]
        gid=gate_id_for_node[nid]
        gates.append(Gate(gid,n.op,len(n.inputs)))

    output_gate_ids=[]
    output_to_gate={}
    next_gate=len(gates)
    for name,_nid in dag.outputs:
        output_gate_ids.append(next_gate)
        output_to_gate[name]=next_gate
        gates.append(Gate(next_gate,GateKind.OUTPUT,1))
        next_gate+=1

    sinks_by_node:dict[NodeId,list[Pin]]=defaultdict(list)
    for nid in order:
        node=by_id[nid]
        gid=gate_id_for_node[nid]
        for i,src in enumerate(node.inputs):
            sinks_by_node[src].append(Pin(gid,Direction.IN,i))

    for out_index,(name,src) in enumerate(dag.outputs):
        sinks_by_node[src].append(Pin(output_gate_ids[out_index],Direction.IN,0))

    nets=[]
    net_id=0
    for nid in order:
        sinks=tuple(sinks_by_node.get(nid,()))
        if not sinks:
            continue
        nets.append(Net(
            net_id,
            Pin(gate_id_for_node[nid],Direction.OUT,0),
            sinks,
        ))
        net_id+=1

    return DAGCircuitBridge(
        Circuit(tuple(gates),tuple(nets)),
        gate_id_for_node,
        output_to_gate,
    )


def dag_to_circuit(dag: LogicDAG) -> Circuit:
    """
    Convenience wrapper when only the legacy Circuit is needed.
    """
    return dag_to_circuit_bridge(dag).circuit





def evaluate_dag(dag: LogicDAG, inputs: Mapping[str,bool]) -> dict[str,bool]:
    """Evaluate a logical DAG without involving any physical redstone model."""
    by_id=dag.by_id
    values:dict[NodeId,bool]={}

    for nid in dag.topological_order():
        n=by_id[nid]
        if n.op is GateKind.INPUT:
            if n.name is None or n.name not in inputs:
                raise KeyError(f"missing value for input {n.name!r}")
            values[nid]=bool(inputs[n.name])
            continue

        xs=tuple(values[x] for x in n.inputs)
        if n.op is GateKind.NOT:
            if len(xs)!=1: raise ValueError("NOT requires one input")
            values[nid]=not xs[0]
        elif n.op is GateKind.AND:
            values[nid]=all(xs)
        elif n.op is GateKind.OR:
            values[nid]=any(xs)
        elif n.op is GateKind.XOR:
            values[nid]=(sum(bool(x) for x in xs) % 2)==1
        elif n.op is GateKind.NAND:
            values[nid]=not all(xs)
        else:
            raise ValueError(f"unsupported logical DAG op {n.op}")

    return {name:values[nid] for name,nid in dag.outputs}


def dag_stats(dag: LogicDAG) -> dict[str,int]:
    fanout=dag.fanout_counts()
    depth=dag.logic_depths()
    return {
        "nodes":len(dag.nodes),
        "inputs":sum(n.op is GateKind.INPUT for n in dag.nodes),
        "max_depth":max(depth.values(),default=0),
        "max_fanout":max(fanout.values(),default=0),
        "xor_nodes":sum(n.op is GateKind.XOR for n in dag.nodes),
        "primitive_nodes":sum(n.op not in (GateKind.INPUT,GateKind.XOR) for n in dag.nodes),
    }

def describe_dag(dag: LogicDAG) -> str:
    """Human-readable diagnostic representation used by compiler reports."""
    by_id=dag.by_id
    fanout=dag.fanout_counts()
    depth=dag.logic_depths()
    lines=[]
    for nid in dag.topological_order():
        n=by_id[nid]
        args=", ".join(f"n{x}" for x in n.inputs)
        label=f" {n.name}" if n.name else ""
        lines.append(
            f"n{nid}: {n.op.name}({args}){label} "
            f"depth={depth[nid]} fanout={fanout[nid]}"
        )
    for name,nid in dag.outputs:
        lines.append(f"OUTPUT {name} <- n{nid}")
    return "\n".join(lines)
