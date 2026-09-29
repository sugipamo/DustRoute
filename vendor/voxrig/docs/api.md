# 公開API

この文書は、外部controllerが利用する公開面を用途別に示します。正確な引数型と戻り値は`cargo doc --open`で生成されるrustdocを正とします。

## Import

基本操作ではpreludeを利用できます。

```rust
use voxrig::prelude::*;
```

規模の大きな利用側では、用途別moduleから明示的にimportできます。

```rust
use voxrig::{
    client::{Bot, Event},
    entity::EntityState,
    inventory::InventoryState,
    survival::SurvivalState,
};
```

crate rootのre-exportと用途別moduleは同一の型を参照します。

## Module一覧

| Module | 主な責務 |
| --- | --- |
| `client` | `Bot`、接続先、プレイヤー、イベント、sound event |
| `manager` | 1プロセス内の複数Bot管理と集約イベント |
| `physics` | 入力、座標、motion、collision、計測値 |
| `world` | chunk cacheとblock観測 |
| `registry` | 1.16.1のblock、item、entity、sound、recipe名解決 |
| `survival` | 体力、空腹、経験値、effect、attribute、時間、天候 |
| `inventory` | ItemStack、player inventory、window、transaction |
| `interaction` | block座標、面、手、digging状態 |
| `entity` | entity snapshot、metadata、equipment |
| `chat` | raw JSON chatとplayer list |
| `map` | map item icon、部分更新、Arc-backed color data |
| `ui` | scoreboard、team、boss bar、title、tab、world border |
| `progress` | recipe book、advancement、statistics |
| `server_registry` | command tree、server recipes、block/item/fluid/entity tags |

`prelude`は、`Bot`、`BotManager`、`Server`、`Player`、`Event`、`ControlState`、block interactionに必要な基本型だけをexportします。

## 接続と複数Bot

### `BotManager`

- `new(Server)`：接続先に対応するmanagerを作成
- `with_options(Server, ConnectionOptions)`：timeoutに加え、chunk・entity・map・総cache record、CustomPayload、event queueの上限を設定
- `connect(Player)`：新しいBotを接続
- `get(username)`：usernameからBot handleを取得
- `usernames()`：管理中のusername一覧
- `physics_metrics()`：Botごとの物理計測値
- `chunk_storage_stats()`：Bot間で共有中のchunk section buffer数
- `subscribe()`：`BotEvent { username, event }`を購読
- `disconnect(username)`、`disconnect_all()`：切断

同じ`BotManager`から接続したBotは、内容が同一のchunk sectionを自動的に`Arc`共有します。更新はcopy-on-writeです。複数manager間でも共有する場合は`with_chunk_storage(server, Arc<SharedChunkStorage>)`を利用します。

### `Bot`

- `wait_until_ready()`：Join Gameと初期位置を待機
- `username()`、`player()`：identityと位置状態
- `client_settings()`、`set_client_settings(...)`：locale、view distance、chat、skin、利き手、brand
- `server_brand()`：serverの`minecraft:brand` custom payload
- `resource_pack_request()`、`respond_resource_pack(status)`：resource pack要求とstatus応答
- `tab_complete(text, timeout)`：transaction同期済みのcommand/chat補完
- `subscribe()`：そのBotだけのイベントを購読
- `disconnect()`：切断

`Bot`はclone可能な共有handleです。状態getterはsnapshotを返すため、利用側が内部lockを保持することはありません。

## 状態の取得

