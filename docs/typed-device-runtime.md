# Typed device runtime roadmap

The subsequent [integration roadmap](device-integration-roadmap.md) extends this
foundation. The current implementation adds repeater callback definitions,
computed tick priorities, installed shape queries, output-before-shape writes
and explicit dust connection rules. The approved physical-admission prerequisite
shares checked geometry and separates executor support from classification.
Execution/review uses v10 and device programs v4; the v8/v2 validation results
below describe the completed foundation step.

The new repeater bindings read `Delay` (1..4) as a construction-only property;
callback assignments to it fail const validation. Observed repeaters require
explicit, consistent `powered`, `locked`, `delay` and horizontal `facing`, with
support below. Synthetic repeaters retain an omitted-lock default of false;
malformed explicit locks are rejected. `TickCollected` samples the current ready
batch, whereas `TickQueued` samples future reservations. Installed shape handlers
can read gate power; pre-insertion shape handlers still cannot.

The next step extends the shared device vocabulary, with Rust constants as the
authoring format. It does not claim support for additional Minecraft blocks.

All five steps below are complete. No out-of-scope prerequisite was needed.

1. Replace the three embedded device JSON definitions and their local callback
   laws with checked Rust constants. Preserve exported Law programs and existing
   decisions. Reject invalid names, ranges and lifecycle contracts during const
   evaluation; exercise these guarantees with compile-fail examples.
2. Add declared Boolean properties and analog power (0–15), sampled from captured
   state and committed together in one block write before nested notifications.
   Add analog world queries and signal emission through the same shared executor.
3. Select a definition using concrete identity and declared state predicates,
   with an explicit synthetic default. Reject ambiguous registries and unknown
   variants. Continuations retain the selected definition and captured state.
4. Verify the new capabilities with test-only definitions through shared runtime
   operations. Recheck existing devices, intermediate checkpoints, construction
   and retained Minecraft observations. Version the execution contract.
5. Document authoring, compile-time guarantees and remaining runtime checks.

Outside this scope: complete copper-bulb/comparator implementations, torch
history integration, inventory/entity models, arbitrary runtime-loaded programs,
and new public MCP operations. Stop and report if an outside prerequisite should
be implemented first. Minecraft correctness still needs independent evidence;
compilation proves the definition contract, not parity with Minecraft.

Work continues on `codex/data-driven-block-runtime`. Build/test workloads use one
Rust job and one test thread; no Minecraft server is needed for this migration.

## Authoring and compilation

In the foundation revision, `src/device_program/builtin_laws.rs` owns the three callback rules as `StaticLaw`
constants. `builtins.rs` binds them to physical queries and ordered operations
using `DeviceSpec { ... }.checked()`. `schema::registry([...])` checks the complete
built-in registry in a const initializer. No production device definition or
callback law is loaded from JSON. Frozen v1 law JSON is retained only as a test
oracle, and exported catalog `LawProgram` bodies and their IDs remain unchanged.

The Rust enums describe callbacks, properties, queries, orientations, signal
sources and operations. Law column names are still readable string literals;
const validation resolves every name and checks its range, so misspellings in
constant definitions are compile errors. Definitions are compiled into finite
lookup tables on first use through the existing `FiniteLaw` implementation.
The tables themselves are not embedded binaries or generated Rust source.

For example, one atomic operation may update independent Boolean and analog
state, provided the properties and bounded outputs are declared by its definition:

```rust,ignore
Operation::Write {
    when: Binding::Output("write"),
    values: &[
        (Property::Bool(BoolProperty::Powered), Binding::Output("powered")),
        (Property::Bool(BoolProperty::Lit), Binding::Output("lit")),
        (Property::Power, Binding::Output("level")),
    ],
    notifications: WriteNotifications::NeighborsAndShapes,
}
```

