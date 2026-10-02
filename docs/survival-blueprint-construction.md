# Survival Blueprint construction

Started 2026-10-01 on `codex/survival-blueprint-construction`, from develop
`4ba023c`. The previous diagnostics and Voxrig pin work was fast-forwarded into
develop and pushed. Both merged topic branches were deleted locally and on
origin. Main remains at `7f74b37`.

## Intended result

Given a Blueprint, all required building materials and temporary access blocks,
the bot builds a bounded structure in survival mode through ordinary player
actions. Materials are supplied directly in **the bot's inventory**, as confirmed
by the user. The human should specify the design and site, rather than place each
block. Completion requires checking the final structure and declared air spaces,
accounting for consumed materials and removing the bot's temporary access works.

The first target is a small passive structure on an observed level site, with a
roof high enough to require temporary access. The blueprint and material list
must be fixed before testing; a successful single nearby placement is only an
earlier milestone. Existing circuit analysis and adoption do not establish
survival constructibility.

The first bounded milestone uses ordinary, admitted solid temporary blocks, as
specified in the approved goal. Minecraft's dedicated
`scaffolding` block requires its own placement/support/climbing audit before it
can be selected. Native fixture mining below does not establish temporary-access
construction or cleanup acceptance.

## Existing mechanisms and gaps

This is a source audit of `4ba023c`, not new live Minecraft evidence.

| Mechanism | Current evidence | Needed for this goal |
| --- | --- | --- |
| Blueprint authoring, immutable pins, adoption, physical review | [design workflow](blueprint-building-design.md) | Reuse them; keep design validity separate from the ability of a player to construct it |
| Literal site observations, policy, diagnostics and durable uncertain attempts | [construction executor](../crates/dustroute-mcp/src/service/construction_executor.rs), [diagnostics](operation-diagnostics-progress.md) | Reuse the boundaries and evidence kinds; add player actions and material state |
| Normal block-use packets | [1.21.11 `use_on_block`](../vendor/voxrig/src/versions/java_1_21_11/client/operations.rs) | Check actual held item, target face, reach, obstruction, placement rule and received result |
| Received inventory | The same native module receives player slots and treats unsupported components as unavailable | Retain window/state revisions and cursor knowledge needed for transactions; reconcile material consumption |
| Inventory transfer | `select_hotbar` selects only an existing hotbar slot; `set_creative_hotbar` creates items in creative mode | Add ordinary, received-result-checked main-inventory/hotbar transfers in 1.21.11 |
| Player movement | 1.21.11 has `move_flying`, gated by server flight permission | Add survival control ticks, collision, gravity, grounded state and server correction handling |
| Survival locomotion donor | [1.16.1 client](../vendor/voxrig/src/versions/java_1_16_1/client.rs) has `set_control`, `jump` and a control/physics loop | Audit reusable algorithms and expose them through the explicit 1.21.11 adapter; do not redirect a 1.21.11 connection to 1.16.1 |
| Timed mining donor | That 1.16.1 client has `dig_block`; 1.21.11 exposes `dig_creative` | Add survival mining for declared temporary materials, tool constraints and uncertain-result handling |
| Current physical placement | [native physical bridge](../crates/dustroute-mcp/src/voxrig_bridge/operations/physical.rs) switches to creative, teleports and creates held items | Introduce an explicitly selected survival executor that cannot fall back to those actions |
| Current building execution | The shared executor constructs `CommandWrite` batches | Its command schedule is not a player action plan; reuse review, policy, persistence and diagnostics rather than relabeling it |
| Ground and temporary access | Building patterns require an outer air guard, and target placement requires a completely empty region | Define an observed, protected site context and explicit temporary edit scope without silently weakening immutable building requirements |

Voxrig's public `Client` keeps the two protocol adapters separate. The root
`Bot`/`ControlState` APIs belong to 1.16.1; the presence of those exports does not
mean 1.21.11 already supports them. At the audited pin, the 1.21.11 position
receiver reads but discards velocity and has no corresponding survival control
loop. Transport acknowledgements and cached local movement are not proofs of
completed world placement or accepted position.

