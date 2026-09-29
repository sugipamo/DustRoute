# Minecraft 1.16.1 Offline Survival Bot 完了報告

## 結論

[サバイバルロードマップ](survival-roadmap.md)のPhase 7〜16を実装し、Minecraft Java Edition 1.16.1（protocol 736、`online-mode=false`）の実サーバーで検証した。

AIはJSONLを介さずRustクレートを直接呼び出す。1プロセス複数Bot、`Player`由来username、raw protocol情報を失わない型付き状態という設計を維持している。意味分類、経路探索、クラフト計画、戦闘判断、画像・音声renderはAI側の責務である。

## Phase別結果

| Phase | 実装・実サーバー証跡 | 状態 |
| --- | --- | --- |
| 7 生存状態 | health、food、saturation、experience、difficulty、time、weather、effect、attribute、dimension、death、respawnを取得。`/kill`後のrespawnを確認 | 合格 |
| 8 Inventory | 46 player slots、hotbar、armor、offhand、ItemStack/NBT、pickup、drop、slot同期。stone 5個の取得・stack dropとshield装備を確認 | 合格 |
| 9 Block/item操作 | oak logの時間付きdigとack、stone設置、arm swing、use/release itemを確認 | 合格 |
| 10 Window | chest open、通常/right/shift等のclick mode、cursor、action、transaction、reject rollback、player slot同期を実装。chest transferを確認 | 合格 |
| 11 Crafting | raw recipe registry、2×2でplanks/table/sticks、3×3でwooden pickaxeを作成 | 合格 |
| 12 Entity | object/living/player/orb、metadata、equipment、relative move、teleport、velocity、destroyを追跡。cow lifecycleを確認 | 合格 |
| 13 Combat | interact/interact-at/attack、attack cooldown、animation、hurt、server velocityを実装。Botがcowを攻撃して倒すことを確認 | 合格 |
| 14 Container | chest、crafting table、furnace propertyを実装。iron oreとcoalからiron ingotを精錬・回収 | 合格 |
| 15 Chat | raw JSON chat送受信、command送信、player listを実装。複数Bot間chatを確認 | 合格 |
| 16 Sound | raw/named sound ID、公式名、category、source、volume、pitch、sequence、受信時刻を構造化。再生・意味推論なし | 合格 |

## Survival操作の実測

- oak logをsurvivalのdigging時間で破壊し、server acknowledgement成功
- logからplanksを作り、crafting tableとsticksを2×2 craft
- crafting tableの3×3 recipeでwooden pickaxeを作成
- furnaceへiron oreとcoalを入れ、progress propertyを追跡し、iron ingotを回収
- Hungerでfood 20→19を観測し、apple使用後にfood 20、saturation 2.4へ回復
- shieldをplayer inventoryからoffhand slot 45へ装備
- cowのspawn/move/metadataを観測し、cooldown付きattackで倒し、destroyを確認
- 2 Botsがchatを共有し、同じchestを介してstoneを受け渡し

これらは高レベルな自動計画ではなく、AIが直接組み合わせられる低レベルの型付き操作プリミティブとして実装している。

## 複数Bot耐久試験

Java 8 / Minecraft 1.16.1 / offline-mode / release buildで10 Bots × 10分を実施した。各Botは接続時にvitalsとplayer inventoryの初期同期を完了してから、独立したControlStateで移動、sprint、jumpした。

| 項目 | 結果 |
| --- | ---: |
| 時間 | 600秒 |
| Bots | 10 |
| movement packets | 120,065 |
| 位置補正 | 0 |
| 切断 | 0 |
| physics tick p99 | 129 us |
| queue lag p99 | 2.074 ms |
| server TPS | 20.00 |

結果はrepository版の`reports/survival-10bots-10min.md`に保存した。server profilerは693.95秒、13,880 ticks、20.00 TPSだった。

## Protocol整合性で修正した問題

- 通常window clickはaccepted時にserverが全slotを再送しないため、cursor/slotのclient predictionを実装
- transaction reject時はsnapshotへrollbackし、confirm後のserver resyncを受ける
- accepted直前のslot echoに予測を上書きされないようtransaction確定時に再適用
- container末尾36slotとwindow 0のplayer inventoryを双方向に同期
- crafting result clickによるgrid消費をclient側へ反映
- item registryのstackSizeに従って同種stackをmerge
- Player Info updateが未知UUIDへ先行してもread loopを終了しない
- entity velocity packetを自Botの物理へ反映

## 最終品質監査

- `cargo fmt --all -- --check`
- `cargo test`：32件合格、失敗0
- `cargo clippy --all-targets -- -D warnings`
- 実1.16.1 server probes：state、respawn、inventory、interaction、window、craft、entity、combat、furnace、food、chat、sound、cooperation
- JSONL操作経路なし
- [サバイバルロードマップ](survival-roadmap.md)未完了チェック項目なし

## 対象外

Microsoft認証、online-mode暗号化、他protocol version、GUI/render、音声再生、経路探索、クラフト計画、戦闘戦略、elytra、完全なvehicle物理、厳格なアンチチート完全一致は対象外である。
