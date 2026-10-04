# JSON境界の移行

基点: `56d0c07`、`codex/survival-single-client`。
ユーザーの目的は、内部データ・通信をRustの型で表し、JSON処理を公開通信境界へ限定すること。
MCPの入出力に加え、第6段階でMinecraftサーバーのwireに必要なJSONが明示的に承認された。
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

## 第2段階の診断型移行

先行して、挙動検証の`CheckEvidence.behavior`を
`BehaviorDiagnostics`へ置き換えた。通常・有限burst・周期・抽象検証の診断を
用途別の記録として受け渡し、counterexample等の構造化証拠を保持する。
履歴を読めても、検証モデルや実行状態、採用権限を復元する型にはしない。
表示用detailからJSONを除き、構造化診断を別fieldで返す。
欠落・混在した周期/finite reportを別の種類に読み替えない検査も追加した。

再構築失敗の詳細とnative error kindをRust enumにした。ピストン配置のreviewと
device初期条件も型で構成し、JSONへの書込みや読み戻しをなくした。
初期条件はモデルの仮定として表示し、snapshotから隠れたruntime状態を復元した証拠にはしない。
公開応答のfield構造と、保存済みの合格をfresh reviewへ流用しない条件は維持した。

サバイバルのnative診断依存が64種類に広がった時点で、責務と権限の境界を確認するため
一旦停止した。その後ユーザーの承認を受け、[Voxrigの診断データ層](native-diagnostic-records.md)
を別checkoutで実装し、commit `f7209ed7892aeae4b13eca5a4423949311f25317`を取り込んだ。
全体の配置・編集も含む66型のうち、純粋なデータ39型を共有し、27型は別の記録型にした。
ネイティブ値へDeserializeや記録からの逆変換を追加せず、元のAPIの可視性と責務を維持した。

`DiagnosticPayload(Value)`とイベントのopaque evidenceを撤去した。計画source、移動予測、
配置・採掘・退役receipt、checkpoint、cleanup条件は用途別のRust型となった。
`ExecutionEvidence`の種別とイベントphaseの照合を保存前・読込み時に行う。
journal schemaはv2へ更新し、旧v1は拒否する。旧形式の再開用converterは作らない。
公開診断のevidenceは`kind`と`data`で表し、現在の種別とfieldを明示する。
続行は現在の状態を再観測して新しく計画し、checkpointの単回消費と各送信前のdurable intentを保つ。
保存された診断をネイティブ操作へ入力できる経路は設けない。

第2段階完了時点では第3〜8段階は未着手。保存のJSON byte codecは第5段階で扱う。
MCP応答、他workflow、比較キー、組込みLaw、Voxrigのregistry・形状・NBT等の残存JSONは
全面撤去が完了したとは扱わない。

先行改修の検証: translateの診断・更新・runtime採用の統合テスト18件、
診断の不完全/混在形式を拒否するunit 1件、MCPのnative bridge 6件・failure 7件・
子の挙動失敗を再起動後にも採用しない公開経路1件が成功した。
native bridgeの実機試験1件はignoreのまま。既定構成と`--no-default-features`の
MCP all-target Clippy（`-D warnings`）、formatting、差分の空白検査も成功した。
この先行改修では実機・接続・ワールドへ操作していない。Voxrig改修前の検証結果である。

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
その後、`HypotheticalConstructionPlan`からの射影に残るnative opaque fieldも
承認された診断データ層で置き換えた。

追加改修の検証: サバイバル関連39件が成功し、明示実行用の実機等6件はignoreのまま。
公開MCPから保存previewと拒否診断を読み直しても、新規startが`plan_not_live`で拒否される
ことを確認した。durable intent、未知のcheckpoint、単回消費、停止済み所有権、
未観測dropを資材へ計上しない既存検査も成功している。
資材不足がない拒否例では空mapが保存時に省略されるため、読込みの既定値を明示した。
資材不足あり/なしの拒否例を含む保存previewの公開読込み試験を最終差分で再実行し、成功した。
最終差分は既定構成と`--no-default-features`のMCP all-target Clippy（`-D warnings`）、
formatting、差分の空白検査も成功した。この追加改修時点ではVoxrigのcode・vendor pinと実機は変更していない。

### ネイティブ診断の接続と検証

最終の型付きサバイバル関連40件が成功し、明示実機用6件はignoreのまま。
種別と証拠型の不一致ではin-memory状態も保存済みintentも変更しないこと、旧v1の拒否、
不明な採掘の保存・再読込み、保存失敗時の送信前intent保持、checkpointの所有権・
確認済みprefix・単回消費、公開の履歴読込みで`plan_not_live`となることを確認した。
既定構成と`--no-default-features`のMCP all-target Clippy（`-D warnings`）が成功した。

Voxrigは別checkoutでlib 203件、通常doctest 2件・compile-fail 10件が成功し、
実機用8件はignoreのまま。記録からネイティブintent/standingへ戻せないことを含む。
実装は別リポジトリへcommitした後、315ファイルを取り込み、pinとの一致を確認した。
JSON往復による診断射影は残っていない。journal・manifest・checkpoint claimのJSON byte codec、
公開MCPのencode/decode、テスト用wire fixtureはこの段階では残る。
今回、サーバー起動・実機接続・ワールド変更は行っていない。

全MCP libの回帰は220件成功・明示実機/計測10件ignore（541.17秒）。
formattingと差分の空白検査も成功した。第2段階の診断型移行は完了。

## 第3段階: 組込みLawのRust定義

組込み25プログラムを`law::builtins`のRust定数へ移し、`Definition::program()`から
直接`LawProgram`を構成する。既存の有限`StaticLaw`とExpression/Stepおよび射影処理を共有する。
履歴を持つTorchや複数handlerのLawには、`Definition`のconst constructorで参照、重複名、
初期値、履歴capacity、正の予約delay、4096node/深さ32の構造上限を検査する。
有限Lawは引き続き履歴・予約を拒否し、範囲と8192行の制限を検査する。
一般のイベントLawでは条件内の代入を有限Lawの静的範囲判定に読み替えない。
実行時のABI・構造・状態範囲検査はそのまま維持する。

Dust/TorchのBlueprint metadataはlibraryでRustから構成する。下位のminecraft crateへ
設計図メタデータを持ち込まない。全LawのRevision IDと意味、provenanceを維持した。
本番の`laws/*.json`とTorch catalog JSONを撤去し、現在の定義の凍結fixtureはテストへ分けた。
使われていなかったdirect-input、payload v1/v2、geometry v1の4ファイルは削除した。
実機の過去観測証拠は変更しない。

検証済み: 25プログラム全体と凍結fixtureの一致、minecraft lib/統合319件、libraryの
実行Law・コンテキスト・metadata 19件が成功した。追加の物理・採用・compile-fail・静的検査は
以下に最終結果を追記する。これは実機を再観測した検証ではない。

