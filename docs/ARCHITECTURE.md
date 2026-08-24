# Architecture

## Core layers

| Layer | Main modules | Responsibility |
|---|---|---|
| Logical expressions | `logic.py` | Expr とローカル rewrite |
| Logic DAG | `logic_dag.py`, `dag_circuits.py` | 共有可能な論理 IR、lowering、評価 |
| Baseline compiler | `baseline_compiler.py` | 非最適化パイプラインの統合 |
| Cell mapping | `baseline_cells.py`, `cells.py` | primitive gate の物理セル |
| Port realization | `port_realization.py` | typed port の物理 terminal 契約 |
| Routing resources | `routing_resources.py` | conductor/support/keepout/stair/terminal |
| Physical routing | `routing.py`, `multinet.py` | A*、fan-out tree、repeater 配置 |
| Electrical semantics | `electrical.py`, `wire.py` | dust / weak / strong / component semantics |
| Connectivity | `connectivity.py` | potential / active graph と physical step |
| Physical IR | `physical.py` | placed cell、endpoint、route |
| Minecraft export | `minecraft_export.py`, `minecraft_semantics.py`, `minecraft_circuits.py` | Java Edition function / Data Pack |
| Optimization experiments | `placement.py`, `reverse.py`, `cell_library.py` | 配置・置換・reverse abstraction |

## BaselineCompiler

`BaselineCompiler` が現在の基準となる非最適化 compiler です。

```python
result = BaselineCompiler(config).compile(dag)
```

成功条件は「World を作れた」だけではなく、routing legality を通ることです。

- cross-Net contact なし
- support conflict なし
- signal budget 超過なし
- broken physical step なし

## Compatibility wrappers

`raw_half_adder.py` と `dag_baseline.py` は、現在は共通 pipeline の front-end / compatibility wrapper です。独立した compiler 実装を増やさない方針です。
