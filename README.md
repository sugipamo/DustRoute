# DustRoute

DustRoute is a Rust workspace for translating logical circuits into validated
Minecraft Java Edition redstone layouts and translating existing redstone
regions back into logical behavior.

Rust is the canonical and only supported implementation.

## Direction

The workspace is split by responsibility:

```text
dustroute-physical   canonical PhysicalScene observations, ports, evidence, nets, and fragments
dustroute-ir         derived Gate/Expression/Functional views and transformations
dustroute-translate  forward and reverse translation
dustroute-optimize   placement and semantic rewrite optimization
dustroute-app        shared application services for MCP and CLI
dustroute-mcp        AI-facing MCP server and visible Minecraft bot
dustroute-cli        command-line integration
```

`dustroute-physical` is the source of truth for observed Minecraft circuits.
`PhysicalScene` records typed ports, evidence, scan frontiers, and incomplete
observations. Only verified conductive connections are unioned into nets;
verified functional connections form fragments without merging distinct signal
nets across directional devices. Nearby disconnected
fragments are discovered separately as gap candidates and are never unioned by
proximity alone. `dustroute-ir` owns abstraction changes used to project the
physical circuit into local cells, logic expressions, and functional views.

`dustroute-translate` exposes `Translator::forward`, `Translator::reverse`, and
`Translator::verify` as its stable facade. Reverse results carry the canonical
physical scene alongside traceable gate, expression, functional, signal, and
behavior views. The forward placement graph is explicitly named
`PlacementCircuit`; it is not an observation of the Minecraft world.

`dustroute-mcp` is the intended primary user interface. It connects to a visible
Mineflayer player and grounds natural-language references in a player's gaze.
The CLI remains available as a debugging and automation interface.

## Translation pipeline

The forward compiler keeps stable boundaries between logical intent and
Minecraft realization:

```text
LogicDAG
  -> primitive lowering
  -> cell mapping
  -> placement
  -> physical routing
  -> legality validation
  -> Minecraft Data Pack
```

For reverse analysis the ownership direction is physical-first:

```text
observed Minecraft blocks
  -> PhysicalScene and observation frontiers
  -> typed ports and evidence-backed directed connections
  -> conductive nets and verified functional fragments
  -> GateView -> ExpressionView -> optional FunctionalView
```

The upper layers help an LLM and user understand the circuit; they do not
replace the observed physical representation as the source of truth.

Gate and expression views retain physical component IDs and can represent
partial, conflicting, and boundary-limited recognition. Functional labels such
as half adders are optional metadata over the lower-level gates and expressions.
The physical graph retains component IDs, positions, verified edge evidence,
and device delays. Independent temporal analysis describes repeaters, torches,
comparators, pistons, delayed traces, and feedback patterns such as clock and
latch candidates. Explicitly requested reverse truth-table expressions can be
converted back into a `LogicDag`; forward compilation produces the Minecraft
layout directly.

## Implementation status

The Rust workspace provides typed logic DAGs and rewrites, verified physical
cells, placement, electrical simulation, connectivity extraction, fanout-aware
multi-net routing with legality checks, and Java Data Pack export. Built-in
regressions cover half adders/subtractors, MUX, decoder, and full adder.

The logical/physical boundary is deliberately public so a future MCP server can
build, inspect, optimize, and export a circuit incrementally with a user.

## Development

Rust 1.85 or newer is required.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p dustroute-cli -- eval mux2 a=1 b=0 s=0
cargo run -p dustroute-cli -- export half-adder target/half-adder.zip ro_half_rust
cargo run -p dustroute-cli -- export-semantics target/semantics.zip ro_sem
cargo run -p dustroute-cli -- analyze-snapshot snapshot.json
cargo run -p dustroute-mcp
```

The local Minecraft integration harness lives under `.local/` and is excluded
from Git.

## Reverse analysis

`analyze-snapshot` accepts a bounded Minecraft block snapshot and reconstructs
the directional redstone connectivity graph. Verified connections form
physical nets, while nearby disconnected nets remain separate fragments with
gap evidence for broken-circuit analysis. Bidirectional dust runs are projected
into signal components; repeaters, powered blocks, and torch-control edges
provide direction. Source and sink components become inferred terminals.

The CLI JSON report explicitly requests and contains an exhaustive truth table
(up to 16 inferred inputs) and a Boolean expression for each output. Library and
MCP reverse analysis leave truth-table enumeration disabled unless requested.
Known functions such as AND, OR, XOR, NAND, and NOT are identified
directly; other combinational outputs fall back to canonical sum-of-products.

Snapshots use this shape; `properties` contains the Java block-state values
reported by the test client:

```json
{
  "min": { "x": 0, "y": 100, "z": 0 },
  "max": { "x": 20, "y": 110, "z": 20 },
  "blocks": [
    {
      "pos": { "x": 1, "y": 101, "z": 1 },
      "name": "minecraft:repeater",
      "properties": { "facing": "west", "delay": "1", "powered": "false" }
    }
  ]
}
```
