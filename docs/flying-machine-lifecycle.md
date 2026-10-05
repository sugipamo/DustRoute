# 有限飛行の型・採用・配置・撤去

> Historical capture instructions: Mineflayer and its executable harnesses were
> removed on 2026-10-03. Commands below describe the retained trials and are no
> longer runnable in this checkout. Source is retained in Git at `9b62dcf`.
> Use [native setup](../crates/dustroute-mcp/SETUP.md) for the current backend.


指定した空き領域への配置と、有限距離の飛行後の管理を同じ作業範囲で扱う。
空き場所の自動探索は別段階。既存の固定領域観測と物理runtime v17を使う。

## 実装順と境界

1. `SingleOperation` 型で、一度の起動と到達条件を定義する。
2. 既存のBlueprint更新提案・再検証・採用へ接続し、保存・再起動も検証する。
3. 指定した領域全体の空きを確認し、移動・回転したAssemblyを再検証して配置する。
4. 到着後も同じ固定領域を診断し、明示的に選んだ正常状態から撤去する。
5. 隔離Javaサーバーで公開MCPによる一連の操作と拒否条件を確認し、証拠を保持する。

宣言外の機構が前提と判明した場合は実装前に停止して報告する。
追従観測、自動探索、無限走行、プレイヤーや荷物の運搬、破損後の再始動は含めない。

## 一度だけの動作契約

`TypeContract::SingleOperation` は `kind: "single_operation"` として保存する。
`requirement` は入力名 `input`、観測名 `outputs`、初期待機のBoolean列 `initial`、
完了後のBoolean列 `completed` を持つ。分類名や見た目だけで合格にはしない。

入力は初期状態でfalse。初期待機中の全観測が宣言値を保ち、完全な物理状態で
再帰領域を調べ終えた後、そのすべての位相から一度だけtrueを入力する。
trueを保持した各経路が、完了観測を保つ再帰領域へ到達する必要がある。
境界だけでなく内部サンプルも確認する。探索予算切れは未確定であり合格にしない。
途中反転、falseへの復帰、再使用は約束しない。

`PistonDoor` の完了状態探索を共有するが、ドアが保証する完了後の繰り返し操作は
追加しない。物理状態キー・キュー・Lawは共通のものを使い、動作中の入力を
物理runtime側で無視する処理は設けない。
現在のcatalogは型の種類によらずv13。旧v1〜v12は読込みを拒否する。
旧データの扱いは[移行手順](architecture-cutover.md)を参照。

試験用Blueprintは[既存の6ブロック機体](flying-machine-short-course.md)を使う。
型は通過範囲48セルの到着時のBlockKindと2個のピストンの収納状態を指定する。
BlockKindは材質や全プロパティの同一性を意味しない。公開配置・診断・撤去の
照合は別途、Assemblyから計算した全領域のnative名・プロパティを用いる。
実機試験の到達期待値は、宣言した6部品を10ブロック平行移動して作る。

## 公開操作

`flying_machine_assembly_fixture` test fixture adapterが未採用のrecordsと更新提案を出力する。
既存の `test_circuit_change` → `show_operation` → `invoke_operation` で採用する。
`new_placement` の `assembly_target` で原点と水平回転を指定する。全観測領域を
空き領域として検査し、移動先で要求を再検証する。建築順は共通処理を使う。

レバーを一度ONにし、到着後に `manage_assembly(action="diagnose")` を使う。
正常なら `matches_reference`。初期位置との比較である `observe` は `changed`
のままであり、飛行完了を初期配置一致として扱わない。

到着後の撤去は次を明示する。

```json
{"action":"plan_removal","instance_id":"<UUID>","removal_reference":"observed_inputs"}
```

診断と同じ、初期設計に観測した入力を順番に適用した正常状態へ完全一致した場合に
計画する。参照状態と撤去手順をプレビューし、実行時にも再計算・全領域照合する。
未確認領域、動いている配置、欠損、余分な部品、プレビュー後の入力変更は拒否する。
既定の撤去と `undo_operation` は初期配置との一致を要求する。

撤去記録は版付き非JSON `.store` の `dustroute.placed-assembly.v5` に保存する。
旧v1/v2/v3/v4とJSON形式は読込みを拒否する。
旧記録の退避・破棄と再検証は[形式移行](architecture-cutover.md)を参照。
再起動後も現行記録は残るが、実行可能な計画は再作成が必要。
保存した合格・観測だけでは実行できない。

入力値が同じでも別の履歴で別の配置になる機構は、この比較基準に一致しない場合が
ある。また、同じ静止スナップショットが続いても実サーバーの全予約を証明しない。
既存の読戻し・外部操作を避ける運用条件は継続する。

## 確認結果（2026-09-28）

[証拠一覧](evidence/flying-machine-public-20260928.json)と
[観測・公開API・保存記録](evidence/flying-machine-public-20260928.observations.json.gz)
を保持する。原点 (270000, 180, 1000)、R90、Java 1.21.11、1,440セルで確認した。

- 公開Blueprint更新提案を新しく検証・採用し、9段階で配置した。
- レバーを実サーバーtick182459に一度ONにし、6部品が10ブロック先へ到着。
  到着位置・native状態と、固定部品以外の空き領域をサーバー読戻しで確認した。
- 到着後の再起動を挟んだ診断は `matches_reference`、世界への書込みなし。
- 初期状態用の撤去は拒否。`observed_inputs` で新しく計画・プレビューした撤去は
  9段階すべて一致し、再起動後も撤去記録と全領域の空きを確認した。
- 空いていない配置先、欠落したクライアント観測、外部変更、未プレビューの実行、
  プレビュー後の外部変更、再起動前の撤去計画再利用を拒否した。
- MCPを計5回再起動。採用・配置・到着・撤去の記録を各段階で確認した。
- 領域を空に戻し、強制ロードを解除し、サーバーを正常停止した。
- 関連Rustテスト32件、workspace全targetのClippy（警告をエラー扱い）が合格。
  水平4方向の構築・到着参照・撤去・再構築はモデル上で確認した。

今回の実機結果は公開操作と到着点の確認。飛行途中のtick単位の物理比較は
[先行する固定コース試験](flying-machine-short-course.md)の別証拠を参照する。
宣言範囲外の物理機構や追従観測の追加は必要にならなかった。

再実行は新しいrun-idと隔離座標で行う。

```sh
# Retired example command: use docs/development-fixture-adapters.md with a new absolute output path.
# Retired example command: use docs/development-fixture-adapters.md with a new absolute output path.
python3 tools/observe_assembly_construction.py --run-id flying-next-public --x 280000 --fixture .local/flying-next-public.fixture.json --rotation r90 --persistence --capture-construction
```

[生成API](flying-machine-generation.md)では、機体・追加ブロック・距離・回転・鏡像を指定して、この共通経路へ候補を渡せる。

The JSON example commands referenced historically in this document are retired.
Current explicit test-only export/replay instructions are in
[development-fixture-adapters.md](development-fixture-adapters.md).
Retained evidence and hashes describe their original execution and are unchanged.
