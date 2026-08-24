from __future__ import annotations

from collections import defaultdict, deque
from dataclasses import dataclass
from enum import Enum, auto
from typing import Protocol

from .electrical import repeater_input_pos, repeater_output_pos
from .model import BlockKind, Facing, Pos, World, properties
from .wire import HORIZONTAL, hpos, opposite, wire_has_arm, dust_connected




class PhysicalStepKind(Enum):
    """Confirmed one-step physical signal relation between neighboring positions."""

    DUST = auto()
    DUST_TO_REPEATER = auto()
    REPEATER_TO_DUST = auto()
    DUST_TO_BLOCK = auto()
    BLOCK_TO_REPEATER = auto()
    REPEATER_TO_BLOCK = auto()
    SOURCE_TO_DUST = auto()


@dataclass(frozen=True)
class PhysicalStep:
    src: Pos
    dst: Pos
    kind: PhysicalStepKind


def _horizontal_facing_between(src: Pos, dst: Pos) -> Facing | None:
    if src.y != dst.y:
        return None
    delta=(dst.x-src.x,dst.z-src.z)
    return {
        (1,0):Facing.EAST,
        (-1,0):Facing.WEST,
        (0,1):Facing.SOUTH,
        (0,-1):Facing.NORTH,
    }.get(delta)


def physical_step(world: World, src: Pos, dst: Pos) -> PhysicalStep | None:
    """
    Return the confirmed directed one-step electrical relation `src -> dst`.

    This is intentionally stricter than geometric adjacency. It checks actual
    dust shape, repeater facing, and ordinary-block power targets. It is the
    primitive used by route-continuity validation.
    """

    a=world.get(src)
    b=world.get(dst)

    if a.kind is BlockKind.REDSTONE_WIRE and b.kind is BlockKind.REDSTONE_WIRE:
        if dust_connected(world,src,dst):
            return PhysicalStep(src,dst,PhysicalStepKind.DUST)
        return None

    if a.kind is BlockKind.REDSTONE_WIRE and b.kind is BlockKind.REPEATER:
        if repeater_input_pos(world,dst)==src:
            return PhysicalStep(src,dst,PhysicalStepKind.DUST_TO_REPEATER)
        return None

    if a.kind is BlockKind.REPEATER and b.kind is BlockKind.REDSTONE_WIRE:
        if repeater_output_pos(world,src)==dst:
            return PhysicalStep(src,dst,PhysicalStepKind.REPEATER_TO_DUST)
        return None

    if a.kind is BlockKind.REDSTONE_WIRE and properties(b.kind).accepts_weak_power:
        # Confirmed baseline: dust can weak-power its support block and a
        # horizontally adjacent ordinary block. Probe 16 keeps this boundary
        # explicitly under real-Minecraft regression testing.
        if dst==src.offset(dy=-1):
            return PhysicalStep(src,dst,PhysicalStepKind.DUST_TO_BLOCK)
        if _horizontal_facing_between(src,dst) is not None:
            return PhysicalStep(src,dst,PhysicalStepKind.DUST_TO_BLOCK)
        return None

    if properties(a.kind).repeater_reads_stored_power and b.kind is BlockKind.REPEATER:
        if repeater_input_pos(world,dst)==src:
            return PhysicalStep(src,dst,PhysicalStepKind.BLOCK_TO_REPEATER)
        return None

    if a.kind is BlockKind.REPEATER and properties(b.kind).accepts_strong_power:
        if repeater_output_pos(world,src)==dst:
            return PhysicalStep(src,dst,PhysicalStepKind.REPEATER_TO_BLOCK)
        return None

    if a.kind in (
        BlockKind.REDSTONE_BLOCK,
        BlockKind.LEVER,
        BlockKind.REDSTONE_TORCH,
    ) and b.kind is BlockKind.REDSTONE_WIRE:
        facing=_horizontal_facing_between(src,dst)
        if facing is not None and wire_has_arm(world,dst,opposite(facing)):
            return PhysicalStep(src,dst,PhysicalStepKind.SOURCE_TO_DUST)
        return None

    return None


def physical_step_connected(world: World, src: Pos, dst: Pos) -> bool:
    """Boolean convenience wrapper around :func:`physical_step`."""
    return physical_step(world,src,dst) is not None


class EdgeKind(Enum):
    DUST = auto()
    DUST_TO_BLOCK_WEAK = auto()
    BLOCK_TO_DUST_STRONG = auto()
    BLOCK_TO_REPEATER = auto()
    REPEATER_INPUT = auto()
    REPEATER_OUTPUT = auto()
    DIRECT_SOURCE = auto()
    LEVER_TO_SUPPORT = auto()

    # Compatibility aliases used by older callers.
    REPEATER = REPEATER_OUTPUT
    SOURCE = DIRECT_SOURCE
    TORCH = DIRECT_SOURCE


