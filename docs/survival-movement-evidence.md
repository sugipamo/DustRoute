# Survival movement: position evidence before live control

## Finding

The next roadmap step is ordinary walk/stop/jump/settle. Source review found a
construction continuation boundary that cannot be resolved by transplanting the
1.16.1 sender or merely waiting after each movement.

- The existing stationary context requires `position_from_server` and a zero
  resolved received velocity. It is shared by look, placement, mining and fresh
  mining recovery.
- Native 1.21.11 accepts ordinary movement by updating the server player and its
  tracker. Its normal success path does not echo an own-position receipt for each
  step. Correction/teleport messages are different events.
- Current independent observations are viewpoints: relative positions have native
  quantization, rotation/metadata can advance the common receive sequence, and
  ground/velocity fields are not retained as motion evidence. They cannot be
  treated as an exact fresh arrival acknowledgement without changes.

Keeping the present gate unchanged would prevent building after a successful
walk. Simply marking local prediction as received would weaken the meaning of
existing checks. This concerns the final goal directly: movement must end in a
usable, honestly described construction posture.

The [source audit](../vendor/voxrig/docs/survival-movement-foundation.md) and
[validation record](evidence/survival-movement-foundation-20261002.json) distinguish
native control-flow inspection from tests. No new server was started or movement
trial run for this finding. Existing stationary placement/mining remain intact.

## Completed prerequisite

Eight motion-related player attributes now retain native defaults, limits and
received updates with provenance. A typed Rust table routes all supported own
attributes, including the existing four. The unchanged-body oracle verifies
actual tracked definitions and float-derived defaults. This is state projection,
not a completed physics engine or movement capability.

## Concrete proposed next slice

Keep this in the shared 1.21.11 motion/standing layer rather than adding a special
post-walk bypass in placement or mining.

1. Introduce explicit received-pose, client-motion-prediction and movement-attempt
   types. Preserve packet-derived velocity separately from simulated velocity;
   retain each attempted movement before I/O. Reset/correction/interruption must
   invalidate the appropriate generation and leave historical diagnostics.
2. Audit and implement the admitted dry-cube native tick/collision/input model.
   Compare forward/diagonal input, braking, floor/wall/head collision, jump/landing
   and corrections against target-version oracle cases. Do not import the older
   adapter's constants or normalization merely because its tests pass.
3. Define the post-movement standing contract explicitly. A locally settled model
   is prediction, not a server receipt. Extend the independent observer with
   exact player identity, position-specific receipt ordinals, native ground/velocity
   evidence where packets actually supply it and quantization bounds. Evaluate
   what this proves about arrival; stale rotation/metadata updates, silence and
   fixed sleeps must not become confirmation.
4. Connect movement results and fresh standing geometry to the common operation
   gate. Placement/mining use that shared contract, preserving result observations
   and material accounting. Missing or conflicting evidence stays inspectable;
   no automatic resend, teleport, command or mode-change fallback.
5. On the dedicated non-OP fixture, demonstrate walk -> stop -> ordinary placement,
   jump -> land -> placement, and collision/correction handling. Retain input,
   prediction and independent observation as distinct evidence. Only then proceed
   to bounded route/retreat planning, access works and the roofed Blueprint trial.

This is a medium change spanning native state, player observation, motion control
and shared standing admission. It does not add a server MOD or general entity
simulation. If the native audit cannot establish the proposed observation boundary,
report that result before weakening the construction contract.

The user approved this slice. The received-position/local-submission separation
and read-only independent observer receipts have been implemented and tested.
The physics prediction model, survival movement sender and post-walk construction
release remain unimplemented. This is a partial checkpoint, not completion of
steps 1–5 or of the overall construction goal.

## Further finding and explicit stop

Native source inspection found that the ordinary server movement path takes the
moving client's ground flag into `Entity.setMovement`, and remote tracking
packets carry the resulting `isOnGround()` value. Thus an observer's ground receipt
is not an independent server measurement of stopped motion. Relative coordinates
also have quantization bounds, even for encoded zero deltas; a previous velocity
sample and another connection's receive ordering cannot close this proof gap.

The [audited path and input hashes](../vendor/voxrig/docs/evidence/survival-motion-evidence-source.json)
record the evidence. This was source inspection and offline validation; no live
server or new movement trial was used. Following the user's concern stop,
construction admission after walking has not been weakened or enabled.

The correction-path review additionally found a pre-existing remote
`ENTITY_TELEPORT` parser using the old XYZ/byte-angle layout. Native 1.21.11
`EntityPositionS2CPacket` carries an `EntityPosition` change and relative flags.
This unimplemented tracked-player packet now refuses explicitly without publishing
new position evidence. Proper native decoding must be validated before the
movement/correction trial. The separate absolute sync and own-position handlers
remain supported; current offline tests do not establish complete correction
coverage.

## Concrete next contract for review

Recommend an explicit `PredictedAndObserved` standing basis, with separate
fields for predicted tick/pose/velocity/contact, independent observer connection,
world and exact player lifetime, position/ground/velocity receipt ordinals and
quantization bounds, and the geometry revision used for clearance/support/reach.
These records are evidence, not importable authority. They must not be labelled
server-confirmed rest or copied into `position_from_server`.

The implementation sequence would be:

1. Implement and verify the native remote relative-correction packet first.
   Audit and compare the dry-cube tick model against native input, collision,
   braking, jump and landing cases. Keep simulated velocity separate from packet
   velocity, including the downward gravity term while resting on a floor.
2. Retain each local movement intent before I/O. Require a locally settled model
   and a fresh same-instance observer position consistent with its error bounds.
   Observation registration is not a server-time fence; do not claim causality
   or infer acknowledgement from silence, rotation or elapsed time.
3. Recheck current conservative geometry over the position uncertainty range.
   Reject changed generation, corrections requiring replanning, unsupported
   conditions, incompatible observations or interrupted submissions. Do not
   reconstruct authority from saved diagnostic records.
4. Use the shared standing basis in placement/mining. Preserve fresh target,
   inventory and interaction results; mismatches stop for diagnosis rather than
   blind retry. This offers bounded operational confidence, not protection
   against arbitrary concurrent player/world edits.
5. Demonstrate walk -> stop -> place and jump -> land -> place on the non-OP
   isolated fixture before navigation, access works and the final roofed build.

This approach does not require a MOD. The review point is the stated basis for
continuing construction: predicted stability corroborated by observations,
rather than an independent server stop acknowledgement that these packets do
not provide.

## Approved implementation and live stop (2026-10-02 UTC)

The user approved the updated `PredictedAndObserved` contract. Native correction
handling, bounded dry-cube prediction, retained finite control runs and shared
standing admission are implemented in Voxrig. Packet velocity remains separate
from prediction. The [implementation contract](../vendor/voxrig/docs/survival-motion-controls.md)
and [live evidence](../vendor/voxrig/docs/evidence/survival-motion-live-20261002.json)
supersede the earlier unimplemented checkpoint above.

Non-OP live walking -> stopping -> placement and jumping -> landing -> placement
succeeded. The wall test reached its predicted contact but failed post-motion
standing admission. For the X=4 wall, predicted center X=3.699999988079071 plus
native half-width reaches exactly X=4. The observed center X=3.699951171875 has
relative quantization error 1/4096, so conservative body clearance cannot exclude
intersection. The run remains `RequiresInspection`; the complete live test exited
101. This is a demonstrated limitation, not a full passing trial.

Implementation stopped under the user's concern condition. No further movement,
retry or relaxed admission was performed. Both clients disconnected and the
isolated server stopped normally. Offline verification and evidence packaging do
not resolve this wall-contact continuation limitation.

### Concrete next work for approval

1. Preflight terminal clearance before sending controls. A plan should end at a
   resting location with a margin from solids instead of discovering at the end
   that quantized observation cannot support construction admission.
2. Declare collision -> retreat -> rest as an entire bounded input plan when
   testing wall contact. Predict and observe the retreat with the existing model;
   do not append hidden recovery inputs after a failed run.
3. Define explicit recovery for an already failed contact run. Keep observed
   position uncertainty and current-world checks; do not reinterpret received
   coordinates as exact or simply clear `RequiresInspection`.
4. Re-run the declared wall/retreat case and the walk/jump placement regressions,
   then continue bounded navigation and the roofed Blueprint goal.

One additional retained observation needs diagnosis: the two placements each
have target/ACK and held decrements (3 -> 2 and 2 -> 1), while a later inventory
receipt reports 2. No cause or final net material accounting is asserted; the
trace is retained. Entity simulation remains excluded.

## Terminal-clearance recommendation implemented (2026-10-02 UTC)

The user approved the recommendation above. Prospective terminal standing now
uses the same geometry scan as actual standing, with a 1/16-block horizontal
margin. Preview returns a structured admitted/replan result; starting a plan
refuses an unsuitable endpoint before recording or sending controls. Retreat is
explicitly part of the caller's complete input sequence, never hidden recovery.

`prepare_survival_motion_recheck` / `observe_survival_motion_recheck` provide
explicit observation-only reassessment of fully dispatched, predicted-rest failed
runs. They require a fresh same-instance position, current conservative standing
geometry and unchanged own context. Partial dispatch, corrections, foreign or
superseded tokens, stale observations and unsuitable geometry remain refused.
Original failure history remains after successful reassessment; no input is
resent and historical JSON cannot authorize a fresh connection. This is
reassessment when evidence/geometry permits it, not an automatic escape from an
obstructed position.

The [new non-OP live record](../vendor/voxrig/docs/evidence/survival-terminal-live-20261002.json)
passed walking/place, jumping/place, pre-send wall-touch refusal, and declared
wall contact -> retreat -> rest -> place. The three independently observed
placements consumed the supplied dirt 3 -> 2 -> 1 -> empty. The dedicated server
stopped normally. Recovery has TCP fixture coverage; it was not separately
exercised live. Snapshot checks cannot reserve future world geometry.

The previous inventory increase is explained by [retained pickup evidence](../vendor/voxrig/docs/evidence/survival-motion-pickup-diagnosis-20261002.json):
COLLECT receipt 247 identifies the bot as collecting one item; the following
SET_SLOT receipt 248 changes dirt count 1 -> 2. This is distinct from placement
consumption. The item's original drop/source is not established, and no entity
simulation feature was added.

The wall-clearance recommendation is complete within this admitted scope.
Bounded route search, access works, durable construction jobs and the full roofed
Blueprint trial remain roadmap work; the overall goal is not complete.
