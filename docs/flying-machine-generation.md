# フライングマシンの生成

> Historical capture instructions: Mineflayer and its executable harnesses were
> removed on 2026-10-03. Commands below describe the retained trials and are no
> longer runnable in this checkout. Source is retained in Git at `9b62dcf`.
> Use [native setup](../crates/dustroute-mcp/SETUP.md) for the current backend.


共通の物理runtime・SingleOperation型・Blueprint採用・Assembly配置を使い、
有限距離の一方向機を生成する。機体の構成はRustの型付き定数で定義し、生成処理は
ブロックの動きを特別扱いしない。`engine` で駆動部を、`body` で機体の追加部品を選ぶ。
[エンジン定義](flying-machine-engines.md)は同じ生成・検証・配置経路へ接続する。

現在は `harvest_targets` で成熟済みカボチャ・スイカの破壊も要求できる。
初期作物の除去と移動部品の到着を同時に検証する。
[収穫範囲・実機証拠・建築側の残作業](flying-machine-practical-roadmap.md)を参照。
以下の過去の実機証拠は当時の版に対応し、新規検証は v19 を使う。
v19の固定環境・サトウキビは [明示した機体と畑の更新経路](existing-machine-modification.md)で扱い、
この生成APIの `harvest_targets.crop` は引き続きカボチャ・スイカに限る。

| engine | 駆動部 | 使用できるbody |
| --- | --- | --- |
| `slime_relay`（省略時） | 上側のオブザーバーからスライム経由で給電する6ブロック機 | 下記の4種類 |
| `honey_direct` | 前後のオブザーバーから直接給電し、ハチミツで連結する8ブロック機 | `compact`（8個）、`side_blocks`（10個） |

以下は `slime_relay` の機体で、移動部品数にはエンジンを含む。

| body | 機体 | 移動するブロック数 |
| --- | --- | --- |
| `compact` | 小型の基本機 | 6 |
| `side_blocks` | 両側に石を付けた機体 | 8 |
| `slime_wings` | スライムで側面を延長し、先端に石を付けた機体 | 10 |
| `honey_nose` | 先頭にハチミツ、その横にガラスを付けた混在機 | 8 |

これは配置候補を組み立てるレシピであり、未検証の機体を自動的に合格させる表ではない。
ハチミツをスライムの横へ追加しただけでは付着せず、部品が置き去りになる。
そのような候補は到達条件の検証に失敗する。

## 公開API

既存の `test_circuit_change` を使う。

```json
{
  "blueprint": {
    "action": "generate_flying_machine",
    "request": {
      "namespace": "my.flight.001",
      "engine": "slime_relay",
      "body": "honey_nose",
      "distance": 8,
      "rotation": "r90",
      "mirrored": false,
      "attachments": []
    }
  }
}
```

- `namespace`: 保存する不変IDの接頭辞。別の構成を保存するときは別IDを使う。
- `distance`: 1〜16ブロック。指定距離に合わせて停止装置も生成する。
- `engine`: `slime_relay | honey_direct`。省略時は従来のスライム機。
- `body`: 上の表にあるエンジン別の機体。組合せが未定義なら拒否する。
- `rotation`: `r0`（東）、`r90`（南）、`r180`（西）、`r270`（北）。
- `mirrored`: 回転前にz=0面で左右を反転する。
- `attachments`: 基本機と選択bodyへ加える移動ブロック。材質は
  `stone | glass | slime | honey`、最大12個。

追加ブロックの座標は、鏡像・回転前の東向き機体に対して指定する。例えば、
`{"position":{"x":0,"y":0,"z":-1},"material":"glass"}` は基本機の横へガラスを付ける。
座標範囲はx=-4〜4、y=-2〜3、z=-5〜5。重複・固定部品との衝突は拒否する。
範囲内でも、実際に付着して運ばれることや押出し制限を満たすことは別途検証する。

`ok: true` はその候補の生成時モデル検証が合格した意味。
`result.verification.status` は `passed | failed | undetermined`。
失敗・未確定の候補を配置可能な生成成功として返さない。
モデルの予算切れや未対応の配置手順も合格にしない。

生成はカタログを変更しない。`result.records` を `blueprint.action="import"` で取り込み、
`result.request` の `id` を除いて `propose_update` へ渡す。
`show_operation`、明示した採用、`new_placement` の既存手順で続ける。
生成済みの合格報告や保存データは採用・実配置の権限にはならず、毎回再検証する。

