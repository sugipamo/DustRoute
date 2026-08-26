from __future__ import annotations

from pathlib import Path
from itertools import product
import json

from .dag_baseline import BaselineDAGCircuit, compile_baseline_dag
from .dag_circuits import half_subtractor_dag, mux2_dag, decoder1to2_dag
from .logic_dag import evaluate_dag
from .minecraft_export import JavaExportConfig, isolated_build_commands, _xyz


def _wire_bool_check(pos,expected,config,label):
    xyz=_xyz(pos,config)
    off=f"minecraft:redstone_wire[power=0]"
    if expected:
        return (
            f'execute if block {xyz} minecraft:redstone_wire unless block {xyz} {off} run tellraw @a '
            + json.dumps({"text":f"PASS {label}=1","color":"green"},separators=(",",":")),
            f'execute if block {xyz} {off} run tellraw @a '
            + json.dumps({"text":f"FAIL {label}: expected 1","color":"red"},separators=(",",":")),
        )
    return (
        f'execute if block {xyz} {off} run tellraw @a '
        + json.dumps({"text":f"PASS {label}=0","color":"green"},separators=(",",":")),
        f'execute unless block {xyz} {off} run tellraw @a '
        + json.dumps({"text":f"FAIL {label}: expected 0","color":"red"},separators=(",",":")),
    )


def _write_circuit(
    funcs: Path,
    api: str,
    compiled: BaselineDAGCircuit,
    input_names: tuple[str,...],
    *,
    config: JavaExportConfig,
    settle_ticks: int=60,
):
    cdir=funcs/api
    cdir.mkdir(parents=True,exist_ok=True)
    tag=f"{config.namespace}_{api}_origin".replace(":","_")

    build=[
        *isolated_build_commands(compiled.world,config),
        'tellraw @a '+json.dumps({
            "text":f"{api}: DAG baseline built",
            "color":"yellow",
        },separators=(",",":")),
    ]
    (cdir/"_build.mcfunction").write_text("\n".join(build)+"\n",encoding="utf-8")
    (cdir/"build.mcfunction").write_text(
        f"kill @e[type=minecraft:marker,tag={tag}]\n"
        f'summon minecraft:marker ~ ~ ~ {{Tags:["{tag}"]}}\n'
        f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
        f"run function {config.namespace}:{api}/_build\n",
        encoding="utf-8",
    )

    bits_list=[]
    for bits in product((0,1),repeat=len(input_names)):
        name="".join(str(x) for x in bits)
        bits_list.append(name)
        env={k:bool(v) for k,v in zip(input_names,bits)}
        expected=evaluate_dag(compiled.abstract_dag,env)

        case=[
            # clear all external sources
            *(
                f"setblock {_xyz(compiled.input_positions[k].offset(dx=-1),config)} minecraft:air replace"
                for k in input_names
            ),
            f"function {config.namespace}:{api}/_build",
            f"schedule function {config.namespace}:{api}/stimulate_{name} 4t replace",
        ]
        (cdir/f"_case_{name}.mcfunction").write_text("\n".join(case)+"\n",encoding="utf-8")
        (cdir/f"case_{name}.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:{api}/_case_{name}\n",
            encoding="utf-8",
        )

        stim=[
            'tellraw @a '+json.dumps({
                "text":f"{api} {name}: STIMULATE",
                "color":"blue",
            },separators=(",",":")),
        ]
        for k,v in zip(input_names,bits):
            if v:
                stim.append(
                    f"setblock {_xyz(compiled.input_positions[k].offset(dx=-1),config)} "
                    "minecraft:redstone_block replace"
                )
        stim.append(
            f"schedule function {config.namespace}:{api}/check_{name} {settle_ticks}t replace"
        )
        (cdir/f"_stimulate_{name}.mcfunction").write_text("\n".join(stim)+"\n",encoding="utf-8")
        (cdir/f"stimulate_{name}.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:{api}/_stimulate_{name}\n",
            encoding="utf-8",
        )

        check=[
            'tellraw @a '+json.dumps({
                "text":f"{api} {name}: expected "
                       + " ".join(f"{k}={int(v)}" for k,v in expected.items()),
                "color":"gray",
            },separators=(",",":")),
        ]
        for out_name,value in expected.items():
            check.extend(_wire_bool_check(
                compiled.output_positions[out_name],
                bool(value),
                config,
                out_name.upper(),
            ))
        (cdir/f"_check_{name}.mcfunction").write_text("\n".join(check)+"\n",encoding="utf-8")
        (cdir/f"check_{name}.mcfunction").write_text(
            f"execute at @e[type=minecraft:marker,tag={tag},limit=1] "
            f"run function {config.namespace}:{api}/_check_{name}\n",
            encoding="utf-8",
        )

    window=4+settle_ticks+8
    tests=[
        f"function {config.namespace}:{api}/build",
        'tellraw @a '+json.dumps({
            "text":f"{api}: running {len(bits_list)} truth-table cases",
            "color":"gold",
        },separators=(",",":")),
    ]
    for i,name in enumerate(bits_list):
        tests.append(
            f"schedule function {config.namespace}:{api}/case_{name} {2+i*window}t replace"
        )
    (cdir/"tests.mcfunction").write_text("\n".join(tests)+"\n",encoding="utf-8")


