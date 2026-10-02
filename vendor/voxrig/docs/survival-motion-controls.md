# Java 1.21.11 bounded survival motion

The common standing layer now distinguishes `Received` from
`PredictedAndObserved`. The latter means a locally settled dry-cube prediction
corroborated by a position receipt from a distinct observer connection. It does
not mean the server independently measured stopped motion. Neither observer
registration nor a ground bit is a server-time/causal fence.

`preview_survival_motion` is read-only. `start_survival_motion` preflights 1–120
ordinary digital-input ticks ending in released rest and retains the complete
intent before I/O. A connection-owned finite task sends input and position at
50 ms spacing, with delayed missed ticks rather than catch-up bursts. Dropping
a caller's wait does not abandon a jump halfway. The current/attempted tick and
completed dispatch count remain inspectable with `survival_motion` and
`operation_history`, including after closure. There is no automatic replay or
reconnect after a changed path, correction, impulse, failure or missing result.

The admitted model uses normal standing dimensions, default speed/gravity/jump/
step attributes, no received effects, and observed air or the twelve dry full
cubes already admitted by the stationary layer. It includes native directional
input, braking, gravity, collision axis order and tolerances, a 0.6f full-cube
step search, jump cooldown and landing. It does not simulate entities, fluids,
status-effect expiration, arbitrary terrain, sprint, sneak, or pathfinding. An
empty effect projection is not proof that every server effect is absent. The
input sequence is simulated against the starting snapshot and re-evaluated
against current geometry before every send; a changed result requires a new
plan. Falls exceeding three blocks below the initial feet are refused.

Packet-derived velocity is never overwritten by the model. In particular a
settled model retains its next-tick downward gravity velocity. A native position
correction remains a correction, not a normal movement echo; it invalidates
continuation and is not silently overwritten by the next input tick.

After the last dispatch a new exact UUID/entity/spawn/world watch must receive
position matching the predicted endpoint within the native quantization bound.
The 30-second timeout is only a wait budget. Silence, head rotation, old matching
coordinates, and a received zero velocity cannot complete this watch. Subsequent
standing queries also require a live matching observer instance; a busy observer
causes a read-before-action refusal, not a blocking reciprocal session lock.

Standing clearance uses horizontal uncertainty expanded about the prediction.
A supporting cube must intersect every possible horizontal footprint; floor
height comes from the predicted contact with freshly received full cubes. Target
checks conservatively require a clear eye-to-target bounding corridor and the
same in-reach face from its uncertainty extremes. This can refuse usable views
near unrelated solids; it intentionally does not guess an unobstructed view.
Placement still requires the independent existing target/material/interaction
result checks. Existing mining-retirement requirements are unchanged.

## Native correction compatibility

The modern tracked-player correction is `EntityPosition` plus fixed-width
relative flags and a ground bit. Own and remote handlers share its resolver.
Body rotation is separate from head yaw; corrections do not reset the native
tracked relative-position baseline, while absolute sync does. Remote relative
velocity remains unknown when the actual interpolated baseline is unavailable.

## Validation scope

The original Java helpers call unchanged target-version methods and codecs:
1,024 position/velocity/rotation corrections, 108 directional inputs, 72 voxel
collisions, eighteen input encodings, four full-position encodings, and the twelve
admitted materials. The fixtures and provenance distinguish these method oracles
from a complete native client tick/world simulation. Tick composition and step
search additionally use recorded native control-flow inspection and focused Rust
checks. TCP tests exercise retained intent, complete dispatch, interruption,
observer freshness, shared standing release and invalidation.

The ignored `native_survival_walk_jump_collision_and_place` test requires the
explicit dedicated non-OP fixture. Its source is a driver, not proof that a live
trial passed. A live result must be linked separately with its tested commit,
operator fixture log and received traces before claiming verified operation.

## Recorded live checkpoint (2026-10-02 UTC)

The [trial record](evidence/survival-motion-live-20261002.json) pins source
`f204cd2879030648c355ba73619453faf9014c0c` and retains received traces and the
operator/server log. Walk -> rest -> place and jump -> land -> place succeeded
with independent block observations and per-operation material decrements.

The third run reached the predicted wall contact, but conservative standing
admission refused it: the player center was predicted at X=3.699999988079071,
native half-width makes its right face exactly X=4, and the observer's relative
coordinate error expands that face into the wall. The final run is retained as
`RequiresInspection`. The overall live test exited 101, not a complete pass.
Under the user's concern stop condition no further implementation or movement
was performed. The dedicated server shut down normally.

Recommended next change for review: preflight the terminal standing clearance
before sending and select a resting endpoint with space from walls, using the
existing bounded input model. A collision test can touch the wall then retreat
before its terminal rest; this must be declared in the plan and observed, not
silently appended after failure. Define separate explicit recovery for an already
failed contact run before allowing further inputs. Do not label quantized
position as exact or relax the existing clearance check merely to pass the test.