## Architecture and order

Keep the final Blueprint independent of a site-specific execution plan. A plan
contains the observed baseline, protected environment, temporary access scope,
material reservations, player actions and escape/cleanup route. Temporary blocks
do not become permanent Blueprint geometry. They may not occupy a declared
permanent-air region merely because they will eventually be removed; any needed
execution-time allowance must be explicit and reviewed separately from the
immutable final-state obligation.

Voxrig owns version-specific packets, received player/inventory state and player
physics. DustRoute owns bounded navigation, construction sequencing, access-work
planning, edit permissions and Blueprint integration. Preserve Voxrig's separate
contribution history and update the unmodified vendor snapshot only from a
validated source commit, as described in [the vendor policy](../vendor/README.md).

| Order | Work | Acceptance before proceeding |
| --- | --- | --- |
| 1 | Native 1.21.11 survival state and inventory transactions | Ordinary stacks in main inventory can move to the hotbar and back. Received revisions, cursor, mode and connection changes are checked. No generated items or optimistic inventory counts. Unsupported components produce an explicit refusal |
| 1b | Minimum native stationary posture, ground and mining-condition foundation | Preserve own entity identity, received health/velocity and attribute/pose evidence; derive bounded standing contact from reconstructed geometry. Compare native defaults/codecs/contacts and refuse unmodeled motion. Full walking remains step 3 |
| 2 | Nearby survival placement and bounded timed mining | On a prepared, reachable site, ordinary interactions place exact admitted cubes and remove declared temporary cubes. Validate the hit and player occupancy; independently observe block changes and inventory consumption. Acknowledgement alone is insufficient. No mode switch, flight, teleport or block-write commands |
| 3 | Native survival locomotion and bounded navigation | Walk, stop, jump and settle on admitted static geometry; honor corrections and missing chunks. Validate collision, ground support, reach and an escape path. Health/death or unsupported movement conditions stop work; adversarial entity simulation is not part of this milestone |
| 4 | Site-aware Blueprint and temporary access planning | Revalidate the final design in its observed ground context, preserve all protected terrain and permanent air obligations, and plan support/place/access/remove dependencies. Include action order and last safe retreat. Material estimates distinguish permanent consumption, peak temporary inventory and resources dependent on later recovery |
| 5 | Durable survival job execution | Preview the exact site, materials and temporary footprint. Execute one checked player action at a time through shared mutation/policy/error boundaries. Save intent before mutation, read back the result, reconcile inventory, and checkpoint verified progress. Restart requires fresh observation and replanning; never replay an uncertain action blindly |
| 6 | End-to-end isolated survival acceptance | With a non-OP survival bot given inventory and an adopted passive Blueprint, build the declared roofed structure using access works. Observe exact completion and cleanup. Exercise missing materials, blocked access, unexpected edits and disconnects; demonstrate diagnosis and a newly planned continuation |

Steps 1 and 2 deliberately permit a stationary, reachable trial before the larger
locomotion change. They do not satisfy the full building goal. The end-to-end
trial must still run with operator permissions absent: using commands to set up
an isolated fixture by a separate test actor is not the builder's execution path.

Start with a finite admission list of passive cube materials and plain inventory
stacks. Placement-state recipes for stairs, slabs and directional equipment need
independent player-interaction verification before expansion. Resource gathering,
crafting, chest retrieval, terrain excavation, arbitrary fluids, combat and
arbitrary survival construction are subsequent scope, not implied acceptance.
Access-block removal does not imply recovered item drops: actual recovery needs
its own received inventory evidence. If recovery is necessary to finish with the
supplied inventory, the plan must establish it or stop before spending resources.

## Current progress and stopping point

The user approved the inventory prerequisite, then approved the minimum
posture/ground/mining-condition foundation before nearby placement/mining.
Voxrig changes are on `codex/survival-construction`, preserving the independent
upstream contribution history. DustRoute imports the exact validated source
through the vendor updater.

Implemented milestones (not live building acceptance):