上位のtranslateでは物理・Law・runtime採用・3×3ドアの関連45件が成功（明示計測1件ignore）。
Torchの18ケースの保存済み実機観測とのgame tick単位の比較と予約回復の保持も含む。
minecraft doctest 37件が成功し、そのうち新しい8件のcompile-failで、未知の参照、
不正なdefault・delay・重複名、有限Lawへの履歴・予約追加をコンパイル時に拒否した。

最終差分でminecraft/library/translate/MCPのall-target Clippy（`-D warnings`）が成功。
最初のall-target実行では既存のexampleが共有fixtureのcrop helperを使わずdead-code警告が出た。
そのexampleのfixture importだけへ許可を明示し、再実行が成功した。実行や物理には変更がない。

MCPの`--no-default-features` all-target Clippy、formatting、差分検査も成功した。
Voxrig pinの315ファイル一致を再確認した。組込みLawの本番経路にはJSON読込みが残っていない。
第3段階は完了。比較キー・保存codec・Voxrig内部表現等の後続移行は未完了である。

## 第4段階: 比較・検証キーの非JSON化

共通の小さな`dustroute-codec::canonical`を追加し、RustのSerializeから直接キーを作る。
JSONへの変換・解析はなく、新しいregistry依存も追加しない。Cargo.lockの変更はローカルcrateと
その2箇所の利用関係のみ。固定幅の整数・IEEE floatのbit列、長さ付きtext/bytes、
型・variant・field名・collection境界を記録する。map/structのfieldは符号化したkeyで整列し、
重複key・不足value・宣言した長さの不一致・書込み失敗は拒否する。
この層には読込み、操作権限、再送、native値への復元機能を設けない。
保存codecの移行は第5段階で別に行う。

runtimeは同じ14項目の完全root recordからキーを作り、比較contractをv4からv5へ更新する。
比較は全byte列で行い、hashに置き換えない。world、queue順、carriers、hidden outputs、
生きたhistory、相対deadline、zero epoch、tick section、progress、limitsを保持する。
既存の正規化とrestore照合を変更しない。

MCPのValidationKeyはv2へ更新し、static mixed-IRのdimension/bounds/complete/scopeを
`StaticAnalysisConditions`で渡す。任意のJSON条件objectはキーAPIへ渡せない。
input content、model context、physical admission revisionも引き続き含める。
ContentIdはMCP等のhuman-readable境界では従来のhex、キーでは32byteとして符号化する。
exact snapshotの既存v1 hash、完全payloadのcollision照合、取得ごとのObservationIdは維持する。
公開summaryと不一致応答に`analysis_id_schema`を返し、旧ID・未指定IDで詳細展開しない。
不一致をworldの変更だけと断定せず、回路・モデル・条件の不一致として説明する。

監査対象の他の比較では、typed Ord/Eqや世代内のshape/state hashを使っており、JSONを
キーにする処理は確認されなかった。世代内hashだけをexact before-stateや観測証明に使わない
既存契約を保つ。StateStoreのscope/playerディレクトリは従来のRust DefaultHasherであり、
JSONとは無関係だが保存先の版と一緒に第5段階で整理する。旧保存先の互換探索は追加しない。
公開response・保存codec・観測fixture等に残るJSONの全廃とは扱わない。

初期検証: codec 6件、runtimeのroot比較/restore/正規化9件、MCP snapshot/cache 6件、
公開summaryからの未指定・旧ID拒否と現在IDによる詳細展開1件が成功した。
最初のMCP compileではRegionとRegionBoundsの取り違えが型検査で見つかったため、
実際の解析境界であるRegionBoundsに揃えた。物理計算や座標の正規化は変更していない。
上位の探索・採用と最終静的検査は以下へ結果を追記する。

translateの抽象挙動・runtime採用・電気ピストン採用・統合ピストン挙動の18件が成功した。
codec/minecraft/MCPのall-target Clippy（`-D warnings`）も成功した。
MCPのno-default-features all-target Clippy、formatting、差分検査とVoxrigの315ファイル一致も
確認した。第4段階は完了。実機や保存ファイルの移行はこの段階では行っていない。

## 第5段階: 保存codecと先行する設計図保存

比較キーとは別の`dustroute-codec::storage` APIを追加した。storage v1のheaderと
record schemaを検査してから、固定したcanonical v1 payloadを直接Rust型へ読む。
キーのAPIにはdecoderを公開しない。JSON/汎用Valueの中間表現や新しい外部依存はない。
mapは符号化key順を検査し、重複・順序違反を拒否する。入力の長さからcollectionの
capacityを確保せず、byte上限・深さ128・値数1,048,576を検査する。
旧JSON、別schema、途中切れ、末尾の余剰、長さのoverflow、非有限数も拒否する。
encode後にformat上限を検査し、保存不能なrecordを既存fileへ置換しない。
この検査はdomainの妥当性・操作権限・freshnessの証明ではない。
encoderはmap/structのentryを一時bufferに保持するため、byte上限はheap使用量の厳密な
上限とは扱わない。既存のmodel予算と入力制限は呼出側に残る。

設計図の保存は`catalog.store`、`dustroute.mcp-blueprints.v3`へ移した。
`StoredBlueprints.archive`は`BlueprintUpdateArchive`を直接保持し、
`BlueprintUpdates::from_archive`で従来のschema・参照・履歴・採用の整合性を検査する。
lock、owner照合、16 MiB上限、fileとdirectoryのsync、atomic replaceは維持した。
旧`catalog.json`がある場合に空のcatalogへ置き換えず、保存・読取りを拒否する。
旧fileの変換・削除やfallbackはしない。catalog v13とupdates v5のdomain schema、
MCPの応答形式は変更していない。履歴から採用するときは引き続き新しいreviewを行う。
他の保存codec、scope/playerの保存先識別はまだ移行していない。

### 残りの保存より先に必要となった型の整理案

調査では、次の3fieldにJSON値が保存データそのものとして残っていた。

| 保存field | 作成経路と接続先 | 必要な変更 |
| --- | --- | --- |
| `CircuitRevision.validation` | `service/circuit_reports/revision.rs`の仮想解析・simulation・電気的改造と、`analysis.rs`の階層分類summary | 型付きRevision検証記録と解析・simulation summary。現在の診断範囲・件数・sample・不確実性を保持する |
| `JobRecord.source` | 回路Revisionまたは採用Assemblyから、`EditOrigin`・`ElectricalEditPlan`・ジョブへ受け渡す。保存時の不変条件でも比較する | 2種のsource enumと用途別の比較。不変条件で比較していた全fieldを保持し、保存時のreviewを今の合格と解釈しない |
| `PlacedAssembly.last_observation` | live `InstanceObservation`の表示にrevalidationと`diagnose_instance`の結果を追加して保存する | 保存専用の観測・診断記録。現在の観測へ戻す変換を作らない |

