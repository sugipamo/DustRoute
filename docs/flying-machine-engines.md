# エンジン定義の一般化

> Historical capture instructions: Mineflayer and its executable harnesses were
> removed on 2026-10-03. Commands below describe the retained trials and are no
> longer runnable in this checkout. Source is retained in Git at `9b62dcf`.
> Use [native setup](../crates/dustroute-mcp/SETUP.md) for the current backend.


状態: 下記範囲で完了（2026-09-29）。基点 `a374ad9`、ブランチ `codex/piston-adhesion`。
目標は、新しいエンジンをRustの
型付き定義として追加し、専用の実行処理を増やさず、共通の生成・検証・採用・配置・
到着診断・撤去へ接続すること。

## 作業順序と完了条件

1. 駆動部の部品・向き、固定設備、起動レバー、停止接触列、到着状態、対応する機体を
   定義へ分離する。定義の選択は物理runtimeを切り替えない。
2. 給電経路と駆動部配置が異なる2エンジンを同じ生成器へ通す。
   各候補を現在の物理法則で新しく検証し、独立に宣言した到着図と照合する。
3. 方向・鏡像・距離、非接着の負例、公開APIと保存後の採用を確認する。
4. 両エンジンを隔離Java実機で公開MCPから配置・起動し、到着読戻し・診断・撤去・
   再起動後の扱いを確認して証拠を保存する。

宣言外の物理機構・別の前提機能を先に追加する必要が判明した場合は、着手前に停止して
ユーザーへ報告する。構造の自動探索、往復、無限走行、複数入力の時系列制御、
エンティティ輸送、移動する観測窓は今回の範囲に含めない。

## 定義の範囲

公開生成要求の `engine` は `slime_relay`（省略時）または `honey_direct`。
既存の要求は前者を選ぶ。すべて東向きの有限片道機として定義し、生成時に水平回転・
鏡像を適用する。起動は固定レバーのOFF→ONを一度だけ与える。

| engine | 駆動部 | compactの移動部品 | body |
| --- | --- | --- | --- |
| `slime_relay` | 上向き観測のオブザーバーからスライム経由でピストンへ給電 | 6 | 既存の4種類 |
| `honey_direct` | 水平のオブザーバーからピストンへ直接給電し、ハチミツで連結 | 8 | `compact`、`side_blocks` |

ハチミツ機はスライム機の材質置換だけではない。観測器を前後へ移し、連結材を増やし、
固定起動設備も後方へ移す。定義にないbodyの組合せは生成時に拒否する。
追加ブロックは従来どおり指定でき、接着・負荷・停止の成否をその候補で検証する。

`definitions.rs` の `EngineDefinition` は移動部品、固定部品、起動位置、停止接触列、
到着状態の上書き、bodyの表を持つ。向きは `Facing`、部品状態はenumで記述する。
到着図は宣言値から作り、モデルの出力を期待値へコピーしない。
共通の `recipe.rs` が展開し、以後は既存のBlueprintと物理検証へ接続する。
別エンジンの追加には定義と選択enumの登録、検証ケースと実機証拠の追加が必要。

この定義形式は対応済みの静止部品と1回の起動を扱う。実行中のピストン状態、
任意の起動手順、任意の停止装置を一般的に表現できるものではない。
公開APIは登録済み定義を選択するもので、任意JSONのエンジン定義を取り込むAPIは追加しない。

## 検証記録

[証拠一覧](evidence/flying-machine-engines-20260929.json)と
[公開API・読戻し・保存記録・サーバー記録](evidence/flying-machine-engines-20260929.observations.json.gz)
を保存した。Java 1.21.11、共通runtime v17は変更していない。

- スライム機の4形状とハチミツ機の2形状について、4方向×鏡像の有無の計48構成が合格。
  各方向の距離は1・4・8・16。全距離との直積を実行した意味ではない。
- 非接着の荷物、過大な接続部品、未定義のengine/body、座標衝突・範囲外、探索予算切れを
  合格にしない。エンジン指定を省略した旧要求も受け付ける。
- 登録外の幾何テスト定義を同じ展開器へ通し、北・南向きの鏡映と回転、初期と異なる
  到着状態の宣言を確認した。この幾何テストを飛行成功の証拠にはしない。
- 両エンジンで公開APIの生成がカタログを書き換えないこと、取り込み・提案・再起動後の
  新しい採用を確認した。Rustの関連テスト9件、workspace全targetのClippyとfmtが合格。

| 隔離実機 | 生成時／配置時の回転 | 原点 | 移動部品 | 飛行 | 配置／撤去 |
| --- | --- | --- | --- | --- | --- |
| `honey_direct` / `compact`、鏡像 | R90 / R90 | (300000,180,1000) | 8 | 西へ4 | 各11段階一致 |
| `slime_relay` / `slime_wings` | R0 / R270 | (310000,180,1000) | 10 | 北へ5 | 各13段階一致 |

いずれも公開MCPから再生成・取り込み・採用・配置し、独立に宣言したnative到着図と
全領域のサーバー確認読戻しが一致した。生成時と配置時の回転を合成して確認した。
到着診断は `matches_reference`、各試験でMCPを5回再起動した。
初期位置を前提とした撤去の拒否と、観測入力から到着状態を求める撤去の成功を確認した。
配置先の占有、観測不足、外部変更、未プレビュー、プレビュー後の変更、再起動前の計画再利用も拒否した。
試験後は全領域が空気であること、強制ロード解除、サーバー正常停止を確認した。

2件の実機結果を全構成の実機保証へ拡大しない。新しい定義・候補・配置先は再検証する。
今回の実機検証は公開操作と到着状態を対象とし、飛行途中のtick単位の物理比較を新たに
実施したという主張は含めない。起動時刻はサーバーの適用記録と対応付けた。

## 利用と定義の追加

公開APIの例（生成だけでは採用・実配置されない）:

```json
{"blueprint":{"action":"generate_flying_machine","request":{
  "namespace":"my.honey.engine.001", "engine":"honey_direct", "body":"compact",
  "distance":4, "rotation":"r90", "mirrored":true
}}}
```

生成後の取り込み・提案・採用は[既存の生成手順](flying-machine-generation.md)と同じ。
エンジン定義を追加する場合は、次の順に進める。

1. `definitions.rs` に `EngineDefinition` を定数として追加する。nativeのピストンの向きと
   オブザーバーの観測方向を区別し、固定起動設備・停止接触列・到着状態も記述する。
2. `FlyingMachineEngine` と定義選択に登録する。共通runtimeへエンジン名の分岐を追加しない。
3. 型・native到着状態・配置順・撤去を共通経路で検証し、負例と実機証拠を残す。
   定義形式を超える物理作用・入力手順が必要なら、対応済みとして登録する前に範囲を相談する。

今回の再現コマンド（再実行時は新しいnamespace・run-id・空き座標を使う）:

```sh
target/debug/examples/generate_flying_machine request.json > .local/engine.fixture.json
python3 tools/observe_assembly_construction.py --run-id engine-next --x 320000 --fixture .local/engine.fixture.json --rotation r90 --persistence --capture-construction
```
