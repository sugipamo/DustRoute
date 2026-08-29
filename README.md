# DustRoute

DustRoute is a Rust workspace for translating logical circuits into validated
Minecraft Java Edition redstone layouts and translating existing redstone
regions back into logical behavior.

Rust is the canonical and only supported implementation.

## Direction

The workspace is split by responsibility:

```text
dustroute-model      shared logic and Minecraft world types
dustroute-translate  forward and reverse translation
dustroute-optimize   placement and semantic rewrite optimization
dustroute-app        shared application services for MCP and CLI
dustroute-mcp        AI-facing MCP server and visible Minecraft bot
dustroute-cli        command-line integration
```

`dustroute-translate` exposes `Translator::forward`, `Translator::reverse`, and
`Translator::verify` as its stable facade. Optimization depends on the shared
model and translation types; translation does not depend on optimization.

`dustroute-mcp` is the intended primary user interface. It connects to a visible
Mineflayer player and grounds natural-language references in a player's gaze.
The CLI remains available as a debugging and automation interface.

## Translation pipeline

The compiler keeps stable boundaries between logical intent and Minecraft
realization:

```text
LogicDAG
  -> primitive lowering
  -> cell mapping
  -> placement
  -> physical routing
  -> legality validation
  -> Minecraft Data Pack
```

These boundaries are intended to become the API used by a future MCP server,
where an LLM and a user can construct and inspect circuits incrementally.

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
the directional redstone connectivity graph. Bidirectional dust runs are
collapsed into signal components; repeaters, powered blocks, and torch-control
edges provide direction. Source and sink components become inferred input and
output terminals.

The JSON report contains the detected terminals, unsupported components, an
exhaustive truth table (up to 16 inferred inputs), and a Boolean expression for
each output. Known functions such as AND, OR, XOR, NAND, and NOT are identified
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