単にこの3fieldを置換するだけでは完結しない。Assembly sourceが含む`ReviewResponse`と
`ReviewDiagnostics`は表示用Serializeだけを持ち、staticな説明文と`ReviewFinding`を含む。
観測側も`InstanceObservation`を意図的にDeserializeできなくしている。
これらへDeserializeを追加すると、履歴とfreshな結果の境界を不明確にする。
保存専用のowned記録と一方向の射影を設計し、関連workflowの型付き受け渡しを先に
整える必要がある。内部Valueを独自の汎用treeへ改名して残す案は採らない。

推奨する順序変更は、**第5a段階のcodec・設計図保存 → 第7段階のうち上記の型付き
report/sourceの接続 → 第5b段階の残る全store移行 → 第6段階 → 第7段階の残り → 第8段階**。
責務と物理法則、fresh review・before-state・単回消費・durable intentは維持する。
scope/player保存先のversion変更と全storeのschema/拡張子変更は第5b段階でまとめる。
今回の型整理は表示境界と複数workflowにも広がるため、順序変更の改修前で停止して報告した。
ユーザーが「履歴専用の記録型を先に整える」を承認したため、この順序で再開する。
第5段階全体は未完了。Voxrigや実機への変更は行っていない。

第5a段階の検証: codec 13件、設計図の公開MCP・再起動・採用・配置/撤去/診断の回帰25件が
成功した。追加の旧JSON file拒否と最終静的検査は完了時に追記する。
旧JSONが残る保存先への書込みを拒否し、旧fileを保持して新storeを作らない追加試験も成功。
codec/translate/MCPのall-target Clippy（`-D warnings`）、MCP no-default-featuresの
all-target Clippyが成功した。最初の設計図試験指定はmodule名の違いで0件だったため、
件数は修正後の25件を使う。サーバー起動・実機接続・ワールド変更は行っていない。

## 承認された第7a段階: 履歴専用型とreport/source接続

3つの保存fieldを、用途を限定したRust型へ接続した。
汎用JSON treeや文字列によるvariant判定は追加していない。

| field | 記録型と作成経路 | 保存後の用途 |
| --- | --- | --- |
| `CircuitRevision.validation` | `recorded_revision::RevisionValidation`。仮想状態・simulation・電気的改造・Assembly検証はtyped builderから作る。階層summaryは`recorded_analysis::CircuitIdentity`を共有する | 検証時の診断表示。現在の接続・観測や操作許可を表さない |
| `JobRecord.source`と計画のsource | `placement_source::PlacementSource`の回路Revision/採用Assembly variant。採用時のreviewは`RecordedReviewResponse`へ一方向に写す | 出典と不変の作業意図。保存されたreviewを再採用・再送の権限にしない |
| `PlacedAssembly.last_observation` | `recorded_instance::RecordedInstanceReport`。freshな`InstanceObservation`と`PlacementReview`を明示的に射影し、pureな診断を含める | 観測・再検証・撤去可否の履歴。live baselineへ戻す変換はない |

`ReviewResponse`、`PlacementReview`、`ValidatedAssemblyPlacement`、
`InstanceObservation`にはDeserializeを追加していない。
これらにDeserializeまたは記録型からのFromが追加されると型推論が曖昧になり、
コンパイルが失敗する回帰テストを置いた。
reviewと観測の変換ではfieldを列挙し、fresh側のfieldやvariantの追加を見逃さない。
Voxrigのlive scene/plan/tokenと保存診断の境界は変更していない。

公開MCPのfield名・status・null/省略の区別、分類の順位、候補8件・gate12件のsample上限、
診断のcountと位置、読戻し時計の区別、64箇所を超える電気的差分の拒否を維持する。
既存fieldの`fresh`は型内部で`fresh_at_recording`とし、記録時の意味を明記した。
`fresh_review`、`fresh_target_review`、`removal_eligible`も記録時の結果だけを示す。
再開時の判断は現在の観測、新しいmodel proofとtarget reviewから行う。
sourceの比較はenum/owned field全体のEqとなり、意図の不変条件を維持する。

この段階で保存されるpayloadは型付きだが、第5b段階のstoreはまだJSON byte codecを使う。
保存schemaと拡張子、scope/player識別、durable journal/checkpointのcodec移行は未実施。
実機接続、サーバー起動、ワールド変更、Voxrigへの変更は行っていない。

第7a段階の回帰では、MCP libraryの全体実行が224件成功・2件失敗・10件ignoredだった。
追加した大差分試験はfixtureにcallback対象のピストンがなく検証が対象外となっていたため、
fixtureを修正し、Revision記録の2件が成功した。
ドア付き建築の既存試験は生成時に`verification_not_established`/未確定となったが、
単独で再実行すると配置・診断・撤去まで成功した（323.64秒）。
この経路には既存の30秒のreview計算予算があり、並行実行時の負荷は原因の候補である。
再現の原因を確定したとは扱わず、予算・物理法則・合格条件は変更していない。
全体で失敗した2caseもそれぞれ再検証で成功し、実機依存10件を除く226caseの成功を確認した。
最終の履歴型・観測・診断workflow・Revision記録の関連11件も成功した（118.27秒）。
Clippyの指摘に従い、履歴revalidationの大きいreviewをBoxで保持した。
保存/公開fieldは変わらず、reviewと観測のlive側structをfieldごとに分解することで、
追加fieldを射影から落とした場合もコンパイルで検出する。
最終形のMCP all-target Clippyとno-default-features all-target Clippyは
いずれも`-D warnings`で成功した。formatting、差分検査とVoxrigの315ファイル一致も確認した。
第7a先行段階は完了。次は第5b段階の残る保存codecと保存先識別の移行へ戻る。

## 第5b段階: 残る保存経路と保存先識別

第7aで整えた履歴専用型を、既存の版付きstorage codecへ直接接続した。
productionの保存・読取りでJSON/Value treeを介さない。native scene・plan・intent・token・
watch・接続寿命の復元経路や、保存した合格だけで実行する経路は追加していない。

| 保存対象 | 現行file / storage schema | 符号化byte上限 |
| --- | --- | --- |
| 修復TTL plan | `repairs/<UUID>.store` / `dustroute.repair-plan-store.v1` | 16 MiB |
| CircuitRevision TTL envelope | `circuit_revisions/<UUID>.store` / `dustroute.circuit-revision-store.v1` | 4 MiB + envelope用4096 byte |
| construction job | `construction-jobs/<UUID>.store` / `dustroute.construction-job.v3` | 16 MiB |
| placed Assembly | `assembly-instances/<UUID>.store` / `dustroute.placed-assembly.v5` | 32 MiB |
| edit history | `world-edits/<UUID>.store` / `dustroute.world-edit.v2` | 32 MiB |
| survival manifest | `survival-jobs/<UUID>/manifest.store` / `dustroute.survival-job-manifest-store.v1` | 16 MiB |
| survival status | 同directoryの`status.store` / `dustroute.survival-job-status-store.v1` | 16 MiB |
| durable execution journal | jobの`execution/record.store` / `dustroute.survival-execution.v3` | 16 MiB |
| checkpoint claim | 同execution directoryの`continuation-claim.store` / `dustroute.survival-checkpoint-claim-store.v1` | 16 MiB |

