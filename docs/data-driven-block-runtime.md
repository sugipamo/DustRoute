# Data-driven block execution migration

This records the completed v7 migration. Current v8 execution uses
[Rust constant definitions](typed-device-runtime.md), replacing the device JSON
authoring described below and adding multi-property and analog primitives.

Started 2026-09-27 on `codex/data-driven-block-runtime` from develop
`8e56da75c25900ee25e5b4ef6dbeb8ee374e69f4`.

## Goal and scope

Make local devices execute through shared input sampling, finite-law decisions,
and ordered world effects. Reuse the existing synchronous runtime, scheduler,
checkpoint ownership and `LawProgram`/`FiniteLaw`; do not create another world
engine. Migrate the lamp and observer, including command preprocessing and
moving-observer lifecycle. Add a stone button with explicit use and automatic
release as the extension exercise. A definition must describe its queries,
law bindings and effect ordering, rather than merely naming a Rust handler.

The Minecraft Java 1.21.11 behavior and retained observations are authoritative.
Preserve observable writes, callback order and future behavior for existing
supported circuits. Internal continuation records may change under a new
execution revision; no checkpoint conversion or automatic old-proof adoption.

## Roadmap

| Step | Work | Acceptance | Status |
| --- | --- | --- | --- |
| 1 | Audit current device laws, scheduler, construction and saved-state boundaries; record target button behavior | Explicit query/effect vocabulary and version/scope decisions | Complete |
| 2 | Compile declarative device definitions against finite laws; execute ordered effects with captured, resumable state | Invalid definitions rejected; executor contains no lamp/observer/button-specific callback branches | Complete |
| 3 | Move lamp and observer callbacks, pre-write sampling and arrival/removal into the common path | Existing device, construction, moving-observer and reference-door regressions agree with retained Java evidence | Complete |
| 4 | Register stone-button use/release and electrical traits through the same path | Directional/support power, repeated use, release, removal and restoration tests; no new device-specific event/continuation variant | Complete |
| 5 | Update execution pins, catalog dependencies and documentation; run scoped checks and replay evidence | Saved incompatible execution records rejected; current review/placement regressions pass; evidence and remaining boundaries documented | Complete |

## Design boundary

- Tables cover finite local facts. World traversal, block mutation, notification
  delivery and motion planning stay shared algorithms, not a global truth table.
- Definitions bind bounded law inputs to explicit queries and outputs to an
  ordered effect program. A suspended program retains its revision, captured
  block state, resolved decision and next step in runtime state.
- Writes drain their nested notifications before later program steps. Queued
  ticks retain position/concrete-block guards and the ready/pending distinction.
- Unknown blocks, incomplete observations and unimplemented effects remain
  errors. No default behavior or changed fixture is used to manufacture a pass.
- Stone-button support initially means explicit simulation use, automatic
  release, electrical output and the existing command lifecycle. New public MCP
  button operations and button-input behavioral proof protocols are excluded;
  the existing exploration contract continues to bind actual levers.
- Profiles pin both executable laws and the device integration contract. No
  unversioned user-supplied runtime scripts or arbitrary Blueprint code execution.

## Exclusions and stop condition

No new torch/comparator integration, wooden-button/arrow or entity behavior,
pressure-plate occupancy, inventory/fluids/slime/honey mechanisms, general new
MCP operations, checkpoint migration, or replacement of piston motion planning.
No full-world table enumeration or rewrite of other supported proof models.

If a prerequisite outside this declaration should be implemented first, stop
implementation and report the concrete dependency, reason and proposed scope
to the user. Do not silently expand the task. Difficulty within this declared
architecture migration is not itself a reason to abandon it.

## Validation plan

Use serial offline/locked Rust builds (`-j 1`) and tests (`--test-threads=1`).
Do not run Rust compilation alongside a Minecraft server. Prefer the retained
Java observations for migration regression; static button source inspection and
model tests are distinguished from a new live button trial. Run focused device,
restore, execution-context, construction, review and placement checks, retained
Python comparison suites, formatting and workspace/all-target Clippy. Preserve
raw observations and archived evidence unchanged. Record actual results here
and in a new evidence manifest before completion.


## Implemented architecture

