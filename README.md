# DustRoute

Minecraft Java Edition のレッドストーン回路を、論理回路から物理配置へ翻訳・検証するための実験的コンパイラです。

現在の到達点は、**論理 DAG から Minecraft 上で実際に動く組合せ回路を生成し、Data Pack 経由で実機検証できる baseline** です。最適化はまだ主役ではなく、まず「正しく翻訳できること」を優先しています。

実機確認済みの範囲は次のとおりです。

- 低レイヤ意味論 / 接続 probe 01〜20
- 半加算器
- 2:1 MUX
- enable 付き 1→2 decoder

## できること

- `LogicDAG` で論理回路を表現
- XOR を NOT / AND / OR へ lowering
- 固定の検証済みレッドストーンセルへ mapping
- fan-out を考慮した baseline placement
- dust / repeater / BLOCK_POWER port を含む物理 routing
- support、電気的 keepout、stair clearance の予約
- source→sink の信号距離を見た repeater 挿入
- Minecraft 上で本当に隣接接続できるかの route continuity 検査
- Java Edition 用 Data Pack の生成

## まず試す

Python 3.11+ を想定しています。外部依存は現在ほぼありません。

```bash
python -m dustroute.tests.run_all
```

baseline compiler の例です。

```python
from dustroute import (
    BaselineCompiler,
    BaselineCompileConfig,
    mux2_dag,
)

compiler = BaselineCompiler(
    BaselineCompileConfig(
        spacing_x=12,
        lane_gap=8,
        allow_ripup=False,
    )
)

result = compiler.compile(mux2_dag())
print(result.world.bounds())
print(result.input_positions)
print(result.output_positions)
```

論理だけを確認したい場合は、DAG を直接評価できます。

```python
from dustroute import mux2_dag, evaluate_dag

mux = mux2_dag()
print(evaluate_dag(mux, {"a": True, "b": False, "s": False}))
# {'out': True}
```

## Minecraft で確認する

`datapacks/` に、実機回帰用の Data Pack を同梱しています。

Minecraft Java Edition のワールドの `datapacks/` フォルダへ ZIP のまま入れ、ワールド内で `/reload` してください。

低レイヤ probe:

```text
/function ro_sem:tests
```

半加算器:

```text
/function ro_half_base:tests
```

MUX / decoder:

```text
/function ro_circuits:mux/tests
/function ro_circuits:decoder/tests
```

Data Pack の詳細は [`datapacks/README.md`](datapacks/README.md) を参照してください。

## 翻訳パイプライン

このプロジェクトでは、論理と Minecraft 物理配置を直接つなげず、中間層を明示的に分けています。

```text
LogicDAG
  -> primitive lowering
  -> Circuit bridge
  -> cell mapping
  -> placement
  -> port realization
  -> routing resources
  -> physical routing
  -> route validation
  -> World
  -> Minecraft export
```

各層の責務と、どこまでが「意味論」でどこからが「物理実現」なのかは、トップレベルの [`TRANSLATION_PIPELINE.md`](TRANSLATION_PIPELINE.md) にまとめています。

## ドキュメント

- [`TRANSLATION_PIPELINE.md`](TRANSLATION_PIPELINE.md) — 翻訳層の全体像
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — モジュール構成と責務
- [`docs/DAG_IR.md`](docs/DAG_IR.md) — Logic DAG IR
- [`docs/SEMANTICS.md`](docs/SEMANTICS.md) — レッドストーン意味論と実機確認範囲
- [`docs/VALIDATION.md`](docs/VALIDATION.md) — Python / Minecraft 回帰戦略
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — 開発時の安全な進め方

## 現在の制約

- full adder と 2-bit ripple-carry adder の DAG は表現できますが、現 baseline router は規模が増えると探索が重く、まだ安定して物理配置できません。
- comparator のアナログ意味論、side input、repeater locking、quasi-connectivity などは未検証です。
- Java Edition の厳密な neighbor-update 順序を完全再現する simulator ではありません。
- optimizer は存在しますが、現在の成果の中心は「最適化前でも正しい回路を生成できる baseline」です。

## 開発方針

Minecraft 実機を最終的な参照実装とします。意味論・routing・export に触れる変更では、Python テストだけでなく、必要に応じて `ro_sem`、半加算器、MUX、decoder の実機回帰を行います。

現時点の Python 回帰は **76 tests PASS** です。

## License

Apache License 2.0 で公開しています。詳細は [`LICENSE`](LICENSE) を参照してください。

DustRoute は独立したプロジェクトであり、Mojang Studios または Microsoft の公式製品ではなく、両社から承認・提携・支援を受けたものではありません。
