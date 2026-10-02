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
- The current uncertainty guard refuses when the off-ray foot support is present.
- A diagnostic counterfactual omitting only that support leaves the native target
  unchanged and makes the uncertainty guard pass. This is NOT a permissible
  standing state or an instruction to remove the support.
- Adding a real obstacle at `[1,-59,5]` changes the native first hit to that obstacle.

The sampled rays characterize this refusal; finite samples alone do not prove
clearance for every possible eye position. No production native guard or vendor
snapshot has been changed. Stage 1 is incomplete; stages 2–5 have not started.

## Concrete prerequisite proposed for approval

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
than committing to an unverified workaround sequence. Approval is pending under
the user's instruction to stop when an undeclared prerequisite is preferable.

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
