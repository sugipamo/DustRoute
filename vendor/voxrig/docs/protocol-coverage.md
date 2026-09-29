# Protocol 736 coverage

対象はMinecraft Java Edition 1.16.1 play stateです。`minecraft-data`のprotocol 736定義と`Bot::read_loop`をpacket ID単位で照合し、clientbound `0x00..=0x5b`の全92 IDに明示的な分岐があります。

## 永続状態

- player、motion、survival、inventory/window、entity、player list
- chunk/block、biome、light、heightmap、block entity、map
- scoreboard、team、boss bar、title、tab list、world border、world view
- command tree、recipes、tags、recipe book、advancement、statistics
- passenger/attach、camera、resource pack request、server brand

## 一過性event

- sound/stop sound、particle、world/block action、explosion
- chat、combat、animation、pickup、break progress、dig acknowledgement
- completion、recipe response、NBT query response、editor open通知

Keep Alive、Teleport Confirm、compression、transaction acknowledgementなど、応答が必須のpacketは内部で処理します。未知のpacket IDは将来の壊れたversion混入を許容するため最終fallbackで無視しますが、protocol 736の既知IDがfallbackへ入ることはありません。

「分岐があること」は機能の実server検証を意味しません。parserはunit testとfixtureで検証し、packet往復・server acceptance・耐久性は実サーバーprobeで別に確認します。
