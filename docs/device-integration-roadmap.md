# Device definition integration

Work continues on `codex/data-driven-block-runtime`. Minecraft Java 1.21.11 is
the behavioral reference. Device rules and bindings are checked Rust constants;
new observations or evidence do not silently upgrade an earlier approval.

**Stop boundary:** milestone 1 is complete; milestones 2–4 are unstarted.
The [physical admission proposal](device-physical-admission-proposal.md) explains
why a broader prerequisite is recommended before introducing a new block kind.
That broader redesign is not part of the declared callback/state milestones and
remains a proposal. The overall goal is not complete.

## Ordered milestones

1. **Repeater (complete).** Replace dedicated callback delivery with the
   shared interpreter. Add directional gate queries, distinguish collected ticks
   from pending ticks, bind tick priority to a bounded result, and express output
   notifications before shape updates. Preserve physical admission, explicit
   placement/removal and exact continuation restoration. Check the retained live
   observations and short-pulse/locking/ordering regressions. Version the runtime
   contract when event payloads or callback ordering change.
   Dust attachment is now a separate `WireConnectionRule` from signal emission.
2. **Waxed copper bulb.** Admit explicit powered/lit state, rising-edge toggling,
   installed-state power sampling and comparator-readable output. Include fresh
   construction, observation, checkpoint and placement/readback boundaries. No
   oxidation or random-tick model is implied.
3. **Comparator with circuit inputs.** Add compare/subtract mode, gate input
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
record the final checks. The additional physical-admission proposal remains
unimplemented; this result does not complete the overall goal.
