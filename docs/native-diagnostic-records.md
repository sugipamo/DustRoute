# サバイバル診断の型移行で見つかった境界

調査基点: DustRoute `f478986`とVoxrig `5bace7be7cd941e1340ad94052e922db23c4f892`。
診断JSONを置き換える際の設計判断と実装記録。2026-10-03 UTCにユーザーが改修を承認し、
Voxrigの別checkoutで実装・検証後、DustRouteへ接続した。

## 見つかった依存

DustRouteの`DiagnosticPayload(Value)`は単一のreceiptではない。
StandingContext、仮想移動・照準・再接続条件、能力、インベントリ交換、PlayerState、
配置intent/status、移動記録、採掘intent/status、接続更新の証拠を包む。
それらのfield型をたどると、現行のネイティブ定義64種類へ広がる。
接続更新の証拠が旧OperationHistoryを含むため、元の採掘以外の配置・移動も依存に入る。

64種類すべてを新設するという意味ではない。数値・座標・属性・入力・状態ラベル等の
純粋なデータ型を共有できれば、別の保存用型が必要な範囲は小さくなる。
単純にすべてを二重定義すると、ネイティブ変更のたびに両側の型を追従する負担が生じる。

一方、現在の次の型はserialized fieldを持ちながら、importできない設計になっている。

- `HypotheticalReconnectBoundary`: 将来の接続境界に対する条件。fieldはprivate。
- `MiningRetirementWatch`: 元の採掘と独立観測の登録済みwatch。fieldはprivate。
- `PlayerMotionWatch`: observer・generation・entity instanceに結び付くwatch。fieldはprivate。
- `SurvivalMotionRecheck`: 登録済みの再観測要求。fieldはprivate。
- `SurvivalMotionRecord`: 公開の診断fieldのほか、保存しない受信境界・observer sessionを持つ。

既存型へまとめてDeserializeを付ける移行は行わない。原点identity、watch、token、
Session等のlive guardを保存形式から再作成する機能も追加しない。

## 採用した構造

Voxrigのchecked survival APIに、診断専用のデータ層を設ける。
Rust型の定義は以下の三群に分け、共用可能なfieldを一度だけ定義する。

1. 純粋なデータ: 座標、ブロック状態、属性、入力、受信ordinal、結果ラベル等。
   executable capabilityを含まないものに限り、型付き保存・読込みで共有する。
2. 診断記録: standing/movement、inventory、placement、mining/recoveryの用途別記録。
   freshな値から一方向に作る。接続guardやwatchを復元する逆変換は設けない。
3. liveなchecked値: 接続・登録されたwatch・操作token・場面の原点identity。
   現在のnon-Deserialize契約とnative側の毎回のadmissionを維持する。

例えば、`StandingContext`から`RecordedStandingContext`を作って保存できても、
`RecordedStandingContext`は`validate_survival_scene`や操作APIへの入力型にはならない。
再開時には現在のワールド・所持品・位置を読み、新しい場面と計画を作る。
同様に、保存した採掘やretirementの記録は、結果が不明な旧操作を再送する権限を持たない。

公開APIの呼出契約と責務は維持するが、共通データの抽出に伴う型構造・fieldの変更は
実装前に範囲を特定する。データを共有するためにprivate guardを公開しない。
Voxrig側には設計図・建築の手順選択・資材予約・永続ジョブを持ち込まない。
DustRoute側はこの診断型をRecordedStep、checkpoint、manifest、イベントへ組み込む。
旧データの互換復元より、新しいschemaで拒否・診断専用・単回消費を明確にする。

## 作業と検証の順序

- Voxrigの純粋なデータとlive guardを分類し、共有できる基本型から移す。
- 用途別の診断射影を追加する。non-Deserializeの値と異なる型として公開する。
- nativeの単発操作・受信・採掘退役・移動の回帰と、型を取り違える呼出のcompile-failを確認する。
- separate checkoutへcommitした後、検証済みrevisionをvendor updaterで取り込む。
- DustRouteのopaque payloadを撤去し、イベント・保存schema・再読込み・checkpointを検証する。
- 保存された診断から権限や自動再送が復元されないこと、durable intentと単回消費の順序を確認する。

## 実装結果

最初の依存調査は64型。全体の配置・ブロック編集も診断射影へ統一した結果、対象は66型となった。
39型は接続や操作権限を持たない純粋なデータとして共有し、27型は別の`Recorded*`型を公開した。
`checked_survival::diagnostic`を公開入口とし、`ToDiagnostic`で直接Rustの値を射影する。
JSONへ書き出してから読み戻す処理はない。

ネイティブ値と記録のfieldは、元の定義位置にある`diagnostic_record!`で一度だけ宣言する。
元のfieldの可視性、derive、methodは維持する。`native_only`の接続guard・受信境界は
記録へ含めない。privateなwatchの識別情報を記録へコピーしても、ネイティブwatch自体を
公開・生成する経路にはならない。記録からネイティブ値への逆変換は実装しない。

DustRouteでは`DiagnosticPayload(Value)`を除去し、計画・checkpoint・イベントをこの型へ
接続した。採掘後の照合条件も`RecordedCleanupRecoveryPlan`へ一方向に射影する。
イベントの`ExecutionEvidence`は用途別のenumとなり、種別と証拠型が一致しない場合は
保存前と読込み時に拒否する。診断型の導入時にはjournalを`dustroute.survival-execution.v2`
へ更新した。その後の[保存codec移行](json-boundary-migration.md)では非JSONのv3へ移した。
旧v1を変換する経路は設けず、旧形式は拒否する。checkpointのwire構造は保ち、v2のままとした。
再開は現在の状態からの新規計画・検証と単回消費を維持する。

Voxrig revision: `f7209ed7892aeae4b13eca5a4423949311f25317`。
別リポジトリへcommitし、vendor updaterで315ファイルを取り込んだ。直接vendor編集は行っていない。
このcommitはローカルの履歴であり、今回pushやPR作成は行っていない。

検証: Voxrigのlib 203件成功・明示実機用8件ignore、doctestの通常例2件とcompile-fail 10件成功。
ネイティブ計画・intent・standing・watch・移動記録をDeserializeできないこと、
記録から採掘intentやstandingへ戻せないことを含む。private watchの寿命検査と
結果不明の採掘記録の保存再読込みも成功した。all-target Clippy（`-D warnings`）、
formatting、差分検査、vendor一致検査が成功した。
DustRouteの最終検証結果は[移行記録](json-boundary-migration.md)を参照。
実機操作は行っていない。JSONの保存codecは第5段階で移行する。
