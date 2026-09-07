# MCP公開機能ガイド

この文書は、現在公開している機能の入口です。APIの役割、利用順、IDの意味、対応範囲をまとめています。詳細なJSON仕様は [MCP JSON contracts](mcp-api-v1.md)、起動と設定は [セットアップ](../crates/dustroute-mcp/SETUP.md) を参照してください。

機能の到達点は「観測 → 仮の回路をrevisionとして編集・検証 → 配置計画 → 現地照合 → 反映・確認」です。検証済み1×2ピストンドアは、固定構成の設置・認識・開閉・撤去まで対応しています。任意の回路の全入力動作や、無人運転での復旧を保証する段階ではありません。

## 目的から選ぶ入口

| やりたいこと | 入口と流れ |
| --- | --- |
| 接続と設定を確認する | `get_bot_status` |
| 見えているブロックを確認する | `get_world` |
| 視線先の回路を調べる | `test_circuit` → 必要なら `convert_from_circuit` / `get_circuit_ir` |
| 範囲を指定して調べる | `set_region` を2回 → `show_region` → 返された `circuit_id` で解析 |
| 空想回路を作り、案を分岐・修正する | `test_circuit_change(circuit_id)` → `test_circuit_change(revision_id)`。保存内容は `get_circuit_revision` |
| revisionを現実へ反映する | `new_placement(revision_id)` → 共通のプレビュー・実行 |
| 用意済み回路を設置する | `new_placement(circuit)` → 共通のプレビュー・実行 |
| 既存回路の修復を検討する | `new_repair(circuit_id)` → 必要なら `get_repair_context` → 案を選んで共通のプレビュー・実行 |
| 既存配線を最適化する | `new_optimization(circuit_id, …)`。既知マクロの置換候補なら `new_macro_optimization` |
| 既設1×2ドアを開閉する | 観測 → `new_piston_door_operation(circuit_id, target)` → 共通のプレビュー・実行 |
| 入力変化に対する挙動を実世界で試験する | `new_transition_test(circuit_id)` → 共通のプレビュー・実行。復元も試験種別の手順に従う |

共通の操作は `show_operation(operation_id)` → 内容を確認 → `invoke_operation(operation_id, confirm=true)` です。対応する種類では `undo_operation(operation_id, confirm=true)` で復元します。`get_operation` は処理状況・結果の確認に使います。

「計画の作成」と「実世界への書き込み」は別です。既定の `DUSTROUTE_READ_ONLY=true` では書き込みを実行できません。観測に伴うbotの移動や、選択領域の表示は発生する場合があります。

## 通常公開（21 API）

`DUSTROUTE_MCP_TOOL_PROFILE=default` の一覧です。

| API | 役割 | 結果・注意点 |
| --- | --- | --- |
| `get_bot_status` | 接続・設定確認 | 接続状態、対象プレイヤー、適用ポリシー |
| `get_world` | 生の物理観測 | 観測ブロック。高水準の回路設計案ではない |
| `set_region` | 観測範囲の選択 | 視線でfirst/secondの角を指定 |
| `show_region` | 選択範囲の表示と再観測 | 新しい `circuit_id` と `mechanisms` |
| `clear_region` | 選択解除 | 選択を解除する。設置したブロックを消す操作ではない |
| `test_circuit` | 軽量な回路診断 | `circuit_id`、診断、局所説明、機構解釈 |
| `convert_from_circuit` | 物理・論理の解釈 | 保存観測の詳細解析。選択範囲から新規取得する経路もある |
| `get_circuit_ir` | IRの概要・詳細取得 | 同じ観測に対する `analysis_id` / `node_id` で詳細を展開 |
| `test_circuit_change` | 仮編集と版の保存 | 観測IDか親revision IDを指定。新しい `revision_id`、差分、検証結果 |
| `get_circuit_revision` | 保存した版の読み出し | 親、元観測、差分、検証。全ブロックは `include_snapshot=true` |
| `new_placement` | 配置計画 | 組み込み名 `circuit` または `revision_id` のどちらか |
| `new_repair` | 修復候補の計画 | 修復案。候補作成だけでは反映しない |
| `get_repair_context` | 修復判断の補助 | 根拠、競合する解釈、確認事項 |
| `new_optimization` | 観測配線の最適化計画 | 現行の候補生成は端点固定の非分岐dust経路に限定 |
| `new_macro_optimization` | 検証済み候補の置換計画 | 同じ `circuit_id` に対して返された `component_id` が必要 |
| `new_piston_door_operation` | 既設ドアの開閉計画 | Java 1.21.11の既知1×2構成、`target: open/closed` |
| `new_transition_test` | 遷移試験の計画 | 対応入力・安全性・観測条件を満たすシナリオのみ |
| `show_operation` | 操作のプレビュー | 種類ごとの変更内容・対象を確認 |
| `invoke_operation` | 計画の実行 | `confirm=true` と実行条件の再検証が必要 |
| `undo_operation` | 対応操作の復元 | 全操作で使えるわけではない。下記参照 |
| `get_operation` | 処理状況・結果確認 | 通常公開。恒久的な履歴一覧ではない |

