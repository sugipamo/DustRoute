from __future__ import annotations

from dataclasses import dataclass
from enum import Enum, auto


class GateKind(Enum):
    INPUT = auto()
    OUTPUT = auto()
    NOT = auto()
    AND = auto()
    OR = auto()
    XOR = auto()
    NAND = auto()


class BlockKind(str, Enum):
    AIR = "air"
    SOLID = "solid"
    TRANSPARENT = "transparent"
    REDSTONE_WIRE = "redstone_wire"
    REDSTONE_TORCH = "redstone_torch"
    REPEATER = "repeater"
    COMPARATOR = "comparator"
    LEVER = "lever"
    REDSTONE_BLOCK = "redstone_block"
    PISTON = "piston"


class Facing(str, Enum):
    NORTH = "north"
    EAST = "east"
    SOUTH = "south"
    WEST = "west"
    UP = "up"
    DOWN = "down"


class WireConnection(str, Enum):
    NONE = "none"
    SIDE = "side"
    UP = "up"


@dataclass(frozen=True, order=True)
class Pos:
    x: int
    y: int
    z: int

    def offset(self, dx: int = 0, dy: int = 0, dz: int = 0) -> "Pos":
        return Pos(self.x + dx, self.y + dy, self.z + dz)


@dataclass(frozen=True)
class BlockProperties:
    """Static installation/electrical capabilities of a block kind.

    The previous prototype compressed these rules into two broad booleans
    (`can_be_powered`, `conducts_to_dust`).  That made a redstone block look too
    much like a strongly-powered ordinary block.  These fields keep the roles
    distinct:

    * a normal opaque block may *receive* weak/strong block power;
    * only a strongly-powered normal block may drive adjacent dust;
    * a redstone block is instead a direct signal source and does not acquire or
      propagate an ordinary block-power state through a neighboring block.
    """

    supports_components: bool
    receives_weak_power: bool = False
    receives_strong_power: bool = False
    repeater_reads_block_power: bool = False
    strong_power_drives_dust: bool = False

    @property
    def can_be_powered(self) -> bool:
        """Compatibility alias for older code."""
        return self.receives_weak_power or self.receives_strong_power

    @property
    def conducts_to_dust(self) -> bool:
        """Compatibility alias for older code."""
        return self.strong_power_drives_dust

    @property
    def accepts_weak_power(self) -> bool:
        return self.receives_weak_power

    @property
    def accepts_strong_power(self) -> bool:
        return self.receives_strong_power

    @property
    def repeater_reads_stored_power(self) -> bool:
        return self.repeater_reads_block_power


BLOCK_PROPERTIES: dict[BlockKind, BlockProperties] = {
    BlockKind.AIR: BlockProperties(False),
    BlockKind.SOLID: BlockProperties(
        supports_components=True,
        receives_weak_power=True,
        receives_strong_power=True,
        repeater_reads_block_power=True,
        strong_power_drives_dust=True,
    ),
    # Generic glass-like support: physical support is allowed by this model,
    # but it does not store ordinary block power.
    BlockKind.TRANSPARENT: BlockProperties(supports_components=True),
    BlockKind.REDSTONE_WIRE: BlockProperties(False),
    BlockKind.REDSTONE_TORCH: BlockProperties(False),
    BlockKind.REPEATER: BlockProperties(False),
    BlockKind.COMPARATOR: BlockProperties(False),
    BlockKind.LEVER: BlockProperties(False),
    # A constant direct source, deliberately not an ordinary powerable block.
    BlockKind.REDSTONE_BLOCK: BlockProperties(supports_components=True),
    # Mechanism semantics are not modeled yet; do not pretend it is an opaque
    # powered-block carrier until a dedicated probe/kernel rule exists.
    BlockKind.PISTON: BlockProperties(supports_components=True),
}


def properties(kind: BlockKind) -> BlockProperties:
    return BLOCK_PROPERTIES[kind]


