# DustRoute 向けクライアント導入調査

状態: 1.16.1を版別モジュールへ分離し、1.21.11の接続・受信観測を追加。
両版を隔離サーバーで確認した。第3段階の追加実装は承認後に再開し、
[移動中状態を持つクライアント更新機構](client-piston-reconstruction.md)の限定範囲を実装・検証した。
第4・5段階は未着手。
検証範囲・未解決の試行は [検証記録](version-adapter-validation.md) を参照。
調査日: 2026-09-29。

## 作業場所と基準

- リポジトリ: <https://github.com/sugipamo/Voxrig>
- ローカル: `/root/Voxrig`
- remote: `upstream`（上記リポジトリ。push はしていない）
- ブランチ: `codex/dustroute-client`
- 調査した上流コミット: `6434b2cd8d7328d397b34b0151660a9882d844fc`
- 接続先プロジェクト: `/root/DustRoute`、基準 `b1762b9`
- Voxrig は MIT。生成データ・fixture の出典は既存の
  [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) を維持する。

目的は、自分たちで受信・状態反映・観測の因果関係を追跡できるクライアントを持ち、
検証できた範囲でコマンドによる常時照合を不要にすること。
Rust 化だけでサーバー状態との一致が保証されるとは扱わない。

## 既存の土台

Voxrig は接続・packet parser・chunk cache・型付き操作 API を持つ。
world の snapshot は lock 内で取得し、領域別 revision を付けられる。
`ChunkSnapshot` は Arc による共有、更新時の copy-on-write を使う。
`None` は未ロードであり Air ではないという区別もある。
受信とクライアントの状態管理を Voxrig、回路の設計・シミュレーション・採用・診断を
DustRoute に置く責務分担は可能。

根拠: [architecture](architecture.md)、[state-and-events](state-and-events.md)、
`src/client.rs::observe_snapshot` / `block_snapshot`、`src/world.rs::ChunkSnapshot`。

## バージョンアダプタによる共存

ユーザーの指定により、1.16.1 を 1.21.11 で置き換えず、同じライブラリに両方を持たせる。
まず既存動作を維持して 1.16.1 の実装を明示的な層へ抽出し、その横に 1.21.11 を追加する。
以下は全体設計であり、各段階の実装・検証状況は別記録で区別する。

| 層 | 責務 |
| --- | --- |
| 共通の公開 Rust API | 観測、移動・操作要求、接続状態、機能の対応状況を公開する |
| 共通の接続・状態管理 | 受信順、接続世代、状態反映、snapshot revision、未ロードと切断を管理する |
| バージョンアダプタ | 版ごとの接続状態遷移、packet decode/encode、chunk・item・entity の wire 表現を扱う |
| バージョン別 registry とクライアント処理 | block state properties、衝突形状、dimension の解釈、移動・操作・通知処理の版差を扱う |
| 共通の通信部品 | TCP、frame、圧縮、数値 encode/decode など、同じ仕様を確認できる処理を共有する |

提案する版別モジュールは `versions::java_1_16_1` と `versions::java_1_21_11`。
公開する操作・観測の意味を共通化し、送受信の表現と版固有の処理は各モジュールへ置く。
1.21.11 の情報を 1.16.1 のデータ形式へ丸めない。共通化できない情報は型付きの版別表現を残す。

接続前に `MinecraftVersion` のような Rust enum で版を明示し、接続ごとに adapter を固定する。
Rust の型・trait と網羅的な match で、必要な実装が欠けた場合に検出できる構造を優先する。
設定ファイル内の任意式や、各処理へ分散したバージョン条件は導入しない。
同一プロセス内で別の BotManager が別バージョンへ接続しても混ざらないことを完了条件にする。

共存のために必要な変更点:

- `PROTOCOL_VERSION`、`CLIENT_INFO` と静的な `Bot::protocol_info()` / `capabilities()`
  だけでは接続先を表せない。接続に紐づく版・対応機能の情報を設ける。
  既存の 1.16.1 呼出しの扱いも明示し、1.21.11 の未実装操作は型付きエラーで拒否する。
- `registry.rs` と `collision.rs` の固定データを版別にする。
  同じ整数 state ID を別版で同じブロックとして扱わず、registry の識別情報を
  snapshot、観測・変換の境界、共有 cache の利用条件へ結び付ける。
- `World::block` の Y=0..255、`apply_chunk` の16 sectionなどの前提を、
  adapter が解釈した dimension 情報へ移す。
- `World::contact_effects` の state ID 9668、流体処理の `state - 34` / `state - 50`
  のような版依存を、registry の properties・意味または版別処理へ切り出す。
  一方、差が確認されていない計算まで版ごとに複製する必要はない。
- registry ID の対応、パケットID、単純な値の差は Rust 定数・生成データで表し、
  状態遷移や item/window の手順の差は型付き実装で表す。
- 受信イベント、cache 更新、snapshot を接続世代・受信順・revisionで対応付ける。
  ローカルの順序保証とサーバー上の現在状態の保証は区別する。

この共存は Voxrig クライアントの対応範囲である。
DustRoute のシミュレータや採用検証を 1.16.1 対応に広げる作業は含めず、
DustRoute の初期接続先は引き続き 1.21.11 とする。
コマンド確認を除去できるかは、アダプタ導入とは別に実機比較で判断する。

## 導入前に必要な変更

