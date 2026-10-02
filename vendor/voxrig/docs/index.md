# Voxrig ドキュメント

`Voxrig`は、外部の頭脳から直接操作するMinecraft Java Edition 1.16.1向けheadless clientクレートです。

`Bot` の既存説明は1.16.1専用です。共存する1.21.11観測アダプタは
[移行計画](dustroute-integration.md)、[対応範囲と検証](version-adapter-validation.md)、
[ピストン通知と階段の未同期](piston-client-update-prerequisite.md)を参照してください。
追加実装は[クライアント側ピストン処理](client-piston-reconstruction.md)、
レビュー方針は[PR準備](pull-request-preparation.md)にまとめています。

## 利用者向け

- [導入と最初の接続](getting-started.md)
- [対応機能と制約](capabilities.md)
- [公開API](api.md)
- [Java 1.21.11のサバイバル所持品交換](survival-inventory.md)
- [Java 1.21.11の静止・接地判定の基盤](survival-standing-context.md)
- [Java 1.21.11の採掘完了・中断の実機比較](survival-mining-comparison.md)
- [Java 1.21.11の送信中断と切断後の履歴](survival-outbound.md)
- [Java 1.21.11の限定サバイバル採掘と継続境界](survival-mining.md)
- [Java 1.21.11の採掘接続の退出確認と再接続](survival-mining-retirement.md)
- [Java 1.21.11の共通ロード完了処理](survival-interaction-loading.md)
- [Headless client API拡張ロードマップ](headless-api-roadmap.md)
- [API契約と所有権](api-contracts.md)
- [状態・イベントの扱い](state-and-events.md)
- [Protocol 736 coverage](protocol-coverage.md)
- [設計と責任境界](architecture.md)

初めて利用する場合は「導入と最初の接続」から読み、操作の一覧は「公開API」、対応可否は「対応機能と制約」を参照してください。

## 開発・検証

- [開発とテスト](development.md)
- [Server teleport後の位置補正調査](teleport-investigation.md)
- 実サーバーの構築（repository版の`test-server/README.md`）
- 耐久試験の結果（repository版の`reports/`）

## 完了済み計画と記録

以下は現行仕様ではなく、実装経緯と検証証跡を保存する文書です。

- [物理ロードマップ](history/physics-roadmap.md)
- [物理実装完了報告](history/physics-completion-report.md)
- [サバイバルロードマップ](history/survival-roadmap.md)
- [サバイバル実装完了報告](history/survival-completion-report.md)

- [Ordinary survival placement and material receipts](survival-placement.md)
