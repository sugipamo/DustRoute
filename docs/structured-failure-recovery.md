# Structured failures and recovery

Goal: carry facts from observation, model, transport, execution and storage into
MCP failures without guessing server effects from an error message.

Implementation order:

1. Preserve cause categories and bounded observation/model details in Rust.
2. Record failed phase, submitted/verified/durable progress and secondary errors
   in the common construction executor and its durable callers.
3. Connect ordinary placement, repair/undo, piston actions, transition tests and
   operation history. Preserve existing admission and consumed-plan rules.
4. Verify interruption, readback mismatch/unavailability, persistence failure,
   restart and recovery information; document the public response contract.

Sending a command is not verification of server application. A transport timeout
may have effects and does not prove zero writes. Persisted observations remain
historical evidence; failures never grant replay or fresh execution authority.
Unknown phases/causes from unmigrated string boundaries remain explicitly unknown.
Stop if an undeclared prerequisite or major blocker needs doing first.

Compiler, tests and private-server work are serialized. No new host changes or
kernel fault trials are needed for this goal.

Implementation branch: `codex/structured-failure-recovery`, based on develop
`830c053`. Implementation commit: `9a0a8a0`. The roadmap above is implemented;
validation is summarized below.

## Public facts

Instrumented failures return `dustroute.error.v2`. Existing `error` text remains;
use `failure` for machine decisions. `FailureCause` keeps a coarse category,
message, phase and details. Native Voxrig categories are retained, including the
original native kind. Reconstruction issues, missing-cell positions and up to
64 recovery chunks survive the observation boundary. Model budgets retain the
resource, actual count and maximum. Mismatch details contain at most 64 positions
with a truncation flag and full count. Message text is never parsed to infer a
category. Existing string-only model/admission boundaries remain `unknown`.

| Progress field | Meaning |
| --- | --- |
| `phase` | Last execution stage; each cause also records its own phase |
| `world` | `not_attempted`, `unknown`, or independently `verified` for the relevant target |
| `submitted_changes` | Known lower bound of locally submitted complete changes; `null` if delivery progress is unavailable |
| `total_changes` | Planned target changes, when known; restoration can submit additional changes |
| `verified_steps` | Independently observed completed target steps, not command acknowledgements |
| `durable_verified_steps` | Verified prefix whose checkpoint save succeeded; `null` when not tracked |
| `persistence` | `not_required`, `intent_saved`, `checkpoint_saved`, `final_saved`, or `uncertain` |
| `operation_consumed` | This attempt consumed its execution capability |

`verified` is bounded by the existing observer/contract's scope. It does not prove
hidden queues are empty, reconstruct runtime history or upgrade a client read
into server-confirmed evidence. A moving observation remains unsuitable for a
stationary execution baseline. Piston transition/restoration operations report
verification of the restored initial state. Repair rollback has a separate
`restoration.world` field: verifying rollback does not make the failed repair
target verified.

Native mutation tracking retains its known prefix even when the enclosing
timeout cancels the request future. The Mineflayer compatibility bridge sends an
additive `dustroute.bridge-failure.v1` envelope alongside the original error text.
It reports explicit admission rejection with zero block effects and captures
runtime submission prefixes without inventing an error category. Older peers
still work; lost/unstructured replies leave submission progress unknown.

## Recovery and storage

`failure.primary` is the first failure; cleanup, restoration and later save
errors appear in `failure.secondary` with their phases. A later save failure
does not replace the original cause. Errors when reading restoration results
are exposed separately from observed mismatches. Transition tests retain errors
from both waits, recording cleanup and both restoration reads.

`recovery` tells the caller whether to reobserve, replan or inspect saved records.
An unavailable read requires fresh observation even when no write was attempted;
a changed baseline or failed target comparison requires replanning.
The advice considers both primary and secondary causes. A secondary readback or
save failure retains its diagnostic requirement without replacing the original
cause. Saved intent/checkpoints also direct callers to their durable history;
that advice never claims the latest save succeeded. Revision placement context
mismatches retain differing coordinates, including changed unedited cells.
See [the supported-scope follow-up](failure-handling-stability.md).
`same_operation_replay_allowed` is always false for failures. Existing repair
rollback and explicit transition restoration remain available under their
existing admission rules; these fields do not authorize either action. Ordinary
placement and repair responses include the source operation ID, and failed
attempts are recorded as failed in the operation registry.
Refusing a replay does not overwrite the original consumed attempt's diagnostic
result under the same operation ID.
The MCP tool envelope also marks a top-level `ok: false` response as an error.
Successful history queries containing a failed operation remain successful
queries. The envelope checks only the top-level outcome and skips unknown JSON
fields without allocating another snapshot or trace tree.

The common executor saves possible submission intent before each batch, adding
one journal replacement per batch. Verified steps and durable checkpoints remain
separate when saving fails. A storage error can occur after rename but before
directory synchronization; `uncertain` means the stored file might have changed.
It never means that the old record is definitely intact or that replay is safe.
Optional diagnostic fields are additive in archived edit/assembly/job attempts.
Loading them restores history only, never fresh observations or execution rights.
An error during a final save may be available only in the response; it cannot be
assumed present in the failed save's archive.

For uninstrumented string-only failures, the MCP framing layer returns
`failure.progress: null`, unknown phase and nullable recovery requirements.
Explicit legacy error codes still supply a coarse category. Callers must not
treat missing information as no effects, no consumed plan, or permission to retry.

## Further common-boundary migration and live reporting

The follow-up preserves typed causes in common player/selection/capture and
snapshot paths, including numeric parameter constraints and coordinate details.
Read-only errors with no execution facts use v2 with null progress. Live operation
activity is separate from the saved failure result; it uses the existing request
measurement context and publishes actual execution facts. See
[scope, response fields and remaining String boundaries](operation-diagnostics-progress.md).

## Verification

Regression coverage includes checkpoint failures before submission and after
world verification; submitted prefixes and lost replies; preservation of primary
errors when final persistence also fails; native categories and reconstruction
details; bounded mismatches; model budget details; correct failed operation
history; persisted uncertain repair/edit intent and rejection of replay after
restart. Tests use offline transport fixtures and existing model proofs. These
fixtures verify orchestration and reports, not Minecraft physics or host-kernel
failure behavior. Validation results are recorded below.


2026-10-01 validation:

- 173 distinct Rust cases passed across the broad and related runs. The initial
  broad run passed 166 cases and found one misplaced new test assertion; that
  assertion was moved to the intended failure scenario. The related final run
  passed 131 cases, and the additional transition failure case passed both
  activation/cleanup and recording/restoration error modes. Final observation
  refusals and checkpoint handling were also rechecked. Four opt-in performance
  trials remained ignored.
- The previously passing expensive Blueprint/building/job suites were not
  repeated after final response refinements. This is a union of successful
  regression cases across runs, not a fresh full-suite pass for every refinement.
- Both native-feature and compatibility configurations passed strict Clippy for
  all targets. Compatibility-only bridge and failure tests passed 10 and 7 cases.
  Scoped formatting and diff checks passed; compatibility JavaScript tests passed
  15 cases.
- No new Minecraft live trial or host modification was performed.

Exact commands and results:
[validation summary](evidence/structured-failure-recovery-20261001.json).
