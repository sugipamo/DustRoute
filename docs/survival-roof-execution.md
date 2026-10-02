# Fixed roof execution and remaining survival roadmap

Approved 2026-10-02. Execute these as successive goals, preserving the original
roofed construction objective. Stop and report genuine correctness concerns or
necessary undeclared prerequisites; do not weaken physics, scope or evidence.

See the [next-milestone roadmap](survival-construction-roadmap.md) for user-visible
outcomes, stage dependencies, acceptance criteria and deferred scope.

1. **Complete:** a complete authored roof sequence through the common executor,
   exact site and temporary cleanup verified on isolated non-OP vanilla.
2. **Active:** generate access/action sequences from a bounded design and observed site.
3. Connect adopted Blueprint/policy and public MCP job orchestration.
4. Reobserve a stopped site and create a new checked continuation; no replay of
   serialized native tokens or uncertain old actions.
5. Exercise whole-building material shortage, blocked access, unexpected edits
   and disconnects, retaining correct diagnoses and remaining work.

## First goal contract

Keep the existing five-by-five roof (relative y=6), four corner columns (y=0..5),
49 cobblestone, protected flat stone ground and exact interior air. Translate
standing ground to y=-60 in the isolated fixture. The floor is never editable.
Supplied materials are in the bot's inventory. Temporary works use admitted dirt
cubes outside the permanent structure/interior; drops never count as supplied
resources. The candidate uses two compact alternating-column access towers,
first for the rear columns and then for the front columns/roof, followed by
explicit descending cleanup and ground retreat. Candidate details may be adjusted
read-only until a complete shared-native geometry proof exists.

All ordering/waypoints are authored for this reference design. Bounded native
control preview selects inputs to reach those waypoints without a second physics
model; this is not arbitrary Blueprint/action generation. Every proposed step
must pass the existing construction checker before any live construction begins.

Use a fresh isolated vanilla 1.21.11 world on localhost:25572. Non-OP builder
NatMineBot starts at [2.5,-60,7.5]; independent NatMineView at [7.5,-60,2.5].
Fixture console supplies the initial floor/air, positions and complete material
budget only. Received traces, checked plan, source revisions, durable journal,
independent exact final observation and all attempted refusals are retained.
A preflight-only run may capture the prepared world and test detached candidates;
it must not submit building/movement/mining actions. Live execution is enabled
only by the separately selected acceptance run after preflight succeeds.

## Preflight finding and concern stop

The first candidate (`4b1f1eb`) was evaluated with `execute=false` in a fresh
isolated world. It planned one temporary dirt at `[2,-60,6]` and a 29-tick jump
from `[2.5,-60,7.5]` to hypothetical feet `[2.5,-59,6.544924947876652]`.
The next proposed permanent cube `[0,-60,4]`, clicking the ground's upper face
`[0,-61,4]`, was refused with `uncertain eye corridor is not clear`.
None of those planned edits or movement inputs were executed. The retained native
history has no placement, mining, survival motion or local movement submission.
Fixture console setup remains distinct from construction. The server stopped
normally, and the preflight refusal is retained as a refusal, not a passing roof.

The cause is Voxrig `survival::uncertain_target_in`: for nonzero horizontal
position uncertainty it visits **every cell in the axis-aligned box** between
eye and target, enlarged by the uncertainty. Every non-air cell other than the
hit block causes rejection before the four extreme rays are examined. The
standing dirt `[2,-60,6]` lies in that box although the intended ray passes above
it. The native motion document already describes this deliberate conservative
limitation; this is its manifestation in the roof candidate, not evidence of
unsafe native acceptance or proof that all other roof sequences are impossible.

A **test-only native characterization** now uses this exact pose and scene:

- Native center ray and 25 sampled rays across the admitted horizontal error
  `2/4096 + 1e-9` all hit the intended ground block's upper face within reach.
- Placement geometry with zero aiming error admits the requested cube.
- The prior uncertainty guard refused when the off-ray foot support is present.
- A diagnostic counterfactual omitting only that support leaves the native target
  unchanged and makes the uncertainty guard pass. This is NOT a permissible
  standing state or an instruction to remove the support.
- Adding a real obstacle at `[1,-59,5]` changes the native first hit to that obstacle.

