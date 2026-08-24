from __future__ import annotations

from .baseline_compiler import (
    BaselineCompileConfig,
    BaselineCompileResult,
    BaselineCompiler,
    fanout_aware_origins,
)
from .logic_dag import LogicDAG


# Backwards-compatible public name.
BaselineDAGCircuit=BaselineCompileResult


def _generic_fanout_aware_origins(dag,bridge,*,spacing_x,lane_gap):
    return fanout_aware_origins(
        dag,bridge,spacing_x=spacing_x,lane_gap=lane_gap
    )


def compile_baseline_dag(
    abstract_dag:LogicDAG,
    *,
    spacing_x:int=12,
    lane_gap:int=8,
    allow_ripup:bool=True,
) -> BaselineDAGCircuit:
    return BaselineCompiler(BaselineCompileConfig(
        spacing_x=spacing_x,
        lane_gap=lane_gap,
        allow_ripup=allow_ripup,
    )).compile(abstract_dag)