def export_half_subtractor_datapack(
    output_dir: str|Path,
    *,
    config: JavaExportConfig=JavaExportConfig(
        namespace="ro_halfsub",
        pack_format=71,
        test_delay_ticks=60,
    ),
):
    """
    Half subtractor truth-table regression (DIFF/BORROW), compiled through
    the same DAG baseline pipeline as the mux/decoder pack.
    """
    output_dir=Path(output_dir)
    funcs=output_dir/"data"/config.namespace/"function"
    funcs.mkdir(parents=True,exist_ok=True)

    hs=compile_baseline_dag(half_subtractor_dag(),spacing_x=12,lane_gap=8)

    _write_circuit(funcs,"halfsub",hs,("a","b"),config=config)

    help_lines=[
        'tellraw @a '+json.dumps({"text":"half subtractor validation","color":"gold"},separators=(",",":")),
        'tellraw @a '+json.dumps({"text":"/function ro_halfsub:halfsub/tests","color":"aqua"},separators=(",",":")),
    ]
    (funcs/"help.mcfunction").write_text("\n".join(help_lines)+"\n",encoding="utf-8")

    (output_dir/"pack.mcmeta").write_text(
        json.dumps({
            "pack":{
                "pack_format":config.pack_format,
                "description":"DAG half-subtractor Minecraft validation",
            }
        },indent=2)+"\n",
        encoding="utf-8",
    )
    return output_dir


def export_multi_circuit_datapack(
    output_dir: str|Path,
    *,
    config: JavaExportConfig=JavaExportConfig(
        namespace="ro_circuits",
        pack_format=71,
        test_delay_ticks=60,
    ),
):
    output_dir=Path(output_dir)
    funcs=output_dir/"data"/config.namespace/"function"
    funcs.mkdir(parents=True,exist_ok=True)

    mux=compile_baseline_dag(mux2_dag(),spacing_x=12,lane_gap=8,allow_ripup=False)
    decoder=compile_baseline_dag(decoder1to2_dag(),spacing_x=12,lane_gap=8,allow_ripup=False)

    _write_circuit(funcs,"mux",mux,("a","b","s"),config=config)
    _write_circuit(funcs,"decoder",decoder,("en","s"),config=config)

    help_lines=[
        'tellraw @a '+json.dumps({"text":"DAG circuit validation","color":"gold"},separators=(",",":")),
        'tellraw @a '+json.dumps({"text":"/function ro_circuits:mux/tests","color":"aqua"},separators=(",",":")),
        'tellraw @a '+json.dumps({"text":"/function ro_circuits:decoder/tests","color":"aqua"},separators=(",",":")),
    ]
    (funcs/"help.mcfunction").write_text("\n".join(help_lines)+"\n",encoding="utf-8")

    (output_dir/"pack.mcmeta").write_text(
        json.dumps({
            "pack":{
                "pack_format":config.pack_format,
                "description":"DAG multi-circuit Minecraft validation",
            }
        },indent=2)+"\n",
        encoding="utf-8",
    )
    return output_dir
