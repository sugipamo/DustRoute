# Architecture cutover

This continues `architecture-migration.md` with explicit authorization to retire
old formats and public paths. Temporary build failures during migration are
allowed. The delivered tree must compile and pass the affected regressions.
Existing saved data is not deleted automatically. Removed schemas are rejected
explicitly; they are never silently reinterpreted as current evidence.

## Work order

| Step | Responsibility | Completion condition |
| --- | --- | --- |
| 1 | Rust/JS mutation protocol | Typed requests and submission receipts; shared contract examples; invalid batches rejected before writing; readback remains separate |
| 2 | Source records and native states | Typed provenance; explicit initialization versus literal export; retired catalog/instance formats rejected |
| 3 | Storage | One atomic replacement implementation with explicit durability and caller-owned locks |
| 4 | Operations and application workflows | One owner for transient operation variants; explicit lifecycle guards; workflows separated from MCP presentation |
| 5 | Block declarations and state | Distinct capability contracts and kind-specific state boundaries; no universal supported flag |
| 6 | Runtime comparison | Adapter-owned comparison semantics using a controlled generic runtime boundary; no scheduler internals made public |
| 7 | Translation entry points | Responsibilities have designated modules; redundant compatibility exports removed |
| 8 | Regression organization | Focused construction scenarios plus a representative restart/recovery lifecycle; affected Rust/JS/Python checks pass |

No new Minecraft feature, live world mutation, entity support or survival
builder is required. If such prerequisite work or a system-wide blocker appears,
stop and report it. Compatibility work alone is no longer a reason to defer.

## Status

All eight implementation areas are complete, with the affected regressions and
workspace static checks passing. No scope-expanding prerequisite or overall
blocker was found. This does not claim that all architectural debt is removed.
The [follow-up verification and retirement](architecture-cutover-validation.md)
adds omitted regression suites and removes the remaining update/store formats.

## Ownership after the cutover

- `bridge_protocol` owns typed command/physical batches and submission receipts.
  `NativeBlockState` parses only literal state syntax. Both Rust and JS consume
  the shared mutation fixture. JS validates the complete batch before effects.
- `SourceIdentity` owns the exact adopted record, context and catalog data.
  Catalog transactions return that Rust type directly to construction review.
  Grounded reflection also receives a typed `GroundedSource` rather than parsing
  its own tool response; its diagnostic report is presentation data only.
  Unrelated append-only definitions may be added; replacing a pinned definition
  cannot pass the source comparison. Stored data still cannot create a proof.
- `minecraft_export::native_block_state` requires an `ExportPurpose`:
  `InitialPlacement` preserves initialization policy; `ExactElectrical` preserves
  admitted device/wire state. `initial_java_block_state` is the explicit string
  convenience for initialization. Exact snapshot export no longer parses its own
  generated command string or patches its properties afterward.
- `OperationPlans` owns all five transient plan variants under one identity map.
  Typed handles select one family; dispatch reads the kind once. TTL repairs,
  durable Blueprint/instance records and progress history remain separate owners.
  Invocation and revision enums consume attempts before writes. Repair lifecycle
  is persisted before submission. Lost replies cannot make a plan reusable;
  rollback submission and rollback readback are distinct results. Repair preview
  persistence shares the mutation lock, so it cannot reset an in-flight attempt.
- `WorldEditor` has only bridge and policy dependencies. `PlacementWorkflow`,
  `RepairWorkflow`, `OptimizationWorkflow` and `AssemblyService` declare their
  stores, operation owners and world-effect dependencies explicitly. Optimization
  receives an immutable circuit and has no bridge capability. Services return
  structured reports; MCP keeps request routing, player resolution, text framing
  and default response metadata. These services stay in the MCP crate because
  their transport/persistence contracts belong to this adapter;
  moving files alone would not make them generic application APIs.
- `BlockKind::contract()` exhaustively declares analysis capabilities and port
  layout. Scene projection consumes that declaration. Unknown raw observations
  and coarse passive evidence keep their existing rejection/downgrade rules.
  `ElectricalBlock` exposes distinct validated wire, device, piston, head and
  carrier states without making raw observations unrepresentable. `DeviceState`
  binds declared property reads to a validated borrowed block. These views have
  no Deserialize and do not grant support, construction or scheduler admission.
- `RootComparisonAdapter` declares the comparison revision and allowed queued
  payloads. The scheduler retains private queues, clocks, carriers and restore
  identity checks. `RootBehaviorState<P>` implements the established normalization
  behind that opt-in boundary. The electrical adapter owns `PistonBehaviorState`
  and `BEHAVIOR_COMPARISON`; the canonical comparison bytes and physical rules
  are unchanged. Independent execution/proof models are not combined.
- `storage::replace` is the one temporary-file/rename implementation. TTL records
  choose `ReplaceOnly`; catalogs and Assembly journals choose `FileAndDirectory`.
  Catalog durability is explicitly strengthened from file sync to file **and
  directory** sync. Callers retain their own locks, size limits, TTL and scope.
  An error after rename can still mean replacement happened; it is not rollback.
- `translate` exposes functions through their owning modules, with `world` and
  `ir` aliases for domain crates. The former flat root exports are removed.
  Analysis classification, scenarios, explanation and equivalence are separate
  implementation modules. No new parallel compatibility facade is introduced.