- Ordinary component-free main-inventory/hotbar swaps retain cursor/revision and
  per-slot receive evidence. Pending intent survives timeout/cancellation, and
  both received destination updates are required before confirmation. Native
  SWAP codec comparison and real loopback transport cover the boundary.
- Own-player identity, health, packed velocity, supported attributes and pose
  updates are observed. Native initial defaults carry a different evidence kind
  from received updates; parsing shares the remote-player mechanisms.
- A finite standing context reads reconstructed dry full cubes and air, checks
  body clearance and foot contact, and preserves world/receive provenance.
  Survival look rechecks it and uses the derived ground bit. Unknown motion,
  unsupported posture/fluid/geometry, missing chunks, reconstruction issues and
  unmodeled impulse/vehicle contexts refuse stationary work.
- Native 1.21.11 comparisons cover standing dimensions, all admitted cube
  states, mining attribute IDs/defaults/limits, packed velocity/look codecs and
  contact boundaries. Foundation source tests passed **136 tests, 1 ignored**, with
  Clippy, documentation, formatting and package-input checks passing. The
  [validation record](evidence/survival-foundation-20261002.json) separates these
  observations from live-server evidence.

Nearby validated survival placement, safe mining continuation, walking/navigation,
the site/temporary-access planner and durable survival Blueprint executor remain
unimplemented. A bounded mining observation API exists as recorded below. No
survival live build was performed by these milestones.
The subsequent test-private native mining comparison below is a separate finite
world-edit trial. Effect updates currently lack a complete-list/expiration projection;
unknown effects must not become an exact mining-duration assumption.

The user approved [the mining intent/result proposal](survival-mining-cancellation.md)
after native inspection found that early finish can schedule a later break which
abort does not clear. A dedicated vanilla survival comparison then reproduced
normal finish and the delayed break after early finish plus abort. Its disconnect
case observed miner removal and retained stone for 9.3 seconds; it does not
authorize reconnect/replay from shutdown alone. The test-private driver, raw
evidence and fixture-control timeout are retained in the vendored
[native comparison](../vendor/voxrig/docs/survival-mining-comparison.md).
Source validation passes **136 tests, 2 ignored**; the native opt-in test passes
separately. See [the integration record](evidence/survival-mining-comparison-20261002.json).

A new prerequisite was found in the shared sender: cancellation/error during
the separately awaited frame writes can leave the stream reusable for automatic
responses. This is source inspection, not an injected partial-write reproduction.
This initially stopped sender/mining work. The user approved the
[sender proposal](survival-send-cancellation.md), and the following changes are
now implemented:

- The live 1.21.11 sender closes interrupted frames to all further queued user
  and automatic writes. It wakes teardown, shuts down the stream and exposes
  `UncertainDispatch` without claiming undo. Six bounded stream/TCP tests cover
  successful fragmentation/compression, lock-wait cancellation, partial write,
  write/flush errors, queued responses, teardown and historical pending intent.
- Held-hotbar selection distinguishes receive from ordered submission, with
  intent before I/O. Stationary empty-hand dirt/stone mining stores START,
  FINISH and ABORT separately with monotonic sequences. Its read-only result
  boundary requires a fresh target-specific received block/section update;
  abort/acknowledgement, cached air and unrelated updates do not substitute.
  Conflict/chunk/world changes retain inspection, and closure retains history.
- A prototype native API comparison passed all three existing cases. Its
  original traces and limits are retained separately from final offline checks:
  it preceded the conservative continuation restriction. The amended ignored
  driver has not been rerun after stopping on the new concern.

Native control flow reveals another distinction: externally supplied air may
precede the update that clears delayed mining. A received air result therefore
does not prove safe immediate replacement. The current gate blocks all following
mutations even after observed removal (`continuation_validated: false`). This
is source inspection, not a reproduced replacement race. Following the user's
concern stop condition, the release/recovery implementation and additional live
trials are stopped for review of the
[concrete continuation proposal](survival-mining-continuation.md).

