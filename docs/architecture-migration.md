# Architecture migration

This document records the previous compatibility-preserving pass. The subsequent
[architecture cutover](architecture-cutover.md) supersedes its deferred items
and storage/API compatibility policy.

This implements the [readability audit](architecture-readability-audit.md) on
`codex/coauthoring-architecture`. Public behavior, JSON, stored records and
execution-profile revisions remain compatibility constraints. Tables are checked
Rust constants. Distinct physical executors and proof contracts remain distinct.

## Work order and acceptance

| Audit | Migration | Acceptance | Status |
| --- | --- | --- | --- |
| A03 | Typed internal requests and discovery/preview results | Internal decisions no longer parse their own public responses | Implemented: typed proposals, discovery and preview; source identity deferred |
| A05 | Named device-law bindings | Registry ordering cannot select a different law; pinned contexts unchanged | Implemented: named, const-checked device-law pins |
| A09 | Blueprint definitions, catalog, archive, validation and expansion modules | Existing imports and archive round trips unchanged | Implemented; regression suites pass |
| A10 | Optimizer planning, materialization, verification and routing modules | Existing candidate and verification results unchanged | Implemented; regression suites pass |
| A08 | Translation analysis responsibilities | Public facade retained; internal dependencies explicit | Implemented: topology, drivers, tables, network inference; crate-wide facade regrouping deferred |
| A04 | Validated readback boundary and explicit snapshot semantics | Only validated bridge responses produce confirmed observations | Implemented: validated observation capability; exporter redesign deferred |
| A06 | Block state access/mutation boundary | Raw observations retained; canonical fields remain consistent | Implemented: piston/wire mutation views; full Block redesign deferred |
| A02 | Operation ownership and lifecycle | Preview, expiration, execution and undo guards preserved | Implemented: placement and selection ownership; global registry deferred |
| A01 | MCP application responsibilities | Services have explicit dependencies rather than access to the entire MCP object | Implemented: discovery service, reports and request schema; remaining use cases deferred |
| A07 | Execution model and comparison-state boundaries | Existing normalization, ordering and physics unchanged | Implemented: named comparison record; executor abstraction deferred |
| A12 | Explicit storage contracts | Existing locking, TTL and durability guarantees preserved | Implemented: typed TTL namespaces; IO consolidation deferred |
| A11 | Bridge session and protocol boundaries | Wire protocol and observation/acknowledgement distinction preserved | Implemented: explicit bootstrap, session and RPC; write DTOs deferred; 14 offline tests pass |
| A13 | Shared test and observation helpers | Independent oracle retained; no implicit server startup on import | Implemented: shared offline harness and neutral Python helpers |

Small typed boundaries come first, then independent module changes, then stateful
service boundaries. An item with a material concern is recorded here and skipped
while independent work continues. A blocker affecting the whole migration, such
as a necessary shared-contract redesign, stops all work and is reported to the user.
Deferral is not reported as implementation completion.

## Validation

Use focused existing regression tests for each responsibility, then workspace
static checks and affected integration suites. Rust commands run individually
with `--offline --locked -j1`; tests use `--test-threads=1`. No live Minecraft
server or host changes are part of this migration.

## Implementation notes

The independent migrations below are implemented and validated. The items with
concerns were skipped as instructed and remain explicit future work. This is
not a claim that all architectural debt is gone. No overall blocker was found.

### Completed boundaries

- `service/circuit_capture.rs` receives only `BotBridge` and `McpPolicy` and
  returns typed discovery/evidence. The MCP facade owns player selection and
  response rendering. `service/circuit_reports/` separates world inventory, IR,
  revision and truth-table projections, without access to the MCP service.
  Internal capture does not call a public tool and parse its JSON.
  `service/requests.rs` owns the input schema. Update proposals are
  constructed field by field, checked by Rust.
- `service/placement_registry.rs` stores plan, dimension, source variant and
  applied state together. Selection geometry and dimension also share a lock.
  Progress records and persistent instances retain their distinct owners.
