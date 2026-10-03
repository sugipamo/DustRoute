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
