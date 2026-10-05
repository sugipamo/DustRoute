# 導入ガイド

[English](getting-started.md) · [日本語](getting-started.ja.md)

先に[概要](../README.ja.md)と[対応表](capabilities.ja.md)を参照してください。
このページでは準備を説明します。サーバー登録・transport・全ポリシー設定は
[運用者向け設定仕様](../crates/dustroute-mcp/SETUP.md)にあります。

## Minecraftへの反映方法を選ぶ

| 経路 | 準備するもの | 権限と範囲 |
| --- | --- | --- |
| 観測・試作 | 起動したサーバー、bot接続、視線用の指定プレイヤー。材料は不要 | 既定の読取り専用ポリシー。nativeブロック観測はOP不要。視線の追跡回復・領域プレビューには権限付きコマンドを使う場合がある。オフライン設計図作成は実接続不要。 |
| コマンド施工 | 対応する設計、完全に観測した現地 | botのOP、許可領域・プレイヤー・dimension、変更ポリシー、プレビュー・確認。所持品は消費しない。 |
| サバイバル施工 | 採用済みの地面付き受動建築設計、botの材料・仮設材、明示した作業・移動・退避範囲 | 非OPの通常操作と変更ポリシー。観測botの追加は不要。能動回路の施工・チェスト補給は対象外。 |

コマンドによる新規Assembly配置には、ガードを含む空き領域が必要です。
サバイバル施工は宣言した地面付き現地と移動契約を使います。
実行前検査は異なり、一方の計画は他方を実行する権限になりません。

## 接続を準備する

信頼できる非公開のMinecraft Java 1.21.11サーバーをオフライン認証で使います。
bot・プレイヤーの正確な名前をwhitelistに登録します。botの既定名は
`DustRouteBot`です。同じアカウントを他のクライアントから同時使用しないでください。
サーバーをホストする場合はJava 21が必要です。補助MOD、Node.jsブリッジ、
別のVoxrig checkoutは不要です。オフライン認証サーバーは外部公開しないでください。

リポジトリのルートで、Rust/Cargoと固定した依存を使ってビルドします。

```bash
cargo build --locked -j1 -p dustroute-mcp
```

依存取得後は`--offline`も指定できます。既定ビルドにVoxrigを含みます。
以下は読取り専用のstdio起動例です。プレイヤー名・保存先を自身の設定へ置き換えます。

```bash
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_ASSIST_PLAYER=YourMinecraftName \
  DUSTROUTE_MC_AUTH=offline \
  DUSTROUTE_MC_VERSION=1.21.11 \
  DUSTROUTE_READ_ONLY=true \
  DUSTROUTE_STATE_DIR=/private/path/to/dustroute-state \
  target/debug/dustroute-mcp
```

MCPクライアントに、この実行ファイルと環境変数を設定します。stdioはMCPメッセージ用です。
代わりに、運用者向け資料のloopback限定HTTP `/mcp`も使えます。遠隔HTTP公開は未対応です。
新しいサーバーは[非公開試験サーバーの設定](../crates/dustroute-mcp/SETUP.md#prepare-the-vanilla-server)
を参照してください。その例はcreativeのコマンド試験用です。
サバイバル施工では、survivalのbotと所持品を用意します。

## 最初の作業前に確認する

1. AIに`get_bot_status`を呼び、接続・指定プレイヤー・ポリシーを報告してもらいます。
2. `get_world`で既知の小領域を観測するか、回路を見て`test_circuit`を使います。
   視線には指定プレイヤーのログインと追跡が必要です。
3. dimension、範囲、観測の完全性、証拠の由来を確認します。
   観測不成立は空き領域ではありません。未対応ブロックを普通の固体として扱いません。
4. [利用手順](workflows.ja.md)を選びます。実世界を変更するときは、運用設定で
   `DUSTROUTE_READ_ONLY=false`と許可領域・プレイヤー・dimensionの適切な制限を
   明示し、ツールが返した具体的な計画を確認します。

## 保存されるものを理解する

採用カタログ、Assembly instance、ジョブ履歴には永続的な`DUSTROUTE_STATE_DIR`を
設定してください。既定の保存先は一時ディレクトリです。
観測`circuit_id`はプロセス内の短期記録、回路Revisionの既定保持期限は1時間です。
カタログ・履歴の保存は古い実行権限を復元しません。再起動後は観測し直し、新しい計画を
作ります。[ID期限](mcp-public-features.md#ids-and-retention)と
[復旧手順](workflows.ja.md#診断復旧する)を参照してください。

[利用手順](workflows.ja.md)へ進むか、[文書案内](README.ja.md)からAPI・機能仕様を選んでください。
