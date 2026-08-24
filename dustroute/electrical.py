from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping

from .model import BlockKind, Facing, Pos, World, properties
from .wire import HORIZONTAL, add, hpos, opposite, wire_has_arm, dust_connected


MAX_SIGNAL = 15


@dataclass(frozen=True)
class PoweredBlockState:
    """Stored electrical state of an ordinary powerable block.

    ``weak`` and ``strong`` are intentionally separate because consumers differ:

    - a repeater can read either weak or strong block power;
    - adjacent dust can be driven only by strong block power;
    - a redstone block is not represented here at all. It is a direct source.
    """

    weak: int = 0
    strong: int = 0

    @property
    def level(self) -> int:
        return max(self.weak, self.strong)

    @property
    def powered(self) -> bool:
        return self.level > 0


@dataclass(frozen=True)
class DeviceOutputState:
    """Sequential device outputs frozen during one instantaneous solve."""

    repeater_powered: Mapping[Pos, bool]
    torch_lit: Mapping[Pos, bool]


@dataclass(frozen=True)
class InstantaneousElectricalState:
    """Result of the zero-delay/fixed-point electrical phase."""

    signal_levels: dict[Pos, int]
    block_power: dict[Pos, PoweredBlockState]
    iterations: int
    converged: bool

    def signal(self, pos: Pos) -> int:
        return self.signal_levels.get(pos, 0)

    def power(self, pos: Pos) -> PoweredBlockState:
        return self.block_power.get(pos, PoweredBlockState())


def clamp_signal(level: int) -> int:
    return max(0, min(MAX_SIGNAL, int(level)))


def repeater_input_pos(world: World, pos: Pos) -> Pos | None:
    block = world.get(pos)
    if block.kind is not BlockKind.REPEATER or block.facing not in HORIZONTAL:
        return None
    return add(pos, HORIZONTAL[opposite(block.facing)])


def repeater_output_pos(world: World, pos: Pos) -> Pos | None:
    block = world.get(pos)
    if block.kind is not BlockKind.REPEATER or block.facing not in HORIZONTAL:
        return None
    return add(pos, HORIZONTAL[block.facing])


def component_output_level(
    world: World,
    pos: Pos,
    devices: DeviceOutputState,
) -> int:
    """Direct output level of a component/source at ``pos``.

    This does not imply that a neighboring ordinary block stores power. That is
    handled separately by confirmed strong-power transfer rules.
    """

    block = world.get(pos)
    if block.kind is BlockKind.REDSTONE_BLOCK:
        return MAX_SIGNAL
    if block.kind is BlockKind.LEVER and bool(block.powered):
        return MAX_SIGNAL
    if block.kind is BlockKind.REDSTONE_TORCH and devices.torch_lit.get(pos, False):
        return MAX_SIGNAL
    if block.kind is BlockKind.REPEATER and devices.repeater_powered.get(pos, False):
        return MAX_SIGNAL
    # Comparator semantics are deliberately not guessed yet.
    return 0


def dust_weak_power_targets(world: World, dust_pos: Pos) -> tuple[Pos, ...]:
    """Ordinary blocks weak-powered by one dust position.

    The current verified model powers the support block below and opaque blocks
    in directions in which the dust has an arm.
    """

    targets = [dust_pos.offset(dy=-1)]
    targets.extend(
        hpos(dust_pos, facing)
        for facing in HORIZONTAL
        if wire_has_arm(world, dust_pos, facing)
    )
    return tuple(targets)


def _strong_power_contributions(
    world: World,
    devices: DeviceOutputState,
) -> dict[Pos, int]:
    """Confirmed component -> ordinary-block strong-power transfers.

    Currently modeled and real-world-probed:

    - powered lever -> its explicit support block;
    - powered repeater -> ordinary block directly in front.

    Redstone blocks are intentionally absent: they are direct constant sources,
    not stored strong power in an adjacent normal block.
    """

    strong: dict[Pos, int] = {}
    for pos, block in world.items():
        output = component_output_level(world, pos, devices)
        if output <= 0:
            continue

        target: Pos | None = None
        if block.kind is BlockKind.LEVER:
            target = block.support_pos(pos)
        elif block.kind is BlockKind.REPEATER:
            target = repeater_output_pos(world, pos)

        if target is None:
            continue
        if not properties(world.get(target).kind).accepts_strong_power:
            continue
        strong[target] = max(strong.get(target, 0), output)

    return strong


def compute_powered_blocks(
    world: World,
    signal_levels: Mapping[Pos, int],
    devices: DeviceOutputState,
) -> dict[Pos, PoweredBlockState]:
    """Compute weak/strong stored power of ordinary blocks."""

    weak: dict[Pos, int] = {}
    strong = _strong_power_contributions(world, devices)

    for pos, block in world.items():
        if block.kind is not BlockKind.REDSTONE_WIRE:
            continue
        level = clamp_signal(signal_levels.get(pos, 0))
        if level <= 0:
            continue
        for target in dust_weak_power_targets(world, pos):
            if properties(world.get(target).kind).accepts_weak_power:
                weak[target] = max(weak.get(target, 0), level)

    result: dict[Pos, PoweredBlockState] = {}
    for pos, block in world.items():
        props = properties(block.kind)
        if not props.can_be_powered:
            continue
        result[pos] = PoweredBlockState(
            weak=clamp_signal(weak.get(pos, 0)),
            strong=clamp_signal(strong.get(pos, 0)),
        )
    return result