## debug追加（7 API）

`DUSTROUTE_MCP_TOOL_PROFILE=debug` では合計28 APIになります。以下は通常利用の必須入口ではありません。

| API | 役割 |
| --- | --- |
| `get_visible_player` | botが追跡できるプレイヤーの確認 |
| `get_player_gaze` | 視線の低レベル観測 |
| `resolve_looked_at_circuit` | 視線からの接続領域探索・選択候補の作成 |
| `get_circuit_placement` | 配置計画と復元差分の詳細取得 |
| `new_component_removal_plan` | 明示した部品の除去計画 |
| `start_selected_region_conversion` | 選択領域の非同期変換開始 |
| `stop_operation` | 対応する非同期処理の停止。実世界の変更を取り消す操作ではない |

内部メソッド名の `invoke_circuit_placement`、`invoke_repair` 等は直接呼ぶ公開APIではありません。用途ごとの専用実行入口を探さず、共通の操作APIを使用します。ドア専用の読み取りAPI `get_piston_door_state` は公開していません。

## IDと保存の違い

| ID・対象 | 意味 | 有効期間・再起動 |
| --- | --- | --- |
| `circuit_id` | 実世界を観測した不変スナップショット | メモリ内、15分。最大64件のため上限到達時は早く失われる場合がある。再起動で消失 |
| `revision_id` | 仮の回路の不変な版 | 状態ストアに保存、既定1時間。読み出しで期限を延長しない。同じ保存スコープなら再起動後も読める |
| `parent_revision_ids` | 編集元revisionの参照 | 現行は0件または1件。同じ親から複数の子を作ると分岐。mergeは未実装 |
| `base_observation_id` | 最初の実世界観測への参照 | 新しいrevisionは元の観測内容も保持。元IDが消えても照合用データが残る |
| `analysis_id` / `node_id` / `component_id` | 特定の観測・解析に属する識別子 | 別の観測に流用しない。revisionや操作IDの代わりにはならない |
| `operation_id` | 配置・修復・開閉・試験などの計画／実行記録 | 寿命と復元可否は操作種別による。下表参照 |

revisionは現実の状態を表す証拠ではありません。`new_placement(revision_id)` が改めて現地照合を行い、別の操作IDを発行します。保存した観測やrevisionを読むだけでは、現在の世界へ自動更新されません。

revision保存先は `DUSTROUTE_STATE_DIR`、保持期限は `DUSTROUTE_PLAN_TTL_SECONDS` で設定します。期限切れの親を参照する子でも、子自身のスナップショットは独立しています。永続アーカイブやmerge履歴の管理機能ではありません。

## 実行と取り消しの条件