Const validation rejects unknown/duplicate law columns, out-of-range assignments,
non-Boolean conditions, input domains over 8,192 rows, invalid query bindings,
undeclared or duplicate property writes, incompatible notification orientation,
timers without a tick handler, and mutation during removal/pre-write shape
callbacks. One callback may contain at most one atomic state write. Expressions
use conservative bounds; conditions do not narrow a numeric expression's type.
`checked()` called outside a const initializer panics on invalid specifications;
the built-in registry always uses const initialization.

## State and selection

The current property vocabulary is `Powered`, `Lit`, `Open`, `Locked` (Boolean)
and `Power` (0–15). This is not an arbitrary property-name escape hatch. The
definition explicitly identifies the primary Boolean mirrored by `Block.powered`;
secondary Booleans use named block properties, including on synthetic blocks.
`Power` uses `Block.power_level`. Observed state must agree with these canonical
fields. A multi-property write builds and validates one replacement before
publishing it, then delivers nested callbacks before the next operation.

`ReceivingLevel` reads the maximum signal over six neighbors, including conductor
power. `SideLevel` reads a specified absolute face. `SignalLevel::Analog` emits
the stored 0–15 value through the same directional/attached emission operation
used by Boolean devices. Unknown surrounding space and inconsistent observations
remain runtime errors. There is no inventory-based comparator query yet.

Selection uses `BlockKind`, concrete Minecraft identity and declared state
predicates together. A synthetic block requires an explicitly admitted definition.
Registry validation rejects intersecting selectors (including synthetic defaults)
unless their state predicates are disjoint. All variants of a kind must agree
on the canonical primary Boolean. Unknown materials never inherit another
material's rule. Material-specific duration can be expressed by binding a
different constant to an input of the same bounded law. New variants still need
registry/profile admission and independent observation/placement verification.

Continuations retain the captured block, selected definition, resolved operations
and instruction index. If the write changes the selected state variant, the
remaining operations belong to the captured definition. A later scheduled tick
selects from the then-current physical state, guarded by concrete block identity.
These operations are included in exact checkpoints.

The foundation used execution/root-exploration **v8**, pinning device-program revision
**v2**. It rejected previous v7 contexts and checkpoints. No old approval is
silently upgraded; start review from explicit fresh state. Catalog schemas and
the three existing immutable callback Law bodies are unchanged.

## Evidence boundary

Test-only definitions exercise independent powered/lit state, analog subtraction,
all 16 signal values, directional output, state-dependent selection after a
write, delayed callbacks and restoration at each microstep. They run the shared
sampler, effect executor and scheduler through a private test adapter, and are
not registered as Minecraft blocks. Separate tests check material selection,
unknown identities, observation inconsistencies and missing surrounding space.

This extension does not add copper bulbs, comparators, wooden-button arrows or
inventory devices to the supported block set. Local history, item/entity effects,
relative comparator-facing queries and arbitrary persistent registers still
need shared primitives. Compile-time validity does not establish Minecraft
behavioral equivalence or make a simulation-only input publicly available in MCP.

## Validation

- **309 distinct Rust tests passed**: the complete Minecraft crate, scoped Law
  and execution-context tests, construction/adoption/relocation tests and 15
  public MCP tests using a transport stub. The full workspace test suite was
  not rerun.
- **11 compile-fail doctests passed**, including eight new examples checking
  typed properties, unknown names, numeric bounds, overlapping variants and
  invalid lifecycle operations.
- **14 Python tests passed**, including retained reference-door and transient
  comparisons using the rebuilt simulator. No live server or new trial ran.
- Workspace/all-target Clippy with warnings denied, formatting and diff checks
  passed. Rust jobs and test threads were limited to one.
- The capability test covers both initial lit states, all 16 signal levels and
  synthetic/observed representation (64 combinations), with exact restoration
  after every microstep. Follow-up tests also verify the final single-source
  Law registration and unchanged published programs.

[Commands, results and implementation hashes](evidence/typed-device-migration-20260927.json)
record the validation and its scope.
