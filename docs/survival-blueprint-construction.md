# Survival Blueprint construction

Started 2026-10-01 on `codex/survival-blueprint-construction`, from develop
`4ba023c`. The previous diagnostics and Voxrig pin work was fast-forwarded into
develop and pushed. Both merged topic branches were deleted locally and on
origin. Main remains at `7f74b37`.

## Latest status (2026-10-02 UTC)

The user approved [inventory-interruption diagnosis and recovery](survival-inventory-recovery.md).
Native receive-time typed diagnosis and caller-side target reconciliation are
implemented. Validation now repeats natural cleanup and an explicitly injected
hand-change comparison. The older concern evidence remains historical; the full
roofed build and durable jobs are still unfinished.

- Native inventory, placement/mining continuation, locomotion, bounded navigation
  and shared hypothetical geometry have isolated non-OP evidence.
- The declared real temporary platform placement/climb/retreat/removal trial
  passed, including three mining retirement/recovery cycles.
- Caller-side candidate sequence, edit-scope and material checks are implemented;
  their roof contract/resource tests are distinct from player geometry acceptance.
- Automatic access/action selection, durable survival jobs and the complete
  adopted roofed build/cleanup acceptance remain unfinished. The goal is active.

The user approved correcting the [native aiming uncertainty discrepancy](survival-aim-uncertainty.md)
and consolidating Voxrig first. The correction and isolated edge comparison are
complete. Version-selected checked operations and explicit mining retirement /
fresh recovery now share a public client boundary; caller navigation/construction
imports have migrated. Design, permissions, resource policy, route selection and
durable jobs remain DustRoute responsibilities. The full roofed build is pending.

The sections below retain the earlier checkpoints and their original limitations.
The latest detail is in [site planning](survival-site-planning.md).

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

## Initial inventory and standing foundation checkpoint

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

## Bounded native controls checkpoint (2026-10-02 UTC)

The approved shared motion layer now implements the modern correction layout,
dry-cube walk/jump prediction, connection-owned bounded input runs and explicit
received versus predicted-and-observed standing. A run retains intent before I/O,
keeps failures inspectable, and uses same-instance independent position evidence
before shared look/placement/mining admission. This is a native-client foundation;
MCP route/access/job integration and the roofed Blueprint milestone remain open.

The non-OP isolated live trial succeeded at walk -> rest -> placement and jump ->
land -> placement. Wall contact then exposed a conservative clearance limitation:
quantized observer position expands the predicted touching body into the wall.
The final run is retained as RequiresInspection; the full live test failed rather
than authorizing construction. The user concern stop was honored, the server was
shut down, and no clearance relaxation or retry was performed.