| 種類 | 計画の保持 | 復元・失敗時の扱い |
| --- | --- | --- |
| 一般の組み込み回路配置 | メモリ内。個別の5分期限はない | `undo_operation` が元ブロックへ復元。現地照合が必要 |
| revisionからの配置 | メモリ内。反映前は5分期限 | 周囲を含む完全照合をして反映・復元。書き込み試行後の不明状態は再実行しない。計画は再起動で消失 |
| 固定1×2ピストンの設置 | メモリ内。反映前は5分期限 | 正常設置後、完全な開状態なら撤去可能。閉状態なら先に通常の開操作が必要 |
| 既設1×2ピストンの開閉 | メモリ内、5分・一回限り | `undo_operation` は非対応。新しい観測から逆方向の開閉計画を作る |
| 修復・最適化 | 状態ストアとメモリ内キャッシュ | 対応案は共通undo経路。ディスクの既定保持は1時間だが、同一プロセスのキャッシュへフォールバックするため厳密な実行期限とは異なる |
| 遷移試験 | メモリ内 | 試験の復元経路を使用。正常完了と全状態の復元を同一視せず結果を確認 |

操作結果が不明、照合が失敗、または `needs_inspection` の場合は、まず再観測します。エラー文字列だけの応答もあるため、`needs_inspection` がないことを復元済み・未実行の証拠にしないでください。revision配置と固定ピストンの操作では、書き込み試行を消費した後の自動再試行・自動ロールバックを許可していません。一般配置・修復・試験まで、同じ復旧保証があるとは扱いません。

## 対応範囲と検証の意味

- 組み込み配置は半加算器・半減算器・MUX・デコーダー・全加算器と、固定 `piston-door-1x2`。固定ドアは空き領域への設置、平行移動のみで、基点は視線先の3ブロック上です。
- 機構認識は既知1×2構成の完全一致でドアと判定します。他のピストン構成は未特定として返します。広い領域内の複数機構の分割認識は未対応です。
- revision編集は追加・削除・ブロック属性の全置換。1回64編集、最大4096ブロック、保存レコード4 MiB、シミュレーション1〜256 tick。元の観測範囲内に限定します。
- 不成立のrevisionも診断付きで保存できます。`structurally_valid` はモデル上の配置検証であり、全入力動作や完全なJava属性検証の証明ではありません。
- revision反映は元観測との一致、周囲1ブロックの現在状態、共有配置検証、属性を失わない書き出しを要求します。一般のピストン配置を許可する機能ではありません。
- 実世界への配置は既存のコマンド書き込み経路です。サバイバルでの資材調達や、人と同じ手順での建築を意味しません。
- merge、エンティティ、長期連続運転の耐久・性能最適化、任意の回路の完全自律設計は、この区切りの対象外です。

## 詳細と検証記録

| 内容 | 文書 |
| --- | --- |
| revision編集・保持・現実への反映 | [空想回路revision](circuit-revisions.md) |
| 固定1×2の観測・設置・開閉・撤去 | [Piston door MCP v1](piston-door-mcp-v1.md) |
| ピストンの低レイヤ検証 | [Piston low-layer validation](piston-low-layer-validation.md) |
| 配置検証の境界 | [World validation boundary](world-validation-boundary.md) |
| APIレスポンス仕様 | [MCP JSON contracts](mcp-api-v1.md) |
| LLMのツール選択・実行判断 | [MCP利用ガイド](../crates/dustroute-mcp/README.md) |
| サーバー・botの起動とポリシー | [セットアップ](../crates/dustroute-mcp/SETUP.md) |
| 実サーバー検証の実行方法 | [E2E README](../crates/dustroute-mcp/mineflayer/e2e/README.md) |

直近の機能検証はMCPの62テストとClippy。固定1×2の設置・開閉・撤去、およびrevisionの追加・削除・属性変更の反映・復元は、それぞれ専用Java 1.21.11環境で3試行を通しています。この公開機能整理では動作を変更せず、公開一覧と文書の整合性を確認しています。
