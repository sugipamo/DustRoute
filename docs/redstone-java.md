はい。まずは **Java Edition の高精度レッドストーン回路シミュレータ向け**に、実装仕様表として切ります。

重要なのは、「入力→出力」だけではなく **何を契機に再評価されるか／どのイベントキューに乗るか／同tick内の順序を保存する必要があるか** です。QCはJava版固有で、piston・dropper・dispenserが対象です。また「論理上poweredだがupdateを受けていないため未作動」という状態が実在します。([Minecraft Wiki][1])

### コア仕様表

| 対象                           | 入力・判定                      | 状態                                 | 遅延              | 発生させるもの                  | 実装上の注意                                          | 優先度    |
| ---------------------------- | -------------------------- | ---------------------------------- | --------------- | ------------------------ | ----------------------------------------------- | ------ |
| **Redstone Dust**            | 周囲から signal 0–15           | `power :: 0..15`, 接続shape          | 基本即時            | 周辺block update / power変化 | 単純BFS禁止。**update order** が回路結果に影響               | **S**  |
| **Solid / Conductive Block** | strong/weak power          | block自体に永続signal stateを持つというより照会対象 | 即時              | 周囲へのactivation条件         | `strongPower` / `weakPower` を分離                 | **S**  |
| **Repeater**                 | 背面入力、側面lock                | powered / delay / locked / facing  | 1–4 RS ticks    | scheduled tick、前方power   | 入力変化時に即出力を書換えない                                 | **S**  |
| **Comparator**               | rear、side、container signal | mode / powered / outputLevel       | scheduled       | 前方power/update           | analog 0–15必須。repeaterとはtick priority差があるケース    | **S**  |
| **Torch**                    | attached blockのpower       | lit/unlit、burnout履歴                | tick依存          | 周辺power/update           | burnoutには**時系列履歴**が必要                           | A      |
| **Observer**                 | 観測面のblock-state変化          | powered                            | pulse           | scheduled pulse/update   | redstone powerでなく **block state transition**を見る | **S**  |
| **Piston**                   | normal activation + QC     | extended / facing / moving state   | block event系    | block move/update        | おそらく最難関。判定と実際の移動を分離                             | **S+** |
| **Sticky Piston**            | 同上                         | 同上 + retract semantics             | 同上              | block move               | 短パルス・0-tickで通常piston以上に特殊                       | **S+** |
| **Dropper**                  | normal + QC                | `triggered`, inventory             | activation tick | item transfer/update     | QC対象。trigger edgeを保持                            | A      |
| **Dispenser**                | normal + QC                | `triggered`, inventory             | activation tick | dispense action          | dropper同様だがitemごとのaction差                       | A      |
| **Hopper**                   | inventory + powered state  | cooldown / inventory               | cooldownあり      | item push/pull           | redstone eventだけでなくinventory simulationが必要      | B〜S    |
| **Container**                | contents                   | inventory                          | —               | comparator signal変化      | fullness→analog出力の変換                            | B      |
| **Lever/Button**             | user action / timer        | powered                            | buttonのみ時間      | neighbor updates         | update発生源として重要                                  | A      |
| **Pressure Plate**           | entity presence            | powered / level                    | entity tick依存   | update/power             | entity subsystemを入れるなら必要                        | C      |
| **Rail系**                    | power/entity               | shape/powered                      | 種別依存            | block/entity update      | 高度装置ではupdate sourceとしても使われる                     | C      |
| **Target**                   | projectile hit             | power level                        | pulse           | signal/update            | entity simulation領域                             | C      |

**S = シミュレータ基盤として必須、A = 高互換性に必要、B/C = 対応範囲次第**です。

---

### さらに重要な「イベント種別」の表

ここはブロック表以上に重要です。

