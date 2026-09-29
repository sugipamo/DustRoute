# Minecraft Rust Bot 物理実装 完了報告

## 対象と結論

Minecraft Java Edition 1.16.1（protocol 736、offline-mode）を対象に、AIからRustクレートを直接呼び出して操作できるヘッドレスBot基盤を実装しました。1プロセスで複数Botを管理し、各Botのユーザー名は`Player`構造体で指定します。JSONLや標準入出力を介した操作プロトコルは使用しません。

物理ロードマップのPhase 0〜6はすべて完了し、実サーバー上の最終耐久試験も合格しました。

## 実装済み範囲

- `Player::offline(username)`によるBot identity
- `BotManager`による1プロセス・複数Botの接続、取得、計測、切断
- Botごとの独立した20 Hz physics loopと`ControlState`
- 移動、視点、ジャンプ、sprint、sneak
- AABB衝突、段差、ブロックcollision shape、未取得chunkでの安全停止
- 水・溶岩・流れ、ladder・vine・scaffolding、特殊ブロック物理
- chunk・block stateを使った周辺マップ観測
- movement packet、位置補正、切断、tick時間、queue lagの計測
- 異常終了時にも残るatomicな耐久試験checkpoint

この報告書作成時点ではクラフト、window操作、道具使用は物理ロードマップの対象外でしたが、その後[サバイバルロードマップ](survival-roadmap.md)のPhase 7〜16として実装・検証済みです。現在の結果は[サバイバル完了報告](survival-completion-report.md)を参照してください。

## 検証結果

| 構成 | 時間 | movement packets | 補正 | 切断 | tick p99 | queue lag p99 | RSS | server TPS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 Bot | 60分 | 72,001 | 0 | 0 | 198 us | 1.846 ms | 12,688 KiB | 20.00 |
| 10 Bots | 30分 | 360,061 | 0 | 0 | 193 us | 2.066 ms | 36,400 KiB | 20.00 |
| 50 Bots | 10分 | 601,310 | 0 | 0 | 94 us | 2.078 ms | 128,836 KiB | 20.00 |

すべての構成で切断0、位置補正0、physics tick p99 5 ms未満を達成しました。10 Bots時のserver TPSも合格基準19.5以上に対して20.00です。

## 最終監査

- `cargo fmt --all -- --check`: 合格
- `cargo test`: 23件合格、失敗0
- `cargo clippy --all-targets -- -D warnings`: 合格
- `scripts/run_endurance.sh`: shell構文検査合格
- [物理ロードマップ](physics-roadmap.md): 未完了チェック項目0
- `src/`、`examples/`、`scripts/`、`test-server/`: JSONL実装0
- prismarine-physics fixture: 生成スクリプト、依存lockfile、生成済みデータを保存

fixtureの再生成にはNode.jsが必要です。監査時の実行環境にはNode.js本体がなかったため、その場での再生成比較のみ未実行です。生成済みfixtureを使うRust側の差分試験は合格しています。

## 証跡

- 計画と各Phaseの合格条件: [物理ロードマップ](physics-roadmap.md)
- 1 Bot耐久結果: repository版の`reports/1bot-1hour.md`
- 10 Bots耐久結果: repository版の`reports/10bots-30min.md`
- 50 Bots耐久結果: repository版の`reports/50bots-10min.md`
- 中断時checkpoint: repository版の`reports/interrupted-checkpoint.md`
- 再現用サーバー設定: repository版の`test-server/README.md`
- 耐久試験ランナー: `examples/endurance.rs`、`scripts/run_endurance.sh`
