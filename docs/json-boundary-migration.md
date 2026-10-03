# JSON境界の移行

基点: `56d0c07`、`codex/survival-single-client`。
ユーザーの目的は、内部データをRustの型で表し、JSON処理をMCPの入出力へ限定すること。
組込み定義はRustの定数テーブルを使う。旧形式の維持のためだけの互換経路は作らない。
過去の実機証拠は現行の実行形式と分ける。

## 順序とゴール

各段階でゴールを作成し、必要な検証が成功してから次の段階へ進む。

1. **MCPと内部処理の分離**: 設計図コマンドを型付き応答にし、文字列応答の再解析を除去する。
   公開入口が符号化し、認可・生成・採用・履歴・live activityを型で受け渡す。
2. **診断情報の型付け**: `DiagnosticPayload(Value)`、検証report、失敗詳細などを用途別の型へ置換する。
   履歴は診断専用とし、native計画・token・再送権限をDeserializeで復元しない。
3. **LawのRust定義化**: 組込みLawのJSON解析を型付き定義へ移し、実行時の法則検査と物理挙動を保つ。
4. **比較・ハッシュ**: 型付きキーと決定的な非JSON符号化へ移す。変化するID・証拠・cacheの版を明示する。
5. **保存形式**: 共通の版付きcodecと制限付きI/Oへ移す。旧版拒否、atomic save、再起動時の検証を確認する。
6. **Voxrig内部表現**: registry・形状・NBT・component表現を型へ移す。別checkoutで検証してpinを更新する。
   Java 1.16.1と1.21.11のadapterは共存させる。
7. **共通処理の可読性**: MCPの長いworkflowと物理遷移を分解する。イベント順序・巻戻し・trace契約を維持する。
8. **最終検証と依存撤去**: 本番コードの残存JSONを監査し、不要な依存を除去して保存・再起動・実機操作を確認する。

停止条件: 必要なJSON通信例外、責務移動、正当性条件の変更、大きなブロック要素、
または宣言外の先行作業が必要なら、その変更を開始せず、根拠と具体案をユーザーへ報告する。
Voxrigはプロトコル・物理・観測・単発操作・接続生命周期、DustRouteは設計・計画・資材・
足場所有権・永続ジョブを担当する。

## 第1段階

設計図コマンドの読取り・生成・採用・拒否・captureは`Response`、`ReadRecord`、
`OperationResponse`、`Generated`などのRust型を返す。応答型にはDeserializeを実装しない。
公開MCP handlerでのみJSONのtextへ変換し、`get_operation`の文字列再解析を除去する。
認可拒否も`FailureCause`のまま公開入口へ渡し、拒否時は保存処理を開始しない。
live activityは型付き応答に付加し、保存済み履歴や採用証明へ混入させない。

BlueprintUpdatesのarchive射影も型で作り、catalogをJSON化してから読み戻さない。
保存schema・JSONの構造と再検証条件は維持する。第5段階のcodec変更は未実施。
他workflowのValue応答、保存codec、request sizeのJSON測定、piston配置のdisplay reportは
この時点で残る。これらを含めた全面撤去の完了とは扱わない。

検証: MCPのlibテスト218件が成功し、明示実行用の実機・計測10件はignoreのまま。
BlueprintUpdatesの統合テスト10件も成功。MCP既定構成のall-target Clippy（`-D warnings`）、
formatting、差分の空白検査が成功した。認可拒否前の保存開始防止、live activityと履歴の分離、
再起動後の採否、偽造した過去の合格の再利用拒否、配置・撤去・修復の既存経路を含む。
最初の対象指定はmodule名の相違により0件だったため、上記はその後の全lib実行の結果を使う。
この段階では実機を再起動したり、ワールドへ変更を加えたりしていない。

## 第2段階の先行改修と停止位置

第2段階は未完了。先行して、挙動検証の`CheckEvidence.behavior`を
`BehaviorDiagnostics`へ置き換えた。通常・有限burst・周期・抽象検証の診断を
用途別の記録として受け渡し、counterexample等の構造化証拠を保持する。
履歴を読めても、検証モデルや実行状態、採用権限を復元する型にはしない。
表示用detailからJSONを除き、構造化診断を別fieldで返す。
欠落・混在した周期/finite reportを別の種類に読み替えない検査も追加した。

再構築失敗の詳細とnative error kindをRust enumにした。ピストン配置のreviewと
device初期条件も型で構成し、JSONへの書込みや読み戻しをなくした。
初期条件はモデルの仮定として表示し、snapshotから隠れたruntime状態を復元した証拠にはしない。
公開応答のfield構造と、保存済みの合格をfresh reviewへ流用しない条件は維持した。

サバイバルの`DiagnosticPayload(Value)`とイベントのopaque evidenceはまだ残る。
manifestのpreview全体は`RecordedConstructionPreview`へ移したが、その計画記録中の
source・移動予測・照準・再接続条件はまだopaque payloadである。
これらのnative field依存をたどると既存のVoxrig定義64種類に広がり、
純粋なデータとlive watch/tokenを分ける設計判断が必要になった。
[具体的な改修案](native-diagnostic-records.md)を作成し、Voxrig側の変更を開始する前で停止した。
全64種類を二重定義する案や、native型へ一括してDeserializeを付ける案は採っていない。
第3〜8段階は未着手。第2段階の完了後に次のゴールを作成する。

先行改修の検証: translateの診断・更新・runtime採用の統合テスト18件、
診断の不完全/混在形式を拒否するunit 1件、MCPのnative bridge 6件・failure 7件・
子の挙動失敗を再起動後にも採用しない公開経路1件が成功した。
native bridgeの実機試験1件はignoreのまま。既定構成と`--no-default-features`の
MCP all-target Clippy（`-D warnings`）、formatting、差分の空白検査も成功した。
今回は実機・接続・ワールドへ操作していない。Voxrigのcodeとvendor pinは変更していない。

### DustRoute内で完結する追加改修

開始前のadmissionとバックグラウンドtask失敗は`JobFailure`にした。
`JobRefusalCode`と`ExecutionError`を型のままジョブ所有者・保存診断へ渡し、
MCP失敗応答をJSONにしてから`DiagnosticPayload`へ包む経路を除去した。
公開の拒否codeとfield構造は維持し、成功や自動再送を表す`true`、未知の拒否codeは
履歴の読込みでも受け入れない。

manifestのpreviewには既存の`RecordedConstructionPlan`と型付き探索結果を保持する。
探索数、候補位置、資材不足、拒否例は純粋なデータとして共有する。
native計画と`GeneratedConstructionPlan`にDeserializeは追加せず、記録からnative計画へ
戻す変換も設けない。プレビューの読込みでnativeのwatchやtokenは生成しない。
`HypotheticalConstructionPlan`からの射影に残るnative opaque fieldは、承認待ちの別改修で扱う。

追加改修の検証: サバイバル関連39件が成功し、明示実行用の実機等6件はignoreのまま。
公開MCPから保存previewと拒否診断を読み直しても、新規startが`plan_not_live`で拒否される
ことを確認した。durable intent、未知のcheckpoint、単回消費、停止済み所有権、
未観測dropを資材へ計上しない既存検査も成功している。
資材不足がない拒否例では空mapが保存時に省略されるため、読込みの既定値を明示した。
資材不足あり/なしの拒否例を含む保存previewの公開読込み試験を最終差分で再実行し、成功した。
最終差分は既定構成と`--no-default-features`のMCP all-target Clippy（`-D warnings`）、
formatting、差分の空白検査も成功した。Voxrigのcode・vendor pinと実機は変更していない。