def is_powerable_block(kind: BlockKind) -> bool:
    return properties(kind).can_be_powered


@dataclass(frozen=True)
class Block:
    kind: BlockKind
    facing: Facing | None = None
    powered: bool | None = None
    delay: int | None = None
    support_offset: Pos | None = None
    wire_connections: tuple[tuple[Facing, WireConnection], ...] | None = None

    def support_pos(self, pos: Pos) -> Pos | None:
        if self.support_offset is None:
            return None
        return pos.offset(
            self.support_offset.x,
            self.support_offset.y,
            self.support_offset.z,
        )

    def wire_connection(self, facing: Facing) -> WireConnection | None:
        if self.wire_connections is None:
            return None
        return dict(self.wire_connections).get(facing, WireConnection.NONE)


AIR = Block(BlockKind.AIR)


class World:
    def __init__(self) -> None:
        self._blocks: dict[Pos, Block] = {}

    def set(self, pos: Pos, block: Block) -> None:
        if block.kind is BlockKind.AIR:
            self._blocks.pop(pos, None)
        else:
            self._blocks[pos] = block

    def place(
        self,
        kind: BlockKind,
        x: int,
        y: int,
        z: int,
        *,
        facing: Facing | None = None,
        powered: bool | None = None,
        delay: int | None = None,
        support_offset: Pos | None = None,
        wire_connections: tuple[tuple[Facing, WireConnection], ...] | None = None,
    ) -> Pos:
        pos = Pos(x, y, z)
        if support_offset is None and kind in (
            BlockKind.REDSTONE_WIRE,
            BlockKind.REPEATER,
            BlockKind.COMPARATOR,
        ):
            support_offset = Pos(0, -1, 0)
        self.set(
            pos,
            Block(
                kind,
                facing,
                powered,
                delay,
                support_offset,
                wire_connections,
            ),
        )
        return pos

    def get(self, pos: Pos) -> Block:
        return self._blocks.get(pos, AIR)

    def remove(self, pos: Pos) -> None:
        self._blocks.pop(pos, None)

    def items(self) -> tuple[tuple[Pos, Block], ...]:
        return tuple(sorted(self._blocks.items()))

    def positions(self) -> tuple[Pos, ...]:
        return tuple(sorted(self._blocks))

    def clone(self) -> "World":
        world = World()
        world._blocks = dict(self._blocks)
        return world

    def fill(self, a: Pos, b: Pos, block: Block) -> None:
        for x in range(min(a.x, b.x), max(a.x, b.x) + 1):
            for y in range(min(a.y, b.y), max(a.y, b.y) + 1):
                for z in range(min(a.z, b.z), max(a.z, b.z) + 1):
                    self.set(Pos(x, y, z), block)

    def bounds(self) -> tuple[Pos, Pos] | None:
        if not self._blocks:
            return None
        positions = list(self._blocks)
        return (
            Pos(
                min(p.x for p in positions),
                min(p.y for p in positions),
                min(p.z for p in positions),
            ),
            Pos(
                max(p.x for p in positions),
                max(p.y for p in positions),
                max(p.z for p in positions),
            ),
        )

    def support_issues(self) -> tuple[str, ...]:
        requires_support = {
            BlockKind.REDSTONE_WIRE,
            BlockKind.REDSTONE_TORCH,
            BlockKind.REPEATER,
            BlockKind.COMPARATOR,
            BlockKind.LEVER,
        }
        issues: list[str] = []
        for pos, block in self.items():
            if block.kind not in requires_support:
                continue
            support = block.support_pos(pos)
            if support is None:
                issues.append(f"{block.kind.value} at {pos} has no support")
            elif not properties(self.get(support).kind).supports_components:
                issues.append(
                    f"{block.kind.value} at {pos} invalid support {support}"
                )
        return tuple(issues)

    def validate_supports(self) -> None:
        issues = self.support_issues()
        if issues:
            raise ValueError("Invalid support:\n" + "\n".join(issues))
