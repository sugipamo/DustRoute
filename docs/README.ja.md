# 文書案内

[English](README.md) · [日本語](README.ja.md)

全体像から目的別の操作へ、さらに正確な契約・実装へ進める順序です。
標準言語は英語とし、利用者向けの入口には日本語版も用意します。
API・アーキテクチャ・証拠の詳細資料は英語のみでも構いません。

## この順序で読む

| 深さ | 知りたいこと | English | 日本語 |
| --- | --- | --- | --- |
| 1. 概要 | 何のためのツールか | [Project overview](../README.md) | [概要](../README.ja.md) |
| 2. 対応範囲 | 自分の作業が可能か、どんな条件か | [Capabilities](capabilities.md) | [対応表](capabilities.ja.md) |
| 3. 準備 | 接続・材料・権限は何が必要か | [Getting started](getting-started.md) | [導入ガイド](getting-started.ja.md) |
| 4. 利用手順 | 観測・設計・施工・復旧をどう進めるか | [Workflows](workflows.md) | [利用手順](workflows.ja.md) |
| 5. 契約 | ツール・ID・実行条件の詳細 | [Public MCP reference](mcp-public-features.md)と下の機能案内 | 詳細資料は英語標準 |
| 6. 実装・証拠 | 実装方法と確認した内容 | [詳細資料一覧](README.md#architecture-and-development) | 詳細資料は英語標準 |

## 機能から選ぶ

| 作業 | 契約 | 証拠・さらに詳しい資料 |
| --- | --- | --- |
| 観測・回路の解釈 | [公開MCP](mcp-public-features.md)、[機能モデル](physical-function-model.md) | [nativeクライアント](voxrig-rollout.md)、[視線試験](native-client-usability.md) |
| 既存地点の編集 | [Revision](circuit-revisions.md)、[編集範囲](world-edit-scope.md)、[区画ジョブ](large-circuit-regions.md) | [native設計図・区画実機検証](blueprint-iteration-live-validation.md) |
| 定義の再利用・検証 | [設計図MCP](blueprint-mcp.md)、[検証エラー](blueprint-review-diagnostics.md) | [アーキテクチャ](blueprint-architecture.md) |
| 建築・設備の設計 | [建築入力](blueprint-building-design.md)、[小建築](blueprint-building.md) | [実機での更新](blueprint-iteration-live-validation.md) |
| 所持品で建築 | [公開サバイバル施工](survival-public-construction.md) | [単一botの設計と証拠](survival-single-client-plan.md) |
| ピストン・ドアの配置 | [カスタム配置](custom-piston-assembly-placement.md)、[1×2](piston-door-mcp-v1.md)、[3×3型](piston-door-type.md) | [電気試験](piston-electrical-live-evidence.md)、[参照ドア](reference-door-live-construction.md) |
| 有限フライングマシンの生成 | [生成](flying-machine-generation.md)、[配置から撤去](flying-machine-lifecycle.md) | [エンジン定義](flying-machine-engines.md)、[収穫範囲](flying-machine-practical-roadmap.md) |
| 診断・復旧 | [Assembly診断](assembly-diagnosis.md)、[instance管理](placed-assembly-management.md) | [共通診断](diagnostic-system.md)、[失敗契約](structured-failure-recovery.md) |

## 詳細資料と過去の記録

全技術資料は[英語版の詳細一覧](README.md#detailed-reference-catalog)にあります。
モデルの合格、実機の観測、履歴の保存は異なる証拠です。
試験は版・backend・宣言した条件ごとに読み、過去の観測を新しい実行許可として扱いません。

Mineflayerの[過去の試験資料](evidence/legacy-mineflayer/README.md)は証拠として保持しています。
現在の起動手順は[導入ガイド](getting-started.ja.md)から確認してください。
ローカル検査は[開発ガイド](development.md)にあり、自動push/PR検査のCIはありません。

入口と英日対応の維持方針は[文書方針](documentation-policy.md)を参照してください。