The sampled rays characterize this refusal; finite samples alone do not prove
clearance for every possible eye position. This characterization did not change any production guard or vendor snapshot. Stage 1 is incomplete; stages 2–5 have not started.

## Approved prerequisite: continuous ray uncertainty

Refine the native uncertainty/occlusion check before further roof work. Keep
Voxrig responsible for geometry, and keep DustRoute's permission/resource/sequence
responsibilities unchanged.

1. Use the same native ray direction, face and reach definitions. Derive a
   conservative interval ending no earlier than any permitted eye's intersection
   with the intended face. Keep all possible eye origins within the declared
   position-error box.
2. Check possible intervening occupancy against the swept ray volume, for example
   by testing the center segment against cells expanded by the eye-error bounds.
   This must conservatively cover the **continuous** uncertainty volume; four or
   25 sampled rays are not a substitute. Treat unknown/unsupported occupancy as
   refusal and retain conservative treatment of non-full-cube cells.
3. Retain target-face/reach consistency, numerical boundary handling, actual
   received/independently-observed pose admission, standing clearance, body
   collision, native mutation guards and placement/mining receipts. Do not shrink
   uncertainty, weaken support checks or create a test-only execution exception.
4. Share the corrected check across real placement, mining and hypothetical
   geometry. Test this off-ray support case, true intervening obstacles, edge/
   corner-only ambiguities, reach endpoints, missing cells and same-face bounds.
   Compare against the existing target-version ray helpers where applicable.
5. Validate/commit native changes separately, import the unmodified vendor pin,
   then repeat the complete roof preflight. Only after it passes, declare and run
   the non-OP live construction/cleanup acceptance through the common executor.

Estimated scope: a localized-to-medium native geometry change plus focused
regression/live checks. Changing build order may avoid this one refusal, but
would not remove the same conservative limitation from general scaffold/roof
work. The recommendation is to fix the shared admission mechanism first rather
than committing to an unverified workaround sequence. The user approved this
prerequisite explicitly; implementation and validation are now in progress.

## Retained validation

[Source-pinned preflight and characterization evidence](evidence/survival-roof-preflight-20261002.json)
includes the refusal, complete explicitly started receive traces, fixture/server
logs and diagnostic test source. Native characterization source is `6b47f26`;
production/vendor remains `1a8f258`. The native characterization passed one test;
consumer related checks passed 19 tests (four opt-in cases ignored in that run).
Native and consumer all-targets Clippy and formatting passed. Initial consumer
Clippy found two redundant test-only `BlockFace` clones, which were removed; the
initial check log is retained. The roof preflight itself remains a refusal, with
zero executed construction actions. Its isolated server stopped normally.

## Approved implementation progress

The native implementation now bounds the face-plane intersection over the whole
three-axis eye-error box. It checks a continuous center segment against cells
expanded by that error, retaining native DDA/clip tolerances. Only cells the beam
can visit are read; all intervening non-air/unknown cells still refuse. This does
not approximate partial shapes as full outlines: it conservatively refuses their
owning cells whenever native DDA might visit them. Extreme native rays remain an
additional consistency check, not the occlusion proof. Both actual and detached
placement/mining call this shared implementation. Body/support checks are unchanged.

Native source `1877209da48a78443dd7d30e7db6720bd6775416` is imported with 294
verified managed files. Native library validation: 194 passed, six explicitly
ignored live fixtures; all-target Clippy passed. DustRoute's Voxrig-enabled
survival tests: 19 passed, four ignored live fixtures. An earlier consumer test
invocation omitted the feature and selected zero tests; it is not validation of
this path.
Complete roof preflight and live construction remain required. The historical
preflight refusal and its original source pins are retained unchanged.

## Second preflight: shared Blueprint anchor concern (2026-10-02 UTC)

The approved native fix and Voxrig-enabled consumer checks passed. The new
`execute=false` preflight at DustRoute `648e923` / Voxrig `1877209` advanced past
the previous sight refusal and reached `design()` after the final hypothetical
scaffold removal. It failed while generating the grounded Blueprint, BEFORE
`preview_construction_sequence` and BEFORE creating the real executor. This is
not a complete checked plan and not live building acceptance.

