from __future__ import annotations

from dataclasses import dataclass
from itertools import product
from pathlib import Path
import json

from .model import Block, BlockKind, Facing, Pos, WireConnection, World
from .cells import PhysicalCell, PortKind


HORIZONTAL = {
    Facing.NORTH: (0, 0, -1),
    Facing.EAST: (1, 0, 0),
    Facing.SOUTH: (0, 0, 1),
    Facing.WEST: (-1, 0, 0),
}


@dataclass(frozen=True)
class JavaExportConfig:
    """
    Java Edition command exporter configuration.

    Coordinates are emitted relative to the command source (`~ ~ ~`) by
    default, so `/function <namespace>:build` constructs the circuit at the
    executor's current position.
    """
    namespace: str = "dustroute"
    solid_block: str = "minecraft:stone"
    transparent_block: str = "minecraft:glass"
    relative: bool = True
    test_delay_ticks: int = 20
    # Java 1.21.5 uses data pack version / pack format 71. Newer game
    # versions can override this without touching the exporter.
    pack_format: int = 71
    # Real-game test isolation area around the cell bounds.
    reset_margin_xz: int = 3
    reset_margin_up: int = 3
    reset_margin_down: int = 2
    foundation_depth: int = 1
    foundation_block: str = "minecraft:stone"


def _coord(n: int, relative: bool) -> str:
    if not relative:
        return str(n)
    return "~" if n == 0 else f"~{n}"


def _xyz(pos: Pos, config: JavaExportConfig) -> str:
    return " ".join((
        _coord(pos.x, config.relative),
        _coord(pos.y, config.relative),
        _coord(pos.z, config.relative),
    ))


def _facing_from_offset(offset: Pos) -> Facing | None:
    mapping = {
        (0, 0, -1): Facing.NORTH,
        (1, 0, 0): Facing.EAST,
        (0, 0, 1): Facing.SOUTH,
        (-1, 0, 0): Facing.WEST,
    }
    return mapping.get((offset.x, offset.y, offset.z))



def _opposite_horizontal(facing: Facing) -> Facing:
    return {
        Facing.NORTH: Facing.SOUTH,
        Facing.SOUTH: Facing.NORTH,
        Facing.EAST: Facing.WEST,
        Facing.WEST: Facing.EAST,
    }[facing]


def _bool(v: bool | None, default: bool = False) -> str:
    return "true" if (default if v is None else v) else "false"


def java_block_state(block: Block) -> str:
    """
    Translate one internal block into a Java Edition block-state string.

    This intentionally keeps the internal physical model independent from the
    Minecraft serialization format.
    """
    if block.kind is BlockKind.SOLID:
        return "minecraft:stone"
    if block.kind is BlockKind.TRANSPARENT:
        return "minecraft:glass"
    if block.kind is BlockKind.REDSTONE_BLOCK:
        return "minecraft:redstone_block"

    if block.kind is BlockKind.REDSTONE_WIRE:
        connections = dict(block.wire_connections or ())
        props = []
        for facing in (
            Facing.NORTH, Facing.EAST, Facing.SOUTH, Facing.WEST
        ):
            state = connections.get(facing, WireConnection.NONE)
            props.append(f"{facing.value}={state.value}")
        # Export topology, not simulated dynamic state.
        props.append("power=0")
        return "minecraft:redstone_wire[" + ",".join(props) + "]"

    if block.kind is BlockKind.REPEATER:
        internal = block.facing or Facing.NORTH
        facing = _opposite_horizontal(internal).value
        delay = max(1, min(4, block.delay or 1))
        return (
            "minecraft:repeater["
            f"delay={delay},facing={facing},locked=false,powered=false]"
        )

    if block.kind is BlockKind.COMPARATOR:
        internal = block.facing or Facing.NORTH
        facing = _opposite_horizontal(internal).value
        return (
            "minecraft:comparator["
            f"facing={facing},mode=compare,powered=false]"
        )

    if block.kind is BlockKind.REDSTONE_TORCH:
        support = block.support_offset
        if support is not None and support.y == 0:
            outward = _facing_from_offset(
                Pos(-support.x, -support.y, -support.z)
            )
            if outward is not None:
                return (
                    "minecraft:redstone_wall_torch["
                    f"facing={outward.value},lit=true]"
                )
        return "minecraft:redstone_torch[lit=true]"

    if block.kind is BlockKind.LEVER:
        support = block.support_offset
        powered = _bool(block.powered)
        if support is not None:
            if support.y < 0:
                facing = (
                    block.facing
                    if block.facing in HORIZONTAL
                    else Facing.NORTH
                )
                return (
                    "minecraft:lever["
                    f"face=floor,facing={facing.value},powered={powered}]"
                )
            if support.y > 0:
                facing = (
                    block.facing
                    if block.facing in HORIZONTAL
                    else Facing.NORTH
                )
                return (
                    "minecraft:lever["
                    f"face=ceiling,facing={facing.value},powered={powered}]"
                )
            outward = _facing_from_offset(
                Pos(-support.x, -support.y, -support.z)
            )
            if outward is not None:
                return (
                    "minecraft:lever["
                    f"face=wall,facing={outward.value},powered={powered}]"
                )
        facing = (
            block.facing if block.facing in HORIZONTAL else Facing.NORTH
        )
        return (
            "minecraft:lever["
            f"face=floor,facing={facing.value},powered={powered}]"
        )

    if block.kind is BlockKind.PISTON:
        facing = (block.facing or Facing.NORTH).value
        return f"minecraft:piston[facing={facing},extended=false]"

    if block.kind is BlockKind.AIR:
        return "minecraft:air"

    raise NotImplementedError(f"Java export for {block.kind}")


