# Validation Strategy

このプロジェクトでは、検証を3段階に分けます。

## 1. Python unit / integration

```bash
python -m dustroute.tests.run_all
```

現在は 76 tests です。

主なカテゴリ:

- logic / DAG
- electrical semantics
- cells
- connectivity
- routing
- compiler
- optimization experiments
- Minecraft export

## 2. Static physical validation

生成 World に対して、物理 route を検査します。

```python
validate_route_continuity(...)
validate_routing_legality(...)
```

特に `physical_step_connected()` は、単なる座標隣接ではなく Minecraft の接続方向・dust shape・repeater facing を確認します。

## 3. Java Edition regression

Minecraft 実機を最終参照とします。

### Low-level compatibility suite

`ro_sem` probe 01〜20 は、以下を含む最小物理意味論です。

- source / dust strength
- weak / strong block power
- repeater read / refresh / direction
- torch support semantics
- corner / stair up / stair down
- BLOCK_POWER leaf boundary
- cell output -> routing -> cell input

### Complete circuits

実機で全 truth-table PASS を確認済み:

- half adder
- 2:1 MUX
- enabled 1-to-2 decoder

## 変更時の再確認

次のモジュールに触れた場合は、実機回帰の優先度が高いです。

- `electrical.py`
- `wire.py`
- `connectivity.py`
- `port_realization.py`
- `routing_resources.py`
- `multinet.py`
- `cells.py`
- `minecraft_export.py`

Python PASS は Minecraft PASS の代替ではありません。