The body clearance `layout` port in `building/sources.rs` is always anchored at
`Pos::default()` (world origin), while its pattern contains world-coordinate
offsets. The roof's known region is `[-1,-61,-1]..[5,-53,5]`; origin is outside it.
The reviewer therefore reports an undetermined fixed terminal at `[0,0,0]` with
`unknown initial coordinate`. Individual part/space anchors already use positions
inside their geometry. This issue is in the common building source generator,
not Voxrig visibility, Minecraft observation, or a placement failure.

The trial's `design()` used `unwrap`, so this error panicked before its usual
JSON/history/trace save; the output JSON is empty. Retained server/test logs plus
the pinned `execute=false` source establish where the attempt stopped, but do not
supply a complete received-trace record. Do not present this as an observed
complete planning pass. Server shutdown finished normally (exit 0), test exit
101. Only fixture setup commands modified this isolated world; no construction
or movement controls were dispatched. See [the new evidence record](evidence/survival-roof-swept-ray-20261002.json).

### Concrete next prerequisite, not implemented

1. In the common building source generator, select a known, non-reserved anchor
   from the clearance pattern and express each pattern position relative to it.
   Preserve the exact absolute cells, typed obligations, declared region and
   edit permissions. Do not expand the known region just to include world origin.
2. Verify translated positive/negative designs and existing grounded, composed
   and permanent-air contracts. Pattern anchors affect more than this test, so
   prove equivalent absolute targets and preserve rejection of real violations.
3. Return roof design failures through the driver's ordinary error path so its
   evidence survives refusal; this is test-driver error handling, not recovery
   authority or a change to production execution gates.
4. Repeat the full preflight and, only on success, the live roof acceptance.

This is a small-to-medium shared Blueprint correctness prerequisite. Under the
user's stop-on-new-concern instruction, its implementation and dependent live
construction are stopped pending approval. The continuous-ray native change is
complete and separately committed; goal 1 and stages 2–5 remain unfinished.

## Blueprint anchor prerequisite approved

The user approved the shared source fix. The body clearance terminal now uses
its first declared, non-reserved pattern cell as anchor; offsets are translated
relative to it. Empty patterns remain invalid. No known region, protected-ground
contract, permanent-air requirement or edit scope is enlarged. Existing saved
records are not rewritten. Regression checks cover both source callers at
positive and negative translated coordinates, exact absolute pattern coverage,
adoption/reload, ground removal and intrusion into permanent air. The roof driver
propagates Blueprint diagnostics through its evidence-saving error path and
checks the site contract before searching the authored movement sequence.

Full preflight and live acceptance are still required after these checks.

Validation of the anchor change: 24 building/grounded/design/update/door tests
and 19 Voxrig-enabled consumer survival tests passed (four live tests ignored).
Existing tests that inspected body pattern offsets as absolute positions were
updated to add the declared anchor; reservation/air assertions keep their exact
world coordinates. Translation library/tests Clippy passed. The wider translation
all-target invocation found two existing unused helper functions in the unchanged
`flying_machine_assembly_fixture` example; that result is retained, not claimed
as an all-target pass. No unrelated sample cleanup is included.

## Complete preflight and first live roof attempt (2026-10-02 UTC)

At `4fdba7f` with native `1877209`, `preflight-c` passed the complete shared
construction checker: 49 permanent placements, 19 temporary placements, 19
removals and 32 movements (119 actions). The supplied-material proof needs 49
cobblestone and 19 dirt without crediting drops; peak temporary occupancy is 12.
The checked final position is `[1.4345243952333269,-60,-1.4999008496360369]`,
inside the declared ground retreat. `live-a` accidentally omitted `--execute`:
it is another passing preflight only, despite its filename. Both were read-only
after fixture preparation, and their servers stopped normally.

`live-b --execute` then used a new isolated vanilla survival world with no OPs.
The common executor completed 26 actions: 12 permanent cobblestone, seven dirt,
five moves and two temporary removals with two confirmed retirement/reconnect
cycles. All 19 placements and five motions have their normal independent native
receipts. The next movement was refused BEFORE dispatch with
`movement_plan_changed`. The five recorded remaining owned dirt cubes are
`[1,-60,6]`, `[1,-59,6]`, `[2,-60,6]`, `[2,-59,6]`, `[2,-58,6]`.
The partial site is saved in the isolated world; no automatic cleanup, new motion
or terminal retreat was attempted after refusal. Roof completion and final whole
site independent verification have not happened. Test exit 101, server exit 0.
The persistent journal matches the stopped record in the trial output.
See [source pins, checks and retained evidence](evidence/survival-roof-anchor-live-20261002.json).

