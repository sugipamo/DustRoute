# Architecture and development

Run commands from the repository root. For product workflows, start with the
[documentation index](README.md) and [public feature guide](mcp-public-features.md).
For Java server/bot setup, use [MCP setup](../crates/dustroute-mcp/SETUP.md).

## Workspace responsibilities

| Crate | Responsibility |
| --- | --- |
| `dustroute-minecraft` | World primitives, block models, events, scheduler profiles and validation |
| `dustroute-physical` | Physical observations, evidence, ports, nets, fragments and patches |
| `dustroute-ir` | Derived signal/gate/expression/function and temporal representations |
| `dustroute-library` | Reusable specifications, provenance and compatibility evidence |
| `dustroute-translate` | Forward compilation, reverse interpretation and bounded simulation |
| `dustroute-optimize` | Candidate search and structural/behavioral verification |
| `dustroute-app` | Shared application services and placement planning |
| `dustroute-mcp` | MCP orchestration, observed/revision/operation identities and bot bridge |
| `dustroute-cli` | Diagnostic and automation entry points |

Forward compilation follows logical intent → primitive lowering → cell mapping
→ placement/routing → legality validation → Java export. Reverse interpretation
starts with observed blocks and preserves physical evidence before deriving
higher-level labels. `PlacementCircuit` is proposed geometry, not observation.

`World` is mutable data. General placement requires `ValidatedWorld`; edits
invalidate that proof. Persisted plans are proposals and must be checked against
fresh live state. The fixed 1×2 piston preset has a separate exact-contract
proof and does not widen general placement support. See
[validation boundaries](world-validation-boundary.md).

## Toolchain and local checks

The workspace manifest declares Rust edition 2024 and `rust-version = "1.85"`.
Use a current stable toolchain for the existing formatting/lint/test workflow.
Live integration additionally uses Node.js 22/npm and Java 21 with the pinned
Minecraft Java 1.21.11 environment. Rust-only tests do not need Minecraft.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix crates/dustroute-mcp/mineflayer ci
npm --prefix crates/dustroute-mcp/mineflayer test
```

The GitHub Actions workflow has been removed. These are local integration
checks, not checks automatically run on push or pull request. CI configuration,
`.cursorignore` and `.gitattributes` are intentionally absent; `.gitignore`
continues to exclude build products, local worlds, dependencies and secrets.

Run narrower tests for a bounded change, and the workspace checks before
integration. Do not restart live servers merely to validate documentation.

## CLI examples

```bash
cargo run -p dustroute-cli -- eval mux2 a=1 b=0 s=0
cargo run -p dustroute-cli -- export half-adder target/half-adder.zip ro_half_rust
cargo run -p dustroute-cli -- export-semantics target/semantics.zip ro_sem
cargo run -p dustroute-cli -- analyze-snapshot snapshot.json
```

The CLI snapshot analyzer requests bounded functional inference; MCP leaves
exhaustive truth tables opt-in. An incomplete interface or exhausted row/time/
solver budget must return unavailable/error evidence, not partial rows labeled
as a complete function. See [function modeling](physical-function-model.md).

The retained 3×3 piston CLI scenarios are diagnostic-only. Their commands and
limits are in [piston diagnostics](piston-diagnostics.md); successful diagnostic
execution does not authorize construction.

## Snapshot format

`properties` retains observed Java block-state values. An absent cell means air
only inside a complete declared observation; boundary completeness is separate.

```json
{
  "min": {"x": 0, "y": 100, "z": 0},
  "max": {"x": 20, "y": 110, "z": 20},
  "blocks": [{
    "pos": {"x": 1, "y": 101, "z": 1},
    "name": "minecraft:repeater",
    "properties": {"facing": "west", "delay": "1", "powered": "false", "locked": "false"}
  }]
}
```

## Evidence and benchmarks

Sanitized regression observations are tracked under
`crates/dustroute-translate/tests/fixtures` and
`crates/dustroute-mcp/mineflayer/e2e/fixtures`. Runtime recordings, server JARs,
worlds, logs and player lists remain in ignored `.local/` directories.
Historical failure fixtures are retained deliberately; they are not current
feature status or a request to reimplement obsolete goals.

Use [differential testing](physics-differential-testing.md),
[server instrumentation](vanilla-instrumentation.md), and the
[live harness guide](../crates/dustroute-mcp/mineflayer/e2e/README.md) for evidence
capture. Timing claims must preserve the distinction between client packet
order and observed server internals.

Use [performance observation](performance-observation.md) for reproducible
release benchmarks. Measurements and verified candidate contracts remain
separate from general performance claims.
