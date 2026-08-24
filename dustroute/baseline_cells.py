from __future__ import annotations

from .model import GateKind
from .cells import (
    PhysicalCell,
    make_not_top_cell,
    make_buffered_input_cell,
    make_buffered_output_cell,
)
from .compiler import make_and_cell, make_or_buffered_cell


def baseline_cell_for(kind: GateKind) -> PhysicalCell:
    """One deterministic, real-Minecraft-validated cell per primitive gate."""
    if kind is GateKind.INPUT:
        return make_buffered_input_cell("input_buffer")
    if kind is GateKind.OUTPUT:
        return make_buffered_output_cell("output")
    if kind is GateKind.NOT:
        return make_not_top_cell()
    if kind is GateKind.AND:
        return make_and_cell()
    if kind is GateKind.OR:
        return make_or_buffered_cell()
    raise ValueError(f"no baseline cell for {kind}")