| イベント                | 意味                          | 同tick中に発生？ | queue化推奨 | 典型例                   |
| ------------------- | --------------------------- | ---------: | -------: | --------------------- |
| `NeighborUpdate`    | 周囲のblockが変化したことを通知          |        Yes |  **Yes** | dust変化→piston再評価      |
| `StateChange`       | block stateそのもののmutation    |        Yes |  **Yes** | dust power 7→8        |
| `ScheduledTick`     | 将来tickで処理                   |     No/Yes |   **必須** | repeater/comparator   |
| `BlockEvent`        | block固有event                |        Yes |   **必須** | piston extend/retract |
| `PowerQuery`        | 現在powerされるか照会               |          — |  queue不要 | piston activation判定   |
| `InventoryChanged`  | inventory mutation          |        Yes |       推奨 | hopper→container      |
| `EntityInteraction` | entityによる作用                 |        Yes |     後回し可 | pressure plate        |
| `BlockMoved`        | piston移動によるmutation         |        Yes |  **Yes** | BUD/QC更新源             |
| `ObservedChange`    | observerが見るstate transition |        Yes |  **Yes** | piston head出現         |
| `UserInput`         | lever/button等               |        Yes |      Yes | 入力イベント                |

設計としては、

```haskell
data Event
  = NeighborUpdate
      { source :: Pos
      , target :: Pos
      }
  | ScheduledTick
      { target   :: Pos
      , tickKind :: TickKind
      }
  | BlockEvent
      { target :: Pos
      , event  :: BlockEventKind
      }
  | InventoryChanged Pos
  | EntityInteraction Pos EntityId
```

くらいまで分ける価値があります。

---

## Power と Update は絶対に分ける

ここが一番大事です。

QCでは例えば、

```text
空間 y+1 が power 条件を満たす
             ↓
piston は論理上 activate 対象
```

でも、

```text
piston自身にはupdateが届いていない
             ↓
動かない
```

という状態があります。

Minecraft Wikiでも、QCによりpistonがactivation条件を満たしていても、遠すぎてupdateを受けない場合は、後からupdateされるまで作動しないと説明されています。さらにdust・repeater・comparator・torchなど、一部部品は2ブロック相当までupdateを起こすため「即時QC」が成立します。([Minecraft Wiki][1])

したがって、

```haskell
isPowered :: World -> Pos -> Bool
```

と

```haskell
onNeighborUpdate :: Pos -> World -> [Mutation]
```

は別にする。

これはかなり強くおすすめします。

---

## Repeater

Repeaterは最低でもこう。

```haskell
data RepeaterState = RepeaterState
  { facing  :: Direction
  , delay   :: Int       -- 1..4
  , powered :: Bool
  , locked  :: Bool
  }
```

入力：

```text
rear input
side lock input
```

出力：

```text
front only
```

側面からrepeaterまたはcomparatorの信号を受けるとlockされ、lock中は入力に関係なく現在出力を保持します。([Minecraft Wiki][2])

ここで大事なのは、

```haskell
neighborChanged -> powered = newInput
```

としないこと。

概念的には、

```text
neighbor update
      ↓
input query
      ↓
必要なら ScheduledTick を登録
      ↓
scheduled tick到達
      ↓
出力state変更
```

です。

---

## Comparator

最低限：

```haskell
data ComparatorMode
  = Compare
  | Subtract

data ComparatorState = ComparatorState
  { facing      :: Direction
  , mode        :: ComparatorMode
  , outputLevel :: Word8
  }
```

単なるboolean回路として扱わず、

```text
0..15
```

を最初からsignalの基本型にするのがおすすめです。

例えば、

```haskell
newtype Signal = Signal Word8
```

で invariant `0 <= x <= 15`。

### 特に危険

0-tick生成では **repeaterとcomparatorのtick処理順の差**そのものが利用されます。

Minecraft Wikiにも、repeater側を先に処理し、その後comparator側が処理される性質を使って、同tick中に

```text
OFF -> ON -> OFF
```

を作る0-tick generatorが説明されています。([Minecraft Wiki][3])

つまりこれ、

```haskell
Map Tick [Event]
```

だけでは足りない可能性があります。

少なくとも、

```haskell
data Time = Time
  { gameTick :: Int64
  , phase    :: TickPhase
  , priority :: Int
  , seqNo    :: Word64
  }
```

くらいを検討したい。

---

# Piston

ここだけ別格です。

モデル案：

```haskell
data PistonState
  = Retracted
  | Extended
  | MovingExtending
  | MovingRetracting
```

activation判定：

