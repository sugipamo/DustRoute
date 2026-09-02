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
dustroute-library    provenance-aware reusable circuit specifications and evidence
dustroute-translate  forward and reverse translation
dustroute-optimize   placement and semantic rewrite optimization
dustroute-app        shared application services for MCP and CLI
dustroute-mcp        AI-facing MCP server and visible Minecraft bot
dustroute-cli        command-line integration
```

Observed-world optimization supports a conservative phased objective through
the MCP `new_optimization` tool. With `objective` set to
`density_then_wire_length`, planning first favors local density, then recovers
connector cost, and finally requires the whole result to beat the baseline.
Temporary connector growth exists only inside the search: Minecraft receives
only the previewed final patch. The live candidate generator currently supports
a single non-branching dust path with fixed endpoints; broader physical
component relocation remains future work.

`dustroute-physical` is the source of truth for observed Minecraft circuits.
`PhysicalScene` records typed ports, evidence, scan frontiers, and incomplete
observations. Only verified conductive connections are unioned into nets;
verified functional connections form fragments without merging distinct signal
nets across directional devices. Nearby disconnected
fragments are discovered separately as gap candidates and are never unioned by
proximity alone. `dustroute-ir` owns abstraction changes used to project the
physical circuit into local cells, logic expressions, and functional views.

Imported blocks retain their namespaced Minecraft identifier and every
reported block-state property even when DustRoute only has a coarse physical
classification for that block. Per-block capability reports distinguish full,
partial, unsupported, and non-applicable support for physical classification,
connectivity, steady-state behavior, temporal behavior, repair, and placement.
Unsupported semantics make the relevant derived IR stage partial; they do not
erase the underlying observation.

Contract checks are fail-closed at the physical boundary. Live-only devices
such as targets, daylight detectors, containers, sensors, fluids, and rails
remain visible in the snapshot but are marked unsupported for simulation,
repair, and placement. Observers are modeled as directional state-transition
pulse sources (including a one-redstone-tick pulse); exact server-side update
ordering still requires live verification. An inferred truth table is unavailable when
an external input or observable output is unmapped, when the interface is
ambiguous, or when there is no input/output evidence; an empty case set is not
treated as a successful verification. Button and pressure-plate scenarios use
typed drivers, and weighted plate levels are supplied explicitly because
entity occupancy is a live-world observation.

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
comparators, observers, pistons, delayed traces, and feedback patterns such as clock and
latch candidates. Explicitly requested reverse truth-table expressions can be
converted back into a `LogicDag`; forward compilation produces the Minecraft
layout directly.

Temporal analysis also builds a lossless timed circuit before deriving a
steady-state projection. Buffer and wire compression retains the accumulated
delay in redstone ticks (one redstone tick is two game ticks) and the exact
physical path. Every analyzed region is classified as `steady_state_safe`,
`timing_sensitive`, or `temporal_required`. Unequal-delay reconvergence remains
timing-sensitive; feedback and mechanical devices require temporal
interpretation. The bounded simulator handles repeater delay and side locking,
torch delay, basic comparator compare/subtract behavior, observer
state-transition pulses, and lamp off delay;
exact within-tick vanilla update ordering remains live-observation evidence.
Higher-level functional labels are
therefore explicitly scoped instead of silently discarding timing behavior.

Measured behavior traces are classified separately from structural timing
risk. A `0 -> 1 -> 0` or `1 -> 0 -> 1` interval becomes a traceable pulse
observation. Without registered intent it is a `hazard_candidate`; a
steady-state-only contract yields a `transient_deviation`; a stable-signal or
pulse-width contract can confirm a hazard; and a matching pulse contract marks
the event as an `intentional_pulse`. Structural delay differences alone never
claim that a pulse was observed.

## Implementation status

The Rust workspace provides typed logic DAGs and rewrites, verified physical
cells, placement, electrical simulation, connectivity extraction, fanout-aware
multi-net routing with legality checks, and Java Data Pack export. Built-in
regressions cover half adders/subtractors, MUX, decoder, and full adder.

Serializable `Scenario` fixtures share an initial Minecraft snapshot, timed
input actions, observation points, final-state expectations, and pulse-width
contracts. The Rust runner and Mineflayer adapter produce compatible
redstone-tick traces and classify final strength, powered state, event tick,
within-tick order, pulse width, unsupported physics, and torch burnout
candidates separately.

The logical/physical boundary is deliberately public so a future MCP server can
build, inspect, optimize, and export a circuit incrementally with a user.

## Development

Minecraft-vs-simulator physical trace comparison is documented in
[`docs/physics-differential-testing.md`](docs/physics-differential-testing.md).

Rust 1.85 or newer is required. Live Minecraft integration additionally needs
Node.js 22 with npm and a Java 21 runtime. Verify the toolchain before building:

```bash
rustc --version
node --version
npm --version
java -version
```

Install Rust through rustup, use a supported Node.js distribution, and install
a Java 21 JDK or JRE through the operating system package manager. The Rust-only
workspace tests do not require Node.js, Java, or a Minecraft server.

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
from Git. See [`crates/dustroute-mcp/README.md`](crates/dustroute-mcp/README.md)
for the Java 1.21.11 server properties, explicit non-void superflat generator,
offline UUID/whitelist procedure, and visible bridge setup. See
[`crates/dustroute-mcp/mineflayer/e2e/README.md`](crates/dustroute-mcp/mineflayer/e2e/README.md)
for the automated live-test procedure.

Sanitized observation regression fixtures live under
`crates/dustroute-translate/tests/fixtures`. They cover intact and broken
wiring, direction errors, missing support, inversion, signal merges, delayed
paths, unsupported devices, and scan boundaries without requiring a running
Minecraft server.

## Reverse analysis

`analyze-snapshot` accepts a bounded Minecraft block snapshot and reconstructs
the directional redstone connectivity graph. Verified connections form
physical nets, while nearby disconnected nets remain separate fragments with
gap evidence for broken-circuit analysis. Bidirectional dust runs are projected
into signal components; repeaters, powered blocks, and torch-control edges
provide direction. Source and sink components become inferred terminals.

The CLI JSON report explicitly requests a bounded exhaustive truth table (up to
16 inferred inputs) and a Boolean expression for each output. Library and MCP
reverse analysis leave truth-table enumeration disabled unless requested; MCP
large-circuit requests are still bounded by row, settle-tick, estimated-work,
solver-iteration, and elapsed-time budgets and report structured
status/details when evidence is incomplete or a budget is exceeded. Partial
truth-table rows are discarded on any runtime limit. Responses classify the
physical evidence as combinational, timing-sensitive, stateful, or unknown
before a requested table is interpreted.
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