| API | 内容 |
| --- | --- |
| `player()` | entity ID、座標、視点、接地、spawn状態 |
| `motion()` | 速度と移動状態 |
| `environment_state()` | fluid、眼の水没、climbable、特殊接触、足元block |
| `survival_state()` | health、food、経験値、時間、天候、effect、attribute、dimension |
| `inventory()` | player/window slot、cursor、property、pending transaction |
| `open_window_state()` | 現在開いているwindow |
| `block(x, y, z)` | 読み込み済み座標のblock state |
| `observe(radius)` | プレイヤー周囲のblock cube |
| `loaded_chunks()`、`is_chunk_loaded(pos)` | local cacheにあるchunk一覧・存在確認 |
| `chunk_snapshot(pos)` | `Arc` section、biome、sky/block light、heightmap、初期/live block entity NBT |
| `wait_for_chunk(pos, timeout)` | 指定chunkの受信待機 |
| `wait_for_chunks(center, radius, timeout)` | 正方形範囲の全chunk受信待機 |
| `query_blocks(region, state_ids, limit)` | 意味判断を含まないstate ID範囲検索 |
| `raycast_blocks(direction, distance)` | 目の位置から実collision shapeへraycast |
| `targeted_block(distance)` | 現在のyaw/pitchが指す最初のblock |
| `raycast_entities(direction, distance)` | entity固有bounding boxへのraycast |
| `targeted_entity(distance)` | block遮蔽を考慮したcrosshair上のentity |
| `can_reach_*`、`can_see_*` | block/entityの到達距離・遮蔽判定 |
| `digging_info(position)` | reach、視線、採掘可否、tool適性、予測tick |
| `placement_info(position)` | 対象load、reach、視線、playerとのcell交差 |
| `entity(id)` | entity IDに対応するsnapshot |
| `entities()` | 現在track中の全entity |
| `observe_entities(radius)` | 指定半径内のtracked entity |
| `player_list()` | server player list |
| `map(id)`、`maps_snapshot()` | map itemのiconと128×128 color data |
| `ui_state()`、`ui_snapshot()` | scoreboard、team、boss bar、title、tab、world border |
| `recipe_book_snapshot()` | serverが通知したrecipe book設定とunlock ID |
| `statistics_snapshot()` | protocol category/statistic IDごとの統計値 |
| `advancements_snapshot()` | advancement定義、表示、criteria、requirements、進捗 |
| `world_view_snapshot()` | server指定のchunk centerとview distance |
| `camera_entity_id()` | spectator camera対象entity |
| `command_tree_snapshot()`、`tags_snapshot()` | Brigadier command treeとserver registry tags |
| `server_recipes_snapshot()` | server宣言recipeの材料候補、結果、調理情報 |
| `physics_metrics()` | movement/server position packet、補正、tick、queue lag、切断の計測 |

大きな状態を別層へ転送する場合は、`player_snapshot()`、`survival_snapshot()`、`inventory_snapshot()`、`observe_snapshot()`などのrevision付きAPIを利用できます。revisionが変わっていない領域は再転送する必要がありません。所有権と比較規則は[API契約と所有権](api-contracts.md)を参照してください。

`wait_for_block_revision()`、`wait_for_block_state()`、`wait_for_inventory_revision()`、`wait_for_entities_revision()`は、event購読開始との競合やbroadcast lagをsnapshot再確認で吸収する待機primitiveです。
特定cellの値が変わることを待つ場合は`wait_for_block_change()`を使います。

`block()`の`None`は「空気」ではなく、その座標がlocal chunk cacheで利用できないことを表します。

## 操作

### 移動

- `look(yaw, pitch)`
- `control()`、`set_control(ControlState)`、`clear_control()`
- `suspend_movement_for(duration)`
- `jump()`
- `vehicle()`、`set_vehicle_control(...)`、`steer_boat(...)`、`dismount()`

`ControlState`はBotごとに保持され、内部の20 Hz物理loopから反映されます。
`suspend_movement_for()`は入力状態を変えずにmovement producerだけを一定時間停止します。server teleportを事前に把握できる外部controller向けの同期primitiveです。

client物理を通さない`move_relative()`は`bot.unstable()`配下です。通常のcontrollerでは使用しません。

乗り物APIはpassenger状態とserverbound入力を公開します。現段階ではボート等のclient-authoritativeな乗り物物理そのものはまだ実装途中であり、`steer_boat()`だけで完全な移動を保証しません。

serverからのvehicle poseは`vehicle_pose()`と`VehiclePosition` eventで取得できます。明示的なclient-authoritative pose送信は誤用時に補正・切断を招くため、`bot.unstable().send_vehicle_pose(...)`に限定しています。

### Inventoryとwindow

- `select_hotbar(slot)`
- `drop_selected(entire_stack)`、`drop_selected_count(count)`
- `move_player_item(source, destination)`、`equip_from_player_slot(source, EquipmentSlot)`
- `edit_book(book, signing, hand)`
- `click_slot(...)`
- `click_slot_and_wait(...)`
- `close_window()`
- `merchant_offers()`、`select_trade(index)`
- `select_enchantment(option)`、`rename_item(name)`、`set_beacon_effects(...)`
- `update_sign(position, lines)`、`swap_hands()`