```text
normal redstone activation
        OR
quasi-connectivity activation
```

QC対象はJavaでは **piston / dispenser / dropper** で、crafterは対象外です。([Minecraft Wiki][1])

そして、

```text
power detected
    ↓
即 moveBlocks
```

ではなく、

```text
power/update
    ↓
activation check
    ↓
BlockEvent schedule
    ↓
event execution
    ↓
条件確認 / block movement
```

とした方がMinecraftの挙動を表現しやすい。

### Piston検証項目

| テスト                       | 通れば何が確認できる                    |
| ------------------------- | ----------------------------- |
| piston横からlever            | normal activation             |
| piston斜め上power            | QC                            |
| QC powerのみ・updateなし       | BUD state                     |
| QC状態で隣にblock設置            | delayed activation            |
| short pulse sticky piston | retract semantics             |
| 0-tick piston             | event ordering                |
| piston chain              | movement → update propagation |
| slime/honey push          | move graph                    |
| push limit                | movable set                   |
| immovable block           | cancellation                  |
| piston facing obstruction | activation exception          |

QCによる「update待ちpiston」はまさにBUD回路の根幹です。([Minecraft Wiki][1])

---

# Observer

内部的にはこれくらい分離したいです。

```haskell
onBlockStateMutation
  :: Pos
  -> BlockState
  -> BlockState
  -> World
  -> [Event]
```

つまりobserver側に

```haskell
poll :: World -> Bool
```

させるというより、

```text
BlockState A
     ↓ mutation
BlockState B
     ↓
observerへのupdate
```

として扱う。

そうしないと、

```text
A -> B -> A
```

が同じtick内に起こった場合、

tick末尾だけ比較する方式では

```text
A == A
```

になって消えます。

高度redstoneではこれは致命的です。

---

# 0-tick

シミュレータのデータ型テストとしてかなり有用です。

やってはいけないモデル：

```haskell
data SignalState = Off | On

tick world =
  calculateFinalState world
```

欲しいモデル：

```text
tick N:

t0   signal = 0
t0.1 signal = 15
t0.2 signal = 0
```

つまり最終値は同じでも、

```text
0 -> 15
15 -> 0
```

という **2つのtransitionが存在した**ことを失ってはいけない。

0-tickではこの順序制御にBUD pistonも使われます。Minecraft Wikiでも「BUDされたpistonはupdateされた時だけretractする性質」を利用してpistonの動作順を制御する例が説明されています。([Minecraft Wiki][3])

---

# Dropper / Dispenser

```haskell
data TriggerState
  = Triggered
  | NotTriggered
```

をblock stateとして持たせる。

実際にdispenser/dropperには `triggered = true/false` のblock stateがあります。([Minecraft Wiki][4])

重要なのは、

```text
powered=true が続く
```

からといって毎tick発射してはいけないこと。

概念的にはedge-sensitiveに、

```text
not triggered
   +
activation condition
       ↓
trigger
       ↓
action
```

として扱う。

さらにQC対象なので、

```haskell
activationCondition =
    normalPowered pos
 || normalPowered (above pos)
```

に近い判定が必要です。ただし「aboveにpower levelがあるか」ではなく、より正確には **その空間にmechanism componentが存在したならactivateされる条件か** と考えた方がいいです。([Minecraft Wiki][1])

---

# Hopper / Inventory

ここはredstoneシミュレータの範囲によります。

論理回路だけなら、

```text
powered -> locked
unpowered -> unlocked
```

程度から始めてもいい。

Storage Techまで通すなら一気に難しくなり、

```haskell
data HopperState = HopperState
  { inventory :: Inventory
  , cooldown  :: Int
  , enabled   :: Bool
  }
```

に加えて、

```text
pull
push
container change
comparator reevaluation
item entity intake
```

まで必要になります。

実際、高度なstorage回路では **dispenserを直接powerするとhopperまでlockしてしまう**ことを避けるような設計も使われます。([Minecraft Wiki][5])

なので、

```text
redstone subsystem
inventory subsystem
entity subsystem
```

を完全一体化せず、eventで接続する方が後々楽です。

---

# Worldモデル案

かなり現実的なのはこれです。

