# Public survival construction

The native-only `survival_construction` tool connects the bounded construction
generator and executor to adopted Blueprints. Java 1.21.11, offline authentication,
dry passive property-free cubes, a received player inventory and explicitly
bounded edit, temporary, travel and retreat spaces are required. It does not admit
active circuits, entities, resource collection, chest supply or arbitrary terrain.
The existing command-based circuit/building workflows keep their own contracts.

## Prepare and review

1. Author with `test_circuit_change(blueprint.action=generate_grounded_building_design)`.
   Its `request` is a `GroundedBuildingDesignRequest`: a building `design` plus
   `ground_material`. It returns records, a proposed update and the normalized
   specification. This step writes neither the catalog nor Minecraft.
2. Import the returned records, propose the returned update without its generated
   `id`, review with `show_operation`, then adopt through
   `invoke_operation(confirm=true, blueprint_decision={action: adopt})`.
3. Call `survival_construction(action=plan)` with the exact adopted
   `assembly_revision_id`, specification, `scope`, `supplied` material counts,
   `temporary_material` and optional bounded search `limits`.
4. Review `preview.plan`: actions, required supplied materials, world baseline,
   temporary blocks and final position. `supplied` is a planning budget, not a
   receipt that those items are currently present. The plan writes no Minecraft
   state and expires after 900 seconds.
5. Call `action=start, job_id, confirmed=true` only for the reviewed scope.
   `state=admitting` means accepted for revalidation, **not completed**. Use
   `action=get, job_id` until a terminal state is returned.

The public tool's generated JSON schema defines the exact Rust DTOs. Unknown
action fields are rejected. Planning uses the configured player's catalog and
policy; it does not teleport to that player. Search runs on a bounded CPU worker
while network observation continues. At most 32 jobs are retained per process.
An unrelated append-only catalog addition does not invalidate an adopted source;
the pinned Assembly, adoption, runtime context and referenced definitions must
still match exactly at admission. A saved validation pass is never sufficient.

## Native ownership and observations

Normal construction uses one builder/account; an observer is not required for
planning, admission, execution, checkpoints or continuation.
`DUSTROUTE_SURVIVAL_OBSERVER_USERNAME` is optional legacy configuration, not a
survival prerequisite. Independent observers in acceptance fixtures compare
results without participating in the service's admission decisions. No server
MOD or operator privileges are required. World mutation policy must permit it.

Planning temporarily reserves the source connection without dispatching actions.
Execution exclusively holds that source and its checked replacement connections;
other source operations are unavailable until a complete successful handback.
The executor checks the original connection, dimension and standing basis, the
builder's complete received scene, world baseline and actual inventory before
dispatch. Every subsequent action uses fresh native checks. Final checks compare
the full expected site, remaining temporary ownership and retreat condition.

Movement uses the explicitly selected `predicted_dry_cube_v1` contract. Received
positions, model predictions and independent comparisons remain distinct native
Rust types. The 1/16-block horizontal planning reserve is a model-space policy,
not a measured physical error bound. Corrections, impulses, generation changes,
unsupported support/geometry or interrupted dispatch invalidate continuation.
The public `execution_contract` reports `world_evidence=builder_received`,
`independent_observer_required=false`, `server_stop_acknowledged=false` and
`server_position_error_bound=null`. Confirmed world edits remain received
observations; movement is recorded as predicted. These observations are not an
atomic server world lock or proof against later changes.

Voxrig owns physical checks, received state, native single-operation contracts
and connection recovery. DustRoute owns adoption, scope, search, materials,
exclusivity, temporary ownership and durable job records. Owned temporary cleanup
uses the audited direct-vanilla same-profile login boundary: explicitly close the
old source, obtain a fresh successful login under exclusive account ownership,
then validate the declared target condition and current scene. A local close or
elapsed delay alone is not retirement proof. Continuous same-connection mining
reuse is not admitted. The library also retains its independent-retirement API.

## Stops and persisted evidence

| Result | Meaning and next action |
| --- | --- |
| `source_not_adopted_or_mismatched` / `source_changed` | Read the current adoption and specification; propose/review corrections. |
| `generation_refused` | Read the structured cause, shortages or search limits; no execution plan was accepted. |
| `permission_denied` | Check player, dimension, region, action-count and mutation policy. |
| `admission_refused` | Read `failure.error.code`, e.g. `snapshot_mismatch`, `plan_source_changed` or `supplied_materials_missing`; construction was not dispatched. The consumed job cannot replay. |
| `needs_inspection` | Execution or persistence is uncertain. Inspect saved evidence and freshly observe; do not blindly retry. |
| `cancelled_needs_inspection` | Cancellation stops at an executor boundary. It does not prove an outstanding native operation was aborted. |
| `completed` | The builder verified the complete expected site, cleanup and retreat under the declared prediction/received-world contract. Later edits require fresh observation. |
| `checkpointed` | The old executor is sealed after native operation-history and current received-scene/standing checks; request a fresh continuation preview. Model standing remains explicitly predicted. |
| `safe_checkpoint_missing` | The last durable event does not prove a settled idle boundary. Lost native operations require intervention. |
| `checkpoint_site_changed` | Read `diagnosis.conflicts` and inspect the changed cells; no automatic removal or repair was performed. |
| `checkpoint_consumed` | A new job already claimed this boundary. Inspect that job rather than starting another branch. |