class EdgeRequirement(Enum):
    """Dynamic condition required for a potential structural edge to be active."""

    ALWAYS = auto()
    SIGNAL_PRESENT = auto()
    STORED_BLOCK_POWER = auto()
    STRONG_BLOCK_POWER = auto()
    DEVICE_OUTPUT_ACTIVE = auto()


class ElectricalStateView(Protocol):
    def strength(self, pos: Pos) -> int: ...
    def block_strength(self, pos: Pos) -> int: ...
    def strong_power(self, pos: Pos) -> int: ...


@dataclass(frozen=True)
class ConnectivityEdge:
    src: Pos
    dst: Pos
    kind: EdgeKind
    requirement: EdgeRequirement = EdgeRequirement.ALWAYS

    def is_active(self, state: ElectricalStateView) -> bool:
        if self.requirement is EdgeRequirement.ALWAYS:
            return True
        if self.requirement is EdgeRequirement.SIGNAL_PRESENT:
            return state.strength(self.src) > 0
        if self.requirement is EdgeRequirement.STORED_BLOCK_POWER:
            return state.block_strength(self.src) > 0
        if self.requirement is EdgeRequirement.STRONG_BLOCK_POWER:
            return state.strong_power(self.src) > 0
        if self.requirement is EdgeRequirement.DEVICE_OUTPUT_ACTIVE:
            return state.strength(self.src) > 0
        raise ValueError(self.requirement)


@dataclass(frozen=True)
class PhysicalConnectivityGraph:
    """Potential structural signal-flow graph extracted from a World.

    Edges carry explicit dynamic requirements. This prevents a weakly powered
    block from being treated as though it can always drive adjacent dust.
    """

    nodes: frozenset[Pos]
    edges: tuple[ConnectivityEdge, ...]

    def outgoing(self, pos: Pos) -> tuple[ConnectivityEdge, ...]:
        return tuple(edge for edge in self.edges if edge.src == pos)

    def incoming(self, pos: Pos) -> tuple[ConnectivityEdge, ...]:
        return tuple(edge for edge in self.edges if edge.dst == pos)

    def active_edges(self, state: ElectricalStateView) -> tuple[ConnectivityEdge, ...]:
        return tuple(edge for edge in self.edges if edge.is_active(state))

    def _reachable(
        self,
        source: Pos,
        edges: tuple[ConnectivityEdge, ...],
    ) -> frozenset[Pos]:
        adjacency: dict[Pos, list[Pos]] = defaultdict(list)
        for edge in edges:
            adjacency[edge.src].append(edge.dst)

        seen = {source}
        queue = deque([source])
        while queue:
            current = queue.popleft()
            for nxt in adjacency.get(current, ()):
                if nxt not in seen:
                    seen.add(nxt)
                    queue.append(nxt)
        return frozenset(seen)

    def potential_reachable_from(self, source: Pos) -> frozenset[Pos]:
        return self._reachable(source, self.edges)

    def active_reachable_from(
        self,
        source: Pos,
        state: ElectricalStateView,
    ) -> frozenset[Pos]:
        return self._reachable(source, self.active_edges(state))

    # Compatibility: old API meant structural/potential reachability.
    def reachable_from(self, source: Pos) -> frozenset[Pos]:
        return self.potential_reachable_from(source)

    def can_potentially_reach(self, source: Pos, sink: Pos) -> bool:
        return sink in self.potential_reachable_from(source)

    def can_actively_reach(
        self,
        source: Pos,
        sink: Pos,
        state: ElectricalStateView,
    ) -> bool:
        return sink in self.active_reachable_from(source, state)

    def can_reach(self, source: Pos, sink: Pos) -> bool:
        return self.can_potentially_reach(source, sink)

    def conductive_components(self) -> tuple[frozenset[Pos], ...]:
        """Connected components of dust only.

        These components are appropriate for accidental cross-Net short
        detection. Conditional block/repeater edges must not merge Nets.
        """

        dust_edges = tuple(edge for edge in self.edges if edge.kind is EdgeKind.DUST)
        dust_nodes = {
            endpoint
            for edge in dust_edges
            for endpoint in (edge.src, edge.dst)
        }
        adjacency: dict[Pos, set[Pos]] = defaultdict(set)
        for edge in dust_edges:
            adjacency[edge.src].add(edge.dst)
            adjacency[edge.dst].add(edge.src)

        seen: set[Pos] = set()
        components: list[frozenset[Pos]] = []
        for node in dust_nodes:
            if node in seen:
                continue
            stack = [node]
            seen.add(node)
            component: set[Pos] = set()
            while stack:
                current = stack.pop()
                component.add(current)
                for nxt in adjacency.get(current, ()):
                    if nxt not in seen:
                        seen.add(nxt)
                        stack.append(nxt)
            components.append(frozenset(component))
        return tuple(components)

    def undirected_components(self) -> tuple[frozenset[Pos], ...]:
        """Compatibility method over every potential edge.

        Prefer :meth:`conductive_components` for Net-short detection.
        """

        adjacency: dict[Pos, set[Pos]] = defaultdict(set)
        for edge in self.edges:
            adjacency[edge.src].add(edge.dst)
            adjacency[edge.dst].add(edge.src)

        seen: set[Pos] = set()
        components: list[frozenset[Pos]] = []
        for node in self.nodes:
            if node in seen:
                continue
            stack = [node]
            seen.add(node)
            component: set[Pos] = set()
            while stack:
                current = stack.pop()
                component.add(current)
                for nxt in adjacency.get(current, ()):
                    if nxt not in seen:
                        seen.add(nxt)
                        stack.append(nxt)
            components.append(frozenset(component))
        return tuple(components)