### New concern: reconnect changes the prediction initial state

The planned and received starting position for action 27 agree exactly:
`[2.436727277038884,-57,6.5000687949723455]`. Thus this evidence does not indicate
position drift or support enlarging position tolerance. The preceding checked
and independently completed movement ended with model Y velocity
`-0.0784000015258789`. The detached scenario carries that velocity through block
edits. After mining retirement, the fresh connection receives zero velocity and
its `Received` context starts `Model::new` with zero velocity. Only the
`PredictedAndObserved` branch preserves the previous predicted velocity.

Source inspection shows why the phase matters: the first zero-Y intent does not
set `on_ground` in `Model::advance`; the next intent may then use air acceleration
instead of ground acceleration. The executor correctly refuses differing frames.
However, it currently stores only the refusal message, not the fresh refused
trajectory. The exact complete frame delta therefore remains to be characterized;
these are retained initial-state facts and source analysis, not a claimed
comparison of a missing trajectory.

### Concrete next prerequisite, awaiting approval

1. Retain initial model provenance and checked/fresh mismatch evidence before
   any send; add a focused native read-only regression using this pose/support.
2. Represent expected connection-reset/recovery as a typed boundary in native
   hypothetical prediction. A future required receipt must not become fabricated
   already-received position authority. Keep native physics in Voxrig.
3. Have DustRoute's checked removal/retirement sequence use that boundary and
   revalidate the remaining complete plan, matching its actual reconnect policy.
   Do not relax frame equality, omit early frames or assume a small positional
   difference is the only possible effect.
4. Re-run complete preflight and live roof/cleanup/retreat acceptance. The broader
   stopped-job/new-plan facility remains stage 4; this prerequisite concerns the
   normal executor's existing mandatory recovery inside one construction plan.

This changes the shared prediction/execution boundary and is stopped under the
user's new-correctness-concern instruction. The approved Blueprint fix is complete;
the roof goal and subsequent goals are not complete. No runtime recovery change
has been made in response to this new finding.

## Reconnect prediction prerequisite approved

The user approved the explicit lifecycle boundary. Native source `2b6e7bf` adds
`SurvivalScenario::after_expected_reconnect` and a non-deserializable
`HypotheticalReconnectBoundary`. It preserves feet, cells and conservative
standing margins while initializing the shared native model as a received new
connection. The future aim requirement stays explicitly hypothetical. Validation
requires a different connection from the retired source, the same dimension and
exact feet, and an actual received standing basis from a newly captured scene.
It grants no operation authority and does not implement retirement or job policy.

DustRoute's checked removal sequence now retains that obligation, and its common
executor verifies it after actual retirement/reconnect and site reconciliation,
before completing the removal. The authored roof candidate uses the same native
transition. Physics remains in Voxrig. Live and detached previews expose a
canonical tick-zero `initial_frame`; movement comparison retains exact frame
checks and also compares the initial frame. A mismatch now saves both previews
and the first divergent frame index before refusing dispatch.

The recorded pose/support/control regression confirms the root cause: carrying
rest gravity gives first-frame ground contact; received reset starts with zero
velocity, has no first-frame downward contact and changes later horizontal motion.
The explicit reset matches the fresh native preview exactly. The regression also
refuses same-connection and changed-position receipts and confirms no packets
are sent by hypothetical APIs. Native library: 195 passed, six live fixtures
ignored; four doctests and all-target Clippy passed. Complete roof preflight and
live acceptance still need to be repeated with this source.

Voxrig-enabled consumer tests: 19 passed, four live fixtures ignored. Adding the
native initial frame exposed one test-only navigation mock initializer; it now
explicitly supplies its synthetic initial frame. The mock remains a search/budget
test, not a Minecraft physics model or an execution path.