```haskell
data World = World
  { blocks     :: IntMap BlockState
  , blockData  :: IntMap BlockEntity
  , events     :: EventQueue
  , gameTick   :: Int64
  , nextSeq    :: Word64
  }
```

イベント：

```haskell
data Event = Event
  { time   :: EventTime
  , target :: Pos
  , cause  :: Cause
  , body   :: EventBody
  }

data EventTime = EventTime
  { tick     :: Int64
  , phase    :: Phase
  , priority :: Int
  , seqNo    :: Word64
  }
```

特に `seqNo` が地味に重要。

同じ

```text
tick
phase
priority
```

だったときでも、

```text
どっちが先にqueueへ入ったか
```

を決定論的に保存できます。

これならシミュレーション結果が

```text
毎回変わる
```

みたいな悲惨なことを避けられます。

---

## dependency を図にすると

```text
                     ┌──────────────┐
                     │ World State  │
                     └──────┬───────┘
                            │
                      state mutation
                            │
              ┌─────────────┴─────────────┐
              ↓                           ↓
       Neighbor Update              Observer detect
              │                           │
              ↓                           ↓
        block reevaluate            Scheduled event
              │
        ┌─────┴─────┐
        ↓           ↓
   power query    schedule
                    │
        ┌───────────┴────────────┐
        ↓                        ↓
 Scheduled Tick             Block Event
        │                        │
 repeater/etc                 piston
        │                        │
        └──────────┬─────────────┘
                   ↓
              State mutation
                   │
                   └── loop
```

要するに、**Worldをtick単位で丸ごと再計算するのでなく、mutationが次のmutationを生む因果グラフとしてシミュレートする**イメージです。

---

# 実装順も決めるなら

僕ならこうします。

| Phase  | 実装                               | 合格条件                        |
| ------ | -------------------------------- | --------------------------- |
| **1**  | power query / strong weak / dust | 基本論理回路                      |
| **2**  | event queue / neighbor update    | update-drivenになる            |
| **3**  | repeater/comparator              | 正確なdelay                    |
| **4**  | torch                            | torch clock                 |
| **5**  | QC                               | BUD piston待機状態              |
| **6**  | piston block event               | 普通のpiston door              |
| **7**  | observer                         | observer clock              |
| **8**  | 0-tick ordering                  | Purplers系回路                 |
| **9**  | dropper/dispenser                | QC含む                        |
| **10** | hopper/container                 | storage tech                |
| **11** | slime/honey movement graph       | 高度piston                    |
| **12** | entity mechanics                 | rails/plates/item alignment |

特に **Phase 8まで到達してPurplers/ilmango系の0-tick piston回路が再現できたら、かなり本物に近いシミュレータ**と言っていいと思います。

次はさらに実装寄りに、**各ブロックについて `queryPower / onNeighborUpdate / onScheduledTick / onBlockEvent` のどれを実装する必要があるかを一覧化した「Haskell向け型クラス/API表」**まで落とすと、そのまま設計に使えます。

[1]: https://minecraft.wiki/w/Tutorial%3AQuasi-connectivity?utm_source=chatgpt.com "Tutorial:Quasi-connectivity – Minecraft Wiki"
[2]: https://ja.minecraft.wiki/w/%E3%83%AC%E3%83%83%E3%83%89%E3%82%B9%E3%83%88%E3%83%BC%E3%83%B3%E3%83%AA%E3%83%94%E3%83%BC%E3%82%BF%E3%83%BC?utm_source=chatgpt.com "レッドストーンリピーター - Minecraft Wiki"
[3]: https://minecraft.wiki/w/Tutorial%3AZero-ticking?utm_source=chatgpt.com "Tutorial:Zero-ticking – Minecraft Wiki"
[4]: https://ja.minecraft.wiki/w/%E3%83%96%E3%83%AD%E3%83%83%E3%82%AF%E7%8A%B6%E6%85%8B?utm_source=chatgpt.com "ブロック状態 - Minecraft Wiki"
[5]: https://minecraft.wiki/w/Tutorial%3AStorage_minecarts?utm_source=chatgpt.com "Tutorial:Storage minecarts – Minecraft Wiki"
