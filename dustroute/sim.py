from __future__ import annotations

from collections import deque
from dataclasses import dataclass

from .electrical import (
    DeviceOutputState,
    InstantaneousElectricalState,
    PoweredBlockState,
    component_output_level,
    repeater_input_level,
    repeater_input_pos,
    repeater_output_pos,
    solve_instantaneous,
    torch_support_is_powered,
)
from .model import BlockKind, Pos, World


# Backwards-compatible public name.
BlockPower = PoweredBlockState


@dataclass(frozen=True)
class TickState:
    tick: int
    strengths: dict[Pos, int]
    block_power: dict[Pos, PoweredBlockState]
    repeater_powered: dict[Pos, bool]
    torch_powered: dict[Pos, bool]
    instantaneous_iterations: int = 0

    def strength(self, pos: Pos) -> int:
        return self.strengths.get(pos, 0)

    def power(self, pos: Pos) -> PoweredBlockState:
        return self.block_power.get(pos, PoweredBlockState())

    def weak_power(self, pos: Pos) -> int:
        return self.power(pos).weak

    def strong_power(self, pos: Pos) -> int:
        return self.power(pos).strong

    def block_strength(self, pos: Pos) -> int:
        return self.power(pos).level

    def powered(self, pos: Pos) -> bool:
        return self.strength(pos) > 0 or self.block_strength(pos) > 0


class RedstoneTickSimulator:
    """Redstone model split into instantaneous and delayed phases.

    `settle_instantaneous()` solves dust and stored ordinary-block power while
    repeater/torch outputs are frozen. `advance_tick()` samples component inputs,
    advances those delayed outputs by one abstract tick, then settles again.
    """

    def __init__(self, world: World) -> None:
        self.world = world
        self.tick = 0

        self._repeater_powered: dict[Pos, bool] = {}
        self._queues: dict[Pos, deque[bool]] = {}
        self._torch_powered: dict[Pos, bool] = {}

        for pos, block in world.items():
            if block.kind is BlockKind.REPEATER:
                delay = max(1, min(4, block.delay or 1))
                self._repeater_powered[pos] = False
                self._queues[pos] = deque([False] * delay, maxlen=delay)
            elif block.kind is BlockKind.REDSTONE_TORCH:
                # A newly constructed torch starts lit; its support is sampled
                # during the first delayed component transition.
                self._torch_powered[pos] = True

        self._instantaneous = InstantaneousElectricalState(
            signal_levels={},
            block_power={},
            iterations=0,
            converged=True,
        )
        self._strengths: dict[Pos, int] = {}
        self._block_power: dict[Pos, PoweredBlockState] = {}
        self.settle_instantaneous()

    def _devices(self) -> DeviceOutputState:
        return DeviceOutputState(
            repeater_powered=self._repeater_powered,
            torch_lit=self._torch_powered,
        )

    def settle_instantaneous(self) -> TickState:
        """Recompute zero-delay electrical state without advancing time."""

        self._instantaneous = solve_instantaneous(
            self.world,
            self._devices(),
        )
        # Compatibility fields for previous prototype callers.
        self._strengths = dict(self._instantaneous.signal_levels)
        self._block_power = dict(self._instantaneous.block_power)
        return self.snapshot()

    def _settle(self) -> None:
        """Compatibility alias for the old private method."""
        self.settle_instantaneous()

    def snapshot(self) -> TickState:
        return TickState(
            tick=self.tick,
            strengths=dict(self._instantaneous.signal_levels),
            block_power=dict(self._instantaneous.block_power),
            repeater_powered=dict(self._repeater_powered),
            torch_powered=dict(self._torch_powered),
            instantaneous_iterations=self._instantaneous.iterations,
        )

    def _src(self, pos: Pos) -> int:
        """Compatibility helper for direct component output only."""
        return component_output_level(self.world, pos, self._devices())

    def _rep_in(self, pos: Pos, block) -> Pos | None:
        return repeater_input_pos(self.world, pos)

    def _rep_out(self, pos: Pos, block) -> Pos | None:
        return repeater_output_pos(self.world, pos)

    def _read_repeater(self, pos: Pos | None) -> int:
        if pos is None:
            return 0
        return repeater_input_level(
            self.world,
            pos,
            self._instantaneous,
        )

    def advance_tick(self) -> TickState:
        """Advance delayed component outputs by one abstract redstone tick."""

        next_repeaters: dict[Pos, bool] = {}
        for pos, block in self.world.items():
            if block.kind is not BlockKind.REPEATER:
                continue
            input_pos = repeater_input_pos(self.world, pos)
            requested = (
                input_pos is not None
                and repeater_input_level(
                    self.world,
                    input_pos,
                    self._instantaneous,
                )
                > 0
            )
            queue = self._queues[pos]
            queue.append(bool(requested))
            next_repeaters[pos] = queue[0]

        next_torches: dict[Pos, bool] = {}
        for pos, block in self.world.items():
            if block.kind is not BlockKind.REDSTONE_TORCH:
                continue
            next_torches[pos] = not torch_support_is_powered(
                self.world,
                pos,
                self._instantaneous,
            )

        self._repeater_powered.update(next_repeaters)
        self._torch_powered.update(next_torches)
        self.tick += 1
        return self.settle_instantaneous()

    def step(self) -> TickState:
        """Existing public alias for `advance_tick()`."""
        return self.advance_tick()
