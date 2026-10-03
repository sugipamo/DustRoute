# サバイバル診断の型移行で見つかった境界

調査基点: DustRoute `f478986`とVoxrig `5bace7be7cd941e1340ad94052e922db23c4f892`。
診断JSONを置き換える際の設計判断。ここで提案するVoxrig改修は未着手。

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

## 推奨案

Voxrigのchecked survival APIに、診断専用のデータ層を設ける。
Rust型の定義は以下の三群に分け、共用可能なfieldを一度だけ定義する。

1. 純粋なデータ: 座標、ブロック状態、属性、入力、受信ordinal、結果ラベル等。
   executable capabilityを含まないものに限り、型付き保存・読込みで共有する。
2. 診断記録: standing/movement、inventory、placement、mining/recoveryの用途別記録。
   freshな値から一方向に作る。接続guardやwatchを復元する逆変換は設けない。
3. liveなchecked値: 接続・登録されたwatch・操作token・場面の原点identity。
   現在のnon-Deserialize契約とnative側の毎回のadmissionを維持する。

例えば、`StandingContext`から`StandingRecord`を作って保存できても、
`StandingRecord`は`validate_survival_scene`や操作APIへの入力型にはならない。
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

この前提整備を入れる場合、第2段階の中でVoxrig側の型設計を扱う。
第6段階に予定していたregistry・形状・NBT・通信形式の移行とは分ける。
先に別案を採るなら、この部分を保留して後続の独立したLaw移行へ進めることもできる。

## 今回の停止位置

検証report、観測失敗詳細、piston配置の初期条件の型移行は先行して実施。
`DiagnosticPayload(Value)`・サバイバルの履歴はまだ残る。
64種類にまたがるデータ共有とlive型の境界調整は、単純なJSON置換より広い設計変更になるため、
ユーザーの「責務に関わる部分の変更は慎重に、必要に応じて作業を止めユーザーに報告」に従い、
Voxrig側の変更を開始する前で停止する。