def world_setblock_commands(
    world: World,
    config: JavaExportConfig = JavaExportConfig(),
) -> tuple[str, ...]:
    """
    Emit support/full blocks before redstone components so attachment-sensitive
    blocks have a valid support when placed by Minecraft.
    """
    priority = {
        BlockKind.SOLID: 0,
        BlockKind.TRANSPARENT: 0,
        BlockKind.REDSTONE_BLOCK: 0,
        BlockKind.PISTON: 0,
        BlockKind.REDSTONE_WIRE: 1,
        BlockKind.REPEATER: 1,
        BlockKind.COMPARATOR: 1,
        BlockKind.REDSTONE_TORCH: 2,
        BlockKind.LEVER: 2,
    }

    items = sorted(
        world.items(),
        key=lambda item: (
            priority.get(item[1].kind, 1),
            item[0].y, item[0].x, item[0].z,
        ),
    )

    return tuple(
        f"setblock {_xyz(pos, config)} "
        f"{_java_block_state_configurable(block, config)} replace"
        for pos, block in items
    )


def _java_block_state_configurable(
    block: Block,
    config: JavaExportConfig,
) -> str:
    if block.kind is BlockKind.SOLID:
        return config.solid_block
    if block.kind is BlockKind.TRANSPARENT:
        return config.transparent_block
    return java_block_state(block)


def test_region_bounds(
    world: World,
    config: JavaExportConfig = JavaExportConfig(),
) -> tuple[Pos, Pos] | None:
    """
    Bounding box used to isolate a real-game test case.

    The box includes the circuit, input drivers, some redstone influence room,
    and a small volume below for a fresh support/foundation layer.
    """
    bounds = world.bounds()
    if bounds is None:
        return None
    lo, hi = bounds
    return (
        Pos(
            lo.x - config.reset_margin_xz,
            lo.y - config.reset_margin_down,
            lo.z - config.reset_margin_xz,
        ),
        Pos(
            hi.x + config.reset_margin_xz,
            hi.y + config.reset_margin_up,
            hi.z + config.reset_margin_xz,
        ),
    )


