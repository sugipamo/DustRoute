# Failure handling within the supported scope

Baseline: develop `bf3ae23`. This work improves the failure and recovery handoff
for placement, repair and bounded survival construction. It does not extend
Minecraft physics, introduce a server companion, remove mining reconnects or
implement arbitrary crash recovery.

## Roadmap

| Stage | Result | Acceptance |
| --- | --- | --- |
| 1. Failure-path inventory | Compare refusal, dispatch, readback, persistence and cancellation facts | MCP distinguishes known progress, unknown effects and the next diagnostic action |
| 2. Stop and recovery handoff | Preserve the original failure, expose available progress and reuse diagnosis/new-plan paths | No false completion or replay; uncertain sources remain quarantined |
| 3. Continuous survival work | Verify movement, placement, temporary removal, reconnect and subsequent work | Declared ordinary sequences complete; changed prerequisites stop with useful evidence |
| 4. Search failures | Distinguish shortages, unsupported input, exhausted budgets and rejected candidates | A failed search is not reported as physical impossibility or an executable partial plan |
| 5. Public live acceptance | Recheck a small building and supported circuit, with bounded declared changes | Independently compare completion/cleanup and inspection outcomes through MCP |

The first goal covers the inventory and concrete missing diagnostic connections
in stages 1–2. Existing later-stage evidence is retained; this change does not
claim a new continuous-construction or live disturbance campaign.

## Inventory and changes

| Path | Existing protection | Missing connection addressed |
| --- | --- | --- |
| Ordinary placement | Preview, fresh baseline/context, consumed attempt, submission and independent readback | Whole revision-context mismatches now retain a category and differing coordinates rather than only prose |
| Repair and Assembly construction | Durable intent/progress, primary and secondary failures, readback and conditional restoration/reconstruction | Common recovery advice considers secondary readback/persistence failures and directs callers to available durable history |
| Survival summary | A process-local state owns previews, running work and quarantined resources | State-specific typed next actions replace uniform polling/inspection advice |
| Survival persistence failure | `needs_inspection` retains the failed status as diagnostic data | A known completed prefix remains visible without reporting successful persistence or restored authority |
| Survival detailed read | Owner checks and bounded typed records; failed reads refuse | An authorized live snapshot remains available alongside a failed record read; the failed read is not presented as success |
| Historical survival records | Diagnosis only; no deserializable native token or restored controller | Saved running states require inspection, saved previews require new plans, and recorded completion requires fresh observation before new work |
| Native survival failures | Voxrig supplies a typed native error kind; DustRoute retains its own refusal code | Optional `native_error_kind` now survives the public response and typed history instead of being lost in a generic native refusal |

No operation consumption, source handback/quarantine, mutation gate, native
retirement or checkpoint claim condition is relaxed. Detailed-read failures
attach live data only after its owner's policy check. A persistence error can
leave a file changed; neither the old file nor successful writes are inferred
from the error text.

## MCP handoff

The ordinary `dustroute.error.v2` response retains its existing fields. Recovery
advice includes all recorded causes; a later failure does not replace the first
cause or alter submitted/verified/durable counts.

Survival refusals and job reads add a typed `next_action` alongside readable
`next_step`. Actions describe what to inspect or request, never permission to
dispatch. In particular:

- `poll_job` applies to a current admitting/running controller, not a historical
  running status after restart.
- `inspect_execution` retains unresolved-operation and source-ownership limits.
- `inspect_persistence` preserves known progress while the status remains
  `needs_inspection`.
- `continue_checkpoint` requests fresh checkpoint validation and a **new**
  preview; it does not resume saved native steps.
- `inspect_linked_job` follows a recorded continuation claim instead of replaying
  its old checkpoint.
- `observe_before_new_plan` does not make historical completion current evidence.

If a detailed job read fails, `available_live_status` can contain the already
authorized process-local snapshot. The outer response remains `ok: false` and
retains its read failure. Without an authorized live entry, the field is absent.
Successful history queries of failed jobs remain successful read operations.

Job/journal/instance storage schemas and the non-JSON codec are unchanged.
Known-prefix extraction reads the existing `last_status`; presentation advice
is not stored as execution authority. Unknown progress stays null rather than
being converted into zero.
The optional native category is diagnostic data. Old records that omit it still
decode; no category is inferred from their messages and no data conversion is
required. Voxrig's behavior and the existing admission/execution refusal codes
are unchanged.

## Validation boundaries

Focused regressions exercise the real MCP handler and typed store: a changed
unedited context cell refuses before writes; corrupt bounded fixture data does
not erase authorized live progress or expose it to another owner; restarted
history is not treated as a live controller. Common failure tests cover secondary
readback/save failures and consumed attempts with durable intent.

These are offline orchestration/diagnostic checks, not live Minecraft physics
evidence. No server restart, user-world write, process SIGKILL, host fault or
filesystem fault injection is required. Existing declared live evidence remains
in [the capability table](capabilities.md#what-has-been-checked-live).

2026-10-05 offline regression results (`cargo test --offline --locked -j1
-p dustroute-mcp --lib <filter> -- --test-threads=1`):

| Filter | Passed | Ignored |
| --- | ---: | ---: |
| `failure::tests` | 8 | 0 |
| `service::revision_tests::revision_placement_revalidates_cumulative_diff_context_and_undo` | 1 | 0 |
| `service::repair` | 4 | 0 |
| `service::construction_executor::tests` | 1 | 0 |
| `service::transition_failure_tests` | 6 | 0 |
| `service::survival` | 16 | 2 |
| `survival_execution` | 16 | 1 |

These filters cover 52 distinct passing tests. The three ignored cases require
an explicitly selected live server or retained live journal; they were not
enabled. This is focused regression evidence, not a new full-workspace test run.

Both all-target strict Clippy configurations passed:
`cargo clippy --offline --locked -j1 -p dustroute-mcp --all-targets -- -D warnings`
and the same command with `--no-default-features`. `cargo fmt --all -- --check`
and `git diff --check` passed. All 237 local link targets in the changed/new
documents resolved. No additional prerequisite or responsibility change was
needed. Stages 3–5 remain subsequent goals.

## Stop conditions

Report before implementing a prerequisite that changes Voxrig/DustRoute
responsibilities, requires new native operation guarantees, introduces a physics
mechanism/server MOD or expands into universal interrupted-operation recovery.
An advisory response cannot bypass an existing recovery refusal to finish a job.
