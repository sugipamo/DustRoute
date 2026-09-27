# Reference door construction: cause and proposed change

The observer-front precedence proposal below has now been explicitly authorized
and implemented in `ElectricalConstruction::new`. Existing support checks,
rank/coordinate tie-breaking, full callbacks, final-state equality and teardown
are retained. Cycles are rejected without notification suppression or a reset
fallback. [Implementation evidence](evidence/reference-door-construction-order-20260927.json)
records the current production results. The remaining sections preserve the
investigation and the scope of the original proposal; its recorded baseline is
the former ordering, not the current production planner.

The current sequential planner activates an **incomplete two-stage extender**
during construction. The last displaced quartz block is explained by the
recorded observer → conductor/dust → repeater → lower piston chain. This is
consistent with the inspected Java 1.21.11 callback and piston rules; this
investigation found no evidence requiring a physics change. It does not yet
establish live conformance of this construction sequence.

The user's authorization for the investigation increment was **investigation
and a concrete proposal only**. The experiment was a standalone diagnostic
example; production changes were authorized separately afterwards. The runtime,
Blueprint requirements and MCP workflow are unchanged. The door remains unadopted, including the independent incomplete
arbitrary-input proof described in the [adoption audit](reference-door-adoption.md).

## Why step 36 displaces quartz

The planner prefers solids, pistons, dust/repeaters, levers, constant sources,
then remaining devices. Within a rank it sorts by y/x/z, choosing the first
block whose support is already present. It has no observer-front dependency.

The repeater at `(0,3,-1)` cannot be installed until its supporting observer at
`(0,2,-1)` exists. The actual tail of the baseline order is therefore:

| Step | Installed block | Consequence |
| --- | --- | --- |
| 34 | South-facing observer at `(0,2,-1)` | Watches the still-empty cell `(0,2,0)` |
| 35 | Delay-4 repeater at `(0,3,-1)` | Has support; downstream path is now present |
| 36 | South-facing observer at `(0,2,0)` | Changes the first observer's watched cell |
| 37 | Lamp at `(0,2,1)` | Changes the second observer's watched cell, causing another pulse |
| 38–43 | Remaining observers | Arrive after the unintended movement has completed |

The following times are **model game ticks relative to installation 36**,
not measured server times. The diagnostic retains invocation IDs, causal links,
complete traces for steps 35–37, and settled worlds after all 43 steps.

| Tick | Model event |
| --- | --- |
| 0 | Installation sends a facing-side shape notification to observer `(0,2,-1)` |
| 2 | That observer turns ON; its north output powers wool `(0,2,-2)` and dust `(0,3,-2)` becomes 15 |
| 4 | Observer turns OFF and dust becomes 0; the repeater's queued ON tick remains |
| 10 | Repeater turns ON; lower sticky piston `(0,3,0)` extends and pushes the retracted upper piston from y=4 to y=5, and quartz from y=5 to y=6 |
| 12 | Both pushed payloads finish moving |
| 18 | Repeater turns OFF; lower piston receives ordinary `Retract` and pulls the upper piston back |
| 20 | Upper piston is back at y=4; quartz is still at y=6 |

The upper piston is retracted throughout this partial construction. It is moved
as a block; pulling it back does not also pull the separate quartz beyond it.
The rest of the door's sequencing observers have not been installed yet.

The inspected target bytecode supports each relevant rule:

- `ObserverBlock.getStateForNeighborUpdate` schedules a tick on a facing-side
  notification when unpowered; `scheduleTick` uses two game ticks and deduplicates
  pending work. The powered tick schedules its OFF tick two game ticks later.
- `AbstractRedstoneGateBlock.scheduledTick` turns an unpowered repeater ON even
  if the input has already fallen, then schedules OFF. `RepeaterBlock` multiplies
  the delay property by two, giving eight game ticks for this delay-4 repeater.
- `PistonBlock.onSyncedBlockEvent` and `PistonHandler` push the linear chain but
  retract using the block two cells in front as the pull target. A retracted
  piston body does not attach the quartz behind it like a slime/honey block.

Class hashes and trace anchors are in the
[investigation evidence](evidence/reference-door-construction-cause-20260927.json).
An isolated server replay of the construction sequence is still needed before
claiming that all its notifications match the target.