| 項目 | 確認した現状 | DustRoute に必要な対応 |
| --- | --- | --- |
| Minecraft バージョン | Java 1.16.1、protocol 736 固定 | 1.16.1 を維持し、Java 1.21.11 の adapter を追加。定数変更だけでは接続・decode できない |
| 接続状態とパケット | 1.16.1 の login/play、固定 ID の分岐 | 対象版の login/configuration/play、必須応答、切断・再設定の扱いを整備 |
| chunk と dimension | 1.16.1 の bitmap/16 section/biome 形式 | 対象版の chunk、dimension 高さ、負の Y、palette と unload/再ロードに対応 |
| block state | 1.16.1 の state ID と名称が中心 | 対象版の ID と全 properties の対応を持ち、DustRoute の native state へ欠落なく変換 |
| ピストンによる更新 | Block Action はイベントとして公開 | ピストン通知、通常の状態更新、必要なクライアント処理の関係を対象版で検証 |
| 観測の出所と時刻 | ローカル revision と時刻、broadcast event | 接続世代、受信順、状態反映順、snapshot の境界、欠測を追跡できる観測 API |
| DustRoute 接続 | 現行は Mineflayer 向け bridge と確認証拠を要求 | Rust adapter と観測契約を設計し、操作の受理と配置確認を分離 |

実装上の根拠:

- `src/protocol.rs::PROTOCOL_VERSION`、`src/client.rs::CLIENT_INFO` と接続処理。
- `src/client.rs::read_loop` の `0x0a` は `Event::BlockAction` を emit する。
  その分岐には、移動先や隣接階段の形状を更新する処理はない。
  この事実だけで 1.21.11 の階段問題の原因を確定したとは扱わない。
- `src/world.rs::apply_chunk` / `apply_multi_block_change` は 1.16.1 の形式。
- `src/registry.rs::BlockData` と `block_name_from_state` は、DustRoute が必要とする
  完全な state properties の公開変換 API にはなっていない。
- `docs/state-and-events.md` の revision は接続内・同じ状態領域内で比較するもの。
  サーバーの game tick やサーバー確認の証拠に読み替えない。
- DustRoute の `crates/dustroute-mcp/src/bridge.rs::ConfirmedRegion::validate` と
  `bridge_protocol.rs::PhysicalPlacementMode` は、現在の観測・実行方式に依存する。
  ローカル snapshot に既存の `server_confirmed` を付けて通す変更はしない。

現在の両プロジェクトの隔離試験は offline-mode を使用できる。
Microsoft 認証の追加は初期導入の前提にしない。必要になった時点で別途範囲を決める。

## 推奨する段階と完了条件

1. **1.16.1 を維持してアダプタ層を抽出する。**
   通信部品、状態管理、版固有の decode/encode・registry・クライアント処理を分ける。
   1.16.1 の既存 parser、状態更新、操作、移動の回帰試験と隔離サーバー試験を行う。
   この段階で新しい版へ接続できるとは表示しない。

2. **1.21.11 の観測用アダプタを追加する。**
   必須の接続・状態遷移・応答と chunk/block 更新を先に対応する。
   矩形領域の全セル・全 properties と未ロードを区別して取得できること、
   再接続・再設定・dimension 変更で旧 cache を混同しないことを確認する。
   protocol・registry データは版と出典を固定し、現在の 1.16.1 機能をそのまま
   1.21.11 対応と表示しない。

3. **階段問題を受信から snapshot まで追跡する。**
   受信 packet、decode 結果、world revision、snapshot の対応を保存・再生できるようにする。
   隔離環境で既存のピストン・階段 fixture を使い、サーバーの記録と比較する。
   正しい state packet の未反映、Block Action から必要なクライアント処理の不足、
   受信情報だけでは確定できない状態を区別する。
   最終状態だけでなく、遅延・chunk 再ロード・欠測時に古い観測を採用しないことも確認する。
   この段階のコマンド照合や計測 MOD は、移植の比較検証用として使える。

4. **DustRoute を Rust API へ接続する。**
   まず観測 adapter、次にレバー入力・配置・撤去を移す。
   視線・対象プレイヤー・移動・inventory を含む既存 bridge の必要 API を棚卸しし、
   対応した機能だけを公開する。Voxrig に Blueprint や回路シミュレータを持ち込まない。
   観測根拠の違いを型と採用条件に明示し、ローカル予測をサーバー観測に偽装しない。
   参照ドア、フライングマシン、修理・取消しを隔離環境で確認する。

5. **検証できた範囲でコマンド確認と Mineflayer 依存を除去する。**
   新クライアントを使って対象の操作を一巡でき、欠測・切断・外部変更を適切に扱えることが条件。
   受信情報だけでは確定できないケースが残れば報告し、保証を暗黙に弱めない。
   同じコードを使うシミュレータの予測との一致だけでは、独立した実機検証の代わりにしない。

## 承認済みの範囲と停止条件

バージョンアダプタ層の抽出、1.21.11 の追加、受信観測の比較、DustRoute接続、
検証した範囲でのコマンド確認除去という順序は、ユーザーの「この順番で対応」で承認された。
1.16.1 を破棄する案は採用しない。宣言範囲に無い大きな前提作業が新たに必要と判断した場合は
作業を停止して報告する。

DustRoute 側の依存関係・既存 bridge・コマンド確認はまだ変更していない。
remote push は未実施。コマンド確認を完全に廃止できるかは未検証。

前回の停止は、調査から新たに具体化した中規模以上のクライアント更新機構について、
以前の「実装が重そうなポイントを見つけたらいったん止めて」という指定に従うもの。
版の接続・decodeという承認済みの実装は進め、結果を検証している。
その後「移動中状態も対応」「Voxrigを大きく変えてよい」「後でPRを出せるように」という
承認と再開指示を受け、通常・粘着ピストンの開始・完了・反転、階段・支持更新を追加した。
この範囲の再承認は不要。別の大きな前提が必要になった場合の停止条件は引き続き適用する。

上流への大きな変更を提案する場合は [CONTRIBUTING.md](../CONTRIBUTING.md) の手順も確認する。
今回、外部への issue・PR・メッセージ送信は行っていない。