- Construction tests have independent persisted fixtures for admission, human
  damage diagnosis, removal guards, same-process undo and interrupted recovery.
  The last scenario retains the full restart/reconstruction journal chain. The
  fake transport supplies receipts/readbacks; it remains separate from live
  Minecraft conformance evidence.

## Retired formats and APIs

This is a coordinated source/API/storage cutover, not an automatic upgrader.
Existing user files are not rewritten or deleted by this migration.

| Removed contract | Current contract | Action |
| --- | --- | --- |
| Blueprint catalog schemas v1–v12 | `dustroute.blueprint-catalog.v13` | Preserve or discard old archives explicitly; recreate definitions and freshly review/adopt intended Assemblies |
| Blueprint update archive schemas v1–v4 | `dustroute.blueprint-updates.v5` for every history | Recreate/freshly review proposals; content-dependent version selection has been removed |
| Player-scoped MCP Blueprint store v1 or missing grounding map | `dustroute.mcp-blueprints.v2` with a required map | Preserve/move the old store outside the active state directory and recreate/freshly review; no automatic reset or upgrade |
| Placed Assembly schemas v1–v3 | `dustroute.placed-assembly.v4` with typed source identity | Old instance records are rejected, including list operations that encounter them; preserve/move them outside the active registry before starting a fresh registry |
| Repair archives with `previewed`/`applied` booleans or missing contract/boundary fields | Required `lifecycle`, contract and boundary records | Diagnose again and create a new repair proposal; do not retry an old token |
| RPC `write_blocks` / `place_physical_blocks` | `submit_command_batch` / `submit_physical_batch`, envelope `dustroute.bridge-mutation.v1` | Update Rust MCP and JS Bridge together; a mismatched peer rejects the unknown RPC before entering its write handler |
| Bridge mutation `Value` inputs/results | `CommandWrite` / `PhysicalChange` and typed submission receipts | Build typed batches; still obtain independent readback |
| `translate::java_block_state` and flat root function/type exports | `minecraft_export::initial_java_block_state`, purpose-specific native export and owning-module imports | Update imports; no deprecated forwarding symbols remain |
| Runtime-owned `PistonBehaviorState::COMPARISON` | Electrical adapter alias and `piston_runtime::BEHAVIOR_COMPARISON` | Import through the adapter; generic comparison stays scheduler-owned |

Historical captured evidence files remain unchanged and are not importable as
current validation proof. Bundled catalog containers use v13; immutable definition
IDs such as `*.v1` are not archive versions and are not renamed. Old saved Blueprint update archives
that contain retired catalogs also fail loading; saved passes are never promoted
by changing only the header. Keep old data outside the active state directory if
it is useful for reference, or discard it deliberately once no longer needed.

Discarding history does **not** remove or restore any blocks in Minecraft. A
fresh construction still requires a freshly reviewed source and an empty,
completely observed target. Existing live structures whose old instance journal
was discarded require fresh observation and an explicit recovery decision;
this cutover does not invent durable ownership or recovery evidence for them.
The mutation lock for expendable repairs remains per service process; it is not
promoted to the cross-process transaction guarantee of the Assembly registry.

## Verification

Final affected Rust checks: **807 passed**, including doctests. JavaScript:
**16 passed**. Python: **6 passed**. No tests were ignored in these runs.
Workspace/all-target Clippy passed with warnings denied; formatting and diff
whitespace checks passed.

| Selection | Passed | Evidence covered |
| --- | ---: | --- |
| Minecraft, library and optimizer, including doctests | 464 | Runtime comparison/restore, declared states, current catalog round trips, retired schema rejection and optimization |
| Translation library unit tests | 160 | Analysis, native export purposes, diagnostic projections and observation boundaries |
| Thirteen translation integration suites | 77 | Command placement, electrical pistons, adhesion, stairs, flying machines, door behavior and fresh adoption after reload |
| Translation doctests | 6 | Raw state cannot bypass validated placement/policy boundaries through the public API |
| MCP library | 100 | Typed transport, scoped stores, invalid/partial observations, explicit operation states, restart/reconstruction/removal and repair transport failure |

The ordinary-door negative test still rejects the inverted control binding.
Its stale diagnostic-text assertion was corrected to the existing generic
initial-observation message and now also checks the counterexample's held input
and disagreement. The verifier implementation was not changed for that correction.

Reproducible commands from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --offline --locked -j1 --workspace --all-targets -- -D warnings
cargo test --offline --locked -j1 -p dustroute-minecraft -p dustroute-library -p dustroute-optimize -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-translate --lib -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-translate --test command_placement_regression --test electrical_piston_construction --test adhesion_construction --test stair_construction --test flying_machine_construction --test passive_shapes --test behavior_types --test blueprint_updates --test ordinary_reference_door --test physical_periodic --test promotion --test repeated_settling_adoption --test runtime_adoption -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-translate --doc -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-mcp --lib -- --test-threads=1
npm --prefix crates/dustroute-mcp/mineflayer test
```

Run `python3 -m unittest test_server_readback` from `tools/`. Rust commands were
run one at a time. This is the affected-suite selection, not every workspace
test. The bridge tests use offline transports. No Minecraft server was started
and no live world was changed for this cutover.
