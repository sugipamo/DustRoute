# Server teleport後の位置補正調査

## 背景

自然生成地形の10 Bots耐久試験では、180秒ごとの`spreadplayers`後に多数の`PositionCorrection`が集中した。通常移動中は補正0であり、server teleport直後だけに発生していたため、条件を分離して比較した。

## 比較条件

すべてMinecraft Java 1.16.1、protocol 736、offline-mode、Peaceful、同じ固定seedのdefault world、10 Botsで実施した。

| 条件 | 距離 | Producer停止 | 補正数 | 合計補正距離 | 最大補正距離 |
| --- | --- | --- | ---: | ---: | ---: |
| controlのみ5秒前に解除 | 遠距離 | なし | 2,130 | 2,364.05 | 532.07 |
| controlのみ5秒前に解除 | 8 block以内 | なし | 602 | 152.61 | 57.04 |
| teleport受信後1秒barrier | 遠距離 | 受信後 | 109 | 2,237.85 | 452.51 |
| 5秒前から7秒停止＋受信後barrier | 遠距離 | 前後 | 31 | 4.01 | 1.67 |

各条件は4分間に1回teleportした結果である。最終条件ではraw metricも追加し、server position packetは41だった。内訳は意図した10 Bots分の10 packetと、追加応答31 packetに一致する。teleport後60秒間は41/31から増加しなかった。

過去の10分試験は3回のteleportで1,623補正だったが、raw position packet metric追加前のため、上表の直接比較には使用しない。

## 切り分け結果

`clear_control()`だけでは20 Hz physics loopが停止しない。入力が空でも重力・collision計算とmovement packet送信は続くため、補正量は減らなかった。

近距離の既読込地形へ限定すると補正は減ったため、chunk/地形同期も二次要因である。ただし602回残ったため主因ではない。

teleport受信後にmovement producerを1秒止めるだけで、同じ遠距離条件の補正は2,130回から109回へ約94.9%減少した。さらにserver予告を利用して5秒前から止めると31回へ約98.5%減少した。

以上から、主因はserver teleport前後に20 Hz producerが送るmovement packetと、authoritative position更新との競合である。遠方chunkの未同期は増幅要因と判断する。

## 実装した対策

- server position受信後、内部movement producerを20 ticks停止
- barrier中もteleport confirmとserver positionへの応答は継続
- `Bot::suspend_movement_for(Duration)`を追加
- server予告を把握できるcontrollerは、入力状態を失わず事前停止可能
- `PhysicsMetrics::server_position_packets`を追加し、補正判定とraw packet数を分離

通常のserverはteleportを予告しないため、受信後barrierは常にclient内部で適用する。専用serverや上位controllerがteleport予定を把握できる場合だけ、事前の`suspend_movement_for`を追加する。

## 評価

受信後barrierだけでもserver kick・切断を起こさず大幅に改善した。事前停止可能な管理serverでは、1 Botあたり平均3.1回の追加位置応答まで削減できた。

理想は意図したteleport 1 packet/Botのみであるため完全一致ではないが、救済teleport用途としては実用範囲と評価する。さらに削減する場合は、固定時間ではなく新位置周辺chunkと接地状態のready条件をbarrier解除条件にする。

## 証跡

- control停止・遠距離：repository版の`reports/natural-terrain-paused.md`
- control停止・近距離：repository版の`reports/natural-terrain-near-paused.md`
- 受信後barrier：repository版の`reports/natural-terrain-barrier.md`
- 事前停止・raw metric付き最終結果：repository版の`reports/natural-terrain-prewarn-final.md`
