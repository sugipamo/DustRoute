from __future__ import annotations

from dataclasses import dataclass, replace
from enum import Enum, auto

from .model import GateKind, Pos
from .cells import PlacedCell, RotationY
from .physical import PhysicalCircuit, Node, Route, Endpoint
from .cell_library import CellLibrary, default_cell_library


@dataclass(frozen=True)
class PlacementWeights:
    wire_distance: float = 1.0
    bounding_volume: float = 0.002
    cell_block_count: float = 0.05
    overlap_penalty: float = 1_000_000.0


@dataclass(frozen=True)
class PlacementScore:
    total: float
    wire_distance: int
    bounding_volume: int
    cell_block_count: int
    overlaps: int


class MutationKind(Enum):
    MOVE = auto()
    ROTATE = auto()
    REPLACE_CELL = auto()


@dataclass(frozen=True)
class PlacementMutation:
    kind: MutationKind
    cell_id: int
    dx: int = 0
    dy: int = 0
    dz: int = 0
    rotation: RotationY | None = None
    candidate_name: str | None = None


@dataclass(frozen=True)
class PlacementOptimizationResult:
    circuit: PhysicalCircuit
    initial_score: PlacementScore
    final_score: PlacementScore
    accepted: tuple[PlacementMutation, ...]


def clone_physical_circuit(pc: PhysicalCircuit) -> PhysicalCircuit:
    out = PhysicalCircuit(
        cells=dict(pc.cells),
        routes=dict(pc.routes),
        _nc=pc._nc,
        _nr=pc._nr,
    )
    return out


def _fresh_endpoint(pc: PhysicalCircuit, ep: Endpoint) -> Endpoint:
    if ep.cell is None:
        return ep

    # Preserve input/output role based on which side of a route refreshes it.
    node = pc.cells[ep.cell]
    try:
        port = node.placed.output_port(ep.port)
        return Endpoint(ep.cell, ep.port, port.pos, port.kind, port.facing)
    except StopIteration:
        port = node.placed.input_port(ep.port)
        return Endpoint(ep.cell, ep.port, port.pos, port.kind, port.facing)


def refresh_route_endpoints(pc: PhysicalCircuit) -> None:
    """
    Refresh transformed endpoint coordinates after cell move/rotation/replacement.

    Existing route geometry is intentionally discarded: placement optimization
    is a pre-routing estimate, and accepted placement is routed again afterward.
    """
    new_routes = {}
    for rid, route in pc.routes.items():
        src = _fresh_endpoint(pc, route.source)
        sink = _fresh_endpoint(pc, route.sink)
        new_routes[rid] = Route(rid, src, sink, (), ())
    pc.routes = new_routes


def _occupied_counts(pc: PhysicalCircuit) -> tuple[dict[Pos, int], int]:
    counts: dict[Pos, int] = {}
    block_count = 0
    for node in pc.cells.values():
        for pos, _ in node.placed.blocks():
            counts[pos] = counts.get(pos, 0) + 1
            block_count += 1
    return counts, block_count


def _bbox_volume(pc: PhysicalCircuit) -> int:
    positions = [
        pos
        for node in pc.cells.values()
        for pos, _ in node.placed.blocks()
    ]
    if not positions:
        return 0
    dx = max(p.x for p in positions) - min(p.x for p in positions) + 1
    dy = max(p.y for p in positions) - min(p.y for p in positions) + 1
    dz = max(p.z for p in positions) - min(p.z for p in positions) + 1
    return dx * dy * dz


def _manhattan(a: Pos, b: Pos) -> int:
    return abs(a.x-b.x) + abs(a.y-b.y) + abs(a.z-b.z)


def placement_score(
    pc: PhysicalCircuit,
    weights: PlacementWeights = PlacementWeights(),
) -> PlacementScore:
    """
    Cheap pre-routing cost.

    Route paths are ignored; only current source/sink terminal positions matter.
    This makes move/rotate/cell-choice mutations cheap enough for local search.
    """
    wire = sum(_manhattan(r.source.pos, r.sink.pos) for r in pc.routes.values())
    counts, blocks = _occupied_counts(pc)
    overlaps = sum(max(0, n-1) for n in counts.values())
    bbox = _bbox_volume(pc)

    total = (
        weights.wire_distance * wire
        + weights.bounding_volume * bbox
        + weights.cell_block_count * blocks
        + weights.overlap_penalty * overlaps
    )
    return PlacementScore(total, wire, bbox, blocks, overlaps)


