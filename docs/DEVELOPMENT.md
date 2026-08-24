# Development Guide

## 安全な変更順序

機能追加や最適化は、次の順序を推奨します。

```text
1. Logic/DAG 単体で意味論を確認
2. primitive lowering を確認
3. cell mapping を確認
4. static routing legality を確認
5. Python regression
6. Minecraft low-level regression
7. complete circuit regression
8. optimizer 比較
```

## 新しい Minecraft 意味論を追加するとき

推測で simulator へ規則を追加せず、まず最小 Data Pack probe を作ります。

推奨フロー:

```text
Minecraft probe
 -> 実機確認
 -> electrical/connectivity kernel
 -> Python regression
 -> compiler/routing で利用
```

## 新しい回路を追加するとき

1. `dag_circuits.py` に `LogicDAG` を追加
2. `evaluate_dag()` で truth table を確認
3. `BaselineCompiler` で compile
4. `validate_routing_legality()` を通す
5. Minecraft exporter に truth-table runner を追加
6. 実機確認

## Router scalability

現在の次の大きな課題です。full adder / 2-bit ripple adder は論理 DAG として表現できますが、baseline routing の探索コストが高く、安定 compile には未到達です。

改善時も、既存の half adder / MUX / decoder の物理 artifact を壊さないことを優先します。

## Golden baseline

現在の実機確認済み Data Pack は `datapacks/` にあります。routing / semantics / export の変更では、これらを compatibility baseline として扱ってください。