`device_program.rs` compiles the files in `crates/dustroute-minecraft/devices/`.
Their inputs select shared queries (constant, powered state, receiving power,
pending tick and front-side source). Their existing immutable `LawProgram`
produces finite rows through `FiniteLaw`. Effects bind those outputs to Boolean
state writes, guarded tick scheduling and directional notifications, in declared
order. Device metadata also controls observed identities/state properties,
attachment/output orientation, support/conduction, signal direction, fresh-state
admission and optional command initialization.

`time/piston_runtime/devices.rs` executes those primitive effects. Each write's
nested callbacks drain before the next effect. The captured block, resolved
operations, program revision and instruction index are part of `DeviceRun`,
which is retained by exact checkpoints. Pending device ticks carry only the
shared callback identity and concrete-block guard; behavior-state normalization
retains them without losing their remaining delay. No mutable device adapter or
per-Blueprint timer exists.

The dedicated lamp/observer tick and after-tick variants and their native rule
wrappers have been removed. Construction preprocessing, command addition,
removal and moving-device arrival use the same definitions. Quiet observer
command initialization is metadata consumed by the existing construction planner.
The actual world scheduler and piston motion algorithm remain shared Rust code.
Wire/repeater callbacks and the separate fixed-geometry proof models have not
been migrated to this new device vocabulary.

At completion of this migration, execution and root-exploration profiles were **v7**. The execution profile
pins `dustroute.device-programs.java-1-21-11.v1` and fourteen law roles, adding the
stone-button law. Existing lamp/observer laws and recorded observations are
unchanged. Saved electrical v1–v6 contexts are rejected; this does not change the
catalog schema or rewrite historical source Revisions. New review uses explicit
fresh initial conditions. An old v6 adoption is not a current placement proof.

## Stone-button boundary

The pinned Java 1.21.11 `ButtonBlock`, `Blocks` and `BlockSetType` bytecode was
inspected without starting Minecraft. Stone registration specifies 20 game ticks
and disables arrow activation. Explicit use while unpowered writes `powered`,
delivers flag-3 notifications and the source/support notifications, then queues
the release. Use while already powered does not extend the deadline. The release
rechecks powered state and clears it; removal notifies while an old scheduled
tick remains protected by concrete block identity. Sound, game-event listeners,
explosions, projectiles and entities are outside this block-only scope.

The extension adds a law, a device definition and catalog/profile registration;
it adds no stone-button-specific tick, continuation or branch in the common
query/effect executor. Both synthetic `Button` state (the declared stone model)
and complete observed `minecraft:stone_button` records are supported. Other
button materials, missing attachment evidence and freshly observed powered
buttons without their pending history are rejected.

This is static source-audit and model-test evidence for the new button, not a
new live-server comparison. No public button-use tool or button-input exploration
protocol was introduced. Library execution can combine a button with the
existing admitted electrical/piston components. Inventory, entities and other
excluded mechanisms remain future work.


## Completion evidence

All five steps are complete for the declared scope. The retained observations
were not changed and no out-of-scope prerequisite was needed.

- **307 distinct Rust tests passed**, across the Minecraft crate and scoped
  library, translation and public MCP tests. This includes 15 actual MCP tests
  with a transport stub; the full workspace test suite was not rerun.
- **14 Python tests passed**, including the reference door's 381 tick-end states,
  524 ordered writes and 2,118 callback/carrier boundaries, plus 20 retained
  piston/observer movement captures and state-restoration checks.
- Workspace/all-target Clippy with warnings denied, formatting and diff checks
  passed. Builds used offline/locked dependencies, one Rust job and one test
  thread. No Minecraft server or new live trial was started.
- Initial test-selection mistakes were corrected: the construction test target
  is `electrical_piston_construction`, and public MCP tests are under
  `service::blueprint_tests::`. The zero-match invocation is explicitly excluded
  from validation counts.

[Validation commands and implementation hashes](evidence/device-program-migration-20260927.json)
and [button source provenance](evidence/device-program-source-20260927.json)
record the evidence and its limits. Current registration is an immutable built-in
registry. Runtime-loaded arbitrary device programs, stateful torch-history
integration, analog writes and inventory/entity effects remain separate work.
