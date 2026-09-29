# 状態・イベントの扱い

## Snapshotとevent

通常の状態getterは「現在値」をowned cloneとして返します。対応する`*_snapshot()`は同じowned値に領域別revision、最終更新時刻、取得時刻を付けます。`subscribe()`はその後の変更を通知するbroadcast receiverです。

推奨手順は次の通りです。

1. event receiverを作成する。
2. 必要なsnapshotを取得する。
3. eventを差分通知として処理する。
4. receiver lag、timeout、transaction reject後はsnapshotを再取得する。

event streamだけを永続的な唯一の状態源として扱わないでください。Tokio broadcastは遅いconsumerに対してlagを通知し、古いeventを保持し続けません。

## 主なsnapshot

- `Player`：identity、entity ID、座標、向き、接地、spawn
- `MotionState`：velocityとphysics状態
- `SurvivalState`：vitals、経験値、環境、effect、attribute、ability
- `InventoryState`：player/window slot、cursor、property、transaction
- `EntityState`：種類、座標、速度、metadata、equipment
- `BlockObservation`：絶対座標とblock state ID
- `PlayerList`：serverのplayer list
- `ChunkSnapshot`：Arc-backed section、biome、light、heightmap/block entity
- `MapStore`：map iconと128×128 color
- `UiState`：scoreboard、team、boss bar、title、tab、world border
- `RecipeBookState`、`AdvancementState`、`StatisticsState`
- `CommandTree`、`ServerRecipes`、`ServerTags`

revisionは同じ接続の同じ状態領域内だけで比較します。例えば`inventory_snapshot().revision`が前回と同じなら、上位層へのinventory再転送を省略できます。player revisionとinventory revision、異なるBot、再接続前後のrevisionは比較できません。

snapshotは内部lockを保持しません。Inventory、Entity一覧、block cubeはowned cloneまたは新規`Vec`ですが、`ChunkSnapshot`のsection・light・NBT bufferとmap colorは`Arc`共有されます。詳細は[API契約と所有権](api-contracts.md)を参照してください。

## Eventの分類

- lifecycle：login、spawn、disconnect、error
- world：chunk/light/block/block entity、爆発、map、particle、world event
- player：position、server correction、生存状態、respawn
- inventory：slot、held item、pickup、window、transaction
- interaction：dig acknowledgement、break progress
- entity/combat：spawn、update、destroy、animation、combat
- communication：chat、player list
- sound：named/raw sound event
- UI/progress：scoreboard、team、boss bar、title、tab、advancement、recipe、statistics
- protocol補助：resource pack、completion、command tree、tags、NBT query、camera、attach

## Raw情報

- Chatはraw JSON componentを保持します。
- ItemStackはoptional NBT payloadを保持します。
- Entity metadataはprotocol上の型を保ちます。
- 未知のsoundもraw IDを保持して通知します。

上位層は必要に応じて履歴、filter、距離、方向、危険度、自然言語表現へ変換できます。