CircuitRevision本体・snapshotの予算もJSON推定から実際のstorage符号化4 MiBへ移した。
manifestのdomain schemaは`dustroute.survival-job.v1`、checkpointはv2のまま。
job・Assembly・edit・journalの保存schemaは上表へ更新する。公開MCPの応答field構造、
preview v2・job response v2は維持するが、履歴の内部schema fieldには現行版が表示される。

通常の保存先は`<DUSTROUTE_STATE_DIR>/storage-v2/<scope identity>`へ分離した。
scopeとplayerは`dustroute.storage-identity.v1\0`とnamespace/valueのu64長さframeを
SHA-256で識別する。player catalogは`blueprints/<player identity>/catalog.store`。
旧DefaultHasherのroot/player directoryは探索・変換・削除しない。
明示rootを使う試験・組込み呼出しは、そのroot内で現行fileを使う。

現在の保存先に旧`.json`だけが残る場合、読取り・上書き・空履歴への置換を拒否する。
現行fileと旧fileが両方ある場合は現行fileを読み、旧fileを保持する。Assembly一覧は
UUIDを重複排除して一度だけ読む。旧形式を隔離するには利用者が明示的に退避し、現在の
観測・要求から新しく検証・計画する。現存するユーザーデータへの操作は行っていない。
checkpoint claimは現行・旧JSONのどちらが残る場合も消費済みとし、再発行しない。

共通readは上限+1 byteまで読み、codecが版・完全性・構造・型を検査する。
TTLは同じbyte列からtimestampを先に読み、期限切れならpayload型へ戻す前に除去する。
通常readでTTLを延長しない。owner/id、不変のjob source、boundary prefix、取消しの永続性、
registry lock、atomic replace、各storeの従来のsync契約を保つ。
送信前にはjournalの256 KiB、construction完了境界の64 KiBを実際の符号化byteで予約する。
codecの容量超過・schema不一致・不正内容はRust error variantで区別し、journal容量超過を
文字列一致で分類しない。既存のlive検証・再観測・単回消費・durable intent条件は保つ。

この段階は保存経路の移行であり、Voxrigのregistry/形状/NBT、残るworkflowや組込みfixtureの
JSON、実機試験用MCP証拠の集約まで撤去したものではない。それらは第6/7b/8段階へ残す。
サーバー起動・実機接続・ワールド変更は行わない。

検証: codecの14件と、保存・journal・checkpoint・survival履歴の関連24件が成功した。
追加した送信前容量予約の拒否と、旧claimの消費済み判定も全体試験で成功した。
最初のMCP全体実行は221件成功・8件失敗・10件ignored・ドア付き建築1件を別枠とした。
8件は共通の試験用transportがAssembly fileをedit schemaとして読んだため失敗した。
試験側に保存種類を明示するenumを追加し、実際の履歴型をcodecで読むよう修正した。
送信前intentの存在と保存済みprogressの検査は維持し、JSONで再解析しない。
修正後の関連31件はドア付き建築の配置・撤去も含めてすべて成功した（525.82秒）。
先の全体実行で成功したcaseと合わせ、MCPの非ignored 230caseの成功を確認した。
実機依存10件は未実行であり、offline transportは実機の物理証明とは扱わない。
codec/MCPのall-target ClippyとMCP no-default-features all-target Clippyは
`-D warnings`で成功した。Clippyが指摘した試験内の不要なCopy値のcloneを除去した。
formatting、差分の空白検査、Voxrigの315ファイル一致も確認した。
第5b段階は完了。次は第6段階のVoxrig内部表現の整理へ進む。

## 第6段階の予備調査と通信境界の承認

第6段階のゴールを作成し、別checkoutの本番経路を調査した。予備調査時点ではコード改修は未着手。
Voxrig checkoutは既存の未追跡`logs/`を除きclean、DustRouteもcleanだった。
元のVoxrig commitは`f7209ed7892aeae4b13eca5a4423949311f25317`。

| 対象 | 現在の経路 | 移行案 |
| --- | --- | --- |
| 両版block state | `block_state::StateRegistry::parse`から組込みJSONを解析。property kindは文字列 | property種別をenumにし、版別のRust定義から検査・encode/decodeする。ID範囲・property順・boolの順序・値集合を維持する |
| 1.16.1 registry | `registry.rs`がblock/item/material/recipe/entity/soundのJSONを初期化時に解析 | 必要fieldを生成済みRustテーブルへ写す。材料・item IDの照合を数値で表す。mining/crafting/public ID契約を変えない |
| 衝突とoutline | 1.16.1 collisionは`Value`で共通ID/状態別IDを分岐。1.21.11 collision/outlineはtyped structへJSONを解析 | 型付きmappingとRustテーブルへ移す。outlineのNoneをempty shapeへ読み替えず、offset・指紋・座標とbox順序を維持する |
| 1.21.11受信component | `component_nbt.rs`がNBTを汎用JSON treeへ投影し、`SystemMessage.component`に保持 | nativeのtag/整数幅/list/compoundを区別する記録型へ移す。文字列とtranslation/extraの判定、受信sequence、128件queue、64 KiB投影/深さ32/値数16384の既存予算を保つ |
| moving-piston NBT | `piston_nbt.rs`は既に用途別型と直接wire解析を使う | generic treeへ戻さない。未知fieldのskip・状態ID検査・progress/roleの照合を保つ |
| 1.16.1チャット/UI | `chat.rs`の`ChatMessage.json`と`PlayerListEntry.display_name_json`、`inventory::OpenWindow.title_json`、`ui.rs`のタイトル/チーム/bossbar等が受信JSON文字列を保持 | 通信形式の例外と内部の記録型を先に明示する必要がある。MCP限定という現方針のまま無断でJSON decoderや互換経路を追加しない |

