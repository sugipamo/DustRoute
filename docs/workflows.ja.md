# 利用手順

[English](workflows.md) · [日本語](workflows.ja.md)

[対応表](capabilities.ja.md)で作業を選び、このページの手順へ進みます。
接続・権限は[導入ガイド](getting-started.ja.md)、正確な引数は接続先MCPのtool schema、
制約・期限は[公開詳細仕様](mcp-public-features.md)を参照してください。

## 既存回路を理解する

1. 指定プレイヤーの視線先は`test_circuit`、明示座標は`get_world`を使います。
   選択領域のcaptureは`set_region`を2回、その後`show_region`を使います。
2. 返された`circuit_id`を範囲・dimension・完全性とともに保持します。
   詳しく調べるときは`convert_from_circuit`や`get_circuit_ir`を使います。
3. 問題、未対応ブロック、推論・証明の限界を分けて報告します。
   真理値表は対応する入出力がある場合に、予算付きで要求します。

推定した役割は挙動型の保証ではありません。詳細は[機能解析](physical-function-model.md)へ進みます。

## 反映前に変更を試す

1. `test_circuit_change(circuit_id)`で仮想Revisionを作ります。
   `test_circuit_change(revision_id)`で分岐し、`get_circuit_revision`で読みます。
2. 明示した編集、モデル結果、不成立理由を比較します。
   不合格の案も編集可能です。この段階でMinecraftは変更されません。
3. 観測元への対応する変更は`new_placement(revision_id)`で計画します。
   大きなRevisionの区画作業には明示したwork regionsを渡します。
4. `show_operation`で確認し、`invoke_operation(confirm=true)`で実行、
   `get_operation`で結果を読みます。計画・実行前受付は完了ではありません。

基準状態が変わったら、古い操作を再送せず再観測します。
[Revision](circuit-revisions.md)・[区画作業](large-circuit-regions.md)へ進んでください。

## 再利用する設計を作成採用する

1. `get_circuit_revision(blueprint.kind)`でカタログの正確な定義を読みます。
2. データを明示して作成するか、対応する建築形状・地面付き建築・登録済み有限飛行機の
   生成actionを使います。
3. 返されたrecordsをimportし、更新を提案、`show_operation`で確認します。
   物理差分、親・子・共有箇所それぞれの要求を読みます。
4. `invoke_operation(confirm=true, blueprint_decision)`で明示的に採用します。
   必須検査の不合格・未確定は採用を止めます。

採用は不変のローカル記録を追加します。配置や既存instanceの更新は行いません。
[設計図仕様](blueprint-mcp.md)、[建築入力](blueprint-building-design.md)、
[飛行機生成](flying-machine-generation.md)へ進めます。

## コマンドで施工する

1. 現地全体を観測します。新規Assemblyにはガード付き空き領域が必要です。
   観測元への反映には別の由来照合があります。
2. 対応する組込み回路・Revision、または採用済み`assembly_revision_id`と
   適切なtargetを使い、`new_placement`で計画します。
3. `show_operation`で範囲・書込み・初期化・復旧条件を確認します。
   承認した計画を`invoke_operation(confirm=true)`で適用します。
4. 完了・読戻しを確認し、カスタムAssemblyの`instance_id`を保存します。
   区画ジョブは次段階を毎回新しく計画・確認します。

この経路は所持品ではなくOPコマンドを使います。能動機構のモデル検証・設置検証は
実機完了センサーを提供しません。[カスタム配置](custom-piston-assembly-placement.md)と
[instance管理](placed-assembly-management.md)を参照してください。

## サバイバルの所持品で施工する

1. 対応する地面付き受動建築を生成・import・検証・採用します。
2. 恒久材・仮設材をbotのインベントリへ渡します。編集・仮設・移動・退避範囲を宣言します。
3. `survival_construction(action=plan)`を呼び、全手順と必要材料を確認します。
   計画用の支給予算は、実際の所持品の証拠ではありません。
4. `job_id`と`confirmed=true`で`action=start`を呼び、終端状態まで`action=get`で確認します。
   `admitting`は成功ではありません。完了検査には現地全体・仮設撤去・退避を含みます。
5. 計画的な停止は`action=checkpoint`を要求し、`checkpointed`を待ちます。
   後で`action=continue`により新しい観測・所持品から新規プレビューを作ります。
   プロセス再起動後も使えます。新しいジョブを確認して開始します。

通常作業は非OPのbot一体で実行します。移動は明示した予測であり、受信した世界の証拠は
サーバーロックではありません。`cancel`はrollbackではなく、失われた未解決操作は
自動再開できません。[サバイバル仕様](survival-public-construction.md)を参照してください。

停止・拒否時は`next_action`、エラー、判明している進捗を確認します。詳細の読込みが
失敗しても、権限確認済みの`available_live_status`を返す場合があります。読込みの成功や
再送許可にはなりません。[失敗処理の詳細](failure-handling-stability.md)を参照してください。

## 診断・復旧する

| 状況 | 次の手順 |
| --- | --- |
| 回路が故障していそう | `new_repair`。複数の説明がある場合は`get_repair_context`で確認してから修正を選ぶ |
| 配置済みAssemblyが壊れた | `manage_assembly(action=diagnose)`で新しい設計差分を確認。対応する場合は新しい`plan_reconstruction`を検討 |
| 計画・最適化で候補が採用されなかった | 型付きの原因・実際の上限・候補拒否や検証結果を確認し、条件を見直して新しく探索します。探索終了は不可能の証明ではありません。[詳細](search-failure-diagnostics.md) |
| 操作失敗・書込み不明 | 結果を保持し、コンテキスト全体を観測、変更・保護セルを確認。自動再試行しない |
| MCPを再起動した | 永続履歴を読み、再観測して新規計画。保存した合格は実行可能な証明ではない |
| 有限飛行機が到着した | 宣言した入力から導く参照で診断。到着状態の撤去計画は`removal_reference=observed_inputs`を明示 |
| サバイバルをcheckpointした | `continue`で新規計画。checkpointには永続的な継続claimが一つだけある |
| 取消しを求められた | 操作固有の条件と現地を確認。取消し非対応、再起動後に取消し記録が残らない操作もある |

診断は差分を示します。原因・変更者や修復計画の成立を保証しません。
[診断](assembly-diagnosis.md)、[復旧契約](mcp-public-features.md#execution-and-recovery)、
[失敗情報](structured-failure-recovery.md)へ進んでください。

さらに詳しく調べるときは[文書案内](README.ja.md)から機能を選びます。
