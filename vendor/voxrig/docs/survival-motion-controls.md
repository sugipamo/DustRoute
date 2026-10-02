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
checks cover a continuous swept ray volume with the full observed eye-error box.
For admitted full cubes, affine plane-intersection bounds require every possible
eye to enter the same face strictly inside its edges and native reach. A segment
versus expanded-cell test covers every cell that native outline DDA can visit
before that face, including traversal/clipping tolerances. Any non-air or unknown
cell there is refused; cells outside the beam do not obstruct it. This remains
conservative for partial/protruding outlines because native selection only reads
shapes of visited cells. Native extreme rays additionally check consistency, but
are not the continuous occlusion proof. Grazing/edge/reach ambiguity is refused.
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

## Approved terminal clearance and explicit reassessment

`preview_survival_motion` now reports `terminal_clearance` independently of its
frames. `start_survival_motion` uses the same pure geometry scan as standing to
require a resting endpoint with a 1/16-block horizontal margin and conservative
support, before retaining/sending any input. This margin exceeds the admitted
observer quantization envelope (at most 2/4096 plus numeric tolerance about the
prediction). A touching endpoint is rejected as requiring replanning. Preview
remains useful for explaining the refusal. Callers declare the entire retreat
in their inputs; the client does not append controls or choose a hidden route.

An already failed run can be explicitly reassessed with
`prepare_survival_motion_recheck` and `observe_survival_motion_recheck`.
Preparation registers a fresh exact-run observer fence and leaves failure intact.
Only fully dispatched, predicted-rest runs with unchanged own generation, pose,
velocity/attributes and original observer lifetime are eligible. Observation
requires a new matching position and current conservative standing geometry.
Interrupted/corrected runs cannot be cleared by this API. Tokens cannot be
imported, reused after success, or used after another preparation supersedes them.
No method sends controls, automatically reconnects, or replays old input. The
original problem remains in history after successful reassessment. A still
obstructed or uncertain contact stays refused; it is not an escape maneuver.

The updated live driver first demonstrates the old wall-touch plan's pre-I/O
refusal, then explicitly declares contact -> released ticks -> backward input ->
rest, and places a third block after the resulting standing admission. This
section describes implementation; the separately pinned trial record establishes
whether execution passed.

The [terminal-clearance live result](evidence/survival-terminal-live-20261002.json)
pins `bc2c13cb6a6927992049cfe0812b248dafcba285`: walk/place, jump/place,
wall-touch preflight refusal, and planned wall-contact/retreat/place all passed.
Three independently observed dirt placements consumed the supplied stack 3 -> 2
-> 1 -> empty. The dedicated server shut down normally. Observation-only recovery
has TCP fixture coverage (including interruption/correction refusal); no separate
live recovery scenario is claimed. Route search and complete Blueprint building
remain separate roadmap work. The prior inventory increase is explained by the
[retained pickup receipts](evidence/survival-motion-pickup-diagnosis-20261002.json).

## Multi-heading prediction for caller-owned route planning

`SurvivalControl` carries a heading and digital input for each tick.
`preview_survival_path` / `start_survival_path` use the same bounded physics and
finite sender as the fixed-heading convenience methods. Preview/history now
retain the exact controls and starting generation. The 120-tick bound is shared
as `MAX_SURVIVAL_CONTROL_TICKS`; the client does not search for a route.

`start_previewed_survival_motion` recomputes a supplied preview while holding the
send-intent lock. Different connection/world generation, world revision, position,
player context or predicted frames reject before I/O. Saved or caller-modified
previews are constraints, not authority: current mode, geometry, terminal margin,
observer and mutation gates still apply. TCP coverage exercises turning around
an obstacle and rejects stale previews without emitting controls.

## Roof preflight regression (2026-10-02 UTC)

`roof_diagonal_view_allows_off_ray_foot_support_but_refuses_occlusion` preserves
the exact pose from the first refused roof candidate. At predicted feet
`[2.5,-59,6.544924947876652]`, placement towards the upper ground face at
`[0,-61,4]` now admits the dirt supporting the player at `[2,-60,6]` outside the
continuous ray volume. Inserting a real ray obstacle still refuses placement.
Standing support/body checks and the admitted pose-error bounds are unchanged.

Focused coverage also exercises all six target faces, vertical and horizontal
origin error, face/corner boundaries, reach endpoints, unknown cells, conservative
partial-shape refusal and closed segment contact. Existing native outline oracle
fixtures remain the reference for direction/traversal. This describes geometry
validation; complete live roof execution is separately evidenced by DustRoute.
