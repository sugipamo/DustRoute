# Stabilization and legacy paths

Initial audit: 2026-09-26. Stabilization preserves the declared capabilities and
removes redundant execution paths. A historical name alone does not establish
that a path is unused. This inventory covers MCP dispatch/storage and the
Blueprint/piston integration boundaries; it is not a claim that every obsolete
path in the workspace has been found.

## Removed in the first pass

| Path | Problem | Result |
| --- | --- | --- |
| `DustRouteMcp.repair_plans` memory fallback | A disk record could expire or disappear and still authorize preview/apply/undo from memory. Restart changed the outcome. Repair and optimization share this storage. | The scoped disk store is authoritative. Missing/expired plans are unavailable regardless of restart. The stored format is unchanged. |
| `invoke_circuit_placement`, `undo_circuit_placement`, `invoke_repair`, `undo_repair` | Private forwarding methods duplicated the already unified public operation entry points; they had no other callers or independent validation. | `invoke_operation` / `undo_operation` dispatch directly to the existing checked mutation methods. Public tools and their schemas are unchanged. |
| Ignored repair-plan load errors during operation dispatch | `.ok().flatten()` hid an unreadable record as an unknown operation and continued dispatch. | Preview/apply/undo return `internal` for a load error before contacting the bridge. Missing/expired IDs still return `not_found`. |
| Superseded user guidance | The migration-baseline table appeared current, and some public feature tables described only original-coordinate Assembly placement. | The old table is marked historical and links to current evidence; the public guide describes both supported Assembly placement routes. |