def reset_test_region_commands(
    world: World,
    config: JavaExportConfig = JavaExportConfig(),
) -> tuple[str, ...]:
    """
    Safely clear the isolated test region.

    Attachment-sensitive redstone components are removed first. If their
    support blocks were removed first, Minecraft would break them via neighbor
    updates and spawn item entities, making the test area look unstable and
    potentially interfering with inspection.
    """
    bounds = test_region_bounds(world, config)
    if bounds is None:
        return ()
    lo, hi = bounds
    region = f"{_xyz(lo,config)} {_xyz(hi,config)}"

    # Remove components before supports/full blocks.
    component_blocks = (
        "minecraft:redstone_wire",
        "minecraft:repeater",
        "minecraft:comparator",
        "minecraft:redstone_torch",
        "minecraft:redstone_wall_torch",
        "minecraft:lever",
    )

    cmds = [
        f"fill {region} minecraft:air replace {block}"
        for block in component_blocks
    ]

    # Now it is safe to remove the remaining blocks.
    cmds.append(f"fill {region} minecraft:air replace")

    # Clean up any stale drops already present in the isolated AABB.
    # Selector coordinates are marker-relative because this command is run
    # from the per-gate origin marker.
    dx = hi.x - lo.x
    dy = hi.y - lo.y
    dz = hi.z - lo.z
    cmds.append(
        f"execute positioned {_xyz(lo,config)} run "
        f"kill @e[type=minecraft:item,dx={dx},dy={dy},dz={dz}]"
    )

    return tuple(cmds)


def foundation_commands(
    world: World,
    config: JavaExportConfig = JavaExportConfig(),
) -> tuple[str, ...]:
    """
    Create a flat deterministic foundation below the circuit.

    This is intentionally broader than individual component support blocks, so
    tests are easy to inspect manually and do not depend on existing terrain.
    """
    bounds = world.bounds()
    if bounds is None:
        return ()
    lo, hi = bounds

    # The foundation sits directly beneath the minimum circuit Y. Existing
    # explicitly modeled support blocks are then placed over/within it by build.
    top_y = lo.y - 1
    bottom_y = top_y - max(0, config.foundation_depth - 1)

    a = Pos(
        lo.x - config.reset_margin_xz,
        bottom_y,
        lo.z - config.reset_margin_xz,
    )
    b = Pos(
        hi.x + config.reset_margin_xz,
        top_y,
        hi.z + config.reset_margin_xz,
    )
    return (
        f"fill {_xyz(a,config)} {_xyz(b,config)} "
        f"{config.foundation_block} replace",
    )


def isolated_build_commands(
    world: World,
    config: JavaExportConfig = JavaExportConfig(),
) -> tuple[str, ...]:
    """
    Deterministically reconstruct a test bench:
      clear -> foundation -> modeled circuit blocks.
    """
    return (
        *reset_test_region_commands(world, config),
        *foundation_commands(world, config),
        *world_setblock_commands(world, config),
    )


def cleanup_commands(
    world: World,
    config: JavaExportConfig = JavaExportConfig(),
    *,
    margin: int = 2,
) -> tuple[str, ...]:
    bounds = world.bounds()
    if bounds is None:
        return ()
    lo, hi = bounds
    lo = Pos(lo.x-margin, lo.y-margin, lo.z-margin)
    hi = Pos(hi.x+margin, hi.y+margin, hi.z+margin)
    return (
        f"fill {_xyz(lo,config)} {_xyz(hi,config)} minecraft:air replace",
    )


def _input_driver_position(port) -> tuple[Pos, Pos | None]:
    """
    Return (driver position, support position if a lever is used).
    """
    facing = (
        port.facing
        if port.facing in HORIZONTAL
        else Facing.WEST
    )
    dx,dy,dz = HORIZONTAL[facing]

    if port.kind is PortKind.BLOCK_POWER:
        return port.pos.offset(dx,dy,dz), port.pos

    if port.kind is PortKind.WIRE:
        return port.pos.offset(dx,dy,dz), None

    raise ValueError(port.kind)


def _input_commands(
    cell: PhysicalCell,
    values: dict[str, bool],
    config: JavaExportConfig,
) -> tuple[str, ...]:
    cmds = []

    for port in cell.inputs:
        value = bool(values[port.name])
        driver, support = _input_driver_position(port)

        if port.kind is PortKind.BLOCK_POWER:
            # A wall-mounted lever is an explicit real-game power source for
            # the support block targeted by the port.
            delta = Pos(
                support.x-driver.x,
                support.y-driver.y,
                support.z-driver.z,
            )
            outward = _facing_from_offset(
                Pos(-delta.x,-delta.y,-delta.z)
            ) or Facing.NORTH

            state = (
                "minecraft:lever["
                f"face=wall,facing={outward.value},"
                f"powered={'true' if value else 'false'}]"
            )
            cmds.append(
                f"setblock {_xyz(driver,config)} {state} replace"
            )

        elif port.kind is PortKind.WIRE:
            if value:
                cmds.append(
                    f"setblock {_xyz(driver,config)} "
                    "minecraft:redstone_block replace"
                )
            else:
                cmds.append(
                    f"setblock {_xyz(driver,config)} minecraft:air replace"
                )

    return tuple(cmds)