See the [specific next proposal](survival-movement-evidence.md#concrete-next-work-for-approval)
and [pinned live record](../vendor/voxrig/docs/evidence/survival-motion-live-20261002.json).
Voxrig source is `7915c253ea075926b1576274881e5b426e85c088`; actual live code was
`f204cd2879030648c355ba73619453faf9014c0c` (the later commit adds evidence only).
Offline: 177 tests passed, five optional live tests ignored, one doc test passed,
Clippy/all-targets and formatting passed. This checkpoint does not complete the
overall survival construction goal.

## Terminal clearance and deliberate retreat verified (2026-10-02 UTC)

The approved follow-up prevents known unsuitable stopping positions before I/O,
exposes terminal replanning diagnostics, and supports explicit fresh-observation
reassessment of eligible fully dispatched failures. Actual and prospective
standing share the same conservative geometry implementation. No stopped
acknowledgement, automatic retry, hidden retreat or historic-state import is
introduced.

The non-OP trial now passes: walk/place, jump/place, rejection of a wall-touch
endpoint before input, then planned contact/retreat/rest/place. Three independent
block observations and material decrements 3 -> 2 -> 1 -> empty are retained.
The old unexplained count increase has a matching pickup receipt. See the
[implementation and scope](survival-movement-evidence.md#terminal-clearance-recommendation-implemented-2026-10-02-utc)
and [pinned live result](../vendor/voxrig/docs/evidence/survival-terminal-live-20261002.json).

Voxrig is pinned to `30afe28534e8b611ad5da8b77d8c20ecab35b861` (277 files);
actual live code was `bc2c13cb6a6927992049cfe0812b248dafcba285`, followed by
an evidence-only commit. Offline: 179 passing tests, five opt-in trials ignored,
one doc test, clean Clippy/format/package list. Observation-only recovery is
covered by TCP tests, not a separate live scenario. Next roadmap work remains
bounded route/retreat selection in the caller, site/access planning, durable job
integration and the complete roofed construction trial. The broad goal is active
and unfinished.

## Bounded route selection and return verified (2026-10-02 UTC)

DustRoute now searches finite multi-heading native controls without duplicating
player physics. Every successful candidate shares its initial observation;
execution revalidates the selected prediction under the native intent lock.
Body bounds, terminal clearance, explicit search limits and a predicted round
trip constrain the result. The return is predicted again after world changes.

The isolated non-OP wall-detour/ordinary-placement/return trial passed: 71
candidate predictions in 167 ms, 56 outbound ticks, independent dirt observation,
received inventory decrement and observed return to the starting area. See the
[architecture and limits](survival-navigation.md) and
[pinned evidence](evidence/survival-navigation-live-20261002.json).
Root library tests: 181 passed, five ignored; the opt-in live test passed
separately. Voxrig source: 180 passed, five ignored, one doc test; vendor pin
`5149f340b0728fe1747afde29ff52124645b3134` (277 managed files).

The route API is a Rust planning foundation, not a new public MCP construction
command. Site-aware immutable design requirements, temporary access scheduling,
durable survival execution and the roofed build/cleanup acceptance remain open.

## Grounded design foundation and access-planning concern (2026-10-02 UTC)

`generate_grounded_building_design` now authors a separate ground-aware immutable
contract through the shared building/Blueprint machinery. The baseline already
contains the flat ground. Ground remains protected during modeled construction
and removal; material counts include only new structure. Shared differential
physics proves restoration to that baseline. Named permanent air cannot be
overridden by ground. Adoption, serialization/restart, missing ground and blocked
interior are tested, together with unchanged ordinary building design/update
paths (20 passing targeted tests).

The new API does not establish live observation or player constructibility and
is not yet a public MCP entry. Future access geometry is the next prerequisite:
current native movement previews read only the live received world, so they
cannot establish a route on not-yet-placed scaffolds or after planned removals.
In accordance with the user's concern stop, that native extension is unimplemented
pending review of the [concrete proposal](survival-site-planning.md).
No live trial was performed in this grounded-authoring checkpoint. The complete
roofed construction goal remains unfinished.

## Approved hypothetical geometry and shared search checkpoint (2026-10-02)

The user approved the future-geometry prerequisite. Voxrig now captures bounded
immutable static scenes, evaluates hypothetical edits using shared native player
geometry, and keeps hypothetical predictions type-distinct from live movement
previews. DustRoute shares one route-search kernel between these two prediction
types; only live routes expose execution. The original concern is no longer
waiting for approval. Details and remaining work are in
[site planning](survival-site-planning.md#approved-implementation-in-progress).

Native tests pass 183 cases plus two doc tests, including a compile-fail type
boundary check. Root search tests and all-targets Clippy pass. The
[isolated live comparison](evidence/survival-scenario-live-20261002.json) verifies
identical captured/live route predictions, hypothetical obstruction invalidation
without a live edit, and actual non-OP detour/place/return. Source is Voxrig
`d2db52a9395fb9873b21da668041f74a6ee9ed0a` (280 managed files), live-tested in
DustRoute `1ed4a13`.

Next: declared actual temporary-access placement, climbing, retreat and mining
cleanup comparison; then access/dependency/material planning, durable execution
and full adopted roofed-structure acceptance. The whole goal remains active.

## Actual temporary access verified (2026-10-02 UTC)

The [declared finite access trial](survival-temporary-access-trial.md) passed on
an isolated non-OP vanilla 1.21.11 server. Three ordinary dirt placements consumed
3 -> 2 -> 1 -> empty. The bot climbed the platform, retreated to supported ground,
removed all three cubes and completed three explicit mining retirement/recovery
cycles. Each movement matched the earlier hypothetical frames before dispatch;
independent observations confirmed placements, endpoints, removals and the final
stone floor with exact air above it.

The [pinned evidence](evidence/survival-access-live-20261002.json) retains complete
received traces, connection changes, inventory outcomes and server/check logs.
Tested DustRoute source is `94c975c`; Voxrig is
`be55a64bc32da1ff0265ff0bdeb591f13d623137` (280 managed files). Native tests:
183 passed, five ignored; native and MCP all-targets Clippy passed; root formatting
passed. The opt-in live test passed in 47.31 seconds including the fixture gate.
The dedicated server saved and stopped with exit 0. Item drops were not credited
as resources and their collection is not part of this acceptance.

This verifies the declared access sequence, not automatic access-layout selection
or the complete adopted roofed build. Next are access/dependency/material planning,
durable survival execution and full construction/cleanup acceptance. The broad
survival construction goal remains active.

## Candidate site/action/material checks (2026-10-02 UTC)

A read-only caller layer now checks a proposed complete sequence using grounded
Blueprint geometry, explicit edit/temporary scopes and the shared native future
world predictor. It refuses protected/foreign removal and incomplete structure
or cleanup, and requires a safe final retreat. Materials distinguish permanent
use, cumulative temporary use without assumed recovery and peak temporary blocks
in the world. Diagnostics include action/cell context and structured shortages.
See [the contract and remaining work](survival-site-planning.md#candidate-sequence-validation-foundation-2026-10-02-utc).

This is candidate validation, not automatic access-work selection, adoption or a
durable executor. The full elevated roof's player sequence and live construction
acceptance remain unfinished. The broad goal remains active.
