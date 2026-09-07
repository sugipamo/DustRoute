# Piston diagnostic fixtures

The supported public door is the [fixed 1×2 MCP contract](piston-door-mcp-v1.md).
The retained 3×3 shuttle fixtures are diagnostic models and regression evidence,
not installable lever-controlled circuits. This distinction also applies to
successful diagnostic CLI output.

## Retained models

| Fixture | Purpose | Boundary |
| --- | --- | --- |
| [Mechanical 3×3 reference](../crates/dustroute-translate/tests/fixtures/reference_3x3_noncompact_piston_shuttle.json) | Nine ordinary panel blocks, 18 normal pistons | Mechanical geometry and replay |
| [Single-cell shuttle](../crates/dustroute-translate/tests/fixtures/single_cell_piston_shuttle.json) | Two opposed normal pistons moving one block | Synthetic lever-to-pulse driver |
| [3×3 fanout](../crates/dustroute-translate/tests/fixtures/3x3_piston_shuttle_fanout.json) | Serial `1 → 3 → 9` wire/repeater control model | Unsupported stacked wiring and synthetic input sources |

For panel cells `x,y=0..2`, the south-facing open piston is at `(x,y,-1)` and
the north-facing close piston at `(x,y,2)`. Panel blocks occupy `z=0` when closed
and `z=1` when open. Both piston groups finish retracted. A `LeverPulseSequence`
changes remote source levels in the model; it is not a physical pulse generator.
Fixture-specific timing remains recorded in the JSON and integration tests.

The serial schedule is a retained scenario choice, not a current engine
requirement. Independent deferred piston completions now revalidate local
dependencies and rebuild their deltas against the current world. The global
`WorldDelta` parent/before-state checks remain strict. See
[low-layer validation](piston-low-layer-validation.md).

## Why the 3×3 scenario is not deployable

The retained [live diagnostic](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-door-live-diagnostic.json)
records three successful mechanical open/close cycles with command-injected
power. It also records literal-placement failure: all 162 repeaters and 68 of
72 dust blocks disappeared; normal lever activation did not open the panel.
The fixture includes unsupported component stacks and switchable synthetic
redstone-block sources. Adding supports would overwrite other components and
would not supply a physical lever-to-pulse controller.

These observations do not establish a usable 3×3 controller or exact scheduler
timing. The normal scenario entry points reject placement before execution.
Read-only recognition of observed 3×3 geometry is a separate
[recognition contract](observed-3x3-piston-door.md), not an operation capability.

## Reproduce

Run from the repository root:

```bash
cargo run -p dustroute-cli -- run-piston-door crates/dustroute-translate/tests/fixtures/3x3_piston_shuttle_fanout.json cycle --diagnostic
cargo test -p dustroute-translate --test reference_3x3_piston_door
cargo test -p dustroute-translate --test single_cell_piston_shuttle
cargo test -p dustroute-translate --test fanout_probe
```

Diagnostic mode reports `diagnostic_complete` together with failed placement
validation. Without it, the invalid scenario reports `world_validation_failed`
and creates no execution trace.

For the historical live comparison, use the private disposable server from
[setup](../crates/dustroute-mcp/SETUP.md), then:

```bash
cargo build -p dustroute-cli
node crates/dustroute-mcp/mineflayer/e2e/piston-door-live.js
```

The harness refuses nonempty/unloaded test regions and verifies cleanup. Its
zero exit status means the diagnostic ran, not that the 3×3 door is deployable;
inspect the report. No new 3×3 routing, controller, or public operation is
implied by retaining these fixtures.