def _output_boolean_check(
    cell: PhysicalCell,
    expected: bool,
    config: JavaExportConfig,
) -> tuple[str, ...]:
    if len(cell.outputs) != 1:
        raise NotImplementedError(
            "MVP test exporter currently supports one output"
        )

    out = cell.outputs[0]
    pos = _xyz(out.pos, config)

    if out.kind is not PortKind.WIRE:
        raise NotImplementedError(
            "MVP output checker currently expects a WIRE output"
        )

    # A wire output is logically off iff it is exactly power=0. `unless` lets
    # us test Boolean ON without enumerating power=1..15.
    if expected:
        condition = (
            f"unless block {pos} minecraft:redstone_wire[power=0]"
        )
    else:
        condition = (
            f"if block {pos} minecraft:redstone_wire[power=0]"
        )

    return (
        f'execute {condition} run tellraw @a '
        f'{{"text":"PASS","color":"green"}}',
        f'execute unless {condition[3:] if condition.startswith("if ") else condition[7:]} run tellraw @a '
        f'{{"text":"FAIL","color":"red"}}',
    )


def _output_boolean_commands(
    cell: PhysicalCell,
    expected: bool,
    config: JavaExportConfig,
) -> tuple[str, ...]:
    """
    Same Boolean output test as above, expressed explicitly to avoid command
    inversion ambiguity.
    """
    out = cell.outputs[0]
    pos = _xyz(out.pos, config)
    state = f"minecraft:redstone_wire[power=0]"

    if expected:
        return (
            f'execute unless block {pos} {state} run tellraw @a '
            '{"text":"PASS: output ON","color":"green"}',
            f'execute if block {pos} {state} run tellraw @a '
            '{"text":"FAIL: output is OFF","color":"red"}',
        )
    return (
        f'execute if block {pos} {state} run tellraw @a '
        '{"text":"PASS: output OFF","color":"green"}',
        f'execute unless block {pos} {state} run tellraw @a '
        '{"text":"FAIL: output is ON","color":"red"}',
    )


def export_cell_mcfunctions(
    cell: PhysicalCell,
    output_dir: str | Path,
    *,
    truth_fn,
    config: JavaExportConfig = JavaExportConfig(),
) -> Path:
    """
    Export a small Java data-pack-style function tree for real-game validation.

    Files:
      build.mcfunction
      cleanup.mcfunction
      cases/<bits>.mcfunction
      checks/<bits>.mcfunction

    Each case applies input sources and schedules its check after
    `test_delay_ticks` game ticks.
    """
    output_dir = Path(output_dir)
    funcs = output_dir / "data" / config.namespace / "function"
    cases_dir = funcs / "cases"
    checks_dir = funcs / "checks"
    cases_dir.mkdir(parents=True, exist_ok=True)
    checks_dir.mkdir(parents=True, exist_ok=True)

    (funcs / "build.mcfunction").write_text(
        "\n".join(world_setblock_commands(cell.world, config)) + "\n",
        encoding="utf-8",
    )
    (funcs / "cleanup.mcfunction").write_text(
        "\n".join(cleanup_commands(cell.world, config)) + "\n",
        encoding="utf-8",
    )

    input_names = tuple(p.name for p in cell.inputs)

    for bits in product((False, True), repeat=len(input_names)):
        values = dict(zip(input_names, bits))
        tag = "".join("1" if v else "0" for v in bits) or "0"
        expected = bool(truth_fn(values))

        case_lines = list(_input_commands(cell, values, config))
        case_lines.append(
            f"schedule function {config.namespace}:checks/{tag} "
            f"{config.test_delay_ticks}t replace"
        )
        (cases_dir / f"{tag}.mcfunction").write_text(
            "\n".join(case_lines) + "\n",
            encoding="utf-8",
        )

        check_lines = [
            f'tellraw @a {{"text":"case {tag}: {values} -> '
            f'expected {int(expected)}","color":"gray"}}'
        ]
        check_lines.extend(
            _output_boolean_commands(cell, expected, config)
        )
        (checks_dir / f"{tag}.mcfunction").write_text(
            "\n".join(check_lines) + "\n",
            encoding="utf-8",
        )

    # Recent Java versions support data packs and function files; pack format
    # itself changes frequently, so keep it configurable/obvious rather than
    # pretending one hard-coded number is universal.
    pack_meta = {
        "pack": {
            "pack_format": config.pack_format,
            "description": "DustRoute real-game validation",
        }
    }
    (output_dir / "pack.mcmeta").write_text(
        json.dumps(pack_meta, indent=2) + "\n",
        encoding="utf-8",
    )

    return output_dir


