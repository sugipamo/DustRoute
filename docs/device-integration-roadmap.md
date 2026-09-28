# Device definition integration

Work continues on `codex/data-driven-block-runtime`. Minecraft Java 1.21.11 is
the behavioral reference. Device rules and bindings are checked Rust constants;
new observations or evidence do not silently upgrade an earlier approval.

**Current work:** milestone 1 is complete; milestones 2–4 follow the
[approved physical admission prerequisite](device-physical-admission-proposal.md).
The user approved checked physical descriptors, explicit executor admission,
independent Law kind adapters and shared observation/support checks. This work
is complete. The waxed bulb milestone is complete; the comparator milestone is complete, followed by torch integration. The overall goal is not complete.

## Ordered milestones

1. **Repeater (complete).** Replace dedicated callback delivery with the
   shared interpreter. Add directional gate queries, distinguish collected ticks
   from pending ticks, bind tick priority to a bounded result, and express output
   notifications before shape updates. Preserve physical admission, explicit
   placement/removal and exact continuation restoration. Check the retained live
   observations and short-pulse/locking/ordering regressions. Version the runtime
   contract when event payloads or callback ordering change.
   Dust attachment is now a separate `WireConnectionRule` from signal emission.
2. **Waxed copper bulb (complete).** Admit explicit powered/lit state, rising-edge toggling,
   installed-state power sampling and comparator-readable output. Include fresh
   construction, observation, checkpoint and placement/readback boundaries. No
   oxidation or random-tick model is implied.
3. **Comparator with circuit inputs (complete).** Add compare/subtract mode, gate input
   aggregation, output delivery and world-owned internal output state. Keep
   internal output distinct from block-state properties. Define fresh observation
   and restoration rather than inventing missing block-entity values. Inventory
   and entity inputs remain outside scope.
4. **Torch.** Integrate position-owned burnout history, expiry and scheduled work
   into the shared world, using target-version evidence. Verify replacement and
   intermediate restoration before claiming mixed-circuit coverage.

Each milestone requires meaningful tests and an explicit account of supported
simulation, persistence, observation and placement. New public MCP operations,
entities, inventories, fluids and oxidation are outside this goal. If an outside
prerequisite should be implemented first, or an additional substantial redesign
is discovered, stop before implementing it and report the concrete dependency.
The internal-state work already listed in milestones 3 and 4 is planned work.

## Validation policy

Use one Cargo job and one test thread. Start with targeted runtime and definition
tests, then downstream construction/adoption/retained-observation regressions.
Compilation checks contracts, not Minecraft parity. Any new parity claim must
identify the independent source/observation and its limits. Do not run a live
server just to refactor already recorded behavior.

## Repeater source audit

The cached Yarn-mapped 1.21.11 build.6 JAR was inspected with `javap -p -c`:
`AbstractRedstoneGateBlock.scheduledTick/updatePowered/onBlockAdded/`
`isTargetNotAligned` and `RepeaterBlock.getStateForNeighborUpdate/isLocked/`
`getUpdateDelayInternal`. Output notifications precede write shape updates;
the short-pulse follow-up is scheduled **after** the powered-state write and
its nested callbacks return. The previous dedicated runtime queued that follow-up
before draining callbacks. Migration must follow the target implementation and
test this ordering rather than preserve that approximation.

## Milestone 1 result

Repeater callbacks now run through the same checked definitions and interpreter
as lamp, observer and stone button. Dedicated repeater tick, neighbor, shape,
insertion and removal delivery branches are gone. Emission and dust attachment
also use the definition. The old published local repeater Law v1 remains an
immutable catalog record; the current context selects callback Law v2.

The runtime and root-exploration contracts are **v9**, device definitions **v3**.
Prior v8 contexts require fresh review; checkpoints are not converted. New
compile-fail cases cover computed priority bounds, construction-only settings
and pre-insertion query restrictions.

- **301 distinct Rust tests passed:** 241 Minecraft tests, 18 library Law/context
  tests, 40 downstream placement/adoption/observation tests and two public MCP
  review/construction tests using a transport stub.
- **14 compile-fail doctests** and **14 Python tests** passed. The rebuilt model
  matches the retained reference-door tick ends, ordered writes, callback-visible
  worlds and recorded short-input comparisons.
- New tests cover four output directions at delays 1–4, 32 dust-attachment
  arrangements, collected-tick suppression/priority, strict observed admission,
  and exact restoration at every microstep during short-pulse notification work.
- Workspace/all-target Clippy with warnings denied, formatting and diff checks
  passed. One Cargo job and one test thread were used. No live server or new
  Minecraft trial was run; the full workspace test suite was not run.

[Commands, source-class and implementation hashes](evidence/device-integration-repeater-20260928.json)
record the final checks. At that stop the physical-admission prerequisite was still a proposal.
The approved implementation is recorded below; the overall goal continues.

## Approved prerequisite result

The physical-admission prerequisite now has one checked Rust declaration for
current callback geometry, attachment, conduction/support faces and dust
connection. Device definitions, electrical queries and current command
construction consume those facts. Snapshot identity classification also reads
registered device names, without granting executor or placement support.

World profiles explicitly admit block kinds. Compatibility and bounded runners
reject kinds outside their contract; spatial, connection and payload laws have
independent ABI adapters. Historical spatial laws keep their earlier meaning.
Current execution/root exploration is v10, device programs v4, physical
admission v1. Saved v9 contexts are rejected rather than converted.

Validation: 245 Minecraft tests, 16 compile-fail doctests, 18 library Law/context
tests and 40 translation/construction/adoption tests passed. Workspace/all-target
Clippy passed with warnings denied. No live Minecraft trial or new public MCP
operation was run. The next milestone is the waxed copper bulb.

## Milestone 2 result

The four waxed bulb identities execute from one checked definition. LIT and
POWERED remain separate, rising edges toggle LIT, and comparator readout does not
create ordinary signal emission. Generic device export retains both properties
through construction and teardown. Unsupported historical executors and piston
payloads reject the new kind. Execution/exploration is v11, device programs v5,
physical admission v2 and current ordinary placement v3.

Validation: 250 Minecraft tests, 18 compile-fail doctests, 18 library Law/context
tests, 43 translation/construction/adoption tests and 14 Python tests passed.
The rebuilt model passes retained door comparisons. Workspace/all-target Clippy,
formatting and diff checks passed. These are source-derived and model regression
checks; no new live Minecraft trial or full workspace test run was performed.

[Behavior and boundaries](copper-bulb-runtime.md) and
[verification manifest](evidence/device-integration-bulb-20260928.json).

## Milestone 3 result

Compare/subtract circuit execution now uses checked definitions and a separately
pinned 0–15 signal law. Internal output belongs to the common runtime and survives
checkpoint and behavior-state restoration. Bulb readout changes notify comparators
through one conductor, with callback ordering retained. Modes are construction
settings; manual mode switching and missing live block-entity reconstruction are
not claimed. Execution/exploration is v12, device programs v6, physical admission
v3, the core runtime record v4 and root comparison v2.

Validation: 260 Minecraft tests, 20 compile-fail doctests, 18 library Law/context
tests, 45 translation/construction/adoption tests and 14 Python tests passed.
Workspace/all-target Clippy, formatting and diff checks passed. The rebuilt model
also passes retained door comparisons; no new live Minecraft trial, new MCP
operation or full workspace test run was performed.

[Behavior and persistence boundaries](comparator-runtime.md) and
[verification manifest](evidence/device-integration-comparator-20260928.json).
The remaining planned milestone is world-owned torch history and delivery.
