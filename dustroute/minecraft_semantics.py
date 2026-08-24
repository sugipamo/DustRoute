from __future__ import annotations

from dataclasses import dataclass
from itertools import product
from pathlib import Path
import json

from .model import Block, BlockKind, Facing, Pos, World
from .wire import update_wire_shapes
from .minecraft_export import (
    JavaExportConfig,
    isolated_build_commands,
    _xyz,
    _coord,
)


@dataclass(frozen=True)
class SemanticProbe:
    name: str
    description: str
    world: World
    stimulus: tuple[str, ...]
    checks: tuple[str, ...]


def _setblock(pos: Pos, state: str, config: JavaExportConfig) -> str:
    return f"setblock {_xyz(pos,config)} {state} replace"


def _wire_on(pos: Pos, config: JavaExportConfig, label: str) -> tuple[str, ...]:
    xyz=_xyz(pos,config)
    return (
        f'execute unless block {xyz} minecraft:redstone_wire[power=0] run tellraw @a '
        + json.dumps({"text":f"PASS {label}","color":"green"},separators=(",",":")),
        f'execute if block {xyz} minecraft:redstone_wire[power=0] run tellraw @a '
        + json.dumps({"text":f"FAIL {label}: wire is OFF","color":"red"},separators=(",",":")),
    )


def _wire_off(pos: Pos, config: JavaExportConfig, label: str) -> tuple[str, ...]:
    xyz=_xyz(pos,config)
    return (
        f'execute if block {xyz} minecraft:redstone_wire[power=0] run tellraw @a '
        + json.dumps({"text":f"PASS {label}","color":"green"},separators=(",",":")),
        f'execute unless block {xyz} minecraft:redstone_wire[power=0] run tellraw @a '
        + json.dumps({"text":f"FAIL {label}: wire is ON","color":"red"},separators=(",",":")),
    )


def _wire_exact(pos: Pos, power: int, config: JavaExportConfig, label: str) -> tuple[str, ...]:
    xyz=_xyz(pos,config)
    state=f"minecraft:redstone_wire[power={power}]"
    return (
        f'execute if block {xyz} {state} run tellraw @a '
        + json.dumps({"text":f"PASS {label} = {power}","color":"green"},separators=(",",":")),
        f'execute unless block {xyz} {state} run tellraw @a '
        + json.dumps({"text":f"FAIL {label}: expected power {power}","color":"red"},separators=(",",":")),
    )


def _torch_lit(pos: Pos, expected: bool, config: JavaExportConfig, label: str) -> tuple[str, ...]:
    xyz=_xyz(pos,config)
    state=f"minecraft:redstone_wall_torch[lit={'true' if expected else 'false'}]"
    opposite=f"minecraft:redstone_wall_torch[lit={'false' if expected else 'true'}]"
    return (
        f'execute if block {xyz} {state} run tellraw @a '
        + json.dumps({"text":f"PASS {label}","color":"green"},separators=(",",":")),
        f'execute if block {xyz} {opposite} run tellraw @a '
        + json.dumps({"text":f"FAIL {label}","color":"red"},separators=(",",":")),
    )


def _supports(world: World, positions: tuple[Pos,...]) -> None:
    for p in positions:
        world.set(p,Block(BlockKind.SOLID))


