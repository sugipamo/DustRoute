# Live operation readiness and interrupted-operation recovery

Status: **bounded reconstruction implemented and verified; no mandatory companion MOD**.
The user selected repair after damage as the operational priority. The stronger
server guarantee proposed below is deferred, not a prerequisite for ordinary
operations or the bounded reconstruction path.

The follow-up [diagnosis path](assembly-diagnosis.md) compares a registered design
with the world regardless of who changed it. It reports differences separately
from repair admission, including unsupported player-added material. Diagnosis is
read-only in Minecraft; it does not depend on detecting the original failure.

## Current scope: observed-state reconstruction

Use the existing vanilla command path. Compare two complete observations with
the pinned, freshly reviewed Assembly, simulate teardown of the supported
observed layout, then rebuild its declared initial state. This is a new
previewed operation with explicit confirmation, not a retry of an uncertain
old command and not promotion of a historical attempt from snapshot agreement.

Start with stopped construction/removal and the ordinary reference door damaged
by premature input. Keep per-step readback, durable progress, source/target pins,
record revision checks and refusal of changing/moving/incomplete observations.
Reject extra or different material. Matching material alone cannot establish
ownership: the preview must expose all affected blocks, and the operator must
keep other inputs/edits out of the region during repair. The first strategy is
teardown and reconstruction, not minimal-edit repair or rollback.

The model starts with an explicit empty-queue assumption; observations cannot
prove that assumption, rule out a late old command, identify a replaced world
at the same endpoint, or make a check and write atomic. A mismatch stops further
writes and retains the new attempt for inspection. This accepted operational
limit must remain visible in the plan and documentation.

The shared device extension also has [position-owned torch history](shared-torch-runtime.md).
Recent burnout history can survive removal at an otherwise empty coordinate;
block snapshots do not recover it. Applicable fresh-state assumptions are exposed
in the target-review preview. Reconstruction does not promise to clear that
server history or certify future circuit behavior from matching visible states.

The companion proposal below records the optional stronger guarantee. It is
not the current implementation plan or an outstanding request for approval.

### Implemented workflow and evidence