@dataclass(frozen=True)
class GateTestSpec:
    api_name: str
    cell: PhysicalCell
    truth_fn: object


def _origin_tag(
    config: JavaExportConfig,
    gate_name: str | None = None,
) -> str:
    base = config.namespace.replace(":", "_")
    if gate_name is None:
        return base + "_origin"
    return base + "_" + gate_name + "_origin"


def _write_gate_test_api(
    funcs: Path,
    spec: GateTestSpec,
    config: JavaExportConfig,
) -> None:
    """Write marker-anchored public API plus internal relative functions."""
    gate_dir = funcs / spec.api_name
    cases_dir = gate_dir / "cases"
    checks_dir = gate_dir / "checks"
    internal_cases = gate_dir / "_cases"
    internal_checks = gate_dir / "_checks"
    for d in (gate_dir,cases_dir,checks_dir,internal_cases,internal_checks):
        d.mkdir(parents=True, exist_ok=True)

    tag = _origin_tag(config, spec.api_name)

    # Internal builder: coordinates are relative to the marker.
    build_lines = list(isolated_build_commands(spec.cell.world, config))
    off_values = {p.name: False for p in spec.cell.inputs}
    build_lines.extend(_input_commands(spec.cell, off_values, config))
    build_lines.append(
        f'tellraw @a {{"text":"Built {spec.api_name.upper()} test bench",'
        f'"color":"aqua"}}'
    )
    (gate_dir / "_build.mcfunction").write_text(
        "\n".join(build_lines) + "\n",
        encoding="utf-8",
    )

    # Public gate build establishes a persistent origin, so the player may walk
    # around after construction and cases still target the correct circuit.
    public_build = (
        f"kill @e[type=minecraft:marker,tag={tag}]\n"
        f'summon minecraft:marker ~ ~ ~ {{Tags:["{tag}"]}}\n'
        f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
        f"run function {config.namespace}:{spec.api_name}/_build\n"
    )
    (gate_dir / "build.mcfunction").write_text(public_build,encoding="utf-8")

    # Cleanup is also anchored at the stored origin.
    cleanup_inner = gate_dir / "_cleanup.mcfunction"
    cleanup_inner.write_text(
        "\n".join(cleanup_commands(spec.cell.world, config, margin=3)) + "\n",
        encoding="utf-8",
    )
    (gate_dir / "cleanup.mcfunction").write_text(
        f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
        f"run function {config.namespace}:{spec.api_name}/_cleanup\n"
        f"kill @e[type=minecraft:marker,tag={tag}]\n",
        encoding="utf-8",
    )

    input_names = tuple(p.name for p in spec.cell.inputs)
    for bits in product((False, True), repeat=len(input_names)):
        values = dict(zip(input_names, bits))
        bits_tag = "".join("1" if v else "0" for v in bits) or "0"
        expected = bool(spec.truth_fn(values))

        # Internal case mutates blocks at marker-relative coordinates. The
        # delayed check schedules a public wrapper, because scheduled functions
        # do not preserve the original execution position.
        # Each case is self-contained: remove previous circuit/state and
        # nearby external interference, rebuild the test bench, then apply the
        # requested input vector.
        case_lines = list(isolated_build_commands(spec.cell.world, config))
        case_lines.extend(_input_commands(spec.cell, values, config))
        case_lines.append(
            f'tellraw @a {{"text":"{spec.api_name.upper()} case {bits_tag}",'
            f'"color":"yellow"}}'
        )
        case_lines.append(
            f"schedule function {config.namespace}:{spec.api_name}/checks/{bits_tag} "
            f"{config.test_delay_ticks}t replace"
        )
        (internal_cases / f"{bits_tag}.mcfunction").write_text(
            "\n".join(case_lines) + "\n",
            encoding="utf-8",
        )
        (cases_dir / f"{bits_tag}.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:{spec.api_name}/_cases/{bits_tag}\n",
            encoding="utf-8",
        )

        check_lines = [
            f'tellraw @a {{"text":"{spec.api_name.upper()} {values} -> '
            f'expected {int(expected)}","color":"gray"}}'
        ]
        check_lines.extend(_output_boolean_commands(spec.cell, expected, config))
        (internal_checks / f"{bits_tag}.mcfunction").write_text(
            "\n".join(check_lines) + "\n",
            encoding="utf-8",
        )
        (checks_dir / f"{bits_tag}.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:{spec.api_name}/_checks/{bits_tag}\n",
            encoding="utf-8",
        )