def semantic_probes(config: JavaExportConfig) -> tuple[SemanticProbe,...]:
    """
    Minimal probes for simulator semantics.

    Crucially, the base World is passive/unpowered. `stimulus` is applied only
    after Minecraft has had time to settle the constructed geometry.
    """
    probes=[]

    # 01: source -> dust = 15.
    w=World()
    _supports(w,(Pos(1,0,0),))
    w.place(BlockKind.REDSTONE_WIRE,1,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "01_source_to_dust",
        "redstone block directly drives adjacent dust",
        w,
        (_setblock(Pos(0,1,0),"minecraft:redstone_block",config),),
        _wire_exact(Pos(1,1,0),15,config,"source -> dust"),
    ))

    # 02: horizontal dust decay.
    w=World()
    _supports(w,tuple(Pos(x,0,0) for x in range(1,4)))
    for x in range(1,4):
        w.place(BlockKind.REDSTONE_WIRE,x,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "02_dust_decay",
        "dust propagates 15,14,13",
        w,
        (_setblock(Pos(0,1,0),"minecraft:redstone_block",config),),
        (
            *_wire_exact(Pos(1,1,0),15,config,"dust[0]"),
            *_wire_exact(Pos(2,1,0),14,config,"dust[1]"),
            *_wire_exact(Pos(3,1,0),13,config,"dust[2]"),
        ),
    ))

    # 03: correct weak-power isolation.
    #
    #   source -> dust -> [solid block] -> dust
    #
    # Both dust pieces are at the same Y and separated by the opaque block.
    # There is no up/down dust path, so a powered output dust would have to be
    # caused by the block, not by direct dust connectivity.
    w=World()
    _supports(w,(Pos(1,0,0),Pos(-1,0,0)))
    w.place(BlockKind.REDSTONE_WIRE,1,1,0)
    w.set(Pos(0,1,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,-1,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "03_weak_block_no_dust_return",
        "dust weak-powers a solid block; that weak power does not emerge as dust",
        w,
        (_setblock(Pos(2,1,0),"minecraft:redstone_block",config),),
        _wire_off(Pos(-1,1,0),config,"weak block -> dust is blocked"),
    ))

    # 04: repeater can read the same weak-powered block.
    #
    #   source -> dust -> [solid block] -> repeater -> dust
    w=World()
    _supports(w,(Pos(1,0,0),Pos(-1,0,0),Pos(-2,0,0)))
    w.place(BlockKind.REDSTONE_WIRE,1,1,0)
    w.set(Pos(0,1,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REPEATER,-1,1,0,facing=Facing.WEST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,-2,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "04_weak_block_to_repeater",
        "repeater reads a solid block weak-powered by dust",
        w,
        (_setblock(Pos(2,1,0),"minecraft:redstone_block",config),),
        _wire_on(Pos(-2,1,0),config,"weak block -> repeater"),
    ))

    # 05: repeater restores 15.
    w=World()
    _supports(w,tuple(Pos(x,0,0) for x in range(0,5)))
    for x in range(0,3):
        w.place(BlockKind.REDSTONE_WIRE,x,1,0)
    w.place(BlockKind.REPEATER,3,1,0,facing=Facing.EAST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,4,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "05_repeater_refresh",
        "repeater restores downstream dust to 15",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _wire_exact(Pos(4,1,0),15,config,"repeater refresh"),
    ))

    # 06: repeater strong-powers a block, which then drives dust.
    #
    # The source is deliberately absent while the geometry is built. It is
    # inserted only during STIMULATE, forcing an ordinary neighbor update after
    # all downstream components already exist.
    w=World()
    _supports(w,(Pos(0,0,0),Pos(1,0,0),Pos(3,0,0)))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.place(BlockKind.REPEATER,1,1,0,facing=Facing.EAST,delay=1)
    w.set(Pos(2,1,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,3,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "06_repeater_strong_block",
        "repeater strongly powers a solid block; that block drives adjacent dust",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _wire_on(Pos(3,1,0),config,"repeater -> block -> dust"),
    ))

    # 07: support OFF -> attached torch ON.
    w=World()
    w.set(Pos(0,0,0),Block(BlockKind.SOLID))
    w.place(
        BlockKind.REDSTONE_TORCH,1,0,0,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )
    probes.append(SemanticProbe(
        "07_torch_unpowered_support",
        "attached torch is lit when its support block is unpowered",
        w,
        (),
        _torch_lit(Pos(1,0,0),True,config,"torch support OFF -> torch ON"),
    ))

    # 08: weak-power the torch support block through dust.
    #
    #   redstone block -> dust -> [support block] <- attached torch
    #
    # The redstone block is added only during STIMULATE. The dust then powers
    # the ordinary support block, and the probe asks whether the attached torch
    # observes that powered-block state and turns OFF.
    w=World()
    w.set(Pos(-1,0,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,-1,1,0)
    w.set(Pos(0,1,0),Block(BlockKind.SOLID))
    w.place(
        BlockKind.REDSTONE_TORCH,1,1,0,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "08_torch_powered_support",
        "dust-powered support block turns its attached torch off",
        w,
        (_setblock(Pos(-2,1,0),"minecraft:redstone_block",config),),
        _torch_lit(Pos(1,1,0),False,config,"dust -> support -> torch OFF"),
    ))

    # 09: a redstone block adjacent to an ordinary solid block does not make
    # that solid block become a powered support for a torch on its far side.
    # This captures the distinction between a constant source block and a
    # normal block's powered state.
    w=World()
    w.set(Pos(0,0,0),Block(BlockKind.SOLID))
    w.place(
        BlockKind.REDSTONE_TORCH,1,0,0,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )
    probes.append(SemanticProbe(
        "09_redstone_block_no_block_propagation",
        "redstone block does not propagate powered-block state through adjacent solid",
        w,
        (_setblock(Pos(-1,0,0),"minecraft:redstone_block",config),),
        _torch_lit(Pos(1,0,0),True,config,"redstone block beside support -> torch stays ON"),
    ))


    # 10: canonical dust -> repeater -> dust boundary.
    w=World()
    _supports(w,(Pos(0,0,0),Pos(1,0,0),Pos(2,0,0)))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.place(BlockKind.REPEATER,1,1,0,facing=Facing.EAST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,2,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "10_dust_repeater_dust",
        "straight dust -> repeater -> dust connection",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _wire_exact(Pos(2,1,0),15,config,"dust -> repeater -> dust"),
    ))

    # 11: a repeater must not read power presented at its output/front side.
    w=World()
    _supports(w,(Pos(0,0,0),Pos(1,0,0),Pos(2,0,0)))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.place(BlockKind.REPEATER,1,1,0,facing=Facing.EAST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,2,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "11_repeater_reverse_blocked",
        "repeater does not conduct backwards from output side",
        w,
        (_setblock(Pos(3,1,0),"minecraft:redstone_block",config),),
        _wire_off(Pos(0,1,0),config,"repeater reverse direction blocked"),
    ))

    # 12: explicit input-side dust is accepted by a repeater.
    w=World()
    _supports(w,tuple(Pos(x,0,0) for x in range(4)))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.place(BlockKind.REDSTONE_WIRE,1,1,0)
    w.place(BlockKind.REPEATER,2,1,0,facing=Facing.EAST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,3,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "12_dust_to_repeater_input",
        "dust enters the back/input side of a repeater",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _wire_on(Pos(3,1,0),config,"dust -> repeater input"),
    ))

    # 13: horizontal L-corner.
    w=World()
    _supports(w,(Pos(0,0,0),Pos(1,0,0),Pos(1,0,1)))
    for q in (Pos(0,1,0),Pos(1,1,0),Pos(1,1,1)):
        w.place(BlockKind.REDSTONE_WIRE,q.x,q.y,q.z)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "13_dust_corner",
        "dust carries signal around a horizontal corner",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _wire_on(Pos(1,1,1),config,"dust corner"),
    ))

    # 14: one-block stair upward.
    w=World()
    w.set(Pos(0,0,0),Block(BlockKind.SOLID))
    w.set(Pos(1,1,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.place(BlockKind.REDSTONE_WIRE,1,2,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "14_dust_stair_up",
        "dust climbs one block when the stair geometry is open",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _wire_on(Pos(1,2,0),config,"dust stair up"),
    ))

    # 15: the same stair in the descending signal direction.
    w=World()
    w.set(Pos(0,1,0),Block(BlockKind.SOLID))
    w.set(Pos(1,0,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,0,2,0)
    w.place(BlockKind.REDSTONE_WIRE,1,1,0)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "15_dust_stair_down",
        "dust descends one block through valid stair geometry",
        w,
        (_setblock(Pos(-1,2,0),"minecraft:redstone_block",config),),
        _wire_on(Pos(1,1,0),config,"dust stair down"),
    ))

    # 16: leaf dust explicitly powers a BLOCK_POWER-style support.
    w=World()
    w.set(Pos(0,0,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.set(Pos(1,1,0),Block(BlockKind.SOLID))
    w.place(
        BlockKind.REDSTONE_TORCH,2,1,0,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "16_leaf_dust_block_power",
        "leaf dust powers the adjacent support block",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _torch_lit(Pos(2,1,0),False,config,"leaf dust -> BLOCK_POWER"),
    ))

    # 17: fan-out junction remains separate from the final leaf stub.
    w=World()
    _supports(w,(Pos(0,0,0),Pos(1,0,0),Pos(2,0,0),Pos(1,0,1)))
    for q in (Pos(0,1,0),Pos(1,1,0),Pos(2,1,0),Pos(1,1,1)):
        w.place(BlockKind.REDSTONE_WIRE,q.x,q.y,q.z)
    w.set(Pos(3,1,0),Block(BlockKind.SOLID))
    w.place(
        BlockKind.REDSTONE_TORCH,4,1,0,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "17_branch_leaf_block_power",
        "fan-out junction feeds a separate leaf BLOCK_POWER stub",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        (
            *_wire_on(Pos(1,1,1),config,"fan-out side branch"),
            *_torch_lit(Pos(4,1,0),False,config,"fan-out leaf -> BLOCK_POWER"),
        ),
    ))

    # 18: refreshed route then leaf BLOCK_POWER terminal.
    w=World()
    _supports(w,tuple(Pos(x,0,0) for x in range(5)))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.place(BlockKind.REPEATER,1,1,0,facing=Facing.EAST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,2,1,0)
    w.place(BlockKind.REDSTONE_WIRE,3,1,0)
    w.set(Pos(4,1,0),Block(BlockKind.SOLID))
    w.place(
        BlockKind.REDSTONE_TORCH,5,1,0,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "18_repeater_route_block_power",
        "repeater -> routed dust -> BLOCK_POWER leaf",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _torch_lit(Pos(5,1,0),False,config,"repeater route -> BLOCK_POWER"),
    ))

    # 19: buffered-cell-style output crosses a routing boundary into a block input.
    w=World()
    _supports(w,tuple(Pos(x,0,0) for x in range(6)))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)                  # external cell input
    w.place(BlockKind.REPEATER,1,1,0,facing=Facing.EAST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,2,1,0)                  # cell output
    w.place(BlockKind.REDSTONE_WIRE,3,1,0)                  # routed segment
    w.place(BlockKind.REDSTONE_WIRE,4,1,0)                  # leaf terminal
    w.set(Pos(5,1,0),Block(BlockKind.SOLID))                # next cell input block
    w.place(
        BlockKind.REDSTONE_TORCH,6,1,0,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "19_cell_output_to_block_input",
        "buffered cell output crosses routing into BLOCK_POWER input",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _torch_lit(Pos(6,1,0),False,config,"cell output -> route -> cell input"),
    ))

    # 20: repeater output immediately followed by a dust corner.
    # This mirrors a common generated path boundary.
    w=World()
    _supports(w,(Pos(0,0,0),Pos(1,0,0),Pos(2,0,0),Pos(2,0,1),Pos(2,0,2)))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.place(BlockKind.REPEATER,1,1,0,facing=Facing.EAST,delay=1)
    for q in (Pos(2,1,0),Pos(2,1,1),Pos(2,1,2)):
        w.place(BlockKind.REDSTONE_WIRE,q.x,q.y,q.z)
    update_wire_shapes(w)
    probes.append(SemanticProbe(
        "20_repeater_to_corner",
        "repeater output remains connected through an immediate dust corner",
        w,
        (_setblock(Pos(-1,1,0),"minecraft:redstone_block",config),),
        _wire_on(Pos(2,1,2),config,"repeater -> corner -> dust"),
    ))

    return tuple(probes)


