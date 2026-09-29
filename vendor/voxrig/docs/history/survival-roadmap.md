# Minecraft 1.16.1 Offline Survival Bot Roadmap（完了済み）

## 目的

`Voxrig`を、Minecraft Java Edition 1.16.1（protocol 736、`online-mode=false`）のサバイバルを一通りプレイできるRust製headless Botへ拡張する。

AIはJSONLや別プロセスの標準入出力を介さず、Rustクレートの型付きAPIを直接呼び出す。1プロセスで複数Botを管理し、各Botのusernameは`Player`構造体で指定する。

ここでいう「offline」はMicrosoft/Mojang認証を使わないoffline-modeサーバーを意味する。Bot自体はMinecraftサーバーへTCP接続する。

## 完了の定義

Botが外部からアイテムを与えられない状態で、次のサバイバル行動を連続して実行できることを完了条件とする。

1. 周囲を観測して移動する。
2. 木を探して破壊し、ドロップを拾う。
3. inventory内のアイテムを認識する。
4. 原木から板材、棒、crafting table、木の道具をクラフトする。
5. crafting tableを設置し、windowを操作する。
6. 石を採掘して石の道具を作る。
7. 食料を取得し、空腹時に食べる。
8. hostile mobを認識して回避または攻撃する。
9. damage、health、food、death、respawnを処理する。
10. furnaceで燃料と素材を扱い、精錬結果を回収する。
11. 上記を複数Botで実行しても状態が混線しない。

高レベルな探索、経路探索、クラフト計画、戦闘戦略はAI側の責務とする。クレートは、判断に必要な状態と最低限の操作プリミティブを提供する。

## データ変換の方針

クレートはprotocol packetを安全なRust型へdecodeし、同じ対象に対する更新を現在状態へ反映するところまでを担当する。AIが元データから判断できる意味分類、要約、filter、履歴生成、画像・音声へのrenderは原則として行わない。

クレート側で行う変換：

- VarInt、NBT、chunk palette、chat componentなどwire形式のdecode
- protocol IDから1.16.1公式registryの名前への可逆な対応付け
- relative moveや差分slot更新を現在状態へ適用
- packet順序、window transaction、teleportなどprotocol整合性の管理
- 移動packetを正しく送るために不可欠なプレイヤー物理
- digging完了packetの時機を決めるために不可欠な破壊時間計算

AI側で行う処理：

- mobやsoundが危険かどうかの意味判断
- 距離、方向、優先度など、座標から容易に導出できる値の計算
- event履歴、検索、filter、集約
- recipeの選択、材料代替の選択、クラフト計画
- 経路探索、戦闘判断、行動計画
- 画像、音声波形、自然言語への変換

各型は可能な限りraw protocol値も保持し、registry名の付与や状態への反映によって元情報を失わない設計にする。

## 実装済みの基盤

- [x] offline-modeログイン
- [x] packet圧縮、Keep Alive、teleport confirm
- [x] `Player`由来のusername
- [x] `BotManager`による1プロセス複数Bot
- [x] 前後左右移動、視点、ジャンプ、sprint、sneak
- [x] 20 Hz物理、AABB衝突、block collision shape
- [x] 水・溶岩・登攀・主要特殊ブロック物理
- [x] chunk、block state、block更新の取得
- [x] 周辺block観測API
- [x] sound eventのID、位置、volume、pitch取得
- [x] 位置補正、切断、tick時間、queue lagの計測
- [x] 1、10、50 Botsの実サーバー耐久試験

## 必須実装

### Phase 7: 生存状態と基本サーバー状態

- health、food、food saturationの受信と状態保持
- experience level、experience progress、total experience
- game mode、difficulty、world time、weather
- damage、hurt、death、respawnのイベント
- status effectの追加・更新・削除
- movement speedなどプレイヤーattributeの反映
- spawn positionとdimension／respawn時のworldリセット
- health、status、entity event、death messageなど、AIがdamage原因を判断するためのraw情報

合格条件：damage、回復、空腹、effect、死亡、respawnを型付き状態とイベントから判定できる。

### Phase 8: Inventoryと手持ち

