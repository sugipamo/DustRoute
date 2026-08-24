from __future__ import annotations

from dataclasses import dataclass

from .cells import PortKind
from .model import Facing, Pos, World


_HORIZONTAL = {
    Facing.NORTH:(0,0,-1),
    Facing.EAST:(1,0,0),
    Facing.SOUTH:(0,0,1),
    Facing.WEST:(-1,0,0),
}


@dataclass(frozen=True)
class PortRealization:
    """Physical routing contract derived from one typed endpoint."""
    terminal: Pos
    approach: Pos
    leaf_required: bool
    approach_facing: Facing | None


def terminal_for_endpoint(world: World, endpoint) -> Pos:
    """Translate a typed port into the conductor position the router targets."""
    if endpoint.kind is PortKind.WIRE:
        return endpoint.pos
    if endpoint.kind is PortKind.BLOCK_POWER:
        delta=_HORIZONTAL.get(endpoint.facing)
        if delta is None:
            raise ValueError("BLOCK_POWER port requires horizontal facing")
        return endpoint.pos.offset(*delta)
    raise ValueError(endpoint.kind)


def realize_sink_endpoint(world: World, endpoint) -> PortRealization:
    terminal=terminal_for_endpoint(world,endpoint)
    delta=_HORIZONTAL.get(endpoint.facing)
    if delta is None:
        return PortRealization(
            terminal=terminal,
            approach=terminal,
            leaf_required=True,
            approach_facing=None,
        )
    return PortRealization(
        terminal=terminal,
        approach=terminal.offset(*delta),
        leaf_required=True,
        approach_facing=endpoint.facing,
    )


def realize_source_endpoint(world: World, endpoint) -> PortRealization:
    terminal=terminal_for_endpoint(world,endpoint)
    # Sources may fan out after their terminal. A directional output can later
    # gain a fixed departure policy without changing the router API.
    return PortRealization(
        terminal=terminal,
        approach=terminal,
        leaf_required=False,
        approach_facing=endpoint.facing,
    )
