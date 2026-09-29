# API契約と所有権

この文書は、外部controllerが`Voxrig`の戻り値と完了をどう解釈するかを定義します。関数シグネチャの正本はrustdoc、用途別一覧は[公開API](api.md)です。

## Stableとunstable

crate root、`prelude`、`Bot`、`BotManager`から直接利用するAPIを通常の公開面とします。`unstable` moduleは、protocolに近く誤用時にserver補正や切断を招く操作です。`0.x` release間で互換性を保証しません。

現在のunstable操作は、client物理を通さない直接相対移動、acknowledgementを待たないraw digging action、client-authoritativeなvehicle pose送信です。

```rust
bot.unstable().move_relative(0.1, 0.0).await?;
```

通常移動には`set_control()`、block破壊には`dig_block()`を使用してください。

## 操作完了の段階

`Result::Ok`は操作ごとに次のいずれかを意味します。

| 段階 | 意味 | 代表API |
| --- | --- | --- |
| local | client内へ要求を保存 | `set_control`、`jump` |
| dispatched | packetのwrite成功 | `look`、`place_block`、`use_item`、`attack`、chat |
| acknowledged | 対応するserver応答を確認 | `dig_block`、`click_slot_and_wait` |
| observed | 状態snapshotで結果を確認 | 利用側が`block`、`inventory`、eventで確認 |

`raycast_blocks()`、`digging_info()`、`placement_info()`はpacketを送信しない事前観測です。結果は呼出時のlocal world cacheとplayer状態に基づきます。server permission、plugin規則、同時更新は含みません。

`dispatched`はゲーム内結果の成功を保証しません。例えば`place_block()`が成功しても、距離、権限、衝突などを理由にserverが設置を拒否できます。結果が重要な処理は、関連eventまたはsnapshotで確認してください。

Futureをdropしても、既にdispatchしたpacketは取り消されません。特に`use_item_for()`を途中でcancelした場合、利用側が`release_item()`を送る必要があります。

## 継続入力と単発要求

`ControlState`は置換されるまで継続します。`set_control()`の完了はlocal state更新だけを意味し、内部20 Hz loopが後続tickで反映します。

`jump()`は次のphysics tickに対するedge-triggered要求です。水中浮上やツタ上昇のようなhold入力には`ControlState::jump = true`を使います。

## Eventとsnapshot

Eventは変更通知で、完全なevent sourcing logではありません。broadcast receiverが遅れると`Lagged`になり得ます。その場合はgetterまたは`*_snapshot()`から現在状態を再取得します。

`Snapshot<T>`は次を持つowned valueです。

- `revision`: 状態領域ごとの単調増加番号
- `updated_at`: 接続後、その領域が最後に変更された時刻
- `captured_at`: snapshotを作成した時刻
- `value`: owned状態

revisionは同じBotの同じ領域だけで比較できます。異なるBot、playerとinventoryなど異なる領域、再接続前後では比較できません。

```rust
let snapshot = bot.inventory_snapshot().await;
if snapshot.revision != previous_revision {
    send_to_planner(&snapshot.value);
}
```

## 所有とmemory cost

`Bot::clone()`は同じ内部状態を共有する軽量handleです。一方、getterとsnapshotは内部lockを保持させないため、現在はowned cloneを返します。

| API | 所有・確保 |
| --- | --- |
| `player`、`motion`、`control` | 小さいowned copy |
| `survival_state` | mapを含むowned clone |
| `inventory` | slot、NBT、pending transactionを含むowned clone |
| `player_list` | list全体のowned clone |
| `observe_entities` | 対象entityのowned clone |
| `observe` | 毎回新しい`Vec`を確保 |
| `*_snapshot` | 対応getterと同じowned dataにmetadataを付加 |

revisionは不要な再転送を避けるために使えます。通常のgetterはownedですが、`ChunkSnapshot`のsection・light・NBT bufferとmap colorは`Arc` backedです。同じ`BotManager`のBotは内容が同一のchunk sectionを共有し、block更新時だけcopy-on-writeします。chunkのload/unload所有権そのものはBotごとに分離されます。

## Compatibility確認

`Bot::client_info()`、`Bot::protocol_info()`、`Bot::capabilities()`でbuildの能力を確認できます。接続中の宛先は`bot.server_info()`です。

現行buildはJava Edition 1.16.1、protocol 736、offline-mode専用です。online-mode認証、音声再生、他protocol versionは未対応です。

接続・login・play packet・ready待機のtimeoutと、接続ごとのchunk・entity・map・総cache record、CustomPayload、event queueの上限は`ConnectionOptions`で変更できます。上限超過は接続errorとして終了します。自動再接続とbackoffは外部controllerの方針です。

公開されるfallible APIは`voxrig::Result<T>`を返します。再試行や入力修正の判断にはerror文字列ではなく`Error::kind()`を使用します。`ErrorKind`は将来追加され得るため、matchにはfallback branchが必要です。