Final source validation passes **148 tests, 3 ignored**, Clippy, documentation,
formatting and package-input checks. The native API prototype test passed
separately. [The current validation record](evidence/survival-mining-implementation-20261002.json)
identifies the pin, integrated check and scope. The dedicated server was cleanly
stopped. The overall building goal remains unfinished; none of these comparisons
establishes roofed construction, temporary cleanup or safe mining continuation.

## Mining retirement checkpoint and interaction-loading review

The user approved the continuation proposal. Voxrig now registers an independent
observer against the authenticated miner's exact UUID, requires a new vanilla
PLAYER_REMOVE and closed original sender, and explicitly reconnects to read new
site/player/inventory conditions. A cancelled reconnect retains its attempt and
cannot open a second login from the same watch. History never imports authority.

The immutable native comparison reached retirement and fresh-session hotbar
sending for normal finish, early finish+abort, early disconnect and a later
external replacement. That last input was delayed to 25.671 seconds, so it is
not evidence of the delayed-miner replacement race. These results do not validate
the final later restrictions or a subsequent world interaction.

An additional native loading prerequisite was found: the server can discard
game actions until PLAYER_LOADED or 60 player updates, whereas current local
ready only establishes play/position. The new recovery session therefore also
refuses user mutations (`interaction_ready: false`, `recovery_loading_pending`).
The previous mining fixture's private loading notification is not production
support. Following the user's concern stop condition, loading changes and more
live trials are stopped. The [concrete next change](survival-interaction-readiness.md)
keeps loading in the common version-specific connection/operation layer. Survival
placement, navigation, temporary cleanup and the complete roofed build remain
unfinished.

The [retirement checkpoint validation](evidence/survival-mining-retirement-20261002.json)
pins the final source, 152 passing offline tests (3 ignored), Clippy/doc checks
and the integrated all-targets native-feature check. Its native execution pin
and limitations are recorded separately from the final conservative gates.


## Common loading and fresh mining acceptance

The user approved the loading prerequisite. The 1.21.11 connection now sends one
guarded PLAYER_LOADED per received world generation after initial-chunks,
position and own-chunk baselines. All ordinary mutations and fresh mining
recovery share the stage, with retained before-I/O attempts and no elapsed-time
bypass. Recovery also waits for the complete standing halo across chunk edges.

All four native comparison cases pass at the immutable execution revision.
External air/immediate replacement completed at 1,454 ms and the replacement was
later removed at 7,459 ms. Old-session mining reuse therefore remains blocked.
After exact independent retirement, a new connection began public-API mining at
about 250 ms and removed the retained stone, confirmed by a separate observer.
There was no test-private loading notification or fixed login sleep.

The [current integration record](evidence/survival-interaction-loading-20261002.json)
identifies 155 passing offline tests (3 ignored), the separately passing native
test, Clippy/documentation/package checks and DustRoute native-feature integration.
Earlier fixture timeout and the fixed neighbor-baseline failure are retained in
the evidence limitations. The dedicated fixture is stopped.

This completes the approved loading/recovery prerequisite, not the full building
goal. Next is nearby survival placement/material accounting, followed by walking,
access planning, durable jobs and the roofed non-OP construction acceptance.

## Nearby survival placement and received materials

Voxrig now provides `place_survival_cube`, `observe_survival_placement` and
`wait_survival_placement`. The native operation validates healthy stationary
contact, selected plain material, first outline face hit, received adjacent air
and body clearance. Eleven audited passive cube materials are admitted. Raw
survival block-use callers are directed to this checked API; creative behavior
retains its existing packet-submission contract.

The intent is retained before sending. All further user mutations wait for an
exact target-specific block receipt, a fresh selected-slot receipt showing one
consumed material and the processed one-shot interaction sequence. None alone
establishes completion. Unexpected intermediate block/material/context changes
latch inspection even if later updates match; timeout and cancellation do not
resend or erase the attempt. Historical results are not restorable job authority.

The dedicated non-OP vanilla comparison passed: an ordinary main-inventory to
hotbar swap supplied three dirt, followed by two top-face and one side-face
placements on the same connection. An independent observer saw each placed
block, and received counts progressed 3 -> 2 -> 1 -> empty. Completion took
81/80/100 ms in this tiny localhost sample, not a general throughput benchmark.
No builder commands, creative inventory writes, teleport or flight were used.
Console commands only prepared the isolated fixture before release; the fixture
has been stopped cleanly. Empty-hand placement was refused.

