# Captured and hypothetical survival geometry

Java 1.21.11's `capture_survival_scene` copies a complete bounded static region
and the admitted native standing/player parameters under the live session lock.
Capture sends nothing. Unsupported, moving, incomplete or out-of-dimension cells
refuse; omitted cells never become air. Limits are 64 cells per axis and 32768
cells total. Include the standing/swept-collision halo, including cells below
the supporting floor, and target-ray cells in the capture.

`CapturedSurvivalScene::scenario` creates a detached branch. It owns no session,
sender or live inventory. `preview_path` and `after_path` share the exact native
motion model and static geometry reader with live prediction. Chained branches
retain velocity/jump cooldown and require conservative terminal clearance.
`after_edits` requires unique in-bounds cells with exact predecessors and admitted
successors. It cannot remove current foot support or intersect the standing body.
Changes are atomic and copy-on-write. Each branch admits at most 256 cumulative
edits and 4096 cumulative movement ticks; each path retains the 120-tick bound.

`preview_cube_placement` shares native outline traversal, cursor computation,
body occupancy and uncertainty checks with actual placement. Its returned edit
is hypothetical: inventory, permissions and result receipts are not inferred.
`preview_cube_removal` uses the same native target traversal and conservative
uncertainty checks, dirt/stone mining admission, and retained support. It does
not establish mining duration, empty-hand inventory, drop recovery or continuation
after a real mining action. Raw `after_edits` expresses assumed future changes,
not proof that a player can carry them out.

`HypotheticalMovementPreview` is distinct from `SurvivalMovementPreview` and
cannot be passed to the real movement API. A compile-fail doc test checks that
boundary. Private in-memory scenario identity distinguishes branches; serialized
diagnostics cannot reconstruct it. Provenance remains explicitly the original
capture, while hypothetical starting coordinates/bounds are separate fields.

`validate_survival_scene` compares the original capture's connection, generation,
dimension, world revision, player context, position and starting velocity with
current standing. A changed baseline requires replanning. This read-only check
is not a fence or execution permit. Every real movement/interaction still uses
its current-state intent checks and independently observed result. Predicting a
future stage must never replace actual action receipts.

TCP fixture tests compare captured and live movement frames/terminal clearance,
compare hypothetical placement against native live preparation, check scene/live
isolation and stale captures, and exercise placement, platform jumping, retreat,
removal geometry and rejection of deleting foot support. These tests are not
live-server access-work acceptance or the complete survival construction goal.
Caller navigation, reviewed edit scope, material reservations, durable jobs and
full construction/cleanup remain caller responsibilities.

## Separate clearance and prospective aiming

After hypothetical motion, conservative body/support checks retain the 1/16
terminal margin. Target-ray uncertainty instead follows `HypotheticalAimRequirement`:
the original captured position, or an explicitly required future independently
observed endpoint. The latter derives its maximum eye uncertainty from the same
packet-error and model-discrepancy limits used by actual endpoint admission.
It is not a `StandingPositionBasis` and cannot manufacture a received pose.

The requirement is available on a scenario, its movement preview and placement
proposal, including after edits. Actual placement/mining still use current live
standing evidence and their unchanged target-corridor, lifetime and mutation
checks. A missing or unsuitable observer result cannot be replaced by the future
requirement. The [boundary roadmap](survival-library-boundary.md) declares the
isolated edge comparison before its execution.