For a planned safe stop, call `action=checkpoint, job_id` and poll until
`status.state=checkpointed`. The request itself does not confirm idle. An active
mining attempt completes its existing outcome/retirement/reconnect path first;
no new mining is started for the stop. The old executor then checks native
operation history, current standing/inventory and the complete builder-received site,
durably records the checkpoint, seals its old sequence and returns the source.
`cancel` retains its existing uncertain-stop semantics.

After that boundary, including after starting a new MCP process, call
`action=continue, job_id=<old job>, limits=<optional search budgets>`. The tool
rechecks adoption, endpoint/profile, dimension, scope and the current native scene.
It reports completed/unbuilt targets and conditional remaining owned temporary
blocks, counts currently received inventory and searches a **new** complete plan
for remaining placements, cleanup and retreat. Review its preview and then use
`action=start, job_id=<new job>, confirmed=true`. Saved steps or native tokens
are never resumed. Completed permanent blocks cost no new materials; old
temporary works must be removed and do not refund future drops. Matching block
states do not prove who placed them; temporary ownership remains conditional on
the explicitly approved footprint and exact comparison.

A checkpoint has one durable continuation claim, written after fresh admission
checks but before dispatch and held under the prior journal's writer lock.
Competing previews cannot both execute. The claim remains consumed if admission
subsequently fails or its controller is lost; inspect the linked new job. Only
new manifests that retain the normalized specification support this workflow.
Checkpoint schema v2 stores standing provenance for diagnosis only; old v1
checkpoints are not converted. An ordinary historical `revalidate`, an elapsed
wait, or a lost unresolved
mining attempt is insufficient. This is bounded idle-checkpoint continuation,
not arbitrary crash recovery, an atomic world lock or an automatic repair system.

`action=cancel` before start consumes the plan without Minecraft writes. After
start it requests a boundary stop; the stopped executor and exclusive lease are
retained for diagnosis. It is not rollback. A background job can continue after
the requesting MCP call returns; polling does not grant or repeat action authority.

The state store's `survival-jobs/<job_id>` contains a bounded manifest, status and
executor journal. `action=get, include_record=true` returns the complete saved
diagnosis. Routine live progress polling uses the in-memory summary. Restarted
records are always `historical_only=true` and
`execution_authority_restored=false`, even if their last recorded status was
completed. They cannot resume, replay or cancel an old native operation. A new
process still requires outstanding operations to be resolved and a fresh
observation before a new plan. A sealed idle checkpoint supplies the historical
boundary for `continue`; it does not restore the old executor. Automatic recovery
of lost unresolved operations remains outside this milestone.

## Verification

The single-builder path passed its declared isolated non-OP Java 1.21.11
acceptance on 2026-10-03 UTC. Public MCP authoring/adoption produced a completed
115-step roof, 18 temporary removals and retreat without configuring an observer
in the service. A separate test client then compared all 3,120 cells and final
position. Each server and test process exited normally.

Two fresh-world trials checkpointed after placement and after a request made
during mining, then used genuinely separate OS processes to diagnose the saved
boundary and generate new plans. Both completed construction, cleanup and retreat.
Both refused external site changes without edits, rejected missing materials and
refused a second claim of the consumed checkpoint. Placement checkpoint: 11 steps
saved, one temporary retained, 104 new steps completed. Mining checkpoint: 31 steps
saved after one same-profile recovery, six temporary blocks retained, 85 new steps
completed. Historical diagnosis restored no native operation authority.

The complete MCP library suite passed 210 tests; ten explicitly opt-in tests were
ignored by that offline run. Focused survival and prediction/checkpoint regressions
and both all-target Clippy configurations passed. See the
[offline record](evidence/survival-single-builder-offline-20261003.json),
[hashed live traces and controllers](evidence/survival-single-builder-live-20261003.json)
and [single-builder plan](survival-single-client-plan.md).

Earlier [observer-based acceptance](evidence/survival-public-acceptance-20261003.json)
and the [failed observer-readiness continuation](survival-observer-readiness-prerequisite.md)
remain historical evidence. The new path removes that production dependency; it
does not diagnose the old tracking failure. These cases establish bounded sealed
checkpoint continuation, not arbitrary crash recovery or unrestricted construction.