`manage_assembly(action="plan_reconstruction", instance_id=...)` now creates a
new previewed reconstruction operation. It retains the observed baseline,
differences, proposed stages and conditions; invocation freshly reviews and
resimulates, checks the saved record revision and current samples, then uses the
shared per-step write/readback journal. The current v4 registry preserves previous
failed attempts. Retired v1–v3 records are rejected without changing lifecycle
state or execution pins; see the [cutover guide](architecture-cutover.md) and
[public contract](placed-assembly-management.md#reconstruction-after-damage-or-interrupted-work).

The [verification report](evidence/reference-door-reconstruction-20260927.json)
retains 15 passing Rust tests, Clippy/build checks and one isolated Java 1.21.11
trial at `(67008,180,1000)`, R0. Its 43-stage construction was deliberately made
incomplete by removing one quartz block; the 42-stage teardown plus 43-stage
rebuild passed every full-region readback. After MCP restart, two complete
close/open cycles and normal 43-stage removal passed. The owned region was left
empty, force loads removed, and Minecraft stopped normally.

The model regression covers 90 snapshots: all 43 construction boundaries,
all 43 removal boundaries, one missing-part layout, and three settled outcomes
after premature input (1, 3 and 14 modeled game ticks). The public transport
regression originally stopped both placement and reconstruction after a write
whose readback was incomplete. The diagnosis follow-up retains that placement
case and changes the repair interruption to an applied write whose reply is lost.
Both restart MCP, replan from fresh evidence and verify that old failures remain
recorded. This is controlled transport testing, not a host or
filesystem fault experiment. Arbitrary damage and actual live short-pulse repair
are not claimed by the single missing-part live trial.

The [diagnosis follow-up report](evidence/reference-door-diagnosis-20260927.json)
adds cause-independent findings even when repair is blocked, all-rotation normal
open/closed model references, repair replanning at each settled repair-stage
boundary for four damaged states, and the lost-reply transport case. A separate
isolated live trial verifies eight diagnoses, missing-quartz reconstruction,
two close/open cycles and cleanup. General ownership inference, arbitrary-motion
repair and automatic functional certification remain outside this work.

## Findings from current code

- [`scanRegion`](../crates/dustroute-mcp/mineflayer/bridge.js) reads the client's
  block cache. `writeBlocks` separately sends ordinary `/setblock ... replace`
  chat commands and returns a submitted count after two client ticks. It has
  no authoritative per-command receipt or conditional server-side write.
- [`mutate_assembly_construction`](../crates/dustroute-mcp/src/service/assembly_placement/execution.rs)
  checks before a write and validates the result afterwards. Another input,
  queued event or external edit can occur between the check and the write.
  The later readback detects a mismatch after that write has already happened.
- [`Attempt`](../crates/dustroute-mcp/src/assembly_registry.rs) durably retains
  verified progress and errors. It does not retain a separately identified
  in-flight stage and authoritative server execution receipt. A lost reply can
  therefore leave the next stage unapplied, applied, or still in motion.
- The separate instrumentation repository records executed scheduler callbacks
  and some queue queries. It has neither a complete readiness query exposed to
  MCP nor a guarded operation protocol. Its raw recording remains measurement
  evidence; it must not silently become an operational authority.

The placement/rotation evidence remains valid. These findings concern the
stronger requirement that a subsequent tool operation must not touch a busy
mechanism and that uncertain progress must be reconciled before continuation.

## Deferred stronger option: deployment decision

The proposed strong guarantee requires an opt-in **Java 1.21.11 server companion**
on the server where the tool operates. A Fabric companion is the first concrete
target because the existing isolated test environment already uses Fabric.
Pure client snapshots cannot expose the complete server queues or make the
observation and the ensuing mutation indivisible.

This changes the usage prerequisite for the guarded operations. Without that
capability, those operations must return unknown/unavailable and perform no
write; they must not silently fall back to snapshot-only placement. Existing
paths must be explicitly classified during rollout. A tool-wide guarantee must
not be claimed while a supported mutation can bypass the gate.

The initial implementation covers the bounded ordinary reference door and its
construction/removal stages. The protocol and refusal behavior are shared
infrastructure. Other mechanisms, periodic activity and placement paths require
explicit admission; world-wide quiescence is not a new Blueprint type rule.

## Deferred server boundary

1. Negotiate a versioned capability, target dimension, world identity and server
   session identity. Requests are bounded to the authorized region and explicit
   action; no arbitrary remote command/script interface is needed. The first
   transport must stay local or authenticated, tied to the intended world and
   operator permissions. A client-supplied owner name alone grants no authority.
2. At a defined server-thread boundary, atomically read the complete supported
   region and relevant pending block/fluid ticks, block-event queues and moving
   piston carriers. Include chunk availability and the admitted environment
   boundary. Audit all pending/ready/deferred stores in the mapped target build;
   logging only executed events is insufficient. An incomplete view is unknown.
3. Distinguish **ready**, **busy**, **changed** and **unknown**. For the initial
   bounded door, ready requires the reviewed complete expected state and no
   relevant pending work or active motion. A visible opening is not sufficient.
   Unsupported environments or an inability to prove the scope closed remain
   unknown; unrelated world activity must not automatically block a valid scope.
4. For a mutation, repeat the readiness and full expected-state checks and
   perform exactly one permitted action in the same uninterrupted server-thread
   task. A prior observation token is evidence, never a standing permission.
   Reject a changed target/session, stale request, different expected state,
   unloaded scope or busy mechanism before invoking the action.
5. Preserve existing Minecraft behavior: block writes use the audited ordinary
   replace-command entry and its callback flags; any future lever action uses
   its normal semantics. No strict writes, event deletion, time freezing or
   circuit input filters are introduced to make a test pass.
6. Return an operation/stage identity and execution receipt. This says whether
   the one action ran, not that its resulting motion has finished. The next
   stage waits for a new server observation matching its expected result.

This is atomic **check plus one action**, not a multi-block transaction or a
claim that other players can never change the world afterwards. Direct human
input during movement remains outside the accepted ordinary-door guarantee.

## Deferred receipt-based continuation contract

Upgrade the placed-instance journal deliberately. Retain the immutable source
pins, target, plan fingerprint, last verified stage and the next stage's durable
intent: operation identity, stage index, requested action, before/after expected
state fingerprints and any received server receipt. Persist intent before
sending; persist verified completion before advancing. Existing v1 records lack
these facts and must not be automatically promoted into recoverable v2 attempts.

A timeout must not resend a write or toggle blindly. Reconnect, revalidate the
source and plan, query the operation outcome when available, then obtain a fresh
server observation. A duplicate request identity must not execute twice within
its supported receipt lifetime; expired receipts, server restart and world
restore are explicit reconciliation boundaries, not exactly-once guarantees.

| Fresh evidence | Recovery outcome |
| --- | --- |
| Ready, uniquely matching the last verified stage, with the outstanding request known not to execute later | Offer a new previewed continuation from that stage |
| Ready, uniquely matching the next stage and consistent with its recorded action/outcome | Offer a new previewed continuation after that stage; do not repeat it |
| Busy | Observe until a bounded deadline; do not issue another action |
| Changed, ambiguous progress, unknown queued work, unavailable receipt/order, missing chunks or different world | Keep `needs_inspection`, report differences and required evidence; no writes |
| Ready, matching the final state with all required evidence reconciled | Explicitly finalize the recovered attempt; never promote from snapshots alone |

Recovery creates a new reviewed operation referencing the saved attempt and
record revision. It retains ownership, fresh source review, preview, full-region
comparison, expiry and per-step checks. This first stage supports continuation
of a known construction/removal sequence. Arbitrary damage repair and direct
rollback of an unknown partial build require a separately verified plan; simply
reversing the prior writes would ignore redstone callbacks.

## Deferred strong-guarantee implementation sequence

| Phase | Concrete result | Required checks |
| --- | --- | --- |
| A. Read-only server readiness | MCP distinguishes complete, busy and unknown for the bounded door | Queued observer/repeater work even with a matching visible aperture; piston events and active carriers; unloaded chunks; unsupported states; expiry/session change |
| B. Conditional one-action execution | The tool cannot write when the live precondition has changed or the mechanism is busy | Change/input between preview/observation and invocation; fresh check and action cannot interleave; no write on rejection; existing command semantics remain identical |
| C. Durable intent and recovery diagnosis | Interrupted construction/removal can be reconciled against identified stages | Failure before send, applied action with lost reply, delayed outstanding request, persistence failure, MCP restart, ambiguous/unavailable evidence and duplicate request identity |
| D. Reviewed continuation | A recognized stopped attempt can finish through the common guarded path | Public preview/apply/restart; continue both construction and removal; recheck before every stage; full resulting region; reject stale record/changed world |

Start with controlled transport stubs and existing retained evidence. Then use
the isolated server with serial builds, bounded observations and normal cleanup.
No host crash, process SIGKILL, filesystem fault, mount or cgroup experiment is
needed. Do not repeat the full relocation matrix until a changed physical entry
or an observed discrepancy justifies it.

Any future companion/protocol deployment requires a separate decision. A new
physics mechanism or broad repair search needed beyond bounded reconstruction
remains a stop and report condition under the user's instruction.
