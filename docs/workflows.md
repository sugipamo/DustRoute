# Workflows

[English](workflows.md) · [日本語](workflows.ja.md)

Choose a task from [capabilities](capabilities.md), then follow its sequence here.
[Getting started](getting-started.md) explains connection and permissions. Exact
arguments come from the connected MCP tool schemas; the
[public reference](mcp-public-features.md) describes limits and retention.

## Understand an existing circuit

1. Use `test_circuit` for the configured player's gaze target, or `get_world`
   for explicit coordinates. For a selected capture, use `set_region` twice
   followed by `show_region`.
2. Keep the returned `circuit_id` with its bounds, dimension and completeness.
   Use `convert_from_circuit` or `get_circuit_ir` for deeper interpretation.
3. Report findings, unsupported blocks and inference/proof limits separately.
   Request bounded truth tables only when the interface supports them.

An inferred role is not a guaranteed behavioral type. Read
[physical function modeling](physical-function-model.md) for deeper analysis.

## Try a change before applying it

1. Use `test_circuit_change(circuit_id)` to create a hypothetical revision.
   Branch with `test_circuit_change(revision_id)` and read it with
   `get_circuit_revision`.
2. Compare the declared edits, model results and failures. A failed draft can
   remain editable; no Minecraft changes have been made.
3. For a supported change at the captured site, use `new_placement(revision_id)`.
   Supply explicit work regions when using a larger revision job.
4. Review `show_operation`, confirm `invoke_operation(confirm=true)`, then read
   `get_operation`. A planning/admission response is not completed work.

If the baseline changed, reobserve instead of replaying the old operation. See
[revisions](circuit-revisions.md) and [region work](large-circuit-regions.md).

## Author and adopt a reusable design

1. Read exact catalog sources with `get_circuit_revision(blueprint.kind)`.
2. Author explicit data or use a supported generation action: building geometry,
   grounded building design or a registered finite flying machine.
3. Import the returned records, propose the update and review `show_operation`.
   Inspect the physical diff and every parent/child/shared requirement.
4. Explicitly adopt with `invoke_operation(confirm=true, blueprint_decision)`.
   Failed or undetermined required checks block adoption.

Adoption appends immutable local records. It neither places blocks nor updates
old instances. Continue with [Blueprint details](blueprint-mcp.md),
[building input](blueprint-building-design.md) or
[flying-machine generation](flying-machine-generation.md).

## Construct using commands

1. Observe the complete intended site. New-target adopted Assemblies require an
   empty guarded volume; captured-site reflection has a different ancestry gate.
2. Use `new_placement` with a supported builtin/revision or adopted
   `assembly_revision_id` and the appropriate target.
3. Preview the exact region, writes, initialization and recovery conditions with
   `show_operation`. Apply the authorized plan with `invoke_operation(confirm=true)`.
4. Inspect completion/readback and retain the `instance_id` for placed custom
   Assemblies. With region jobs, freshly plan and review every next stage.

This path uses OP commands, not inventory. For active mechanisms, model review
and construction checks do not establish a live readiness sensor. Read
[custom placement](custom-piston-assembly-placement.md) and
[instance management](placed-assembly-management.md).

## Construct using survival inventory

1. Generate, import, review and adopt a supported grounded passive building.
2. Supply permanent and temporary materials to the bot inventory. Declare edit,
   temporary, travel and retreat space.
3. Call `survival_construction(action=plan)` and review its complete plan and
   required materials. A supplied planning budget is not an inventory receipt.
4. Call `action=start` with `job_id` and `confirmed=true`; poll `action=get` until
   a terminal state. `admitting` is not success. Completion includes the final
   site, temporary cleanup and retreat checks.
5. For a planned stop, request `action=checkpoint` and wait for `checkpointed`.
   Later `action=continue` creates a new preview from fresh observations and
   inventory, including after process restart. Review and start the new job.

Normal work uses one non-OP builder. Movement is explicitly predicted; received
world evidence is not a server lock. `cancel` is not rollback, and unresolved
lost operations cannot be automatically resumed. See the
[survival contract](survival-public-construction.md).

## Inspect or recover

| Situation | Next step |
| --- | --- |
| Circuit appears faulty | `new_repair`; inspect competing explanations with `get_repair_context` before choosing a patch |
| Placed Assembly is damaged | `manage_assembly(action=diagnose)` for fresh design differences; review a supported new `plan_reconstruction` if appropriate |
| Operation failed or writes are uncertain | Preserve its result, observe the whole context and inspect the changed/protected cells; do not automatically retry |
| MCP restarted | Read durable history, reobserve and create a new operation; old saved passes are not executable proofs |
| Finite flying machine arrived | Diagnose against its declared input-derived reference; explicitly select `removal_reference=observed_inputs` when planning arrival-state removal |
| Survival job was checkpointed | Use `continue` to create a fresh plan; a checkpoint has one durable continuation claim |
| Undo requested | Check the operation-specific contract and current world; some operations have no undo or no retained undo after restart |

Diagnosis locates differences; it does not prove who caused them or guarantee a
repair plan. Read [diagnosis](assembly-diagnosis.md),
[recovery contracts](mcp-public-features.md#execution-and-recovery) and
[failure information](structured-failure-recovery.md).

Use the [documentation map](README.md) to zoom further into a particular feature.
