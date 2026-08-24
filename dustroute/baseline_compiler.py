from __future__ import annotations

from dataclasses import dataclass

from .baseline_cells import baseline_cell_for
from .cells import PlacedCell
from .logic import Circuit, Direction, Pin
from .logic_dag import LogicDAG, lower_xor_dag, dag_to_circuit_bridge
from .model import GateKind, Pos, World
from .multinet import (
    MultiNetRouting,
    route_all_nets,
    route_all_nets_ripup,
    materialize_multinet,
    validate_routing_legality,
)
from .physical import PhysicalCircuit
from .routing import RouteNotFound


@dataclass(frozen=True)
class BaselineCompileConfig:
    spacing_x: int=12
    lane_gap: int=8
    allow_ripup: bool=True
    ripup_attempts: int=128
    ripup_width: int=3


@dataclass(frozen=True)
class BaselineCompileResult:
    abstract_dag: LogicDAG
    primitive_dag: LogicDAG
    logical: Circuit
    physical: PhysicalCircuit
    routing: MultiNetRouting
    world: World
    gate_to_cell: dict[int,int]
    input_positions: dict[str,Pos]
    output_positions: dict[str,Pos]


def fanout_aware_origins(
    dag: LogicDAG,
    bridge,
    *,
    spacing_x: int,
    lane_gap: int,
) -> dict[int,Pos]:
    """Deterministic consumer-barycenter placement; no search/optimization."""
    users=dag.users()
    depths=dag.logic_depths()
    output_targets={
        name:float(i*lane_gap*3)
        for i,(name,_) in enumerate(dag.outputs)
    }
    desired={nid:output_targets[name] for name,nid in dag.outputs}

    for nid in reversed(dag.topological_order()):
        if nid in desired:
            continue
        values=[desired[u] for u in users.get(nid,()) if u in desired]
        desired[nid]=sum(values)/len(values) if values else 0.0

    layers={}
    for nid in dag.topological_order():
        layers.setdefault(depths[nid],[]).append(nid)

    node_origin={}
    for depth,nids in layers.items():
        used=[]
        for nid in sorted(nids,key=lambda x:(desired[x],x)):
            z=int(round(desired[nid]))
            while any(abs(z-u)<lane_gap for u in used):
                z+=lane_gap
            used.append(z)
            node_origin[nid]=Pos(depth*spacing_x,2,z)

    origins={
        bridge.node_to_gate[nid]:pos
        for nid,pos in node_origin.items()
    }
    for name,nid in dag.outputs:
        src=node_origin[nid]
        origins[bridge.output_to_gate[name]]=Pos(
            src.x+spacing_x,2,int(output_targets[name])
        )
    return origins


class BaselineCompiler:
    """
    One non-optimizing compiler pipeline used by every validation circuit.

    LogicDAG -> XOR lowering -> Circuit bridge -> fixed cells ->
    deterministic placement -> routing -> World -> legality gate.
    """

    def __init__(self,config:BaselineCompileConfig=BaselineCompileConfig()):
        self.config=config

    def compile(self,abstract_dag:LogicDAG) -> BaselineCompileResult:
        cfg=self.config
        primitive=lower_xor_dag(abstract_dag,strategy="sop")
        bridge=dag_to_circuit_bridge(primitive)
        logical=bridge.circuit
        pc=PhysicalCircuit()
        gate_to_cell={}

        origins=fanout_aware_origins(
            primitive,
            bridge,
            spacing_x=cfg.spacing_x,
            lane_gap=cfg.lane_gap,
        )

        for gate in logical.gates:
            gate_to_cell[gate.id]=pc.add_cell(
                gate.kind,
                PlacedCell(baseline_cell_for(gate.kind),origins[gate.id]),
            )

        gate_by_id={g.id:g for g in logical.gates}

        def endpoint_for_pin(pin:Pin):
            gate=gate_by_id[pin.gate]
            cid=gate_to_cell[pin.gate]
            if pin.direction is Direction.OUT:
                return pc.output_ep(cid,"out")
            if gate.kind is GateKind.OUTPUT:
                return pc.input_ep(cid,"in")
            return pc.input_ep(cid,"a" if pin.index==0 else "b")

        try:
            routing=route_all_nets(pc,logical.nets,endpoint_for_pin)
        except RouteNotFound:
            if not cfg.allow_ripup:
                raise
            routing=route_all_nets_ripup(
                pc,
                logical.nets,
                endpoint_for_pin,
                max_attempts=cfg.ripup_attempts,
                ripup_width=cfg.ripup_width,
            ).routing

        world=materialize_multinet(pc,routing)

        # Compilation is successful only when the physical artifact passes the
        # same legality gate used by the regression suite.
        legality=validate_routing_legality(pc,routing,world)
        if not legality.valid:
            raise RouteNotFound(
                "baseline compile produced illegal routing: "
                f"contacts={len(legality.cross_net_contacts)}, "
                f"supports={len(legality.support_conflicts)}, "
                f"budget={len(legality.over_budget_paths)}, "
                f"broken={len(legality.broken_steps)}"
            )

        input_positions={}
        for node in primitive.nodes:
            if node.op is GateKind.INPUT and node.name is not None:
                gid=bridge.node_to_gate[node.id]
                input_positions[node.name]=pc.input_ep(
                    gate_to_cell[gid],"in"
                ).pos

        output_positions={
            name:pc.output_ep(
                gate_to_cell[bridge.output_to_gate[name]],"out"
            ).pos
            for name,_ in primitive.outputs
        }

        return BaselineCompileResult(
            abstract_dag,
            primitive,
            logical,
            pc,
            routing,
            world,
            gate_to_cell,
            input_positions,
            output_positions,
        )
