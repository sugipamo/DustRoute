# Separate standing clearance from hypothetical aiming uncertainty

## Finding (2026-10-02 UTC)

The roofed construction objective remains the five-by-five roof at y=6 with four
columns, protected flat ground and final interior air. No simpler target has
replaced it. During access/action selection, a native geometry discrepancy was
found for placing sideways while standing partly over a supporting cube's edge.
This is a concern stop under the user's instruction. Production behavior has
not been changed; only a characterization test and this proposal are added.

The current `SurvivalScenario::after_path` sets its shared geometry error to
`[1/16, 0, 1/16]`. That value is the intentional conservative terminal standing
margin. The same error is also passed to `uncertain_target_in` for hypothetical
placement and removal. Consequently, the hypothetical aimer treats the entire
standing margin as uncertainty about the eye's actual position.

Actual motion admission has a different contract. `matches_endpoint` requires
per-axis observer packet error at most `1/4096` and model/observer discrepancy at
most that error plus `1e-9`. `StandingPositionBasis::horizontal_error` sums these
two terms. Its maximum admitted horizontal aiming error is therefore
`2/4096 + 1e-9 = 0.00048828225`, whereas future aiming currently uses `0.0625`.
The initial received-pose branch uses zero error; no actual observer receipt is
fabricated by this calculation.

## Reproduction and limits of the evidence

The native loopback fixture puts feet at `[1.2,1,0.5]`, with stone support at
`[0,0,0]` and air at `[1,0,0]`. Rotation `[90,84.61069]` targets the support's east
face to place dirt in that air cell.

- The captured received-pose branch admits placement.
- Advancing the same branch by three released ticks leaves the position exactly
  unchanged and passes conservative terminal clearance.
- Placement then refuses with `target face/reach differs across observed
  position uncertainty` because `after_path` changed the aiming error to 1/16.
- The shared native target checker admits the same hit with the maximum error
  allowed by actual endpoint admission, `2/4096 + 1e-9`.
- Original live capture provenance remains valid and the loopback peer receives
  no action packet. This is a native fixture test, **not Minecraft live bridging**.

Test: `hypothetical_edge_placement_exposes_post_motion_aim_margin` in Voxrig's
`operations/mining/tests.rs`. The pinned evidence records the exact test-only
source and check logs. DustRoute's production vendor remains at `be55a64`.

This demonstrates one relevant false refusal, not that every possible roof
construction schedule is impossible. Different access layouts may avoid this
specific geometry. No unsupported inference is made about crouching, actual edge
walking or complete roof construction. Planning should not silently lower the
roof, weaken final air obligations or widen execution permissions to hide it.

## Proposed next work for approval

1. Name and share the existing endpoint observation limits in the native motion
   layer. Derive the prospective aiming bound from those exact limits; do not
   maintain a second unrelated magic number in the planner.
2. Distinguish conservative body/support clearance from prospective eye-position
   uncertainty in the hypothetical scenario. Preserve the existing 1/16 terminal
   clearance and support checks. Hypothetical aiming may use the smaller bound
   only as an explicit requirement for a later independently observed endpoint.
3. Keep actual placement/removal checks on fresh `StandingPositionBasis` evidence,
   with their current geometry, uncertainty-corridor and lifetime checks. A future
   bound is never a received pose, observation, or permission to submit an action.
   Missing or out-of-bound observation must still stop the real executor.
4. Extend native tests to compare received and future geometry, edge targets and
   ambiguous hits, plus refusal of insufficient support and invalid observation.
   Preserve the compile-time separation between hypothetical and executable plans.
5. Declare and run a small isolated non-OP edge-move/rest/side-placement/retreat
   comparison, retaining input/observation and material evidence. Then return to
   complete roofed access/action selection and the existing durable-job roadmap.

This is a localized native contract correction with caller/live verification.
It does not add crouching, entity physics, resource gathering, terrain excavation
or a command/teleport fallback. No implementation of the correction is included
in the diagnostic checkpoint.

## Diagnostic validation

[The pinned record](evidence/survival-edge-aim-20261002.json) retains native test
source and compressed logs. The focused characterization passed (one test,
188 filtered out); native all-targets Clippy and formatting passed. The native
change is test-only and was not imported into DustRoute's production vendor.
No Minecraft server was started for this diagnostic checkpoint.