## Bounded model experiments

The diagnostic mirrors the existing rank/support order for a control run and
originally checked that it reproduced the production planner's two-cell rejection
and step-36 attribution. Its current v2 output preserves that historical control
and separately reports the current production plan. Other variants are explicit permutations of this fixture,
not an implemented general planner. No existing world or user catalog is used.

| Experiment | Exact declared world reached? | Finding |
| --- | --- | --- |
| Existing order | No; two cells differ | Baseline failure reproduced |
| Swap only the two bottom observers | No; two cells differ | Lamp placement still triggers the observer chain |
| Lamp → middle observer → first observer | Yes | 43 insertion deltas; no additional block changes and no pending timed work during construction |
| Existing order, then ON/OFF held until idle | No; five cells differ | This one reset attempt does not restore the exact geometry; no universal recovery claim is made |

For the successful ordering, the unchanged teardown selection also removes all
blocks in **43 settled removal steps**. This is a simulation result for one
orientation/location, not public MCP placement or live readback evidence.

## Proposed implementation scope

Extend candidate ordering in `ElectricalConstruction::new` with one additional
precedence relation: when a declared non-Air block occupies an observer's
watched cell, construct that block before the observer. Retain the existing
support requirement and rank/coordinate tie-breaking among eligible candidates.
Compute the watched cell from the observer's orientation; internal `Block.facing`
stores the **output** direction, so its opposite is the watched side. Do not
hard-code the reference door's coordinates or invert this convention twice.

For this fixture the combined dependencies are:

```text
lamp (0,2,1)
  → observer (0,2,0)
    → observer (0,2,-1)
      → supported repeater (0,3,-1)
```

This yields steps 34–37 in exactly that order. Earlier placements and the
remaining six observers keep their relative order. The three-block permutation
experiment demonstrates this proposed ordering's result for this specific door.

Treat precedence as a way to generate a candidate, not a Minecraft law or a
certificate. Later writes can still update a watched block, and some layouts
can have cyclic dependencies. Every candidate must still execute full callbacks,
settle after each step, match the complete declared final world, and successfully
model teardown. If dependencies cannot be satisfied or simulation fails, retain
the rejection with diagnostics. General search, notification suppression,
automatic lever reset and a new initialization-action format are not part of
this first proposal.

Existing `ElectricalConstructionStep` records can represent the reordered
writes and expected readbacks. The proposal therefore does not currently call
for a new Law/profile, a persistence schema migration or a new public MCP action.
Only newly computed plans may use the new order; saved operations/placed-instance
records must not have their recorded sequence silently rewritten.

Validation for a subsequent implementation should cover:

1. This door's full exact build and teardown, retaining the failing baseline
   trace as a diagnostic reference rather than accepting its displaced world.
2. Observer orientations, support interactions, watched Air and dependency
   cycles; existing mixed construction fixtures must still pass.
3. Relocation/rotation at actual target coordinates, because notification
   order can depend on coordinates; no source-local pass grants placement.
4. An isolated Java 1.21.11 comparison of baseline and proposed ordinary
   placement, then the public MCP preview/apply/readback/undo path once adoption
   has its own completed proof. Strict initialization cannot substitute.

The ordering change appears **small to medium in implementation size**, with
additional live verification work. The incomplete arbitrary-input adoption
proof is a separate task and is not solved by this ordering change. A need for
general construction search, dynamic reset actions or new physical mechanisms
would require a separate scope decision.

## Retained evidence and current verification

The old construction-order experiment was retired during the 2026-09-27 code
cleanup. Its duplicated rank/support/teardown planner served the original
investigation; it is not a second supported construction implementation. The
historical outputs and disassembly, with hashes in the evidence file, remain
unchanged. No Minecraft server was started for that original investigation.

Use the production planner and retained command-placement regression for current
verification:

```sh
cargo build --offline --locked -j 1 -p dustroute-translate --example audit_reference_door_adoption
target/debug/examples/audit_reference_door_adoption ordinary-construction
cargo test --offline --locked -j 1 -p dustroute-translate --test command_placement_regression
```

The [v6 repair and live trial](reference-door-live-construction.md) supersede this
historical proposal. They account for command preprocessing and explicit observer
initialization, beyond the ordering hypothesis explored here.
