from __future__ import annotations

from pathlib import Path
import json

from .model import Block, BlockKind, GateKind, Pos
from .minecraft_export import JavaExportConfig, isolated_build_commands, _xyz
from .raw_half_adder import compile_raw_half_adder


def _wire_bool_check(pos: Pos, expected: bool, config: JavaExportConfig, label: str):
    xyz=_xyz(pos,config)
    state="minecraft:redstone_wire[power=0]"
    if expected:
        return (
            f'execute if block {xyz} minecraft:redstone_wire '
            f'unless block {xyz} {state} run tellraw @a '
            + json.dumps({"text":f"PASS {label}=1","color":"green"},separators=(",",":")),
            f'execute unless block {xyz} minecraft:redstone_wire run tellraw @a '
            + json.dumps({"text":f"FAIL {label}: output wire missing","color":"red"},separators=(",",":")),
            f'execute if block {xyz} {state} run tellraw @a '
            + json.dumps({"text":f"FAIL {label}: expected 1","color":"red"},separators=(",",":")),
        )
    return (
        f'execute if block {xyz} {state} run tellraw @a '
        + json.dumps({"text":f"PASS {label}=0","color":"green"},separators=(",",":")),
        f'execute unless block {xyz} {state} run tellraw @a '
        + json.dumps({"text":f"FAIL {label}: expected 0","color":"red"},separators=(",",":")),
    )


