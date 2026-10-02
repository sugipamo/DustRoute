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

Configure `DUSTROUTE_SURVIVAL_OBSERVER_USERNAME` before native MCP startup. The
observer must be a distinct account from the builder and configured player, on
the same server and dimension, with the whole work region loaded. Both accounts
must be admitted by server access rules. The tool does not move the observer or
automatically load distant chunks. No server MOD or operator privileges are
required for this survival execution path. World mutation policy must permit it.

Planning temporarily reserves the source connection without dispatching actions.
Execution exclusively holds that source and its checked replacement connections;
other source operations are unavailable until a complete successful handback.
The initial independent observation must match the builder's current complete
scene. The executor then checks the original connection, dimension and position,
world baseline and actual inventory before dispatch. Each subsequent action uses
the common executor's fresh checks. Final independent block comparison and retreat
verification precede completion. These are client observations, not an atomic
server world lock or proof that nobody can change the world afterward.

Voxrig owns physical checks, received state and native single-operation contracts.
DustRoute owns adoption, scope, search, materials, exclusivity and job records.
This integration does not change Voxrig's mining retirement contract.

## Stops and persisted evidence

| Result | Meaning and next action |
| --- | --- |
| `source_not_adopted_or_mismatched` / `source_changed` | Read the current adoption and specification; propose/review corrections. |
| `generation_refused` | Read the structured cause, shortages or search limits; no execution plan was accepted. |
| `permission_denied` | Check player, dimension, region, action-count and mutation policy. |
| `observer_not_configured` / `observer_mismatch` | Restore independent complete observation before making a fresh plan. |
| `admission_refused` | Read `failure.error.code`, e.g. `snapshot_mismatch`, `plan_source_changed` or `supplied_materials_missing`; construction was not dispatched. The consumed job cannot replay. |
| `needs_inspection` | Execution or persistence is uncertain. Inspect saved evidence and freshly observe; do not blindly retry. |
| `cancelled_needs_inspection` | Cancellation stops at an executor boundary. It does not prove an outstanding native operation was aborted. |
| `completed` | The executor recorded final independent verification. Later edits still require fresh observation. |

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
process still requires the operator to resolve outstanding operations and freshly
observe before a new plan. Automatic recovery/replanning is a later milestone.

## Verification

Stage 3 acceptance and exact test artifacts are tracked in the
[roadmap](survival-construction-roadmap.md). A successful compilation alone does
not establish live acceptance.