def apply_mutation(
    pc: PhysicalCircuit,
    mutation: PlacementMutation,
    *,
    library: CellLibrary | None = None,
) -> PhysicalCircuit:
    out = clone_physical_circuit(pc)
    node = out.cells[mutation.cell_id]
    placed = node.placed

    if mutation.kind is MutationKind.MOVE:
        origin = placed.origin.offset(mutation.dx, mutation.dy, mutation.dz)
        new_placed = PlacedCell(placed.cell, origin, placed.rotation)

    elif mutation.kind is MutationKind.ROTATE:
        if mutation.rotation is None:
            raise ValueError("ROTATE mutation needs rotation")
        new_placed = PlacedCell(placed.cell, placed.origin, mutation.rotation)

    elif mutation.kind is MutationKind.REPLACE_CELL:
        if mutation.candidate_name is None:
            raise ValueError("REPLACE_CELL mutation needs candidate_name")
        lib = library or default_cell_library()
        candidate = next(
            c for c in lib.candidates_for(node.logical_kind)
            if c.name == mutation.candidate_name
        )
        verified = lib.verify(candidate)
        if not verified.valid:
            raise ValueError(f"Unverified cell candidate: {candidate.name}")
        new_placed = PlacedCell(candidate.factory(), placed.origin, placed.rotation)

    else:
        raise ValueError(mutation.kind)

    out.cells[mutation.cell_id] = Node(
        node.id,
        node.logical_kind,
        new_placed,
    )
    refresh_route_endpoints(out)
    return out


def candidate_mutations(
    pc: PhysicalCircuit,
    *,
    library: CellLibrary | None = None,
    move_step: int = 2,
    movable_kinds: tuple[GateKind, ...] = (
        GateKind.NOT, GateKind.AND, GateKind.OR, GateKind.XOR,
    ),
) -> tuple[PlacementMutation, ...]:
    lib = library or default_cell_library()
    out: list[PlacementMutation] = []

    for cid, node in pc.cells.items():
        if node.logical_kind not in movable_kinds:
            continue

        for dx, dz in (
            (move_step,0),(-move_step,0),(0,move_step),(0,-move_step)
        ):
            out.append(PlacementMutation(
                MutationKind.MOVE, cid, dx=dx, dz=dz
            ))

        for rot in RotationY:
            if rot != node.placed.rotation:
                out.append(PlacementMutation(
                    MutationKind.ROTATE, cid, rotation=rot
                ))

        for candidate in lib.candidates_for(node.logical_kind):
            if candidate.name != node.placed.cell.name and lib.verify(candidate).valid:
                out.append(PlacementMutation(
                    MutationKind.REPLACE_CELL,
                    cid,
                    candidate_name=candidate.name,
                ))

    return tuple(out)


def optimize_placement(
    pc: PhysicalCircuit,
    *,
    library: CellLibrary | None = None,
    weights: PlacementWeights = PlacementWeights(),
    max_steps: int = 50,
    move_step: int = 2,
) -> PlacementOptimizationResult:
    """
    Greedy hill-climbing placement optimizer.

    Each step enumerates local move/rotation/cell-choice mutations and accepts
    the best strict improvement. Full routing is intentionally deferred until
    the placement has stabilized.
    """
    lib = library or default_cell_library()
    current = clone_physical_circuit(pc)
    refresh_route_endpoints(current)
    initial = placement_score(current, weights)
    current_score = initial
    accepted: list[PlacementMutation] = []

    for _ in range(max_steps):
        best_pc = None
        best_score = current_score
        best_mutation = None

        for mutation in candidate_mutations(
            current,
            library=lib,
            move_step=move_step,
        ):
            try:
                candidate = apply_mutation(
                    current,
                    mutation,
                    library=lib,
                )
            except (ValueError, StopIteration):
                continue

            score = placement_score(candidate, weights)
            if score.total + 1e-9 < best_score.total:
                best_pc = candidate
                best_score = score
                best_mutation = mutation

        if best_pc is None:
            break

        current = best_pc
        current_score = best_score
        accepted.append(best_mutation)

    return PlacementOptimizationResult(
        current,
        initial,
        current_score,
        tuple(accepted),
    )