- player inventory、hotbar、armor、offhandのslot状態
- `ItemStack`：item ID、count、damage、NBT
- Set Slot、Window Items、held item changeの受信
- hotbar slot選択の送信
- item drop（1個／stack）
- item pickupとinventory反映の確認
- armorとshieldの装備
- server authoritativeなslot更新との照合
- Botごとに独立したinventory state

最低限のAPI例：

```rust,ignore
bot.inventory().await
bot.select_hotbar(slot).await?
bot.drop_selected(false).await?
```

合格条件：拾得、消費、耐久値変化、stack増減、装備変更がサーバー状態と一致する。

### Phase 9: ブロック破壊・設置・道具使用

- digging開始、cancel、finish
- block break animationとblock state更新の追跡
- 選択道具、hardness、harvest条件に基づく破壊時間
- 左クリック／腕振り
- block placement
- block face、cursor位置、inside-block情報
- main hand／offhandでのuse item
- 長押し、release use item
- 食事、bucket、flint and steel、bow、shieldなどの共通操作
- interaction失敗、距離超過、サーバー拒否の検出

合格条件：木、土、石、鉱石を適切な時間と道具で破壊し、blockを向きを含めて設置できる。

### Phase 10: Windowとtransaction

- Open Window、Close Window
- Window Items、Set Slot、Window Property
- cursor item
- click mode：通常、right click、shift click、number key、drop、drag、double click
- client action番号とserver transaction確認
- reject時のrollback／resync
- window typeとslot配置
- player inventoryと開いたwindow間の一貫性
- chest、barrel、furnace、crafting tableの最低限のslot mapping

最低限のAPI例：

```rust,ignore
let window = bot.open_window_state().await;
bot.click_slot(window.id, slot, MouseButton::Left, ClickMode::Normal).await?;
bot.close_window().await?;
```

合格条件：遅延やtransaction rejectがあってもアイテム複製、消失、cursorずれを起こさない。

### Phase 11: Craftingとrecipe

- player inventoryの2×2 crafting
- crafting tableの3×3 crafting
- shaped／shapeless recipe
- recipe IDとshaped／shapelessの構造化されたraw recipe定義
- recipe book packetは操作に必要な最小範囲のみ
- 材料配置、結果slotの回収、複数回craft
- container itemと余剰材料
- plank、stick、crafting table、各tierの基本道具
- furnace recipe、fuel、進捗、結果回収

合格条件：原木からcrafting tableと木のpickaxeを作り、丸石から石のpickaxeとfurnaceを作って精錬できる。

### Phase 12: Entity観測

- player、mob、animal、item、projectileのspawn／destroy
- entity ID、type、UUID、位置、向き、velocity
- relative move、teleport、head rotation
- metadata：pose、状態、名前など必要な項目
- equipment
- item entityのItemStack
- entity attributeとstatus effectの必要部分
- entity trackerのchunk／距離による整理
- `observe_entities(radius)`とentity更新イベント

合格条件：近くのitemを拾い、動く動物とhostile mobの現在位置を継続追跡できる。

### Phase 13: Entity interactionと戦闘

- entityへのinteract、interact-at、attack
- attack時の腕振り
- attack cooldown
- weapon、armor、shieldの状態
- knockbackとserver velocityの物理反映
- entity animation、hurt、deathの観測
- projectileの最低限の追跡
- 食料取得に必要な動物への攻撃
- hostile mobへの攻撃とshield使用

合格条件：動く対象へ接近して攻撃し、dropを回収できる。server velocityを受けても位置補正を連発しない。

### Phase 14: 通常サバイバル用container

- chest／barrelへの格納と回収
- furnaceのinput、fuel、output、進捗
- smoker／blast furnaceはfurnace実装を共用
- crafting table
- bedへのinteractionと睡眠結果
- door、button、leverなどへのblock interaction

合格条件：採取物をchestへ格納し、furnaceで食料または鉱石を精錬し、結果を回収できる。

### Phase 15: Chatと最低限の協調

- server／system chatの受信
- offline-mode 1.16.1のchat送信
- command送信
- player listの最低限の追跡
- Bot間の状態を混ぜずに発信元を識別

合格条件：人間プレイヤーと短いメッセージを交換し、許可されたテスト用commandを実行できる。