この最後の対象はMineflayer RPCではない。現行のJava 1.16.1 adapterがnative packetの
text fieldを受け取る経路であり、`get_string`/`optional_string`から各公開fieldへ渡している。
古いJavaチャットでJSON文字列を使うことは[protocol実装側の例](https://github.com/PrismarineJS/node-minecraft-protocol#client-example-joining-a-realm)
とも対応する。Java 1.21.11のsystem messageはNBTであり、既存のJSON treeは内部投影なので
取り除ける。この二つの用途を同一の「JSON」として扱わない。

推奨案は、**Minecraft wireに必要なtext JSONの符号化・復号だけを明示的な例外とし、
受信後の内部保持・registry・物理・診断・保存・比較キーには汎用JSONを使わない**こと。
APIのtext fieldは用途別のRust型へ移し、旧JSON fieldだけを維持する互換APIは追加しない。
この例外を許可しない場合は、まずregistry/形状と1.21.11のNBT投影に範囲を絞り、
1.16.1 textの扱いを未解決として残す案になる。「内部JSONの全撤去」とは報告しない。
adapter自体の削除は、両版共存という承認済みの範囲を変えるため、推奨案に含めない。

「必要なJSON通信例外なら変更前に停止」という停止条件に該当し、いったん実装前に停止した。
その後、利用者が「マイクラサーバーとのやり取りという例外でよい」「内部通信にJSONを
使いたくない」と承認したため、第6段階の実装へ進んだ。Minecraft wireの必要payloadだけを
例外とし、内部JSON RPCや汎用Value treeを許可するものではない。

## 第6段階: 型付きnativeデータへの移行

別checkoutのVoxrigで実装・検証し、commit
`c26c1c6e83fb2e2bcfc37927744f144b0c915c4f`をvendor updaterで取り込んだ。
[型付きnative dataの仕様とAPI変更](../vendor/voxrig/docs/typed-native-data.md)に
生成手順・通信例外・維持する条件を記載した。vendorを直接改修していない。

両版のstate propertyはboolean/integer/enumのRust定義から読む。block/item、
1.16.1のmining/material/recipe/entity/soundと両版のcollision、1.21.11のoutlineは
生成済みRust定数を使い、初期化時にJSONを解析しない。material/tool/recipeのID照合も
数値のまま行う。生成はpinned dataから明示的に行い、各入力のSHA-256を定数fileに記録する。
build時の生成やネットワーク取得は加えない。元のsource data、独立fixtureとライセンスを保持する。
outlineの未対応と空形状、state ID/property順序、shape/box順序と座標bitは維持する。

1.16.1のサーバーtextは`ProtocolText`で受け取り、wire payloadを不透明に保持する。
`as_wire_json()`は通信由来の元payloadだけを返す。汎用JSON treeや内部RPCへ展開せず、
レンダリングや意味解釈は保証しない。欠落と受信した空文字列を区別する。
旧`ChatMessage.json`/各`*_json`fieldを維持する転送APIは加えず、用途名とRust型へ移した。
切断理由もlocal診断とserver textをenumで区別する。local errorの診断文はserver textにしない。

1.21.11の`SystemMessage.component`は`Option<TextNbt>`へ移した。native tag、整数幅、
listの要素tag、byte/int/long array、compound fieldを区別し、汎用JSONへ投影しない。
診断用Serializeはnativeのtag/valueを表示する形式になる。旧JSON projectionのfieldを
indexする互換性は維持しない。literal判定、receive sequence、128件queue、dropped-through、
64 KiB投影・深さ32・値数16384の既存予算を維持する。適正frameで投影できないcomponentは
Noneのまま受信記録を保持し、不適正frameは履歴を更新しない。moving-piston NBTの用途別解析、
単発操作・観測・接続寿命・記録から権限を戻せない条件と両adapterの責務は変えていない。

Voxrigの`serde_json`はdevelopment dependencyに移した。本番の通常依存treeに存在しない。
DustRoute lockfileの差分はVoxrigの通常依存から`serde_json`を除いた1行のみで、packageの
解決versionは更新していない。元JSONデータ、比較fixture、実機試験用の記録toolは残っており、
リポジトリ全体からJSONを撤去した段階とは扱わない。これらと残るworkflowは第7b/8段階で監査する。

Voxrig検証: library 212件、通常doctest 2件・compile-fail 10件が成功。実機依存8件は未実行。
全state ID/propertyと全shape座標bit、outline対応範囲、全recipeと全ての消費item/mining/
material/entity/sound fieldを元fixtureと比較した。NBTの各tag、切断入力、重複fieldの予算、
各上限と履歴の不変性も成功した。all-target Clippyは`-D warnings`で成功。
定数再生成の一致、formatting、空白検査、crate packageに生成toolと定数が含まれることを確認した。

DustRouteの全MCP library試験は229件成功・1件失敗・10件ignored（653.70秒）。
失敗した設計図最適化caseは、候補の一つが抽象検証の時間予算切れで未判定となり、期待する
合格候補数4に対して3だった。実機/Voxrig通信を使わないcatalog内の試験である。
コード・時間予算・判定条件を変更せず、同じcaseを単独で再実行し成功した（34.08秒）。
非ignored 230caseすべての成功を合わせて確認したが、一回の全体実行が全件成功したとは記録しない。
並行負荷の影響は考えられるが、原因の断定や予算緩和の改修は行っていない。

MCP既定構成・no-default-featuresのall-target Clippyは`-D warnings`で成功した。
formatting、差分の空白検査、Voxrig commitと322ファイルの一致も確認した。
実機依存10件は未実行。サーバー起動・実機接続・ワールド変更は行っていない。
第6段階は完了。次は第7b段階の残るworkflowと物理遷移の分解へ進む。

## 第7b段階の分割と第7b-1のゴール

第6段階の後、残るworkflowを調査した。`OperationRegistry`がJSONの`ok`、
`failure.progress`、`execution_progress`から成功・進捗・既に消費された操作を判定している。
公開MCP応答を解析してlive activityへ戻す経路も残る。これらを無条件の成功やzeroの進捗へ
置換してはならず、各workflowの型付き応答と同時に移す必要がある。

第7bを次の順序で扱う。今回のゴールは第7b-1に限定する。

1. **計測・観測診断・共通物理実行の整理**: 計測キーをRust enumで保持し、計測stderrと
   観測エラーDisplayからJSONを除く。物理runtimeのqueue登録とトランザクションを別moduleへ
   分ける。既存の公開計測field、通知順序、巻戻し、trace、checkpointの照合を維持する。
2. **操作結果・進捗の型付き接続**: 各workflowの応答型を共通操作registryへつなぎ、
   成功・進捗・既に消費された操作の判定とlive activityでJSONを再解析しない形へ移す。
   未知とzero、保存された記録と新しい実行事実、一次/二次失敗、再送禁止の条件を保つ。
3. **残る長いworkflowの分解と監査**: 配置・修復・transitionの工程を、admission・送信前の
   永続化・実行・読戻し・失敗/restoreへ分ける。新しい挙動や緩い判定を追加しない。
   第8段階の全体検証と残る依存・fixtureの監査へ接続する。

第7b-1では、実機接続・ワールド変更・保存schemaやVoxrig pinの更新を行わない。
新しいJSON通信例外、責務や正当性条件の変更、大きなブロック要素、宣言外の先行作業が
必要なら変更前に停止して具体案を報告する。第7b全体や内部JSON全面撤去の完了とは扱わない。

### 第7b-1の実装・検証結果

計測の`Measurement.phases`を`BTreeMap<Phase, PhaseMeasurement>`へ移し、enumをJSONへ
変換して文字列キーを取り出す往復を除いた。公開MCPの`dustroute.performance.v1`、
snake-caseのphase名と各counterは維持し、全phase名を公開形式の回帰テストで確認した。
内部の任意stderr計測は、非JSONの`dustroute.performance.trace.v2`診断文になった。
操作名の改行はescapeし、request間の計測分離とblocking workerからの復帰条件を保つ。
観測エラーの`Display`もenumの状態と理由だけを表示し、snapshot全体をJSONに変換しない。
公開観測構造、保存記録、安定baselineを使える条件は変えていない。

共通物理runtimeの所有状態と復元境界は`executor.rs`に残し、queueの登録・重複排除・
配送順序を`executor/queue.rs`、delta・carrier・履歴・通知の適用とmicrostepのcommitを
`executor/transaction.rs`へ分けた。移動した8メソッドの本体は、空白を除いた比較で
変更がないことを確認した。予算、失敗時の巻戻し、traceの確定とcheckpoint照合を維持する。

検証は全てoffline、Cargoは一つずつ`--offline --locked -j1`で実行した。

- `dustroute-minecraft --tests`: 319件成功。途中checkpoint再開、通知順序、遅い段階で
  拒否されたeffectの巻戻し、ピストン・粘着・支持喪失などの物理回帰を含む。
- MCP libraryの`performance` filter: 6件成功、計測用の2件はignoredのまま。
  観測分類の4件、操作診断・取消し・履歴とlive activityの区別の5件も成功した。
  MCP全件・実機依存試験・任意benchmarkは再実行していない。
- `dustroute-minecraft`とMCPの既定構成、およびMCPの`no-default-features`構成で
  all-target Clippyが`-D warnings`で成功した。
- formatting、差分の空白検査、Voxrigの同じcommitに対する322ファイルの一致を確認した。

第7b-1は完了。サーバー起動・実機接続・ワールド変更、依存や保存形式の更新は行っていない。
`OperationRegistry`の結果JSONと公開応答からlive activityへ戻すJSON再解析は残っており、
第7b-2で各workflowの型付き結果と一緒に移す。内部JSON全面撤去はまだ完了していない。

## 内部JSON除去の統合ゴールと最初の分岐点

ユーザーの追加指示により、第7b-2以降を個別ゴールに分けず、内部JSON除去の統合ゴールで
進める。公開MCPと承認済みMinecraftサーバーwireを除き、内部状態・通信・保存・操作結果・
進捗を用途別Rust型で接続する。分岐点は変更前に停止してユーザーへ報告する。

初回監査で、CLIの公開JSON入出力が未決定の境界として見つかった。
`dustroute-cli/src/main.rs`の`analyze-snapshot`はsnapshot JSONファイルを読み、
`run-piston-door`はscenario JSONファイルまたは標準入力を受け取る。両コマンドはJSONを
標準出力へ返す。後者には既存のCLI統合試験と`docs/piston-diagnostics.md`の再現手順もある。
これは内部RPCではないが、MCP境界でもMinecraftサーバーwireでもない。

選択肢は次の二つであり、どちらも内部の汎用JSON結果や文字列再解析を残す理由にはしない。

1. **CLIの外部入出力だけ例外として維持する（推奨案、未承認）**:
   input decodeとoutput encodeをCLIへ限定する。下位crateの
   `world_from_snapshot_json`、`PistonDoorScenario::from_json`等は型付き入力へ移し、
   CLIの結果構築と成功判定もRust型で行う。既存の公開CLI形式を保ちながら内部依存を撤去する。
2. **CLIのJSON入口・出力も撤去する**:
   当該コマンドの廃止か非JSON入力への変更、結果の表示形式、対応するCLI試験・文書を
   一緒に変更する。Rust APIとMCPの回路・物理機能を削除することは含まない。

この選択は外部互換性またはJSON例外の追加に関わるため、監査時点で実装前に停止した。
コード・依存・公開形式は未変更。build/test、実機接続、ワールド変更は行っていない。
ユーザーの選択後、操作registry・live activityの型付き接続から作業を再開する。

### CLI撤去の承認と再開

CLIの利用用途を調査し、ユーザーがCLI撤去・内部APIでのデバッグ方針を承認した。
CLIにはeval、snapshot解析、ピストン診断再現、回路datapack出力、意味論datapack出力の
5入口があった。リポジトリ内の呼び出しは開発文書とCLI専用試験6件であり、MCPから
CLIを起動する経路はなかった。ZIP出力はMCPに同じ入口がないが、Rustの出力APIは残す。

workspaceからCLI crateを外し、binary・manifest・CLI専用試験とlockfileのpackageを
撤去する。引数・標準入力・JSON応答だけの契約は廃止し、開閉・座標移動・synthetic
controller拒否の既存API回帰と、zero pulse width拒否のAPI試験で物理側の契約を確認する。
snapshot・scenarioのJSON専用APIとJSON decodeエラーも下位crateから除き、テストの
独立fixture decodeと型付きworld/scenarioの検査を分ける。CLI向けJSON例外は追加しない。
以降も統合ゴールの操作結果・進捗・残存JSONの整理を続け、CLI撤去だけを完了条件にしない。

### 回路取得情報の型付き接続

回路キャッシュの`StoredCircuit.expansion: Value`を`ExpansionEvidence`へ移した。
接続部品の探索、明示した作業領域、明示した選択領域をenumで区別し、探索上限と読込数を
Rustのfieldとして保持する。未計測の読込数は`None`であり、観測したzeroへ変換しない。
既存の大規模解析入口の閾値選択にだけ従来と同じfallbackを使い、公開reportは未知を維持する。
取得後にJSONへ符号化してcacheへ戻す処理と、解析の分岐でJSON keyを参照する処理を除いた。
取得応答も型付きでMCP facadeへ渡し、公開strategy名・field・欠落/nullの扱いを維持する。
保存形式、Voxrig pin、観測権限、実機への操作は変えていない。

### 次の分岐点: 観測属性の形式と欠測の扱い（未着手）

`transition_conformance::ObservedBlockState.properties`は任意のJSON値であり、
`observed_state`はobject以外を空の属性として扱う。その投影を使う
`normalize_observed_fixture`はtraceを`complete: true`、欠測理由なしで返している。
`InstrumentedBlockState.properties`も各propertyに任意のJSON値を保持し、string以外の
値はJSONの表示文字列で比較する。これらはコード調査で確認した内容であり、新たな実機
不一致や実行時の誤判定を再現したとの主張ではない。

JSON treeをRust側に作り直すのではなく、Minecraft属性に必要なstring・boolean・整数の
用途別型へ移すには、null・array・object・非整数数値などの扱いを決める必要がある。
既存の無条件受入れや空属性への投影を変えるため、判定条件の変更前に停止する。

- **推奨案**: それらを不正な観測属性として入力境界で拒否する。property名と値型・
  位置を診断へ残し、正当な観測値と見なさない。既存の適正fixtureのbool/整数/stringと
  正規化結果は維持し、観測記録をnative操作権限へ戻す経路は追加しない。
- **代替案**: 不正な値を用途別の欠測理由として残し、traceはincomplete/unavailableとする。
  部分的な観測を使うための契約も整える必要があり、拒否より変更範囲が広い。

この改修は未着手。CLI撤去・回路取得情報の作成済み差分の検証と記録を終えた後、
ユーザーの判断を待つ。組込み設計図のJSON初期化、操作registryの結果・live activity、
残るworkflow、診断用exampleなども未完了であり、統合ゴール全体は達成扱いにしない。

### 観測属性の拒否方針の承認と実装

ユーザーが不正な観測属性を入力境界で拒否する案を承認した。bool、string、i64、u64を
`ObservedProperty`で区別し、比較用の属性文字列は各型から正規化する。汎用JSON treeを
別名のRust型として作り直す経路は加えない。不正な構造は保持せず、拒否理由だけを入力の
所有イベントまで渡して属性名・値型・state slot・座標を診断へ含める。独立したstateの
decodeでは座標を取得できないため、未知のままにする。属性省略の既存差（packet fixtureは
必須、instrumentationはempty既定値）を保ち、明示nullを省略へ読み替えない。

packetイベント、instrumentationのstateイベント、Piston body/head/moving payload、
neighbor-update targetの全てに同じ属性契約を適用する。不正な値を正規化済みstateや
completeなtraceへ渡さない。二つのJSON専用parserとstandalone validator exampleを撤去し、
独立fixtureのdecodeはテストへ分け、内部は型付き入力と既存のvalidate APIを使う。
判定条件の変更は承認された不正属性の拒否に限り、native観測・操作権限は追加しない。

### CLI撤去・取得情報・観測属性の検証結果

全てoffline/locked、Cargoは一つずつ`-j1`、テストは単一threadで実行した。

- CLI撤去とsnapshot/scenario API整理後、translate library 180件、`fanout_probe` 12件、
  `observation_fixtures` 6件が成功した。library全件の実行は観測属性の改修前である。
- 観測属性の最終実装では、`observed_properties` 4件、`transition_conformance` 5件、
  `vanilla_instrumentation_fixture` 2件が成功した。属性名・拒否した種類・state slot・
  owner座標、全てのinstrumentation state slot、不正形式と省略の区別を確認した。
  libraryの該当moduleもtransition conformance 9件、instrumentation 17件が成功した。
- MCPの取得情報3件、視線・preview・逆解析1件、操作診断5件、大規模truth tableの
  明示要求と予算1件が成功した。不明と実測zero、公開field、取消し、消費済み履歴の
  保持を確認した。MCP全件・実機試験は再実行していない。
- workspace全targetとMCPの`no-default-features`全targetのClippyが`-D warnings`で成功。
  formatting、差分の空白検査、Voxrig 322ファイルの既存pinとの一致も確認した。

最初のMCP compileでは公開応答への取得情報代入に型不一致があり、公開encode箇所を
修正してから関連テストと静的検査を完了した。依存version、保存形式、native操作契約、
サーバー・ワールドは変更していない。これらは統合ゴールの途中の完了項目であり、
操作registry・live activity・workflow・組込み定義などの残存JSONは未解決である。

### 型付きcomponent catalog入力

`Catalog::from_json`を`Catalog::from_components`へ移した。構築時は全てのrecordを従来と
同じ`insert`へ渡し、論理仕様と重複IDの検査を維持する。presentationのdecodeエラーを
catalogのdomain errorから除き、fixture decodeはテストへ限定する。library unit test
6件が成功し、typed importでも重複IDと誤ったtruth tableを拒否することを確認した。
library全targetのClippyも`-D warnings`で成功した。

### 次の分岐点: 組込み設計図の再生成入口（改修前）

通常のCLI crateとは別に、`dustroute-translate/examples/generate_blueprints.rs`が
JSONを標準出力または指定fileへ生成している。`--check`では固定assetとのbyte一致を
検査し、file更新では既存のtype・classification・revision・assemblyを削除または
再束縛しないことを検査する。これはデバッグだけでなく、固定定義のauthoring入口である。
`docs/blueprint-architecture.md`にも再生成手順として記載されている。

本番の`builtin_blueprints`と`builtin_primitives`はJSON assetを起動時にdecodeする。
固定assetの内容は前者がtype 2件・classification 6件・revision 10件・block 729個、
後者がtype 2件・revision 2件・block 2個であり、ここにassembliesや子inclusionはない。
生成APIの`generate_builtin_blueprints`は既に独立したauthoring recipeとして存在し、
`tests/blueprints.rs`の先頭2試験が固定定義と生成レシピ、およびcompilerの幾何を比較する。

JSON例外を追加せずに進める具体案は次の通り。

1. **推奨: 固定Rust定義・既存API・回帰テストへ統合**。
   blockの座標・kind・向き・支持offset・wire接続・未知/falseをRust定数の用途別型で
   明示し、起動時には既存`BlueprintRecords::catalog`で構造を検査する。compilerは
   起動しない。type/classification/revisionのID・幾何・要求・provenanceは変えない。
   旧JSONは独立回帰fixtureに限って保持し、新定義との完全なcatalog一致を検査する。
   生成レシピとの比較はJSON byteではなくtyped catalog/physical cellで行う。
   再生成用exampleのコマンドは廃止し、生成APIとテストへ移す。上書き機能自体を
   廃止するため、そのfile更新gateだけを削除して無制限更新を残すことはしない。
   公開catalogの重複ID・再束縛拒否、archive schema/構造検証、採用の再検証は維持する。
2. **代替: Rust sourceを生成する開発用コマンドを整備する**。
   JSON入出力を廃止してRust source出力へ変える。固定定義・既存Revisionの保護・
   明示的更新を担う生成器とその検査の整備が追加で必要になる。

組込み定義の非JSON化自体は統合ゴールの範囲である。一方、今回撤去を承認された5入口の
CLIとは別のauthoringコマンドについて、再生成入口を廃止するか新形式の生成器を作るかは
未決定である。この範囲選択の改修前に停止した。上記の組込み定義・example・archive API
にはまだ変更していない。操作registry・live activity・残るworkflowも未完了である。

### 再生成入口の統合承認と固定Rust定義への移行

ユーザーが推奨案を承認した。`builtin_blueprints/data.rs`と
`builtin_primitives/data.rs`へ固定定義を移した。幾何tableの`Cell`はMinecraftの
`BlockKind`、`Facing`、`Pos`、`WireConnection`を使い、未知の給電とfalse、wire観測の
欠落と空mapを区別する。文字列のblock kindを起動時に解析せず、Rust compilerが
tableのvariant・field・値型を検査する。既存のrecord型でmetadataを明示し、
`BlueprintRecords::catalog`で構造を検査する。レシピやcompilerは起動時に呼ばない。

12件のRevision、731個のblock、全てのtype/classification、要求・provenance・IDを
既存の内容のまま保持した。元の二つのJSON assetは内容を変えず独立回帰fixtureへ移した。
再生成用exampleを撤去し、独立した`generate_builtin_blueprints` APIは維持する。
固定定義との比較はtyped catalogとphysical cellで行い、file上書きの入口は残さない。

カタログと更新提案の`to_json`/`from_json` APIも撤去した。カタログは
`BlueprintCatalogArchive`と`archive`/`from_archive`、更新提案は既存の
`BlueprintUpdateArchive`と`archive`/`from_archive`で接続する。schemaと参照の検査、
保存された合格を新しい採用権限にしない再検証条件は変えない。

カタログのarchiveを公開Rust型にした際、canonical/storage codecがstruct名を含める
ことを確認した。serializer上の名前は従来の`Archive`を維持し、以前のprivate recordの
独立定義に対して保存byteとtyped再構築を検査する。保存schemaやcodecは変更しない。

presentationのschema・改ざんfixtureのdecode/encodeはテスト専用supportへ移した。
最適化の状態比較はcatalogそのものを比較し、再構築はtyped archiveを使う。
library/translate/Minecraft crateの直接の`serde_json`依存はdev dependencyとした。
MCP用の既存schema metadataを生成する`schemars`等の間接依存まで撤去したとの
主張ではない。旧JSONを本番保存の互換入力として探索・復元する経路は加えていない。

操作registryの結果、公開応答からlive activityへの再解析、残るworkflow・診断exampleの
JSONはまだ残っている。これらの全面除去まで統合ゴールを達成扱いにしない。

#### 固定定義とtyped archiveの検証

- libraryのunit/integration全64件が成功した。固定fixtureとの全record・展開の一致、
  保存byteの維持、非JSON codecからの復元、schema拒否・重複拒否を含む。
- translateの`blueprints`、`blueprint_updates`、`reference_door_adoption`、
  `flying_machine_adoption`、`runtime_adoption`の計32件が成功した。最初の独立生成
  レシピ比較2件と固定catalog/deviceの6件も成功し、変更後の該当全targetで再確認した。
- optimizeの`reduced_candidate_enters_existing_parent_proposal_and_is_reverified_after_reload`
  1件が成功した。最適化側へJSON依存を追加せず、状態比較とarchive再構築で確認した。
- MCPの`blueprint_mcp_` filterは6件成功した。保存・再起動・採用、妨害されたwire、
  保存された偽の合格、欠測のclearance、保存Assemblyの扱いを確認した。
- workspace全targetとMCPの`no-default-features`全targetのClippyが`-D warnings`で成功。
  初回Clippyで最適化テストの古いJSON helper依存が見つかり、typed比較・再構築へ
  直してから検査を完了した。内部APIの検査を旧JSON helperに依存させていない。

全てoffline/locked、Cargoは単独`-j1`、テストは単一threadで実行した。
実機接続・ワールド変更・依存version更新は行っていない。codecの互換性検査用に
既存workspaceの`dustroute-codec`をlibraryのdev dependencyへ追加した。
formatting、差分の空白検査、Voxrig 322ファイルの既存pinとの一致も確認した。

### MCPの結果フラグとクロック比較の型付き接続

MCP facadeの戻り値を文字列から`CallToolResult`へ変更した。応答の成功・失敗は
JSON文字列へ変換する前に判定し、SDKの成功/失敗resultとして渡す。
`failure::mark_tool_failure`による完成した応答文字列の再解析を撤去した。
公開textのfield、既存の診断補完、未知の進捗とrecoveryは維持する。
履歴取得の内部に失敗した操作が含まれても、取得自体の成功を失敗へ変えない。
応答型の変更でoutput schemaを追加せず、public tool metadataも維持する。

これはworkflow全体の型付き移行の完了ではない。認可エラーをencoded textで
内部へ渡す既存経路、操作registryの`result: Value`、workflowのJSON reportと
`performance::tool_progress`による進捗再解析は残る。過去の記録をlive activityへ
読み直さない条件を維持しながら、実行結果の型付き接続と一緒に移行する。

四ブロックのクロック比較は、JSONを読むexampleから
`periodic_clock_observation`のRust APIへ移した。入力の`ClockCapture`、外部刺激、
固定配置、sample、edge、差分、モデル診断を用途別の型で保持し、比較中にJSONを
生成・再解析しない。exampleのコマンド入口は撤去した。独立したMinecraft観測の
JSON fixtureはテストの入力境界でdecodeし、既存の五つの比較をRust recordに対して
実施する。配置・座標・刺激の余分なfieldを拒否する検査も加え、以前のexact JSON
比較が持っていた固定scopeの条件を緩めない。

モデルのhidden stateは診断用の値として返すだけで、Deserializeや復元経路を加えない。
新profileの有限sample一致、旧profileの反例、finite-burstの証明、周期要件の失敗、
実機での無限動作・restartabilityの未証明を引き続き区別する。

#### MCP応答とクロック比較の回帰検証

MCPの全lib試験は236件成功、失敗0件、実機・計測用10件はignoreのまま。
公開MCPの失敗flagと成功した履歴取得、診断欠落のnull、encode失敗、toolのoutput schema、
採用・再起動・配置・撤去・建築・修復・取消し・再送禁止を確認した。
最初のcompileでhandlerの応答型に依存するテストの修正漏れを検出し、MCP応答は
テスト専用decoderで確認する形へ直した。Rust API自体が返す通常のエラー文字列は
そのまま検査する。結果の文字列再解析を本番の制御経路へ戻していない。

クロック比較の6件も成功した。修正版は641 sample全一致、旧版の48 sampleの反例を
維持し、recovery 261 sampleとqueue診断33 sampleを照合した。scope・coverage・
cleanupの不足、および配置や刺激へ余分なfieldを加えた入力を拒否する。
JSON fixtureは独立した観測資料のままで、期待値を新しい実装から作り直していない。

workspace全targetとMCPの`--no-default-features`全targetのClippyが
`-D warnings`で成功し、formattingと差分の空白検査も成功した。
Cargoは単独・offline/locked・`-j1`で実行した。実機接続、ワールド変更、
Voxrig source/vendor pin、保存schema、依存versionへの変更はない。