- `bridge::ValidatedRegion` is process-local and has no Deserialize. Only bridge
  validation produces it in production. `ConfirmedRegion` remains the public
  DTO for compatibility; operational observations use the validated type.
- `device_callback_law::law_id_for` resolves by block kind at const evaluation.
  Execution contexts bind named roles; device registry order no longer selects
  the callback law. The old law-array API and profile IDs remain unchanged.
- `block_state.rs` supplies piston and wire mutation views. Mechanical state,
  wire strength and wire shape update canonical fields and observed properties
  together. Unknown raw properties are retained, and synthetic blocks do not
  gain invented native observations.
- `piston_behavior/comparison.rs` names the root-comparison fields while keeping
  the v3 ordered-array bytes. Normalization, scheduler delivery and checkpoint
  identity checks are unchanged.
- `blueprint/{records,catalog,validation,expansion,archive}.rs` separates immutable
  definitions, append-only operations, integrity, recursive expansion and archive
  versions. Catalog maps remain private; existing public paths are re-exported.
- `world_reverse/{analysis,drivers,truth_table,network}.rs` separates topology,
  model input mutation, bounded compatibility execution and inferred functions.
  `macro_realize/` separates boundary mapping, planning, routing, ownership,
  structural checks, materialization, steady-state and transition verification.
- `PlanRecordKind` limits the TTL store to repair plans and circuit revisions.
  Blueprint and instance stores keep their existing contracts.
- Mineflayer bootstrap, bot session and RPC framing have separate modules.
  Importing the entry point does not parse environment configuration, create a
  bot, listen on a socket or install signal handlers. Session dependencies are
  explicit and timers/clock/recording state belong to one session.
- `service/test_support.rs` owns the offline MCP session and construction
  transport fixture. `fixture_geometry.py` and `observation_records.py` contain
  common representation helpers; feature scripts depend on them directly.
  Live capture and simulator projections remain separate sources of evidence.

### Concerns deferred while independent work continued

1. **A01/A02: full application-service and global operation-registry migration.**
   Persistent repair records are loaded with TTL on every operation; Blueprint
   proposals and Assembly instances are durable and locked independently; live
   placement plans are memory-only. A single dispatch/storage abstraction could
   alter expiry, consumed-token behavior or lock order. Coherent placement
   records, discovery, request schemas and report projections move now.
   Remaining mutation, optimization and repair orchestration in `service.rs`
   is still outstanding.
2. **A03/A04: source identity and native state export.** Saved Assembly identities
   compare selected JSON fields against newly reviewed sources. Migrating this
   persistence boundary needs explicit legacy-schema cases. Generic Java export
   initializes some devices, whereas electrical export preserves exact native
   state. Combining exporters without modeling that distinction could change
   construction. Both remain explicit existing paths for now.
3. **A05/A06: one block table or full Block enum.** Physical admission, connectivity,
   compatibility execution and proof profiles deliberately admit different
   capabilities. Raw observations also include unsupported identities. A single
   supported flag or enum conversion risks archive/hash/checkpoint changes.
   Kind-specific mutation views and named law bindings are the safe migration.
4. **A07: moving adapter-specific comparison out of the generic runtime.** It
   depends on private scheduler state and exact normalization. Widening scheduler
   access merely to move a file would weaken the boundary. Only encoding is
   isolated; a new adapter comparison interface needs a separate design.
5. **A12: a common atomic writer.** TTL plans use rename without fsync, Blueprint
   archives sync their file, and instance records sync file and directory under
   their own locks. No durability guarantee or failure-cleanup ordering is
   silently changed. Their IO remains separate.
6. **A13: splitting the long end-to-end construction scenario.** The shared
   transport/session harness is extracted, but the linked adoption/restart/
   recovery lifecycle stays intact so its cross-step regressions remain covered.
