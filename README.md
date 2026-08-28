# DustRoute

DustRoute is a Rust compiler for translating logical circuits into validated
Minecraft Java Edition redstone layouts and Data Packs.

Rust is the canonical and only supported implementation.

## Direction

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
```

The local Minecraft integration harness lives under `.local/` and is excluded
from Git.