def _cell_width(cell: PhysicalCell) -> int:
    bounds = cell.world.bounds()
    if bounds is None:
        return 1
    lo, hi = bounds
    return hi.x - lo.x + 1


def _gallery_offsets(
    specs: tuple[GateTestSpec, ...],
    config: JavaExportConfig,
    *,
    gap: int = 8,
) -> dict[str, int]:
    """
    Place test benches left-to-right with enough room for each case's isolated
    reset region. The gap accounts for reset margins on both neighboring cells.
    """
    offsets: dict[str, int] = {}
    x = 0
    for spec in specs:
        offsets[spec.api_name] = x
        x += (
            _cell_width(spec.cell)
            + 2 * config.reset_margin_xz
            + gap
        )
    return offsets


def _write_gallery_api(
    funcs: Path,
    specs: tuple[GateTestSpec, ...],
    config: JavaExportConfig,
) -> None:
    offsets = _gallery_offsets(specs, config)

    # Build every gate side-by-side. Each gate owns a separate marker, so its
    # normal public case API continues to work after the player walks around.
    build_lines = [
        'tellraw @a {"text":"Building gate gallery: NOT | OR | AND | NAND",'
        '"color":"gold"}',
    ]
    for spec in specs:
        x = offsets[spec.api_name]
        build_lines.append(
            f"execute positioned {_coord(x,config.relative)} ~ ~ "
            f"run function {config.namespace}:{spec.api_name}/build"
        )
    build_lines.append(
        'tellraw @a '
        + json.dumps({
            "text": "Starting automatic truth-table tests...",
            "color": "aqua",
        }, separators=(",", ":"))
    )
    build_lines.append(
        f"function {config.namespace}:tests_run"
    )
    (funcs / "tests.mcfunction").write_text(
        "\n".join(build_lines) + "\n",
        encoding="utf-8",
    )

    cleanup_lines = [
        f"function {config.namespace}:{spec.api_name}/cleanup"
        for spec in specs
    ]
    (funcs / "tests_cleanup.mcfunction").write_text(
        "\n".join(cleanup_lines) + "\n",
        encoding="utf-8",
    )

    # Sequential automatic run. Each next case starts after the prior case has
    # had time to settle and print its result.
    spacing = max(2, config.test_delay_ticks + 2)
    timeline: list[tuple[int, str]] = []
    tick = 0
    for spec in specs:
        bit_count = len(spec.cell.inputs)
        for bits in product((False, True), repeat=bit_count):
            bit_tag = "".join("1" if v else "0" for v in bits) or "0"
            timeline.append(
                (
                    tick,
                    f"{config.namespace}:{spec.api_name}/cases/{bit_tag}",
                )
            )
            tick += spacing

    run_lines = [
        'tellraw @a {"text":"Starting automatic gate tests",'
        '"color":"gold"}',
    ]
    for delay, function_name in timeline:
        if delay == 0:
            run_lines.append(f"function {function_name}")
        else:
            run_lines.append(
                f"schedule function {function_name} {delay}t replace"
            )
    run_lines.append(
        f'tellraw @a {{"text":"Scheduled {len(timeline)} cases '
        f'over {tick} ticks","color":"gray"}}'
    )
    (funcs / "tests_run.mcfunction").write_text(
        "\n".join(run_lines) + "\n",
        encoding="utf-8",
    )

    # Human-readable order/reference.
    guide_lines = [
        'tellraw @a {"text":"Gallery order (left -> right):",'
        '"color":"gold"}',
    ]
    for spec in specs:
        x = offsets[spec.api_name]
        guide_lines.append(
            f'tellraw @a {{"text":"  x+{x}: {spec.api_name.upper()} '
            f'  /function {config.namespace}:{spec.api_name}/cases/...",'
            '"color":"white"}'
        )
    (funcs / "tests_help.mcfunction").write_text(
        "\n".join(guide_lines) + "\n",
        encoding="utf-8",
    )