`new_placement.assembly_target` で実座標へ移動・回転できる。生成時の回転に
配置時の回転が追加されるため、生成時の向きをそのまま使う場合は配置回転を `r0` とする。
領域全体の空きを検査し、移動先で要求と配置順を再検証する。
到着後の診断・撤去は[既存の有限飛行ライフサイクル](flying-machine-lifecycle.md)と共通。

## 生成時の検査

1. 初期機体、固定ランチャー、停止装置、既知の空間を組み立てる。
2. 宣言した全部品を指定量だけ移動した到着図を、物理シミュレーションとは独立に作る。
3. 全部品の通過セルから位置観測を生成し、SingleOperationへ結び付ける。
4. 初期待機と一度の起動後の到達を、共通の完全状態探索で検証する。
5. 共通処理で配置・到着参照・到着後撤去を作り、native名・プロパティを含む
   全領域の到着状態が宣言図へ一致することを確認する。

エンジン定義は到着時の部品状態も宣言する。移動部品を平行移動するだけでなく、
その宣言を適用したnative状態を照合する。登録済みの2エンジンは到着時も初期の
機体形状・状態を保ち、固定起動レバーだけがONになる。

観測集合は512セルまで。位置観測の型は既存のBlockKindとピストン状態を使うため、
型単独の保証とnative状態の追加照合は区別する。生成後に手で書き換えた候補へ、
生成時の追加照合結果を流用しない。

往復、繰り返し起動、無限走行、空き場所の自動探索、乗員・エンティティ輸送は対象外。
この生成器は登録されたエンジン定義を変形・拡張するもので、任意の新エンジンを発明する探索器ではない。
別エンジンの追加も、同じ型・検証・配置経路へつなぐ。

## CLIと実機試験

Rustの公開関数は `dustroute_translate::flying_machine::generate_flying_machine`。
公開APIと同じ生成器を使うCLI exampleも用意する。

```sh
cargo build --offline --locked -j 1 -p dustroute-translate --example generate_flying_machine
target/debug/examples/generate_flying_machine request.json > .local/generated-flight.json
```

出力は `records`、`request`（提案IDは省略）、生成要求・検証結果を含む。
不合格なら候補の診断を出力して終了コードを非0にする。
既存の隔離サーバーで新しいrun-id・座標を指定して公開操作を試せる。

```sh
python3 tools/observe_assembly_construction.py --run-id generated-flight-new --x 300000 --fixture .local/generated-flight.json --rotation r0 --persistence --capture-construction
```

この試験は公開MCPで再生成し、CLI候補と一致したものだけを取り込み・採用する。
起動後は宣言した全部品の平行移動と全領域の読戻しを照合する。

## 確認結果（2026-09-28）

[証拠一覧](evidence/flying-machine-generation-20260928.json)と
[公開API応答・サーバー記録・読戻し・保存記録](evidence/flying-machine-generation-20260928.observations.json.gz)
を保持する。Java 1.21.11、共通runtime v17は変更していない。

- モデル検証は4種類×4方向×鏡像の有無で32ケース。各方向の試験距離は順に
  1・4・8・16。全部品の到達、配置、到着後撤去を確認した。
- ガラスの追加成功、付着しないハチミツの不合格、過大な接続部品の拒否、
  座標衝突・範囲外要求の拒否、探索予算切れの未確定を確認した。
- 公開APIの生成がカタログを変更しないこと、取り込み・提案後の再起動を挟んで
  新しく採用できることを確認した。関連Rustテスト5件、workspace全targetの
  Clippy（警告をエラー扱い）、fmtが合格。

| 隔離実機の生成例 | 移動部品 | 飛行 | 配置／撤去 |
| --- | --- | --- | --- |
| `honey_nose`、鏡像、R180、原点(280000,180,1000) | 8 | 西へ6ブロック | 各11段階一致 |
| `slime_wings`＋下側ガラス、R90、原点(290000,180,1000) | 11 | 南へ4ブロック | 各14段階一致 |

両例とも公開MCPで生成し直してから取り込み・採用・配置した。
到着後の診断は `matches_reference`。それぞれMCPを5回再起動し、配置・到着・撤去を
継続管理できることを確認した。空いていない配置先、観測不足、外部変更、未プレビュー、
プレビュー後の変更、再起動前の計画再利用は拒否した。
試験領域の空気、強制ロード解除、サーバー正常停止も確認済み。

この2例の結果を全パラメータの実機保証には広げない。生成要求と配置先ごとの
新しい検証が必要。今回は公開操作と到着状態を実機確認し、飛行途中のtick単位比較を
新たに実施したという主張は含めない。
