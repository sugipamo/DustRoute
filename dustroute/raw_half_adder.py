from __future__ import annotations

from dataclasses import dataclass

from .baseline_cells import baseline_cell_for
from .baseline_compiler import (
    BaselineCompileConfig,
    BaselineCompiler,
    fanout_aware_origins,
)
from .logic import Circuit
from .logic_dag import LogicDAG, half_adder_dag
from .model import Block, BlockKind, GateKind, Pos, World
from .multinet import MultiNetRouting
from .physical import PhysicalCircuit
from .sim import RedstoneTickSimulator
from .wire import update_wire_shapes


@dataclass(frozen=True)
class RawHalfAdder:
    abstract_dag: LogicDAG
    primitive_dag: LogicDAG
    logical: Circuit
    physical: PhysicalCircuit
    routing: MultiNetRouting
    world: World
    gate_to_cell: dict[int,int]
    input_a: Pos
    input_b: Pos
    output_sum: Pos
    output_carry: Pos


# Compatibility helpers retained for existing callers/tests.
def _fixed_cell(kind:GateKind):
    return baseline_cell_for(kind)


def _fanout_aware_origins(primitive_dag,bridge,*,spacing_x,lane_gap):
    return fanout_aware_origins(
        primitive_dag,
        bridge,
        spacing_x=spacing_x,
        lane_gap=lane_gap,
    )


def compile_raw_half_adder(
    *,
    spacing_x:int=12,
    spacing_z:int=8,
) -> RawHalfAdder:
    result=BaselineCompiler(BaselineCompileConfig(
        spacing_x=spacing_x,
        lane_gap=spacing_z,
        allow_ripup=False,
    )).compile(half_adder_dag())

    return RawHalfAdder(
        result.abstract_dag,
        result.primitive_dag,
        result.logical,
        result.physical,
        result.routing,
        result.world,
        result.gate_to_cell,
        result.input_positions["a"],
        result.input_positions["b"],
        result.output_positions["sum"],
        result.output_positions["carry"],
    )


def simulate_raw_half_adder(
    raw:RawHalfAdder,
    a:bool,
    b:bool,
    *,
    ticks:int=64,
):
    world=raw.world.clone()
    for pos,value in ((raw.input_a,a),(raw.input_b,b)):
        source=pos.offset(dx=-1)
        if value:
            world.set(source,Block(BlockKind.REDSTONE_BLOCK))
        else:
            world.remove(source)

    update_wire_shapes(world)
    sim=RedstoneTickSimulator(world)
    state=sim.snapshot()
    for _ in range(ticks):
        state=sim.step()

    return (
        state.strength(raw.output_sum)>0,
        state.strength(raw.output_carry)>0,
        state,
    )


def verify_raw_half_adder(raw:RawHalfAdder|None=None) -> bool:
    raw=raw or compile_raw_half_adder()
    for a,b in ((False,False),(False,True),(True,False),(True,True)):
        sum_,carry,_=simulate_raw_half_adder(raw,a,b)
        if sum_!=(a^b) or carry!=(a and b):
            return False
    return True