def export_raw_half_adder_datapack(
    output_dir: str | Path,
    *,
    config: JavaExportConfig = JavaExportConfig(
        namespace="ro_half_base",
        pack_format=71,
        test_delay_ticks=60,
    ),
) -> Path:
    """
    Export the non-optimized DAG-driven half-adder baseline.

    Logical lowering is fixed, placement is a deterministic fan-out-aware rule,
    and routing must pass static keepout/support/signal-budget validation.
    Minecraft remains the final reference implementation for truth-table tests.
    """
    raw=compile_raw_half_adder(spacing_x=12,spacing_z=8)
    output_dir=Path(output_dir)
    funcs=output_dir/"data"/config.namespace/"function"
    funcs.mkdir(parents=True,exist_ok=True)

    tag=f"{config.namespace}_origin".replace(":","_")

    # Build passively first.
    build_lines=[
        *isolated_build_commands(raw.world,config),
        'tellraw @a '+json.dumps({
            "text":"DAG baseline half adder built: A/B -> SUM/CARRY",
            "color":"yellow",
        },separators=(",",":")),
    ]
    (funcs/"_build.mcfunction").write_text("\n".join(build_lines)+"\n",encoding="utf-8")
    (funcs/"build.mcfunction").write_text(
        f"kill @e[type=minecraft:marker,tag={tag}]\n"
        f'summon minecraft:marker ~ ~ ~ {{Tags:["{tag}"]}}\n'
        f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
        f"run function {config.namespace}:_build\n",
        encoding="utf-8",
    )

    # Phased cases: passive build -> settle -> stimulus -> settle -> check.
    build_settle=4
    for a,b in ((0,0),(0,1),(1,0),(1,1)):
        bits=f"{a}{b}"

        prepare=[
            # Remove previous stimuli before rebuilding passive geometry.
            f"setblock {_xyz(raw.input_a.offset(dx=-1),config)} minecraft:air replace",
            f"setblock {_xyz(raw.input_b.offset(dx=-1),config)} minecraft:air replace",
            f"function {config.namespace}:_build",
            f"schedule function {config.namespace}:stimulate_{bits} {build_settle}t replace",
        ]
        (funcs/f"_case_{bits}.mcfunction").write_text(
            "\n".join(prepare)+"\n",
            encoding="utf-8",
        )
        (funcs/f"case_{bits}.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:_case_{bits}\n",
            encoding="utf-8",
        )

        stimulus=[
            'tellraw @a '+json.dumps({
                "text":f"HALF {bits}: STIMULATE",
                "color":"blue",
            },separators=(",",":")),
        ]
        if a:
            stimulus.append(
                f"setblock {_xyz(raw.input_a.offset(dx=-1),config)} minecraft:redstone_block replace"
            )
        if b:
            stimulus.append(
                f"setblock {_xyz(raw.input_b.offset(dx=-1),config)} minecraft:redstone_block replace"
            )
        stimulus.append(
            f"schedule function {config.namespace}:check_{bits} "
            f"{config.test_delay_ticks}t replace"
        )
        (funcs/f"_stimulate_{bits}.mcfunction").write_text(
            "\n".join(stimulus)+"\n",
            encoding="utf-8",
        )
        (funcs/f"stimulate_{bits}.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:_stimulate_{bits}\n",
            encoding="utf-8",
        )

        sum_expected=bool(a^b)
        carry_expected=bool(a and b)
        checks=[
            'tellraw @a '+json.dumps({
                "text":f"HALF {bits}: expected SUM={int(sum_expected)} CARRY={int(carry_expected)}",
                "color":"gray",
            },separators=(",",":")),
            *_wire_bool_check(raw.output_sum,sum_expected,config,"SUM"),
            *_wire_bool_check(raw.output_carry,carry_expected,config,"CARRY"),
        ]
        (funcs/f"_check_{bits}.mcfunction").write_text(
            "\n".join(checks)+"\n",
            encoding="utf-8",
        )
        (funcs/f"check_{bits}.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:_check_{bits}\n",
            encoding="utf-8",
        )

    # Automatic all-cases run. Calling /tests is self-contained.
    case_window=build_settle+config.test_delay_ticks+8
    run=[
        f"function {config.namespace}:build",
        'tellraw @a '+json.dumps({
            "text":"DAG baseline half-adder: running 00, 01, 10, 11",
            "color":"gold",
        },separators=(",",":")),
    ]
    for i,bits in enumerate(("00","01","10","11")):
        run.append(
            f"schedule function {config.namespace}:case_{bits} "
            f"{2+i*case_window}t replace"
        )
    (funcs/"tests.mcfunction").write_text(
        "\n".join(run)+"\n",
        encoding="utf-8",
    )

    info=[
        'tellraw @a '+json.dumps({
            "text":"DAG baseline half-adder",
            "color":"gold",
        },separators=(",",":")),
        'tellraw @a '+json.dumps({
            "text":f"A source: ({raw.input_a.x-1},{raw.input_a.y},{raw.input_a.z})  "
                   f"B source: ({raw.input_b.x-1},{raw.input_b.y},{raw.input_b.z})",
            "color":"white",
        },separators=(",",":")),
        'tellraw @a '+json.dumps({
            "text":f"SUM wire: ({raw.output_sum.x},{raw.output_sum.y},{raw.output_sum.z})  "
                   f"CARRY wire: ({raw.output_carry.x},{raw.output_carry.y},{raw.output_carry.z})",
            "color":"white",
        },separators=(",",":")),
        'tellraw @a '+json.dumps({
            "text":"Use /function ro_half_base:case_00 .. case_11 or /function ro_half_base:tests",
            "color":"aqua",
        },separators=(",",":")),
    ]
    (funcs/"info.mcfunction").write_text(
        "\n".join(info)+"\n",
        encoding="utf-8",
    )

    # Current-state A fan-out diagnostic. This is intentionally independent
    # from truth-table expectations and is useful when Minecraft disagrees with
    # the static router/simulator.
    from .physical import _wire_terminal_for_endpoint
    base=raw.physical.cell_world()

    a_input_cell=next(
        cid for cid,node in raw.physical.cells.items()
        if node.logical_kind is GateKind.INPUT
        and node.placed.input_port("in").pos == raw.input_a
    )
    a_net=next(
        n for n in raw.routing.nets.values()
        if n.source.cell == a_input_cell
    )

    a_points=[
        ("A external input",raw.input_a),
        ("A buffered Net source",_wire_terminal_for_endpoint(base,a_net.source)),
    ]
    for i,sink in enumerate(a_net.sinks):
        a_points.append((f"A sink {i}",_wire_terminal_for_endpoint(base,sink)))

    debug=[
        'tellraw @a '+json.dumps({
            "text":"A fan-out diagnostic (current state)",
            "color":"gold",
        },separators=(",",":")),
    ]
    for label,pos in a_points:
        xyz=_xyz(pos,config)
        debug.append(
            f'execute unless block {xyz} minecraft:redstone_wire[power=0] run tellraw @a '
            + json.dumps({"text":f"PASS {label}: ON","color":"green"},separators=(",",":"))
        )
        debug.append(
            f'execute if block {xyz} minecraft:redstone_wire[power=0] run tellraw @a '
            + json.dumps({"text":f"FAIL {label}: OFF","color":"red"},separators=(",",":"))
        )
    (funcs/"_debug_a.mcfunction").write_text(
        "\n".join(debug)+"\n",encoding="utf-8"
    )
    (funcs/"debug_a.mcfunction").write_text(
        f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
        f"run function {config.namespace}:_debug_a\n",
        encoding="utf-8",
    )

    (output_dir/"pack.mcmeta").write_text(
        json.dumps({
            "pack":{
                "pack_format":config.pack_format,
                "description":"DAG-driven non-optimized half-adder baseline",
            }
        },indent=2)+"\n",
        encoding="utf-8",
    )
    return output_dir