`preflight-d` with the explicit reset correctly refused a late descent's authored
center preference: the nearest natively admitted landing was 0.2081 blocks from
the waypoint, outside the recipe's 0.13 preference. No construction was sent.
The descent-only waypoint tolerance is now 0.23; candidates must still pass the
same native terminal-clearance, support and travel checks, and execution still
requires exact predicted frames and received reconnect position. This changes
candidate selection, not native physical or execution tolerances.

## Fixed reference completed (2026-10-02 UTC)

Complete preflight `preflight-e` and actual execution `live-c` passed on consumer
`be93ae54f17444310d15f9c5f6cfda04dd12c11f`, native
`2b6e7bfc94e6270054eac5c7b14a74d4657a411c`. The common executor completed all
119 steps: 49 permanent cobblestone placements, 19 temporary dirt placements,
32 movements and 19 temporary removals. Each removal's actual fresh received
start satisfied its planned reconnect boundary. All temporary ownership was
cleared and the bot retreated to ground at
`[1.436997156103756,-60,-1.4998972560732997]`.

Final exact verification used an independent connection over all 3,120 observed
cells. The expected permanent geometry is the full 25-block roof and four
six-block columns; all 49 protected floor cells in the Blueprint also match.
The persisted execution journal equals the record in the trial output, with
`completed_steps=119`, `outcome=observed`, `continuation=completed`. All 21 retained
receive-trace windows are complete. Reconnect windows start after setup; their
received start contexts are retained in the journal. Final independent comparison
is recorded as a count and successful pinned check, not a separate full snapshot.

The isolated non-OP survival test passed in 215.57 seconds including fixture
preparation and preflight; the dedicated server saved and stopped with exit 0.
Fixture commands only supplied the initial environment, position and inventory.
Construction, movement and cleanup used ordinary client actions. No drops were
credited toward the required 49 cobblestone and 19 cumulative temporary dirt.
See [pinned evidence and checksummed archives](evidence/survival-roof-complete-20261002.json)
for the successful runs, the preceding preflight refusal, tests and journal.

Stage 1 is complete. This is a checked authored sequence for the reference
building. Automatic action/access selection, public Blueprint/MCP integration,
new plans after interrupted jobs and whole-building failure coverage remain the
separate stages above.

## Stage 2: automatic sequence generation

The next goal is created. Existing `ConstructionSite` supplies exact baseline,
target cells and explicit permissions. `preview_construction_sequence` is the
final authority for a complete hypothetical candidate. `plan_hypothetical_route`
already searches native predictions, but its round trip is against one unchanged
scene; it is not evidence of cleanup or escape after future edits. The generator
must check the actual subsequent edits and retreat as part of the whole sequence.

Implement in this order:

1. Define bounded caller search inputs, counters and structured failures around
   the existing site and supplied-material budget. Distinguish invalid input,
   unsupported contract, shortage and no complete plan within search limits.
   Preserve geometry refusal examples and unfinished targets for diagnosis.
2. Generate placement/support/standing candidates from target and observed cells
   within the declared scopes. Rank dependency and access candidates, but let
   native geometry decide placement, reach, body clearance and movement validity.
   Neither a topological order nor distance alone proves a player sequence.
3. Search temporary access, placement order, removal and retreat together. Carry
   exact native scenarios, ownership and cumulative material use per branch.
   Apply the explicit native reconnect transition after each hypothetical removal.
   Bound attempts including rejected previews; do not merge states solely by
   rounded position when native velocity, geometry or lifecycle state differs.
4. Pass complete candidates through the common checker. Test the original roof,
   translated geometry and genuinely different dimensions/layouts, plus budget,
   material and scope refusals. Replaying the authored coordinates or receiving
   caller-authored operation lists does not satisfy automatic generation.
5. Run a generated original-roof plan through the same non-OP live executor and
   retain exact independent final-site/cleanup/retreat evidence. Keep the authored
   reference as a regression baseline rather than a fallback success claim.

Initial coverage remains the already admitted passive dry cubes and current
bounded site/native edit/tick budgets. Search is not required to prove physical
impossibility or to find every possible construction sequence. No public MCP,
adoption authority, interrupted-job replay, new native physics or resource
collection is added in this stage. A newly required correctness prerequisite
must be reported before dependent implementation proceeds.