def _source_requirement_for(world: World, pos: Pos) -> EdgeRequirement:
    if properties(world.get(pos).kind).repeater_reads_stored_power:
        return EdgeRequirement.STORED_BLOCK_POWER
    return EdgeRequirement.SIGNAL_PRESENT


def extract_connectivity(world: World) -> PhysicalConnectivityGraph:
    """Extract potential flow edges with explicit activation conditions."""

    nodes = set(world.positions())
    edges: set[ConnectivityEdge] = set()
    items = tuple(world.items())
    wires = tuple(
        pos for pos, block in items
        if block.kind is BlockKind.REDSTONE_WIRE
    )

    # Dust adjacency is structural, but an active-flow edge requires signal
    # at its source. Potential reachability still includes these edges, while
    # active reachability no longer treats an entirely OFF dust line as flowing.
    for index, a in enumerate(wires):
        for b in wires[index + 1 :]:
            if dust_connected(world, a, b):
                edges.add(
                    ConnectivityEdge(
                        a,
                        b,
                        EdgeKind.DUST,
                        EdgeRequirement.SIGNAL_PRESENT,
                    )
                )
                edges.add(
                    ConnectivityEdge(
                        b,
                        a,
                        EdgeKind.DUST,
                        EdgeRequirement.SIGNAL_PRESENT,
                    )
                )

    # Dust can weak-power ordinary blocks only while that dust is powered.
    for pos, block in items:
        if block.kind is not BlockKind.REDSTONE_WIRE:
            continue
        targets = [pos.offset(dy=-1)]
        targets.extend(
            hpos(pos, facing)
            for facing in HORIZONTAL
            if wire_has_arm(world, pos, facing)
        )
        for target in targets:
            if properties(world.get(target).kind).accepts_weak_power:
                edges.add(
                    ConnectivityEdge(
                        pos,
                        target,
                        EdgeKind.DUST_TO_BLOCK_WEAK,
                        EdgeRequirement.SIGNAL_PRESENT,
                    )
                )

    for pos, block in items:
        props = properties(block.kind)
        if props.can_be_powered:
            for facing in HORIZONTAL:
                neighbor = hpos(pos, facing)
                neighbor_block = world.get(neighbor)

                if (
                    neighbor_block.kind is BlockKind.REPEATER
                    and repeater_input_pos(world, neighbor) == pos
                    and props.repeater_reads_stored_power
                ):
                    edges.add(
                        ConnectivityEdge(
                            pos,
                            neighbor,
                            EdgeKind.BLOCK_TO_REPEATER,
                            EdgeRequirement.STORED_BLOCK_POWER,
                        )
                    )

                if (
                    neighbor_block.kind is BlockKind.REDSTONE_WIRE
                    and wire_has_arm(world, neighbor, opposite(facing))
                    and props.strong_power_drives_dust
                ):
                    edges.add(
                        ConnectivityEdge(
                            pos,
                            neighbor,
                            EdgeKind.BLOCK_TO_DUST_STRONG,
                            EdgeRequirement.STRONG_BLOCK_POWER,
                        )
                    )

        if block.kind is BlockKind.REPEATER:
            input_pos = repeater_input_pos(world, pos)
            output_pos = repeater_output_pos(world, pos)
            if input_pos is not None:
                input_block = world.get(input_pos)
                if (
                    input_block.kind is BlockKind.REDSTONE_WIRE
                    or properties(input_block.kind).repeater_reads_stored_power
                    or input_block.kind in (
                        BlockKind.REDSTONE_BLOCK,
                        BlockKind.LEVER,
                        BlockKind.REDSTONE_TORCH,
                        BlockKind.REPEATER,
                    )
                ):
                    edges.add(
                        ConnectivityEdge(
                            input_pos,
                            pos,
                            EdgeKind.REPEATER_INPUT,
                            _source_requirement_for(world, input_pos),
                        )
                    )
            if output_pos is not None:
                output_block = world.get(output_pos)
                if (
                    output_block.kind is BlockKind.REDSTONE_WIRE
                    or properties(output_block.kind).accepts_strong_power
                ):
                    edges.add(
                        ConnectivityEdge(
                            pos,
                            output_pos,
                            EdgeKind.REPEATER_OUTPUT,
                            EdgeRequirement.DEVICE_OUTPUT_ACTIVE,
                        )
                    )

        if block.kind in (
            BlockKind.LEVER,
            BlockKind.REDSTONE_BLOCK,
            BlockKind.REDSTONE_TORCH,
        ):
            for facing in HORIZONTAL:
                neighbor = hpos(pos, facing)
                if (
                    world.get(neighbor).kind is BlockKind.REDSTONE_WIRE
                    and wire_has_arm(world, neighbor, opposite(facing))
                ):
                    edges.add(
                        ConnectivityEdge(
                            pos,
                            neighbor,
                            EdgeKind.DIRECT_SOURCE,
                            EdgeRequirement.DEVICE_OUTPUT_ACTIVE,
                        )
                    )

            if block.kind is BlockKind.LEVER:
                support = block.support_pos(pos)
                if (
                    support is not None
                    and properties(world.get(support).kind).accepts_strong_power
                ):
                    edges.add(
                        ConnectivityEdge(
                            pos,
                            support,
                            EdgeKind.LEVER_TO_SUPPORT,
                            EdgeRequirement.DEVICE_OUTPUT_ACTIVE,
                        )
                    )

    return PhysicalConnectivityGraph(
        frozenset(nodes),
        tuple(
            sorted(
                edges,
                key=lambda edge: (
                    edge.src,
                    edge.dst,
                    edge.kind.value,
                    edge.requirement.value,
                ),
            )
        ),
    )


