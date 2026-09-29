# ピストン通知後の階段形状と、クライアント更新機構の前提

2026-09-29。1.21.11観測アダプタから得た調査結果。
この文書は追加実装前の調査と停止判断の記録。
その後ユーザーが移動中状態を含む実装とVoxrigの大きな変更を承認したため再開した。
**通常・粘着ピストンの限定範囲を実装し、階段の不一致を解消した。**
[実装・実機比較と残る制約](client-piston-reconstruction.md)を参照。
DustRouteへの切替とコマンド確認の除去は未着手。

## 確認できたこと

新規の隔離ワールドを公式Java 1.21.11サーバーで起動し、MOD無しで
`device-stairs-inner-top-left-r0` を原点 `(100,180,100)` に配置した。
Voxrigから通常の使用パケットでレバーをON、800ms後にOFF、さらに4000ms待機した。
この時間は実時間であり、入力のサーバー適用tickを確定した計測ではない。

対象のクォーツ階段 `(102,180,99)` は、初期状態が `inner_left`。
隣の丸石階段が動いた後もVoxrigでは `inner_left` のまま、サーバーの
`execute if block` では `straight` だった。新しい接続で同じチャンクを取得すると
Voxrigも `straight` を読めた。再接続のためにワールドを編集してはいない。

受信sequence 2454–2495の42 packetを、省略・容量超過無しで保存した。
Rust側のTCP受信から展開した各packetのpayloadを、フィールド解釈と状態反映の前に
保存したもので、JSとの通信は存在しない。

- ピストンのBlock Actionは2件。sequence 2464が伸長、2480が収縮。
- Block Changeは10件、Section Blocks Updateは3件。
- 対象のクォーツ階段を更新するstate packetは0件。
- 区間内のchunk再ロード・unloadは0件。
- snapshotの受信境界は初期2453、ON後2474、OFF後2495。
- レバーはOFF→ON→OFF、本体は収縮→伸長→収縮と観測できた。

この試行では、正しい階段state packetを取り落としたパーサー不良や、JS–Rust間の
順序逆転では説明できない。Minecraft全体の全ての古い観測について同じ原因と断定しない。

## 対象版の処理と未実装箇所

ローカルのJava 1.21.11 / Yarn build.6のmapped JARを `javap -c -p` で照合した。

- `PistonBlock.onSyncedBlockEvent` は、給電の再確認をサーバー側で行った後、
  クライアント側でも `move` を呼ぶ経路を持つ。
- `PistonBlock.move` は `PistonHandler` で移動対象・破壊対象を求め、
  moving pistonとblock entityを作り、移動元の除去と隣接状態更新を行う。
- `PistonBlockEntity.tick` は途中状態からの完了と状態の後処理を持ち、
  クライアント側には完了を待つ処理もある。
- `StairsBlock.getStateForNeighborUpdate` は水平の隣接更新で形状を再計算する。

今回のアダプタはstate packetの反映までであり、このクライアント側実行機構を持たない。
ソース照合は不足している処理の根拠であり、この場でvanillaの描画クライアントが
同じ試行を正しく処理したという実測の代わりにはしない。

## ここで停止する理由

ロードマップ第3段階に含めた「必要なクライアント処理の調査」の結果、
移動リスト、moving block entityの寿命、途中反転、隣接通知順を持つ実行機構の追加が
次の前提だと分かった。粘着・スライムを含むDustRouteの対象範囲では、中規模以上になる。
調査項目として想定していたものを、そのまま実装済みと扱うことはできない。

ユーザーの「実装が重そうなポイントを見つけたらいったん止めて」に従い、
この追加機構の実装とDustRouteへの切替は停止する。アダプタ・診断の着手済み部分は
検証して保持し、既存のMineflayerと独立したサーバー読戻しを稼働経路に残す。

## 次に具体化する実装案

1. **受信データとクライアントによる計算結果の出所を型で区別する。**
   接続・dimension・受信sequenceに結び付け、処理できない通知や不足チャンクを
   検出したときは、最新とみなせない領域を明示する。どちらも自動的に
   DustRouteの `ValidatedRegion` やサーバー確認済み証拠には変換しない。
2. **版別のクライアント更新機構を追加する。**
   共通APIの下に `java_1_21_11` 専用のBlock Action適用、移動対象探索、
   moving blockの開始・完了・反転、隣接形状・支持更新を置く。
   最初は今回の通常ピストンと階段から始め、粘着・スライムの範囲を明示的に広げる。
   未対応の材質や途中状態に遭遇したら結果の採用を拒否する。
3. **独立したサーバー記録で検証する。**
   受信packet列の再生、今回のfixture、参照ドア、フライングマシンへ進む。
   クライアント計算とDustRouteモデルを共有した場合でも、両者が同じ結果という
   理由だけで実機一致とは判定しない。
4. **一致と欠測時の拒否を確認した機能からDustRouteへ接続する。**
   コマンド確認を省ける範囲を別途明示する。再接続で形状を取得できた今回の結果だけを
   根拠に、再接続を一般の同期保証や継続観測の代替にはしない。

完了条件は、今回の不一致を解消し、途中状態・反転・chunk欠測を含む宣言ケースで
サーバーとの比較証拠を残すこと。全ブロック対応やサーバー物理の全移植は別範囲とする。

## 保存した証拠

- [capture一覧とSHA-256](evidence/stairs-packets-20260929.manifest.json)
- [初期・ON・OFF snapshotと全42 packet](evidence/stairs-packets-20260929.json.gz)
- [再接続時の領域snapshot](evidence/stairs-reconnected-20260929.json.gz)
- [サーバーの該当ログ](evidence/stairs-server-20260929.log)

`retained_native_packets_reproduce_the_stale_stair_without_js` は、この受信列を
実運用と同じ状態反映関数へ渡し、ON/OFFの全770セルとrevisionを再現する。
これは既知の制約を固定する検査であり、階段同期の合格ケースではない。
キャプチャ時は第1段階コミット後の未commit実装を実行した。保持したpacketを
最終実装で再生できることと、当時の実行バイナリを完全に再現できることは区別する。

サーバーはloopback:25577、offline-mode、新規world `isolated`。
所有するforce-loadは9+4 chunkを解除し、`stop`で全dimensionの保存・正常終了を確認した。
ユーザーの既存ワールドは変更していない。
