# Minecraft 1.16.1 Physics Roadmap（完了済み）

この文書は、`Voxrig`のプレイヤー物理を「接続が切れない」状態から、通常プレイでサーバー補正がほぼ発生しない状態へ段階的に進める実装・検証計画です。JSONLは導入せず、複数Botを管理できるRust APIを維持します。

## 完了の定義

- Java 1.16.1 / protocol 736 / offline-modeの実サーバーで検証する。
- 各Phaseの自動テストと合格条件を満たしてから次へ進む。
- 最終的に1 Bot × 1時間、10 Bots × 30分、50 Bots × 10分の耐久試験を行う。
- 切断0、重大な位置補正0、physics tick p99 5 ms未満を目標とする。
- バニラ完全一致、厳格なアンチチート対応、vehicle、elytra、horseは今回の完了条件に含めない。

## Phase 0: 観測・計測基盤

実装項目：

- `PhysicsMetrics`: movement packet数、位置補正回数、補正距離合計・最大値、切断数、接続時間。
- 最後に送信した位置と送信時刻の記録。
- 初期spawn位置と、移動直後のサーバー位置補正の区別。
- `Event::PositionCorrection`。
- Bot単位およびManager単位のmetrics取得。
- 補正時に今後のvelocityをリセットできるフック。

合格条件：初期spawnを補正として数えず、意図的な不正移動に対する補正と距離を記録できる。

## Phase 1: ControlStateと20 Hz loop

- `ControlState { forward, back, left, right, jump, sprint, sneak }`。
- Botごとに重複しない20 Hz physics task。
- `set_control` / `clear_control`。低レベルの座標移動APIはデバッグ用途として残す。
- packet受信、他Bot、遅い利用側処理からphysics loopを分離する。

合格条件：入力を保持して継続移動し、解除後停止する。複数Botの状態が混線しない。

## Phase 2: 基本物理とAABB collision

- velocity、加速、重力、空気抵抗、ground friction、jump velocity。
- player AABBとX/Y/Z軸別collision解決。
- `on_ground`、fall distance、頭上・壁・足元collision。
- chunk未取得領域では安全に停止。

試験：平地100 block、壁へ10秒、低い天井、穴、platform端、連続100 jump。

合格条件：壁抜け・埋没・空中静止なし。100 jumpで切断0、サーバー補正原則0。

## Phase 3: Block Collision Shape

- 1.16.1 block state IDから0個以上のAABBを引く生成データ。
- slab、stairs、fence、wall、door、trapdoor、carpet、snow、chest、bed、cactus、soul sand、farmland、path。
- 接続状態や開閉状態を含む複数AABB shape。

合格条件：slab/stairsの正しい高さ、doorの開閉、fence越え防止、block update直後のshape更新。

## Phase 4: 通常プレイ向け特殊物理

優先順：

1. step-upとsneak edge protection。
2. sprint、sprint jump、開始・終了packet。
3. 水・溶岩のdrag、浮力、流れ、水面jump。
4. ladder、vine、scaffolding。
5. slime、honey、cobweb、soul sand、ice、bubble column、berry bush。

合格条件：各種類の固定テストコースで期待終了位置と状態を満たす。

## Phase 5: 参照実装との差分試験

- 同じ初期状態、block配置、attribute、tick入力を`prismarine-physics`とRust実装へ与えるfixture。
- tickごとのposition、velocity、on-ground、collision flags、fall distance、sprint stateを比較。
- 平地、停止、jump、sprint、落下、壁、slab、stairsを継続fixture化。

合格条件：対象scenarioが定めた浮動小数点許容差内で一致する。

## Phase 6: 実サーバー耐久試験

- 再現可能な1.16.1テストサーバーと周回コース。
- 1 Bot × 1時間、10 Bots × 30分、50 Bots × 10分。
- 切断、補正回数・距離、tick時間、packet数、メモリ、queue lag、サーバーTPSを収集。
- 異常終了後にも結果を保存する。

合格目標：切断0、重大補正0、微小補正1 Bot-hourあたり1回未満、physics tick p99 5 ms未満、10 Bots時TPS 19.5以上。

## 実装順チェックリスト

- [x] Phase 0: PositionCorrection / PhysicsMetrics
- [x] Phase 1: ControlState / 20 Hz loop
- [x] Phase 2: velocity / AABB / full-block collision
- [x] Phase 3: collision shape data
- [x] Phase 4A: step-up / sneak
- [x] Phase 4B: sprint
- [x] Phase 4C: liquids
- [x] Phase 4D: climbing
- [x] Phase 4E: special blocks
- [x] Phase 5: trajectory differential tests
- [x] Phase 6: endurance tests

## Phase 6 実測結果

Java 8 / Minecraft Java 1.16.1 / protocol 736 / `online-mode=false` / Peacefulの実サーバーで、release buildを使用しています。50 Bots試験は並行中の1 Botを含めるとサーバー負荷51接続です。

| 構成 | 規定時間 | movement packets | 補正 | 切断 | tick p99 | queue lag p99 | RSS | server TPS | 状態 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 1 Bot | 60分 | 72,001 | 0 | 0 | 198µs | 1.846ms | 12,688 KiB | 20.00 | 合格 |
| 10 Bots | 30分 | 360,061 | 0 | 0 | 193µs | 2.066ms | 36,400 KiB | 20.00 | 合格 |
| 50 Bots | 10分 | 601,310 | 0 | 0 | 94µs | 2.078ms | 128,836 KiB | 20.00 | 合格 |

詳細な原データはrepository版の`reports/`にMarkdown checkpointとして保存します。異常終了試験では10秒以内の直近checkpointがatomicに保持されることも確認済みです。

全構成で切断0、重大・微小補正0、physics tick p99 5ms未満を達成しました。10 Bots時のserver TPSも19.5以上という合格目標に対して20.00でした。
