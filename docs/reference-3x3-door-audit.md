# Downloaded 3×3 door: static inspection and next scope

The user supplied `3x3-piston-door-e2180.zip` after selecting Bobiloosky's
[One-Wide 3×3 Piston Door](https://www.planetminecraft.com/project/one-wide-3x3-piston-door-works-on-java-edition/).
The [inspection record](evidence/reference-3x3-door-20260927.json) retains the
archive hash, original block identities/properties, coordinates, chunk hashes
and a bounded snapshot. This is a reference candidate, not a validated Assembly.

Current status: the common runtime reproduces the retained door run, including
381 tick ends, 524 ordered palette writes, 2,118 callback/carrier boundaries and
save/restore. Moving observers also pass three separate live captures. See
[the completed movement increment](staged-piston-motion.md). Public reference-door
construction/adoption remains a separate validation task. The audit below retains
the findings and stop points from earlier increments.

## Source and geometry

- Archive SHA-256: `2f4e45868baa6d06c3fa197c88db50a7de2e57e5180eb15fea267e21d254e8c6`.
- `level.dat` and all sixteen inspected chunks record **Java 1.18.2**, data
  version 2975. The supplied screenshot displays **1.21.11**. These are distinct
  observations; the screenshot does not establish successful target-version
  door operation. The save lists `vanilla` and `Fabric Mods` as enabled packs;
  the ZIP contains no datapack files and the inspected blocks have vanilla IDs.
- Door bounds: `(-14,101,-5)` through `(-14,110,1)`, inclusive; **1×10×7**,
  containing **43 non-air blocks**.
- Input lever: `(-14,110,-2)`, floor mounted, facing east, initially OFF.
- Aperture: `x=-14`, `y=105..107`, `z=-3..-1`; all nine cells are saved as air.
  This records a clear aperture, not a tested opening transition.
- The separate spawn platform has eight glass blocks and one smooth-quartz
  block. It is excluded from the door's inventory and bounded snapshot.

The archive was read without extracting or launching the world. Sixteen chunks
cover `x,z=-32..31`; all non-air blocks at `y>=64` were decoded. An independent
decode using the existing `prismarine-nbt` dependency matched every position,
name and property of the 52 selected blocks, plus the saved version. None of
those chunks contains block entities or saved scheduled block/fluid ticks.
All ten pistons are retracted, all observers and the repeater are unpowered,
the lamp is unlit and all four dust strengths are zero.

## Inventory and boundary at the initial inspection

| Component | Count | Current common piston runtime |
| --- | ---: | --- |
| Sticky piston | 10 | Existing all-facing movement; 2 up, 3 down, 2 north, 3 south. This complete circuit remains unverified. |
| Observer | 8 | Rejected by electrical admission; common callbacks, emission, ticks and movement integration are missing. |
| Redstone lamp | 1 | Rejected by electrical admission; common lit-state transitions, electrical traits and notifications are missing. |
| Redstone wire | 4 | Existing component support; the complete circuit remains unverified. |
| Repeater | 1 | Existing component support; saved delay is 4. |
| Lever | 1 | Existing component support. |
| Smooth quartz | 12 | Snapshot import currently classifies it as coarse; the electrical identity gate also excludes it. |
| Cyan wool | 6 | The same exact-classification and material-admission gap. |

See [`validate_evidence`](../crates/dustroute-minecraft/src/piston_electrical.rs)
and [`block_from_record`](../crates/dustroute-translate/src/snapshot.rs).
No identities have been replaced with stone, and no admission gate has been
relaxed. Existing observer/lamp laws describe separate compatibility or bounded
models; their existence does not establish behavior in the physical callback
runtime.

The lamp at `(-14,101,-1)` is directly in front of the downward-facing observer
at `(-14,102,-1)`. Its changes must be considered by the reference model; it
cannot be assumed to be decorative. The south-facing piston at `(-14,104,-4)`
also has three quartz blocks followed by an observer in its initial push line.
This initially suggested a possible observer-movement dependency. The later
live capture below corrects that static estimate: in both measured normal
cycles, no observer moves. Movement remains an audit case for other input
histories, not an established prerequisite for measured normal operation.

No torches, comparators, inventory blocks, slime or honey blocks occur in the
extracted door. Additional mechanisms could still be exposed by dynamic testing;
their absence here is an inventory result, not a complete behavior proof.

## Scope approved after the initial inspection

The initial inspection stopped before implementation because it identified a
lamp dependency beyond the prior movable-piston change. The user subsequently
approved a goal covering live reference capture, materials, lamp, stationary
observer, moving observer and complete-door comparison, in that order, with an
explicit stop/report condition when substantial implementation work is found.

The goal is: **reproduce this specific 3×3 door's normal close/open
cycle in the shared Java 1.21.11 callback runtime, with retained live evidence.**

1. Establish the target-version baseline in an isolated test world using the
   exact saved block states and original orientation. Record actual lever input
   times and the whole door's state through closing, opening and a repeated
   cycle. Preserve setup/settling evidence and do not treat the 1.18.2 save as a
   target runtime checkpoint. If the original design fails, investigate before
   changing the model or the reference circuit.
2. Audit target observer/lamp code and isolate their transitions: triggering
   shape notifications, directional output, pulse scheduling, pending tick
   identity, movement/completion, lamp input and lit-state notifications.
   Compare focused live cases before integrating the full circuit. Audit exact
   quartz/wool material traits and preserve their identities throughout import,
   electrical queries, movement and placement.
3. Connect these components to the same world and queue as pistons, dust and
   repeaters. Include scheduled state and guards in checkpoint and behavioral
   restoration. Use explicit new law/context revisions where required; do not
   silently change saved verification or adoption decisions.
4. Compare the full reference circuit against the captured inputs and
   intermediate/final states. Keep unresolved discrepancies explicit. The first
   completion condition is normal close/open and repeated cycles; arbitrary
   rapid inputs and mid-cycle reversal are subsequent, separately declared
   cases. Public Blueprint adoption/construction needs its own fresh evidence
   after the runtime behavior passes.

Lamp integration, observers and exact material admission are the concrete
additions revealed by this archive. Moving observers were included provisionally
before obtaining the live reference below. This goal does not require implementing
torch/comparator/inventory mechanics in advance. It also does not claim all
3×3 designs are supported once this one passes.

## Completed first increment: live reference and materials

[Capture a](evidence/reference-3x3-live-20260927.json) used the existing isolated
Java 1.21.11 instrumented server. The exact 43-block layout was translated to
`x=42000`, with the lever at `(42000,191,1000)`, preserving orientation and
identities. Source-audited `/setblock ... strict` initializes the saved states
without shape/neighbor or added-block callbacks (flags 818). After 100 client
warmup ticks, the entire 770-cell region matched the source layout, including
air. This is a fresh test initialization, not proof of public construction or
recovery of a 1.18.2 runtime checkpoint.

Four ordinary lever interaction packets produced server-observed writes at
relative game ticks **0, 100, 200, 300**. Both closed samples contained nine
smooth-quartz aperture blocks. Both open samples had a clear aperture and the
**complete region restored to the original state**. The capture retained:

- 381 consecutive tick-end worlds, reconstructed from 524 checked palette
  commits, with full contiguous server heartbeats;
- 812 carrier records covering quartz, piston bodies and piston heads;
- 108 observer state writes, all observer-to-observer at the original eight
  coordinates; **no moving observer** in these two cycles;
- eight lamp state writes, with actual input-relative timing preserved;
- complete final client readback, empty-region cleanup, removed force-load
  tickets and normal server shutdown.

The compact [retained observation](../crates/dustroute-translate/tests/fixtures/reference-3x3-observed-a-v1.json)
deduplicates 36 distinct tick-end worlds, retains the commit/carrier records and
normalizes time to the first input. It is independent live reference evidence,
**not a successful simulator comparison**. Raw artifacts remain under
`.local/e2e-artifacts/reference-3x3-20260927-a.*`.

Smooth quartz and cyan wool now have exact snapshot classification and explicit
electrical admission. Target `Blocks` registration uses ordinary block settings
for both materials; the existing full-face, conductor and movable-solid rules
apply. Focused tests cover both material roles as conductor/support and moving
payload, lossless construction export, push/pull identity, and continuation
from motion-time checkpoint and behavior state. Other material identities remain
outside the new allowance. The whole reference door is still rejected because
lamp and observer adapters are absent.

The execution and exploration profile IDs advance to **v3** to record this
expanded material boundary. The immutable physics law bodies are unchanged.
Saved v2 contexts are rejected rather than reinterpreted; fresh verification is
required. No saved Assembly references, requirements or adoption decisions were
rewritten.

## Previous stop point: observer notification and scheduling integration

The target implementation exposes a larger integration task than reusing the
compatibility observer model:

- `ObserverBlock.getStateForNeighborUpdate` triggers from the facing-side shape
  notification while unpowered. Comparing generic cached signal/state fields
  is a different boundary.
- `scheduleTick` consults `isQueued(position, block)` before scheduling. The
  pulse callback, block addition and state replacement also interact with the
  scheduled work and output-neighbor notifications.
- Existing instrumentation captures palette writes, ordinary `neighborUpdate`,
  delivered scheduled ticks and carrier activity. It does **not** directly
  capture the observer's shape-trigger callback or scheduling admission/queued
  decisions. Current captures establish results and delivered work, not every
  trigger or pending-queue decision.

Correct integration therefore needs coordinated electrical emission/material
traits, shape callbacks, scheduled events and duplicate handling, saved-state
semantics, and focused instrumentation additions. Those runtime and probe
changes have **not started**, including the lamp adapter. Work stops here under
the user's requested reporting condition, after checking the completed first
increment. The companion instrumentation repository was not modified.

The next bounded implementation increment should combine source-derived lamp
behavior with **stationary** observer trigger/scheduling probes and adapters.
The measured normal cycles do not require prioritizing moving-observer support;
retain that as a separately reviewed extension for other input histories.

Validation of this increment: **276 Rust tests** passed (214 Minecraft, 49
library/translation, 13 MCP Blueprint tests), plus seven existing transient
replay tests. Positive reference replay and five corrupted-evidence rejection
checks passed, as did lossless compact-world expansion, workspace all-target
Clippy with warnings denied, formatting and whitespace checks. The full
workspace test suite was not rerun. Commands, counts and log hashes are in the
[check record](evidence/reference-door-first-increment-checks-20260927.json).

## Historical v4 increment: native devices; stop at movement writes

The user authorized resumption. Native lamp and stationary observer callbacks,
queue membership queries, new immutable laws, and observer probes are now
implemented. The execution/exploration profile is v4 and the generic callback
runtime is v2. See [implementation and evidence](native-device-callbacks.md).

A second isolated normal-cycle capture reproduced the live baseline and records
the actual shape callback and scheduling admission at the first mismatch. The
model first diverges at tick 3: an observer receives its facing-side shape
notification at movement start (tick 1) in Java, while the model emits it only
at carrier completion (tick 3), so its first output is delayed from tick 3 to 5.
The complete 3×3 door comparison remains a failure, including final readback.

Correcting this requires auditing and staging individual movement writes,
notifications and carrier enrollment, with checkpointable continuations. The
existing runtime batches several writes in one delta. That larger movement
refactor has **not started**, in accordance with the user's stop/report request.
Moving observers and public reference-door construction/adoption remain open.

## Completed v5 increment

The user authorized the staged movement implementation. The previous v4 failure
is resolved. [The completion report](staged-piston-motion.md) records the current
runtime boundary, three moving-observer captures, passing complete-door comparison
and regression checks. Historical failure artifacts above have not been replaced.