def extract_active_connectivity(
    world: World,
    state: ElectricalStateView,
) -> PhysicalConnectivityGraph:
    """Return only edges active in one settled electrical state."""

    potential = extract_connectivity(world)
    return PhysicalConnectivityGraph(
        nodes=potential.nodes,
        edges=tuple(
            ConnectivityEdge(edge.src, edge.dst, edge.kind, EdgeRequirement.ALWAYS)
            for edge in potential.active_edges(state)
        ),
    )


@dataclass(frozen=True)
class ConnectivityExpectation:
    net_id: int
    source: Pos
    sinks: tuple[Pos, ...]


@dataclass(frozen=True)
class ConnectivityValidation:
    missing: tuple[tuple[int, Pos], ...]
    accidental_cross_net_connections: tuple[tuple[int, int], ...]

    @property
    def valid(self) -> bool:
        return not self.missing and not self.accidental_cross_net_connections


def validate_expected_nets(
    graph: PhysicalConnectivityGraph,
    expected: tuple[ConnectivityExpectation, ...],
    *,
    state: ElectricalStateView | None = None,
) -> ConnectivityValidation:
    """Compare expected Nets with potential or active extracted connectivity.

    Missing sinks use potential reachability when ``state`` is omitted and
    active reachability when a settled state is supplied. Cross-Net short
    detection always uses unconditional dust conductive components.
    """

    missing: list[tuple[int, Pos]] = []
    for net in expected:
        reachable = (
            graph.potential_reachable_from(net.source)
            if state is None
            else graph.active_reachable_from(net.source, state)
        )
        for sink in net.sinks:
            if sink not in reachable:
                missing.append((net.net_id, sink))

    component_for: dict[Pos, int] = {}
    for index, component in enumerate(graph.conductive_components()):
        for pos in component:
            component_for[pos] = index

    accidental: list[tuple[int, int]] = []
    for index, a in enumerate(expected):
        for b in expected[index + 1 :]:
            if (
                a.source in component_for
                and b.source in component_for
                and component_for[a.source] == component_for[b.source]
            ):
                accidental.append((a.net_id, b.net_id))

    return ConnectivityValidation(tuple(missing), tuple(accidental))
