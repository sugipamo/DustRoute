# 開発とテスト

## 通常の品質確認

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo test --doc
cargo clippy --all-targets -- -D warnings
```

配布packageの内容と、展開後にcompileできることは次で確認します。

```bash
cargo package --allow-dirty
```

## Testの種類

- Unit test：protocol値、registry、NBT、metadata、transaction、physics
- Fixture test：`prismarine-physics`から生成したtrajectoryとの比較
- Probe example：実1.16.1 serverで個別機能を検証
- Endurance test：複数Botの補正、切断、latency、memory、TPSを計測

## 実サーバー

サーバー構築と耐久コースの手順はrepository版の`test-server/README.md`を参照してください。offline-mode serverは信頼できるローカル環境に限定し、Internetへ公開しないでください。

個別probeは`examples/`にあります。接続先、username、対象座標など、各exampleが参照する環境変数や引数を確認して実行してください。

```bash
cargo run --release --example multi_bot
cargo run --release --example interaction_probe
cargo run --release --example cooperation_probe
```

自然生成地形では、serverから3分ごとに安全な地表へ分散teleportする専用scenarioを利用できます。

```bash
bash scripts/run_natural_terrain.sh
```

このscenarioでは`WANDER=true`により簡易入力generatorを使用します。固定方向へ押し続ける通常の耐久入力と違い、周期的な方向転換、0.5秒周期の長いjump pulse、停止検出後の旋回を行います。経路探索の正しさを検証するものではありません。

設定とteleportの扱いはrepository版の`test-server/README.md`を参照してください。
実測結果はrepository版の`reports/natural-terrain.md`に保存しています。
teleport直後の補正burstに関する条件比較は[位置補正調査](teleport-investigation.md)を参照してください。

耐久試験のcheckpointは`reports/`に保存されています。過去の合格値は[物理完了報告](history/physics-completion-report.md)と[サバイバル完了報告](history/survival-completion-report.md)を参照してください。

API拡張後の初期同期と共有chunk storageは、server起動後に次で再検証できます。

```bash
MC_PORT=25566 cargo run --release --example api_surface_probe
```

直近の実行結果は[API拡張ロードマップ](headless-api-roadmap.md)に記録しています。

## Registryとfixture

`data/`には1.16.1専用のregistry、collision shape、recipe、physics fixtureがあります。protocol versionを変更する場合は、コードだけでなく生成データとfixtureを一体として更新してください。

`reference/`はfixture生成の参照実装です。生成物を更新した場合はRust側の差分testと実サーバーprobeを両方実行します。
