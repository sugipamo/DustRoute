# Translation Pipeline

この文書は、論理回路がどのように Minecraft のレッドストーン配置へ翻訳されるかを、層ごとに説明します。

## 1. Logic DAG

最上位は Minecraft を一切知らない純粋な論理 DAG です。

```text
Input / NOT / AND / OR / XOR / NAND
```

各 `LogicNode` は producer node の ID だけを入力として持ちます。この段階には座標、向き、dust、repeater、support block は存在しません。

例: 半加算器

```text
a ---- XOR ---- sum
 \    /
  AND -------- carry
 /    \
b      
```

DAG にすることで、共通部分式の共有と fan-out を明示できます。

## 2. Primitive lowering

Minecraft 用の baseline primitive に落とします。現在の代表例は XOR です。

```text
XOR(a,b)
= OR(
    AND(a, NOT(b)),
    AND(NOT(a), b)
  )
```

ここでも物理配線は作りません。`a` や `b` は共有 producer のままなので、fan-out は DAG 上で保持されます。

担当: `logic_dag.py`

## 3. Circuit bridge

既存の `Gate / Pin / Net / Circuit` IR へ bridge します。

ここで初めて「1 source → N sinks」という論理 Net ができます。ただし、Net はまだ物理的な線ではありません。

```text
logical Net != Minecraft dust path
```

担当: `logic.py`, `logic_dag.py`

## 4. Cell mapping

各 primitive gate を、固定の物理セルへ対応付けます。

例:

```text
NOT -> torch inverter cell
AND -> De Morgan + repeater cell
OR  -> buffered OR cell
INPUT / OUTPUT -> repeater-buffered boundary cell
```

baseline では候補探索を行わず、実機確認済みの固定セルを使います。

担当: `baseline_cells.py`, `cells.py`

## 5. Placement

論理 DAG の depth と consumer の位置関係から、各セルへ座標を与えます。

現在の baseline は consumer barycenter を用いた deterministic placement です。探索型 optimizer ではありません。

目的は、fan-out source と consumer を極端に離さず、routing が合法な解を見つけやすくすることです。

担当: `baseline_compiler.py`

## 6. Port realization

ここが論理と Minecraft 物理接続の重要な境界です。

セルの typed port を、router が扱える物理契約へ変換します。

現在の主な port kind:

- `WIRE`
- `BLOCK_POWER`

`BLOCK_POWER` は「入力ブロックそのものへ wire を置く」という意味ではありません。ブロックの外側に terminal dust を作り、その dust から対象ブロックを給電します。

また sink terminal は leaf として扱い、fan-out junction に再利用しません。

```text
trunk ---- junction ---- other sink
             |
             +-- approach -- leaf dust -> [BLOCK_POWER]
```

担当: `port_realization.py`

## 7. Routing resources

Minecraft の配線では「wire の座標が空いている」だけでは不十分です。

router は次の資源を予約します。

- conductor position
- support position
- electrical keepout
- stair clearance
- terminal reservation

特に dust stair は、接続に必要な空気ブロックを後から support block で埋めると壊れます。そのため stair clearance も routing resource です。

担当: `routing_resources.py`, `multinet.py`

## 8. Physical routing

論理 Net を shared tree として物理配置します。

```text
source
  |
  +--- shared trunk ---+--- sink A
                      +--- sink B
                      +--- sink C
```

別 Net の dust 接触を避け、support / stair clearance を守りながら A* ベースで枝を追加します。

さらに source→sink の完全な tree path を見て、wire run が長すぎる場合は直線部分へ repeater を配置します。

担当: `multinet.py`, `routing.py`

## 9. Physical continuity validation

「path の座標列が連続している」だけでは Minecraft 上の接続を保証できません。

そこで各 adjacent pair を、実際の物理接続規則で検証します。

```python
physical_step_connected(world, src, dst)
```

確認対象には次が含まれます。

- dust -> dust
- dust -> repeater
- repeater -> dust
- dust -> powered block boundary
- repeater -> block
- stair up / down
- repeater facing

担当: `connectivity.py`

## 10. World materialization

セルと route を統合し、Minecraft block の sparse `World` を作ります。

ここで support block、dust shape、repeater orientation が具体化されます。

担当: `model.py`, `wire.py`, `multinet.py`

## 11. Minecraft export

最後に Java Edition の `/setblock` / `/fill` / function へ変換します。

実機検証では、設置順や chunk load による誤判定を避けるため、次のような phased test を使います。

```text
BUILD
 -> settle
STIMULATE
 -> settle
CHECK
```

低レイヤ suite は同一 origin を使い回し、遠方の未ロード chunk に依存しない構成です。

担当: `minecraft_export.py`, `minecraft_semantics.py`, `minecraft_circuits.py`

## 翻訳層で守るべき原則

1. 論理意味論と物理 routing を混ぜない。
2. port の意味を router の暗黙知にしない。
3. Minecraft で必要な空間も routing resource として扱う。
4. 座標連続性ではなく電気的連続性を検証する。
5. Python model が PASS しても Minecraft 実機を最終参照とする。