### Phase 16: AI向けsound event構造化

Minecraft protocolから受け取れるsound eventを、追加の意味推論や再生を行わず、AIが認識できる型付きデータとして提供する。音声波形の生成、OGG decode、スピーカー出力は行わない。

- sound IDを1.16.1のsound nameへ解決
- sound name、category、発生座標またはentity ID、volume、pitchを保持
- event順序を識別できる受信timestamp／sequenceを付与
- 未知のsound IDもraw ID付きで欠落させず通知

最低限のデータ例：

```rust,ignore
SoundObservation {
    raw_id: Some(437),
    name: Some("minecraft:entity.zombie.ambient"),
    category: SoundCategory::Hostile,
    source: SoundSource::Position { x, y, z },
    volume: 1.0,
    pitch: 1.0,
    sequence: 42,
}
```

距離、相対方向、足音・mob・combatなどの意味分類、履歴、filterは、座標・名前・category・時刻を使ってAI側で必要なときに計算する。

合格条件：AIが波形を再生・解析しなくても、受信したsound eventの公式名、category、source、volume、pitch、順序をBotごとに欠落なく取得できる。

## 必須ではないが通常プレイの品質を上げる項目

- biome、light、heightmap
- block entity NBT（看板、container補助情報など）
- explosionとpistonによる移動・block更新
- potion、bow、crossbow、fishing rodの個別挙動
- enchantment、anvil、brewing、villager trade
- map itemのraw pixel／markerデータ（画像renderはAI側）
- scoreboard、boss bar、advancement、statistics
- boat、minecart、horseなどの乗り物
- swimming pose、crawling
- Nether portalとdimension移動

これらは基本的な木材採取から精錬までが安定した後に追加する。

## 対象外

offline-modeの基本サバイバルに不要なため、今回の完了条件には含めない。

- Microsoft／Mojang認証
- online-mode暗号化とsession server認証
- secure chat
- proxy、server transfer
- Forge／Fabric plugin handshake
- protocol version自動判定
- 1.16.1以外のprotocol
- spectator
- creative専用操作
- elytra
- 完全なvehicle物理
- resource packの自動配布UI
- recipe book UIの完全再現
- title、subtitle、action barの描画
- client GUIや映像レンダリング
- 厳格なアンチチートとの完全一致保証

## 実装順チェックリスト

- [x] Phase 7: 生存状態と基本サーバー状態
- [x] Phase 8: Inventoryと手持ち
- [x] Phase 9: ブロック破壊・設置・道具使用
- [x] Phase 10: Windowとtransaction
- [x] Phase 11: Craftingとrecipe
- [x] Phase 12: Entity観測
- [x] Phase 13: Entity interactionと戦闘
- [x] Phase 14: 通常サバイバル用container
- [x] Phase 15: Chatと最低限の協調
- [x] Phase 16: AI向けsound event構造化（再生・意味推論なし）

## 検証方針

各Phaseでunit test、packet fixture、実1.16.1 offlineサーバー試験を行う。サーバー状態を正とし、クライアントの予測と不一致が起きた場合はresyncできる設計にする。

最終試験では人間プレイヤーも同じテストサーバーへ参加し、Botと共同で次を実行する。

1. Botが木材を採取する。
2. 基本道具をクラフトする。
3. 石と食料を取得する。
4. furnaceで精錬する。
5. hostile mobとの遭遇を処理する。
6. chestを介して人間プレイヤーとアイテムを受け渡す。
7. chatと構造化sound認識データを記録する。
8. 複数Botで長時間継続し、切断、transaction不一致、inventory消失、位置補正を計測する。

最終合格条件：通常サバイバルの一連の操作を人手による内部状態修正なしで完走し、アイテム複製・消失、window deadlock、Bot間の状態混線、予期しない切断が発生しない。

## 完了結果

Phase 7〜16はすべて実装・検証済み。2 Botのchat・chest共同操作と、10 Bots × 10分の耐久試験を完走した。耐久結果はmovement packets 120,065、位置補正0、切断0、physics tick p99 129 us、server TPS 20.00だった。

詳細は[サバイバル完了報告](survival-completion-report.md)とrepository版の`reports/survival-10bots-10min.md`を参照する。