[Placement details](../vendor/voxrig/docs/survival-placement.md) and the
[integration record](evidence/survival-placement-20261002.json) identify the native
execution pin, raw traces, 161 passing offline tests (4 ignored), independent
material/packet oracle and integration checks. Live acceptance used dirt; all
11 material definitions and 6 packet faces were checked against the native
oracle, not by an all-material live placement matrix.

Step 2 now has nearby placement/material receipts and bounded mining with explicit
retirement/recovery. The overall goal remains active and incomplete. Next is
step 3: native survival walk/stop/jump/settle on admitted static geometry, including
collision and correction handling. Site/access planning, persistent jobs and
the adopted roofed Blueprint build/temporary cleanup still follow. This milestone
does not turn the existing command construction executor into a survival executor.

## Movement attributes checkpoint and post-walk standing review

The next-step audit retained eight native movement attributes in own-player state:
movement speed, gravity, jump strength, step height, movement efficiency, sneaking
speed, safe-fall distance and fall-damage multiplier. Native defaults and received
updates remain distinct. A typed Rust definition table handles these and the
four existing stationary/mining attributes. The unchanged-body 1.21.11 oracle
checks defaults, bounds and tracked flags; tests cover field routing, modifiers,
truncated batches, other-player isolation and reset behavior. This is state
projection only; it does not establish walking or permit construction after a
locally submitted movement.

A common continuation concern was found before live locomotion: current standing
admission requires a received own position and zero received velocity. Normal
native movement acceptance does not echo an own-position receipt for each step.
Waiting for that receipt can leave construction permanently blocked, while
relabelling prediction as receipt would invalidate existing evidence semantics.
The independent viewpoint tracker also lacks position-specific receipt/ground
and velocity provenance needed for an arrival contract.

Following the user's concern stop condition, movement control and post-walk
construction release are stopped for review of the [concrete evidence-layer
proposal](survival-movement-evidence.md). No movement packets or new live trial
were run, and no server MOD requirement was introduced. Existing stationary
placement/mining remain unchanged. The attribute prerequisite is validated in
the [checkpoint record](evidence/survival-movement-foundation-20261002.json).
The full roofed Blueprint goal remains unfinished.


## Motion evidence checkpoint after approval

The approved shared-layer work now separates own position receipts from local
send attempts, keeps interrupted attempts inspectable, and prevents a pending
send from authorizing another mutation. Remote player observations retain
position-only freshness, exact spawn lifetime, per-axis quantization bounds and
separate ground/velocity receipts. Read-only watches reject stale viewpoints and
changed world/player instances.

The native audit also traced the remote ground flag back to the moving client's
incoming ground bit on the server's normal movement path. A later observer
position/ground packet therefore cannot be promoted to an independent stopped
acknowledgement. Following the user's concern stop, physics prediction, survival
locomotion and post-walk building release are still unimplemented. No new server
or live movement trial was run.

The [updated proposal](survival-movement-evidence.md#concrete-next-contract-for-review)
specifies a predicted-and-observed standing contract, conservative geometry
revalidation and unchanged action-result checks. This is an explicit operational
confidence level, not a server-confirmed-rest claim. The shared-layer prerequisite
is a partial checkpoint; the overall construction goal remains unfinished.


Validation: [motion evidence record](evidence/survival-motion-evidence-20261002.json)
records 168 passing Voxrig tests (4 intentionally ignored native trials), one
passing doc test, clean Clippy and the DustRoute MCP/Voxrig all-targets check.
Voxrig is pinned to `a5028a367b9968da03211d419c35db77e5be6e49` (259 managed files).
The correction-path review also identified a pre-existing legacy remote teleport
layout. It now refuses explicitly; native relative correction support must be
verified before the motion trial. Passing these checks is not live movement
acceptance or completion of the shared prediction/standing layer.
