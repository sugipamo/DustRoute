from __future__ import annotations

from dataclasses import dataclass
from itertools import product
from typing import Callable, Mapping

from .model import Block, BlockKind, Facing, GateKind, Pos
from .cells import PhysicalCell, PortKind, make_not_cell, make_not_top_cell, make_terminal_cell
from .sim import RedstoneTickSimulator
from .wire import update_wire_shapes


TruthFn = Callable[[Mapping[str, bool]], bool]
CellFactory = Callable[[], PhysicalCell]


@dataclass(frozen=True)
class CellMetrics:
    size_x: int
    size_y: int
    size_z: int
    volume: int
    block_count: int
    max_settle_ticks: int


@dataclass(frozen=True)
class CellCandidate:
    name: str
    gate_kind: GateKind
    factory: CellFactory
    truth: TruthFn
    requires_verification: bool = True


@dataclass(frozen=True)
class VerifiedCandidate:
    candidate: CellCandidate
    valid: bool
    metrics: CellMetrics


class CellLibrary:
    def __init__(self) -> None:
        self._candidates: list[CellCandidate] = []
        self._verification_cache: dict[str, VerifiedCandidate] = {}

    def register(self, candidate: CellCandidate) -> None:
        self._candidates.append(candidate)

    def candidates_for(self, kind: GateKind) -> tuple[CellCandidate, ...]:
        return tuple(c for c in self._candidates if c.gate_kind is kind)

    def verified_for(self, kind: GateKind) -> tuple[VerifiedCandidate, ...]:
        out = []
        for c in self.candidates_for(kind):
            v = self.verify(c)
            if v.valid:
                out.append(v)
        return tuple(out)

    def choose(self, kind: GateKind) -> PhysicalCell:
        candidates = self.verified_for(kind)
        if not candidates:
            raise ValueError(f"No verified physical cell for {kind.name}")
        best = min(
            candidates,
            key=lambda v: (
                v.metrics.volume,
                v.metrics.block_count,
                v.metrics.max_settle_ticks,
                v.candidate.name,
            ),
        )
        return best.candidate.factory()

    def verify(self, candidate: CellCandidate) -> VerifiedCandidate:
        cached = self._verification_cache.get(candidate.name)
        if cached is not None:
            return cached

        cell = candidate.factory()
        names = tuple(p.name for p in cell.inputs)
        valid = True
        max_ticks = 0

        if not candidate.requires_verification:
            bounds = cell.world.bounds()
            if bounds is None:
                sx = sy = sz = 0
            else:
                lo, hi = bounds
                sx = hi.x - lo.x + 1
                sy = hi.y - lo.y + 1
                sz = hi.z - lo.z + 1
            result = VerifiedCandidate(
                candidate=candidate,
                valid=True,
                metrics=CellMetrics(
                    sx, sy, sz, sx * sy * sz,
                    len(cell.world.positions()),
                    0,
                ),
            )
            self._verification_cache[candidate.name] = result
            return result

        for bits in product((False, True), repeat=len(names)):
            values = dict(zip(names, bits))
            world = cell.world.clone()
            _drive_inputs(cell, world, values)
            update_wire_shapes(world)
            sim = RedstoneTickSimulator(world)
            state = sim.snapshot()

            previous = None
            stable = 0
            elapsed = 0
            for elapsed in range(1, 17):
                state = sim.step()
                out = state.strength(cell.outputs[0].pos) > 0
                if out == previous:
                    stable += 1
                else:
                    previous = out
                    stable = 1
                if stable >= 2:
                    break

            max_ticks = max(max_ticks, elapsed)
            actual = state.strength(cell.outputs[0].pos) > 0
            if actual != bool(candidate.truth(values)):
                valid = False

        bounds = cell.world.bounds()
        if bounds is None:
            sx = sy = sz = 0
        else:
            lo, hi = bounds
            sx = hi.x - lo.x + 1
            sy = hi.y - lo.y + 1
            sz = hi.z - lo.z + 1

        result = VerifiedCandidate(
            candidate=candidate,
            valid=valid,
            metrics=CellMetrics(
                sx, sy, sz, sx * sy * sz,
                len(cell.world.positions()),
                max_ticks,
            ),
        )
        self._verification_cache[candidate.name] = result
        return result


def _delta(facing: Facing) -> Pos:
    return {
        Facing.NORTH: Pos(0, 0, -1),
        Facing.EAST: Pos(1, 0, 0),
        Facing.SOUTH: Pos(0, 0, 1),
        Facing.WEST: Pos(-1, 0, 0),
    }[facing]


def _drive_inputs(cell: PhysicalCell, world, values: Mapping[str, bool]) -> None:
    for port in cell.inputs:
        value = bool(values[port.name])
        facing = port.facing or Facing.WEST
        d = _delta(facing)

        if port.kind is PortKind.BLOCK_POWER:
            lever = port.pos.offset(d.x, d.y, d.z)
            world.place(
                BlockKind.LEVER,
                lever.x, lever.y, lever.z,
                facing={Facing.NORTH:Facing.SOUTH, Facing.SOUTH:Facing.NORTH,
                        Facing.EAST:Facing.WEST, Facing.WEST:Facing.EAST}[facing],
                powered=value,
                support_offset=Pos(-d.x, -d.y, -d.z),
            )

        elif port.kind is PortKind.WIRE:
            if value:
                source = port.pos.offset(d.x, d.y, d.z)
                world.set(source, Block(BlockKind.REDSTONE_BLOCK))

        else:
            raise ValueError(port.kind)


def default_cell_library() -> CellLibrary:
    lib = CellLibrary()

    lib.register(CellCandidate(
        "not_side_torch",
        GateKind.NOT,
        make_not_cell,
        lambda i: not i["a"],
    ))
    lib.register(CellCandidate(
        "not_top_torch",
        GateKind.NOT,
        make_not_top_cell,
        lambda i: not i["a"],
    ))

    # Lazy imports avoid compiler<->library import cycles while cells are being
    # migrated into the library.
    lib.register(CellCandidate(
        "or_dust",
        GateKind.OR,
        lambda: __import__(
            __package__ + ".compiler",
            fromlist=["make_or_cell"],
        ).make_or_cell(),
        lambda i: i["a"] or i["b"],
    ))
    lib.register(CellCandidate(
        "and_demorgan_repeater",
        GateKind.AND,
        lambda: __import__(
            __package__ + ".compiler",
            fromlist=["make_and_cell"],
        ).make_and_cell(),
        lambda i: i["a"] and i["b"],
    ))

    lib.register(CellCandidate(
        "nand_torch_merge",
        GateKind.NAND,
        lambda: __import__(
            __package__ + ".compiler",
            fromlist=["make_nand_cell"],
        ).make_nand_cell(),
        lambda i: not (i["a"] and i["b"]),
    ))

    lib.register(CellCandidate(
        "input_terminal",
        GateKind.INPUT,
        lambda: make_terminal_cell("input"),
        lambda i: False,
        False,
    ))
    lib.register(CellCandidate(
        "output_terminal",
        GateKind.OUTPUT,
        lambda: make_terminal_cell("output"),
        lambda i: False,
        False,
    ))

    return lib