`click_slot()`は送信したtransaction番号を返します。同じwindowでは未確認transactionを一つだけ許可し、採番・予測・送信を直列化します。承認または拒否まで待つ場合は`click_slot_and_wait()`を使います。

### Blockとitem

- `raycast_blocks(...)`、`targeted_block(...)`
- `digging_info(position)`、`placement_info(position)`
- `dig_block(...)`
- `place_block(...)`、`place_block_and_wait_for_change(...)`
- `use_item(hand)`、`use_item_for(hand, duration)`、`release_item()`
- `swing_arm(hand)`

対象座標、block face、hand、cursor位置は利用側が決定します。到達可能性、視線、採掘時間などのclient既知情報は事前判定APIで取得できます。この判定はserver permissionやplugin規則による拒否を予測しません。

`registry::mining_info(state_id, tool_id)`はplayer位置に依存しない採掘可否、harvest可否、tool効率、予測tickを返します。道具の自動選択は行いません。

acknowledgementを待たないraw `send_digging(...)`は`bot.unstable()`配下です。

### Crafting

- `recipes_for_output(item_id)`
- `craft_once(...)`
- `place_recipe(window_id, recipe_id, make_all)`
- `take_crafting_result(window_id, grid_slots)`

recipeの表現と実行を提供しますが、材料の再帰計算やresource gatheringは行いません。

### Entityと戦闘

- `interact_entity(...)`
- `interact_entity_at(...)`
- `attack(entity_id)`

target、装備、接近、照準、戦術は利用側が決定します。

### Communicationと生存

- `send_chat(message)`、`send_command(command)`
- `respawn()`

## Eventとsnapshot

`Event`は、接続、位置、chunk/block、生存状態、inventory/window、entity、combat、chat、sound、切断、protocol error、server位置補正を通知します。

補助eventとしてresource pack要求、tab completion、world event、particle、UI状態更新も構造化して通知します。未知particle固有payloadは`WorldParticleEvent::raw_data`にも保持します。

`query_block_nbt()`と`query_entity_nbt()`はtransaction IDを割り当て、対応するserver応答まで待機します。block action、recipe response、camera、attach/leash、stop soundも構造化eventとして公開します。

爆発は`Explosion` eventとして通知し、affected blockをworld cacheから除去し、player knockbackをmotionへ反映します。

イベントは変更通知、getterの戻り値は現在状態のsnapshotです。broadcast receiverが遅延した場合はイベントだけから状態を復元せず、対応するsnapshotを再取得してください。

Sound eventはID、公式名、category、座標またはentity ID、volume、pitch、sequence、受信時刻を保持します。Minecraftのsound packetにPCM音声やプレイヤーのマイク音声は含まれません。

## Errorとcancel

fallibleな公開操作は`voxrig::Result<T>`を返します。`Error::kind()`は、`InvalidInput`、`Connection`、`Timeout`、`Disconnected`、`Protocol`、`ResourceLimit`、`Rejected`、`State`、`Other`を安定した分類として返します。表示文字列は診断用であり、制御フローには使用しないでください。

```rust,no_run
use voxrig::{ErrorKind, Result};

# async fn run(bot: &voxrig::Bot) -> Result<()> {
if let Err(error) = bot.wait_until_ready().await {
    match error.kind() {
        ErrorKind::Timeout | ErrorKind::Connection => {
            // 外部controller側の再接続・backoff方針へ渡す
        }
        ErrorKind::InvalidInput => return Err(error),
        _ => return Err(error),
    }
}
# Ok(())
# }
```

`Error::diagnostic()`では内部のerror chainを参照できますが、その具体的な型や文言は互換性保証の対象外です。

Futureをdropすると利用側の待機はcancelされますが、すでにserverへ送信済みのpacketが取り消されるとは限りません。timeout、receiver lag、transaction拒否の後はsnapshotを再取得してください。

`Ok`がlocal受付、packet dispatch、server acknowledgementのどこまでを保証するかは操作ごとに異なります。保証一覧は[API契約と所有権](api-contracts.md)を参照してください。

## Compatibility情報

- `Bot::client_info()`：crate version、protocol、capabilities
- `Bot::protocol_info()`：Minecraft 1.16.1 / protocol 736
- `Bot::capabilities()`：offline mode、world、inventory、soundなどの対応境界
- `bot.server_info()`：実際に接続しているhostとport

利用側は起動時にこれらを検査し、要求するprotocolや能力と一致しない場合は早期に停止できます。
