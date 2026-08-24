# Minecraft Data Packs

Java Edition 実機回帰用の Data Pack です。ZIP のままワールドの `datapacks/` ディレクトリへ配置し、`/reload` してください。

## semantics_01_20.zip

低レイヤ意味論 / 接続 probe 01〜20。

```text
/function ro_sem:tests
```

全自動 suite は同じ origin のテストベンチを再利用するため、simulation distance を超える横長 gallery に依存しません。

個別 probe:

```text
/function ro_sem:14_dust_stair_up/run
/function ro_sem:20_repeater_to_corner/run
```

視覚確認用 gallery:

```text
/function ro_sem:gallery
```

gallery は遠くまで展開されるため、必要に応じてプレイヤーが近づいてください。

## half_adder.zip

DAG -> physical baseline の半加算器。

```text
/function ro_half_base:tests
```

個別ケース:

```text
/function ro_half_base:case_00
/function ro_half_base:case_01
/function ro_half_base:case_10
/function ro_half_base:case_11
```

## mux_decoder.zip

2:1 MUX と enable 付き 1→2 decoder。

```text
/function ro_circuits:mux/tests
/function ro_circuits:decoder/tests
```

ヘルプ:

```text
/function ro_circuits:help
```

## 位置づけ

これらはデモだけではなく compatibility tests です。Python simulator と static validator が PASS しても、Minecraft 実機との意味論差や command update order の問題は残り得ます。