def export_gate_test_datapack(
    output_dir: str | Path,
    *,
    config: JavaExportConfig = JavaExportConfig(),
) -> Path:
    """
    Export a human-friendly gate test API.

    Public functions:
      /function <ns>:build
          Alias for NOT, preserving the first simple workflow.

      /function <ns>:build_not
      /function <ns>:build_or
      /function <ns>:build_and
      /function <ns>:build_nand

      /function <ns>:not/cases/0
      /function <ns>:not/cases/1

      /function <ns>:and/cases/00
      /function <ns>:and/cases/01
      /function <ns>:and/cases/10
      /function <ns>:and/cases/11

    OR/NAND use the same two-input case naming. Each `build_*` creates a
    complete manual test bench with input drivers already present and OFF.
    """
    # Imported lazily to avoid a module cycle: compiler owns the current
    # baseline OR/AND/NAND physical macros.
    from .compiler import make_or_cell, make_and_cell, make_nand_cell
    from .cells import make_not_cell

    specs = (
        GateTestSpec("not", make_not_cell(), lambda i: not i["a"]),
        GateTestSpec("or", make_or_cell(), lambda i: i["a"] or i["b"]),
        GateTestSpec("and", make_and_cell(), lambda i: i["a"] and i["b"]),
        GateTestSpec(
            "nand",
            make_nand_cell(),
            lambda i: not (i["a"] and i["b"]),
        ),
    )

    output_dir = Path(output_dir)
    funcs = output_dir / "data" / config.namespace / "function"
    funcs.mkdir(parents=True, exist_ok=True)

    for spec in specs:
        _write_gate_test_api(funcs, spec, config)

        # Flat aliases are intentionally easy to discover/type.
        (funcs / f"build_{spec.api_name}.mcfunction").write_text(
            f"function {config.namespace}:{spec.api_name}/build\n",
            encoding="utf-8",
        )
        (funcs / f"cleanup_{spec.api_name}.mcfunction").write_text(
            f"function {config.namespace}:{spec.api_name}/cleanup\n",
            encoding="utf-8",
        )

    _write_gallery_api(funcs, specs, config)

    # Keep `/function namespace:build` as the simplest entry point: NOT.
    (funcs / "build.mcfunction").write_text(
        f"function {config.namespace}:not/build\n",
        encoding="utf-8",
    )
    (funcs / "cleanup.mcfunction").write_text(
        f"function {config.namespace}:not/cleanup\n",
        encoding="utf-8",
    )

    help_lines = [
        'tellraw @a {"text":"DustRoute gate test API","color":"gold"}',
        *[
            f'tellraw @a {{"text":"/function {config.namespace}:build_{name}",'
            f'"color":"white"}}'
            for name in ("not", "or", "and", "nand")
        ],
        f'tellraw @a {{"text":"/function {config.namespace}:tests",'
        f'"color":"aqua"}}',
        f'tellraw @a {{"text":"/function {config.namespace}:tests_run",'
        f'"color":"aqua"}}',
        f'tellraw @a {{"text":"cases: {config.namespace}:<gate>/cases/<bits>",'
        f'"color":"gray"}}',
    ]
    (funcs / "help.mcfunction").write_text(
        "\n".join(help_lines) + "\n",
        encoding="utf-8",
    )

    pack_meta = {
        "pack": {
            "pack_format": config.pack_format,
            "description": "DustRoute gate test API",
        }
    }
    (output_dir / "pack.mcmeta").write_text(
        json.dumps(pack_meta, indent=2) + "\n",
        encoding="utf-8",
    )
    return output_dir
