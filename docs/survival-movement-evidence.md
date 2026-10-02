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
