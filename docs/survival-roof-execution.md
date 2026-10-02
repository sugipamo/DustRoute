# Fixed roof execution and remaining survival roadmap

Approved 2026-10-02. Execute these as successive goals, preserving the original
roofed construction objective. Stop and report genuine correctness concerns or
necessary undeclared prerequisites; do not weaken physics, scope or evidence.

1. **Active:** a complete authored roof sequence through the common executor,
   exact site and temporary cleanup verified on isolated non-OP vanilla.
2. Generate access/action sequences from a bounded design and observed site.
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