7. **A08: regrouping the entire public translate facade.** The existing root
   re-exports remain. Analysis, behavior verification and construction share
   types but use different evidence and execution models. Adding blanket
   facade re-exports before deciding ownership would create another overlapping
   entry layer. The independently separable `world_reverse` implementation moves;
   broader facade ownership and `analysis.rs` decomposition remain future work.
8. **A11: typed write requests and acknowledgements across Rust and JavaScript.**
   Existing public write methods accept/return `Value`. A replacement needs
   compatibility cases for validation errors and partial writes, as well as the
   acknowledgement/readback distinction. This migration isolates transport and
   session ownership while retaining that wire contract; it does not claim all
   bridge messages are now typed.

These are recorded deferrals under the user instruction, not blockers to the
independent migrations above. They remain future work and must not be described
as completed architecture replacement.

### Execution and storage contracts to keep separate

The retained model paths are intentional; moving their implementations must not
make their proofs interchangeable:

| Entry | Current responsibility | Boundary retained |
| --- | --- | --- |
| `PhysicsEngine` | Fixed 1×2 piston-door execution and its declared laws | Engine-specific model and evidence |
| `RedstoneTickSimulator` | Compatibility truth tables, scenarios and optimization checks | Its settle/budget and supported-model contract |
| `SynchronousWorldRuntime` | Moving-world behavior, electrical construction and validation | Pinned context, shared queue, carriers and callback ordering |

The common callback implementation still resides in `piston_runtime/devices.rs`,
and `PistonEvent` still includes non-piston callbacks. Renaming/repartitioning
those public runtime concepts is outstanding alongside A07's adapter boundary.

| Store | Lifetime and ownership | Existing write durability |
| --- | --- | --- |
| `PlanStateStore` | Expendable repair plans and circuit revisions with TTL | Temporary file and rename; no fsync |
| Blueprint catalog | Durable, player-scoped immutable definitions and adoption records | Catalog transaction lock, atomic replacement and file sync |
| Assembly instance registry | Durable instance history and guarded execution attempts | Instance/attempt checks, file and directory sync |

These contracts explain why a common operation map or atomic writer is not
introduced in this pass. Typed TTL namespaces cannot select either durable store.

### Verification results

- Final Rust MCP pass: 91 tests passed after registry, harness, request and report
  extraction. This includes public routing, adoption/restart, preview guards,
  construction/undo, partial writes and lost replies through offline transports.
- JavaScript: 14 offline tests passed, including import without startup,
  independent sessions/reconnect state, RPC IDs/errors/metrics and readback.
- Python: six retained server-readback evidence tests passed; all 12 moved helper
  bodies are AST-identical to the previous revision. No server started.
- Workspace/all-target Clippy passed with warnings denied; formatting and diff
  whitespace checks passed.
- Minecraft, library and optimizer suites: 463 tests passed, including doctests.
- Translation analysis: 11 tests passed, including interface inference, truth
  tables and fail-closed work budgets.
- Construction: 10 tests passed across command-placement regression, electrical
  pistons, adhesion, stairs and flying machines.

Total affected Rust checks: 575 passed (including doctests); JavaScript: 14;
Python: 6. No ignored tests or failures in these runs. This is the affected-suite
selection, not a claim to have run every workspace test or a new live trial.

Reproducible checks (from the repository root unless indicated):

```sh
cargo fmt --all -- --check
cargo clippy --offline --locked -j1 --workspace --all-targets -- -D warnings
cargo test --offline --locked -j1 -p dustroute-minecraft -p dustroute-library -p dustroute-optimize -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-translate --lib world_reverse:: -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-translate --test command_placement_regression --test electrical_piston_construction --test adhesion_construction --test stair_construction --test flying_machine_construction -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-mcp --lib -- --test-threads=1
npm --prefix crates/dustroute-mcp/mineflayer test
```

Python check: run `python3 -m unittest test_server_readback` from `tools/`.
The AST equality comparison was a one-time comparison with the pre-migration
functions, not a shared implementation of the physical comparison oracle.
