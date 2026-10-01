# MCP diagnostics and live operation progress

Scope: improve the remaining common observation/admission error boundaries and
make existing operation IDs useful while work is in flight. This work requires
no player login, new Minecraft feature or live-world change.

Implementation order:

1. Carry typed causes through player identity, selection, circuit capture,
   snapshot conversion, explicit work-region reads, and edit/Assembly ownership.
   Keep readable error text, concrete positions and known parameter/budget limits.
2. Share request-local reporting with the existing performance spans. Publish
   execution facts from ordinary placement, repair and the common construction
   executor; propagate reporting into blocking model workers.
3. Expose live activity beside saved operation results, preserving consumed
   attempts and honest cancellation semantics.
4. Verify through offline MCP/transport fixtures and both backend configurations.

Stop if an undeclared prerequisite or major blocker needs doing first.

## Typed diagnostics

The MCP framing layer promotes an actual `FailureCause` into the existing
`dustroute.error.v2` contract. It never classifies an arbitrary message. The
original `error` remains a string. Read-only/admission failures without execution
facts retain `failure.progress: null`; this is not evidence of zero world effects.

Known errors now preserve connection/protocol/serialization/native observation
causes across the common circuit capture boundary. Snapshot errors retain an
invalid facing's coordinate. Coordinate validation retains its position.
Policy/placement budgets retain actual and maximum counts and a resource name.
Numeric input refusals retain `parameter`, `allowed_range` (inclusive) and, if
finite, `supplied_number`. Missing selections, expired circuit IDs and ownership
refusals retain their explicit categories. Debug visible-player reacquisition
and its follow-up visibility read now both retain their causes; a failed refresh no longer returns success. The legacy
`reacquire_error` string remains alongside the structured error. Asynchronous
analysis stores its structured error in the operation result, instead of only an
error message.

This is a bounded migration, not removal of every String in the repository.
Deep Blueprint/compiler/proof/catalog errors still represented only by String
remain `unknown`. Some legacy planning closures still erase a typed inner cause
when converting it back to String. These boundaries need separate migrations;
no message heuristics are used as a substitute. Model-backed mutation paths
retain their existing known execution facts and primary/secondary failures.

## Polling

Call `get_operation(operation_id)` from another MCP request while an existing
`invoke_operation` or `undo_operation` runs. Debug-profile asynchronous region
analysis also provides an ID immediately. The optional sibling `activity` uses
`dustroute.operation_activity.v1`:

| Field | Meaning |
| --- | --- |
| `active` | The tracked request/worker has not ended; not proof of world success |
| `action` | `invoke_operation`, `undo_operation`, or `analyze_region` |
| `elapsed_ms` | Monotonic elapsed wall time, frozen when tracking ends |
| `phase` | Innermost instrumented span, or the last span when none is active |
| `phase_active` | Whether that phase is currently in flight; false between spans and after the request ends |
| `phase_elapsed_ms` | Elapsed wall time of the active span, otherwise null |
| `execution_progress` | Latest known submission/verification/checkpoint facts, otherwise null |
| `cancellable` | Only analysis supports a cancellation request |

Phases include `mutation_queue`, `model_queue`, `model_proof`, `scan`, `write`,
`wait`, `verification`, `checkpoint` and storage phases. Normalization and
analysis have their own spans. Worker spans share the same reporting context,
which is restored when a blocking thread returns or panics. Nested durations
are inclusive, not additive CPU time. Uninstrumented intervals have no active
phase; no estimated completion time, inferred server ticks or invented
percentage is returned. Entry capacity is bounded; if all observer slots are
occupied, additional work can proceed without an activity record.

`submitted_changes`, `verified_steps` and `durable_verified_steps` remain
separate. A pending readback can show one submitted change and zero verified
steps. A failed checkpoint can show verified changes without a saved prefix.
Final response facts replace the live snapshot when the response contains them.

The saved `operation` or edit history remains a historical result, even if
`activity.active` is true for a later invocation under the same ID. Activity does
not overwrite the consumed attempt, grant replay authority, change a plan's
TTL or persist across restart. A concurrent invocation cannot replace an active
observer. An inactive observer means tracking ended, not that an interrupted
request proved its world result or that an orphaned blocking worker stopped.

`stop_operation` requests cancellation only for queued/running region analysis.
An in-flight read or blocking calculation may finish; activity can remain
active until it returns. Late updates/completion/failures cannot turn the
cancelled operation back into running/completed. Mutations do not acquire a
cancellation guarantee by being observable.

New synchronous plans and reads without a preexisting operation ID cannot be
polled through this API. Their opt-in performance measurements remain available.
Exposing those calls as asynchronous jobs would be a separate feature.

## Verification

Offline fixtures exercise the real MCP envelope and handlers. A gated scan
proves that polling works before the transport response arrives; malformed
facing information retains its coordinate; cancellation stays cancelled after
that response. A real ordinary-placement workflow over a mock transport proves
submitted changes remain unverified while readback waits, then become verified
only after the matching readback. Saved consumed attempts survive refusals;
mutation cancellation is refused. These fixtures are not Minecraft physics
conformance evidence. No live server/bot restart or world mutation was performed.

The initial broad native run passed 176 cases and found one history-overlay
regression: an activity record prevented the existing persisted Blueprint result
from being read. Lookup now retains both the saved result and separate activity.
After that correction and more precise normalization/analysis phases, the final
related native run passed 136 cases; the previously failing Blueprint case also
passed both adoption and broken-child modes. The other 40 expensive
Blueprint/building/job cases passed before the final response refinements and
were not repeated. The later visible-player refinement adds one regression case
(both reacquisition and refresh failures), bringing successful native cases across runs to 178, rather
than a claim of a fresh full-suite pass after every refinement. Four optional
performance trials remained ignored. Compatibility related tests passed 128
cases (two optional measurements ignored). The final five diagnostics/progress
cases passed under both configurations after the visible-player refinement.
Strict all-target Clippy passed under both configurations; scoped formatting
and diff checks passed.

Exact commands/results are recorded in
[evidence](evidence/operation-diagnostics-progress-20261001.json).