Implementation: [MCP service](../crates/dustroute-mcp/src/service.rs),
[plan store](../crates/dustroute-mcp/src/state.rs).
Repair/optimization retention is measured from the last save, normally one
hour. Preview and successful apply/undo save again; reads do not extend it.
Expiry prevents admission of a new action, including undo; it does not cancel
an action already running. See [execution and recovery](mcp-public-features.md#execution-and-recovery).

## Callback-profile retirement

The user explicitly ended the requirement to reproduce obsolete piston model
behavior. Java 1.21.11 behavior and actual captured observations are the reference.
The following execution paths are removed, rather than retained behind aliases:

- Horizontal-only, vertical-only and isolated-direct adapters, their runtime
  classes, constructors and direction-specific input scheduling helpers.
- Legacy callback power queries, repeater ticks, lamp/wire notification handlers,
  direct-only admission gates and the now-unused vertical power helper.
- Direction/profile selection in the behavior explorer. It now owns an
  `ElectricalPistonRuntime` directly; construction uses `new_piston_runtime`.
- The obsolete direct-input law and four vertical-only law aliases in built-ins.
- Legacy-output regression generators/tests. Useful motion, identity, mixed-world
  and interruption cases now exercise electrical execution. Raw model captures
  and real observed captures remain unchanged and retain their provenance.
- The old 1×2 interruption runner, comparator and dedicated live actor. Shared
  private-server helpers moved unchanged to `tools/instrumented_server.py` for
  the current mixed-piston and construction observers.

Placement review and location samples report the electrical profile; the old
hard-coded horizontal diagnostic identity is gone. Public schema enums only
advertise the supported electrical callback profile. The synchronous engine still
checks both profile and concrete adapter identity when restoring a state.

### Saved data

Retired callback IDs are `dustroute.horizontal-piston-callbacks.java-1-21-11.v1`,
`dustroute.vertical-piston-callbacks.java-1-21-11.v1` and
`dustroute.piston-direct-callbacks.java-1-21-11.v1`. Their corresponding
`horizontal-piston-root-exploration.v1`, `vertical-piston-root-exploration.v1`
and `piston-direct-root-exploration.v1` context IDs (with the `dustroute.` prefix)
are retired too. The supported IDs are listed in the
[unified runtime contract](unified-piston-runtime.md).

The removed callback world IDs and root-exploration IDs are rejected during
loading. They are not aliases or implicit defaults for the electrical model.
This is an intentional compatibility break: an archive containing a retired
context cannot be loaded by the typed reader, including when that context is in
old proposal history. The application reports an error; it does not clear,
rewrite, convert or drop the file. Existing electrical archives continue through
the same fresh validation, adoption and placement checks.

Recreate an obsolete request from explicitly declared initial conditions under
the current electrical context and revalidate its law requirements and children.
An old pass or checkpoint never authorizes new execution. Automatic conversion
of old archives/checkpoints is outside this cleanup.

## Retained paths and removal prerequisites

| Path | Current consumer or obligation | Boundary for future cleanup |
| --- | --- | --- |
| `RedstoneTickSimulator` | Hypothetical revision simulation, behavior/scenario/physics tracing and optimization still use it. The electrical piston runtime does not replace all its device/model coverage. | Migrate consumers only with a declared equivalent model contract and evidence. Expanding device support is separate feature work. |
| Blueprint-to-cell projection | Built-in cells and the compiler still consume `PhysicalCell`. The projection rejects obligations it cannot carry. | Move consumers to equivalent Blueprint-aware APIs before removing the projection; do not discard type/Law/behavior requirements to make conversion succeed. |
| General placement, fixed 1×2 piston placement and grounded revision reflection | They cover different existing operations from empty-target custom electrical construction. | Share mechanical helpers only when their live validation, source provenance and recovery rules remain intact. An empty-site constructor is not a replacement for existing-world diffs or fixed-door operation. |
| Other catalog/Revision readers and optional historical fields | Current electrical and fixed-geometry records, including optional observation fields, still have active readers. | Retain their validation and evidence checks. The explicitly approved retired callback-context break is described above. |
| Legacy event projections and public response fields | Signal handlers and external clients still consume coordinate events, `error` text, fragment counts and older transition views. | Establish consumer migration and a versioned contract before deleting them. Internal typed events alone do not prove old outputs unused. |
| Historical model captures and live evidence | They distinguish old model predictions, diagnosed differences and actual observations. | Keep provenance and source hashes; old model output is no longer a current acceptance requirement. |

Key source boundaries:

- [Piston runtime](../crates/dustroute-minecraft/src/time/piston_runtime/mod.rs),
  [behavior explorer](../crates/dustroute-translate/src/runtime_behavior.rs),
  [saved runtime contexts](../crates/dustroute-library/src/runtime_behavior.rs).
- [Fixed-geometry simulator](../crates/dustroute-translate/src/sim.rs),
  [scenario caller](../crates/dustroute-translate/src/scenario.rs),
  [optimization caller](../crates/dustroute-optimize/src/realize.rs).
- [Cell consumers](../crates/dustroute-translate/src/cells.rs),
  [Revision reader](../crates/dustroute-mcp/src/revision.rs),
  [public response compatibility](../crates/dustroute-mcp/src/api.rs),
  [event engine](../crates/dustroute-minecraft/src/time/engine.rs).

## Verification and next pass

The MCP regression `repair_operations_reject_unavailable_saved_state_before_and_after_restart`
covers expired, deleted and corrupt records, before and after apply, using both
the original service and a new service instance. Preview, apply and undo must
reject before any bridge connection. Expiry is set in the stored timestamp;
the test does not wait for wall-clock expiry or change the system clock.

`repairs_and_undoes_a_broken_wire_through_the_mcp_workflow` checks the working
preview/apply/undo lifecycle across service recreation and verifies that an
existing service sees the new applied/undone state. Existing placement and tool
profile checks cover the simplified dispatch. These tests use a simulated
bridge; they do not constitute a new live Minecraft trial.

First-pass result: all 80 MCP tests passed (0 failed/ignored); package formatting,
Clippy for all MCP targets with warnings denied, and `git diff --check` passed.
Local file links in the five touched documentation files were checked. No
Minecraft server or live world was used for this pass.

Reproduce the bounded checks with:

```bash
cargo fmt --package dustroute-mcp --check
cargo test --offline --locked -p dustroute-mcp -j 1 -- --test-threads=2
cargo clippy --offline --locked -p dustroute-mcp --all-targets -j 1 -- -D warnings
git diff --check
```

The callback retirement is checked across the workspace, including recorded
server-applied electrical observations, mixed-direction motion and restoration,
law/context loading, Blueprint adoption, construction and MCP lifecycle tests.
No new live trial is implied by replaying a recorded capture.

Old low-layer initial snapshots omit wire arms or leave electrical query
positions outside their known region. They no longer serve as current runtime
pass/fail predictions. Regressions now require explicit rejection of that
insufficient evidence; neither wire connections nor additional known air are
invented. Current replay coverage uses ten complete server-observed electrical
fixtures and their recorded prefixes. The historical captures remain intact.

The workspace run also exposed a test that expected all five torch/support
placements to finish under the default 30-second search deadline. That
completeness regression now explicitly allows 180 seconds for its five bounded
proofs; every candidate must still pass. The product's default search deadline,
proof limits and the separate test of incomplete enumeration are unchanged.

Callback-retirement verification on 2026-09-26:

- The full workspace run completed: 846 passed, 8 failed in five targets, and
  one existing manual scalability measurement was ignored. The failures were
  migrated-test assumptions about incomplete snapshots, the earlier rejection
  of an absent retired law, and the torch-enumeration wall-clock limit above.
- After those test corrections, the affected library targets passed 13 tests,
  the three affected translation targets passed 18 tests, and the bounded
  five-orientation enumeration test passed. The old four-test partial-snapshot
  replay suite is now two explicit insufficient-evidence rejection tests. A
  later simplification of the context round-trip test was rechecked separately.
  The expensive full workspace suite was not repeated after these test edits.
- The full run included all 80 MCP tests and ten recorded electrical fixtures
  with their observed prefixes. No live server/world was started or changed.
- Workspace `check --all-targets`, final `clippy --all-targets -D warnings`,
  formatting and whitespace checks passed. Clippy's first pass found a
  single-element loop left by profile removal; the redundant loop was removed.
- The three Python observation modules passed syntax/import checks without
  starting a subprocess or connecting to a server. The moved server constructor
  and lifecycle bodies were checked for structural equivalence. Local Markdown
  file links were checked.

Commands, log hashes and final source/capture hashes are retained in
[the cleanup verification record](evidence/legacy-path-removal-20260926.json).

```bash
cargo test --offline --locked --workspace --no-fail-fast -j 1 -- --test-threads=2
cargo clippy --offline --locked --workspace --all-targets -j 1 -- -D warnings
cargo fmt --all --check
```

Remaining distinct active paths in the inventory are not deleted merely because
their names say compatibility. Replacing the fixed-geometry device coverage,
existing-world diffs or current response contracts is separate work. If removing
one first requires new device support, a new law, automatic migration or a
further breaking contract change, stop and report that prerequisite.

## Piston cleanup follow-up, 2026-09-27

The [v6 implementation cleanup](piston-code-organization.md) removes the obsolete
construction-order experiment and separates command callbacks, construction
planning, MCP observation/validation and per-stage execution. Shared tick
reservation and typed comparisons replace duplicated machinery. Active bounded
models and existing-world placement contracts remain distinct; evidence and
regression fixtures remain intact. See that record for the scoped inventory and
verification, without treating this as a workspace-wide legacy removal claim.