def export_semantics_datapack(
    output_dir: str | Path,
    *,
    config: JavaExportConfig = JavaExportConfig(
        namespace="ro_sem",
        test_delay_ticks=20,
    ),
    build_settle_ticks: int = 4,
    gap: int = 10,
) -> Path:
    """
    Export phased real-Minecraft semantics tests.

    Every probe follows:
        BUILD (passive geometry)
          -> settle
        STIMULATE (insert/toggle source)
          -> settle
        CHECK

    Public API per probe:
        /function ro_sem:<probe>/build
        /function ro_sem:<probe>/stimulate
        /function ro_sem:<probe>/check
        /function ro_sem:<probe>/run

    Whole suite:
        /function ro_sem:tests
    """
    output_dir=Path(output_dir)
    funcs=output_dir/"data"/config.namespace/"function"
    funcs.mkdir(parents=True,exist_ok=True)

    probes=semantic_probes(config)
    offsets={}
    x=0
    for probe in probes:
        bounds=probe.world.bounds()
        width=1 if bounds is None else bounds[1].x-bounds[0].x+1
        offsets[probe.name]=x
        x += width + 2*config.reset_margin_xz + gap

    for probe in probes:
        pdir=funcs/probe.name
        pdir.mkdir(parents=True,exist_ok=True)
        tag=f"{config.namespace}_{probe.name}_origin".replace(":","_")

        build_internal=list(isolated_build_commands(probe.world,config))
        build_internal.append(
            'tellraw @a '+json.dumps({
                "text":f"{probe.name}: BUILD - {probe.description}",
                "color":"yellow",
            },separators=(",",":"))
        )
        (pdir/"_build.mcfunction").write_text(
            "\n".join(build_internal)+"\n",encoding="utf-8"
        )

        stim_lines=list(probe.stimulus)
        stim_lines.append(
            'tellraw @a '+json.dumps({
                "text":f"{probe.name}: STIMULATE",
                "color":"blue",
            },separators=(",",":"))
        )
        (pdir/"_stimulate.mcfunction").write_text(
            "\n".join(stim_lines)+"\n",encoding="utf-8"
        )

        check_lines=[
            'tellraw @a '+json.dumps({
                "text":f"{probe.name}: CHECK",
                "color":"gray",
            },separators=(",",":")),
            *probe.checks,
        ]
        (pdir/"_check.mcfunction").write_text(
            "\n".join(check_lines)+"\n",encoding="utf-8"
        )

        # Public wrappers always relocate to the persistent marker.
        (pdir/"build.mcfunction").write_text(
            f"kill @e[type=minecraft:marker,tag={tag}]\n"
            f'summon minecraft:marker ~ ~ ~ {{Tags:["{tag}"]}}\n'
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:{probe.name}/_build\n",
            encoding="utf-8",
        )
        (pdir/"stimulate.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:{probe.name}/_stimulate\n",
            encoding="utf-8",
        )
        (pdir/"check.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:{probe.name}/_check\n",
            encoding="utf-8",
        )

        # run is useful for one-at-a-time manual debugging.
        (pdir/"run.mcfunction").write_text(
            f"function {config.namespace}:{probe.name}/build\n"
            f"schedule function {config.namespace}:{probe.name}/stimulate "
            f"{build_settle_ticks}t replace\n"
            f"schedule function {config.namespace}:{probe.name}/check "
            f"{build_settle_ticks + config.test_delay_ticks}t replace\n",
            encoding="utf-8",
        )

    # Whole automatic suite deliberately reuses ONE loaded origin.
    #
    # The old gallery-style suite placed probes hundreds of blocks apart. Past
    # the player's simulation/load distance, scheduled commands could reach
    # unloaded chunks and the visible run appeared to stop (in practice around
    # probe 14/15 for a common distance). Automatic semantic validation should
    # never depend on chunk loading, so each probe now rebuilds the same bench.
    case_window=build_settle_ticks + config.test_delay_ticks + 4
    suite_tag=f"{config.namespace}_suite_origin".replace(":","_")
    tests_lines=[
        f"kill @e[type=minecraft:marker,tag={suite_tag}]",
        f'summon minecraft:marker ~ ~ ~ {{Tags:["{suite_tag}"]}}',
        'tellraw @a '+json.dumps({
            "text":"Simulator semantics probes: same-origin BUILD -> STIMULATE -> CHECK",
            "color":"gold",
        },separators=(",",":"))
    ]

    for i,probe in enumerate(probes):
        delay=i*case_window
        launch_name=f"_launch_{probe.name}"

        # Every delayed launch relocates to the single suite marker. Probe
        # build() will create its own per-probe marker at exactly this origin.
        (funcs/f"{launch_name}.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={suite_tag},limit=1] "
            f"run function {config.namespace}:{probe.name}/run\n",
            encoding="utf-8",
        )

        if delay == 0:
            tests_lines.append(f"function {config.namespace}:{launch_name}")
        else:
            tests_lines.append(
                f"schedule function {config.namespace}:{launch_name} "
                f"{delay}t replace"
            )

    tests_lines.append(
        'tellraw @a '+json.dumps({
            "text":"Automatic probes reuse one loaded test bench; use /gallery for spatial inspection.",
            "color":"aqua",
        },separators=(",",":"))
    )
    (funcs/"tests.mcfunction").write_text(
        "\n".join(tests_lines)+"\n",encoding="utf-8"
    )

    # Optional spatial gallery. This is intentionally NOT the automatic test
    # runner and may extend beyond loaded chunks. It is only for manual visual
    # inspection; the player can move along it as needed.
    gallery_lines=[
        'tellraw @a '+json.dumps({
            "text":"Building semantics gallery left-to-right; distant probes may require moving closer.",
            "color":"gold",
        },separators=(",",":"))
    ]
    for probe in probes:
        ox=offsets[probe.name]
        gallery_lines.append(
            f"execute positioned {_coord(ox,config.relative)} ~ ~ "
            f"run function {config.namespace}:{probe.name}/build"
        )
    (funcs/"gallery.mcfunction").write_text(
        "\n".join(gallery_lines)+"\n",encoding="utf-8"
    )

    help_lines=[
        'tellraw @a '+json.dumps({
            "text":"Simulator semantics API",
            "color":"gold",
        },separators=(",",":")),
    ]
    for probe in probes:
        help_lines.append(
            'tellraw @a '+json.dumps({
                "text":f"/function {config.namespace}:{probe.name}/run - {probe.description}",
                "color":"white",
            },separators=(",",":"))
        )
    (funcs/"help.mcfunction").write_text(
        "\n".join(help_lines)+"\n",encoding="utf-8"
    )

    (output_dir/"pack.mcmeta").write_text(
        json.dumps({
            "pack":{
                "pack_format":config.pack_format,
                "description":"DustRoute phased simulator semantics probes",
            }
        },indent=2)+"\n",
        encoding="utf-8",
    )
    return output_dir