def repeater_input_level(
    world: World,
    input_pos: Pos,
    state: InstantaneousElectricalState,
) -> int:
    """Level a repeater is allowed to read at its rear input position."""

    props = properties(world.get(input_pos).kind)
    if props.repeater_reads_stored_power:
        return state.power(input_pos).level
    return state.signal(input_pos)


def torch_support_is_powered(
    world: World,
    torch_pos: Pos,
    state: InstantaneousElectricalState,
) -> bool:
    """Whether an attached torch's explicit support block is powered.

    The generic component ``signal_levels`` map is intentionally not consulted;
    a torch observes the powered-block state of its support.
    """

    support = world.get(torch_pos).support_pos(torch_pos)
    if support is None:
        return False
    if not properties(world.get(support).kind).can_be_powered:
        return False
    return state.power(support).powered


def direct_level_into_dust(
    world: World,
    dust_pos: Pos,
    neighbor_pos: Pos,
    block_power: Mapping[Pos, PoweredBlockState],
    devices: DeviceOutputState,
) -> int:
    """Non-dust contribution entering a dust block from one neighbor."""

    neighbor = world.get(neighbor_pos)

    if neighbor.kind in (
        BlockKind.REDSTONE_BLOCK,
        BlockKind.LEVER,
        BlockKind.REDSTONE_TORCH,
    ):
        return component_output_level(world, neighbor_pos, devices)

    if neighbor.kind is BlockKind.REPEATER:
        if repeater_output_pos(world, neighbor_pos) == dust_pos:
            return component_output_level(world, neighbor_pos, devices)
        return 0

    props = properties(neighbor.kind)
    if props.strong_power_drives_dust:
        return block_power.get(neighbor_pos, PoweredBlockState()).strong

    return 0


def solve_instantaneous_electrical_state(
    world: World,
    devices: DeviceOutputState,
    *,
    max_iterations: int = 128,
) -> InstantaneousElectricalState:
    """Solve zero-delay dust/block electrical state to a fixed point.

    This phase contains no repeater queue advancement and no torch state change.
    Those are explicit tick transitions performed by the simulator.
    """

    positions = world.positions()
    wires = tuple(
        pos for pos, block in world.items()
        if block.kind is BlockKind.REDSTONE_WIRE
    )

    signal_levels = {
        pos: component_output_level(world, pos, devices)
        for pos in positions
    }
    block_power: dict[Pos, PoweredBlockState] = {
        pos: PoweredBlockState()
        for pos, block in world.items()
        if properties(block.kind).can_be_powered
    }

    for iteration in range(1, max_iterations + 1):
        new_block_power = compute_powered_blocks(world, signal_levels, devices)
        new_signals = {
            pos: component_output_level(world, pos, devices)
            for pos in positions
        }

        for dust_pos in wires:
            best = 0

            for other in wires:
                if other != dust_pos and dust_connected(world, dust_pos, other):
                    best = max(best, max(0, signal_levels.get(other, 0) - 1))

            for facing in HORIZONTAL:
                if not wire_has_arm(world, dust_pos, facing):
                    continue
                neighbor_pos = hpos(dust_pos, facing)
                best = max(
                    best,
                    direct_level_into_dust(
                        world,
                        dust_pos,
                        neighbor_pos,
                        new_block_power,
                        devices,
                    ),
                )

            new_signals[dust_pos] = clamp_signal(best)

        if new_signals == signal_levels and new_block_power == block_power:
            return InstantaneousElectricalState(
                signal_levels=new_signals,
                block_power=new_block_power,
                iterations=iteration,
                converged=True,
            )

        signal_levels = new_signals
        block_power = new_block_power

    return InstantaneousElectricalState(
        signal_levels=signal_levels,
        block_power=block_power,
        iterations=max_iterations,
        converged=False,
    )


class InstantaneousSolveDidNotConverge(RuntimeError):
    pass


# Public compatibility aliases: the clearer names above are canonical.
BlockPower = PoweredBlockState
InstantaneousState = InstantaneousElectricalState


def solve_instantaneous(
    world: World,
    devices: DeviceOutputState,
    *,
    max_iterations: int = 128,
) -> InstantaneousElectricalState:
    state = solve_instantaneous_electrical_state(
        world,
        devices,
        max_iterations=max_iterations,
    )
    if not state.converged:
        raise InstantaneousSolveDidNotConverge(
            f"instantaneous network did not converge in {max_iterations} iterations"
        )
    return state
