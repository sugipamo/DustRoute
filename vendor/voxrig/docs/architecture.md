# 設計と責任境界

## 基本方針

このクレートは外部controllerに対するheadless client、すなわち「身体」です。AI framework、planner、world modelそのものにはしません。

```text
AI / Planner / Behavior Tree
           │ goals・判断・target
           ▼
   外部controller層
           │ typed Rust API
           ▼
      Voxrig
           │ selected protocol 736 / 774
           ▼
 Minecraft 1.16.1 / 1.21.11 server
```

## クレートが担当すること

- protocolのencode/decode
- 接続、圧縮、Keep Alive、Teleport Confirm
- packet順序とserver authoritative stateの同期
- player、world、entity、inventory、survival状態のsnapshot
- raw情報を失わないRust型への変換
- client movement physicsとcollision
- window transactionとreject時のrollback
- 低レベルな操作packet
- Botごとの状態分離と複数Bot管理

## 外部controllerが担当すること

- goalとtask decomposition
- 経路探索と到達可能性
- block、entity、soundの意味解釈
- target、道具、recipeの選択
- resourceとcraftingの計画
- 戦闘戦術
- retryの判断、ジョブの失敗回復、長期記憶
- 複数Botの役割分担と排他制御

## データ変換の基準

wire formatを安全で扱いやすいRust型へdecodeし、差分packetを現在状態へ反映するところまではclientの責務です。そこから「危険」「食料」「目的地」といった意味を付与するのは外部controllerの責務です。

registry名はraw IDと併存させます。NBT、raw JSON chat、metadataなど、上位層が後から解釈できる情報を可能な限り保持します。

## 状態と並行性

1つの`Bot`にはplayer、world、survival、inventory、entity、physics状態があります。cloneした`Bot` handleは同じ状態を共有します。getterはsnapshotを返し、network受信loopや20 Hz physics loopを利用側の処理から分離します。

`BotManager`は複数の`Bot`を所有しますが、各Botの状態や入力は共有しません。集約イベントだけが`BotEvent`として共通streamへ流れます。

## 検査付きサバイバルの境界

[共通API](survival-api.md) は版選択と単発操作・接続ライフサイクルをまとめます。
ネイティブな位置観測、衝突、照準、操作履歴、採掘セッションの退役と再接続検証はVoxrigの責務です。
経路探索、仮設の配置、設計図、資材予約、永続ジョブ、復旧後の再計画は利用側に残します。
再接続は明示呼出しで一度だけ試み、利用側の計画を移植したり自動再試行したりしません。
