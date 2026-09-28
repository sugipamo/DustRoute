# Persistent placed Assemblies

Implementation goal: retain placed custom electrical Assemblies across MCP
restarts, compare fresh world observations with their recorded expectation,
and create freshly reviewed, previewed conditional removal or reconstruction operations.

## Delivery gates

1. A separate versioned registry retains owner, source/adoption/context pins,
   target transform, server endpoint/dimension, expected whole-region state,
   and construction/removal progress without the temporary-plan TTL.
2. Durable intent is saved before world writes; interrupted or uncertain work
   remains `needs_inspection`. Loading data never restores a proof or a runtime.
3. Public listing/reading works after restart. Reobservation reports matches,
   changes, incomplete observation and missing moving-state history separately.
   Fresh source and target review is required before removal planning.
4. Removal requires an applied instance, fresh observation, a new preview and
   confirmation; invocation rechecks record version, source, endpoint/settings
   and whole-region state. Each stage and the final empty region are read back.
5. Tests cover save/reload, changed worlds, missing observations, wrong owner or
   endpoint, stale/consumed plans, interrupted writes and refusal to treat saved
   results as proof. An isolated Java 1.21.11 trial restarts after placement,
   detects an external change, then successfully replans and removes it.

This extends the admitted scope in [custom piston placement](custom-piston-assembly-placement.md).
The v6 command entry and explicit observer initialization repair the
[live command-placement counterexample](reference-door-live-construction.md).
Retired v5 execution pins require an explicit new review/adoption; no historical
pass or placed record can bypass current target review and complete observation.
It does not import old process-local plans, convert checkpoints, repair partial
construction automatically, or implement revision replacement/diff construction.
Server endpoint and dimension are observable; the vanilla bridge does not expose
a persistent world UUID. Deployment must keep a registry associated with its
world, rather than silently repointing it at a replacement world at the same
address. Snapshot agreement is not evidence of unobserved callback history.

Status: all five gates completed for the admitted custom electrical scope.
The [verification report](evidence/placed-assembly-final-checks-20260926.json)
retains command results, log/source hashes and the live evidence reference:
79 MCP tests and 5 construction/relocation tests passed, with no failures or
ignored tests. The expanded public transport regression was rerun successfully
after adding the same-process undo compatibility case. Clippy with warnings
denied, workspace formatting and diff checks also passed.

## Retained verification

The isolated [Java 1.21.11 trial](evidence/placed-assembly-mcp-20260926-a.json)
used a downward-controlled Assembly with upward and horizontal peers, rotated
R270 at `(33152,180,1000)`. It completed 14 construction stages and 14 removal
stages, with whole-region readback after each stage. Distinct MCP process IDs
and exits are retained for restarts after adoption, construction and removal.
The final saved instance is `removed`, with both verified attempt records.

The same trial rejected removal when the observing bot's chunks were unloaded,
when a guard cell had been externally changed, when the input was held ON, and
when the guard changed after preview. It then removed the restored layout using
a new reviewed operation. Both normal input applications are paired with server
write ticks (48412 ON, 48606 OFF). The owned region was verified empty, force
loads removed and the isolated server stopped normally. Bounded server capture
does not establish every construction callback or hidden runtime state.

| Delivery gate | Authoritative evidence |
| --- | --- |
| Durable pins, expected state and no TTL | `assembly_registry` serialization; registry test reopens an old-timestamp record with a one-second plan TTL; live `get` after restart |
| Write-ahead intent and uncertain progress | Public MCP transport test reads the durable `needs_inspection` record before every mock write, fails the second post-write observation, then reopens the one-stage verified prefix without promoting it |
| Fresh observations and review | Transport tests distinguish changed, partial, moving and wrong-endpoint states and reject a forged saved expectation; live trial independently covers changes and unavailable chunks |
| Fresh conditional removal | Transport tests reject stale record revisions and unpreviewed plans; source and transformed-target review are rebuilt; live preview/change guard and all 14 removal readbacks pass |
| Persistence boundaries and isolation | Registry test rejects competing locks, stale saves, wrong owners, truncated data and unknown schemas; unrelated immutable catalog additions remain compatible |

Transport stubs verify orchestration, not Minecraft mechanics. The real server
trial supplies the separate physical evidence. Partial-write handling is a
controlled transport test, not a host crash or filesystem failure experiment.

## Public workflow

Successful custom electrical construction returns `instance_id` (the original
construction operation UUID). Use `manage_assembly` with one of these requests:

```json
{"action":"list"}
{"action":"get","instance_id":"<UUID>"}
{"action":"observe","instance_id":"<UUID>"}
{"action":"diagnose","instance_id":"<UUID>"}
{"action":"plan_removal","instance_id":"<UUID>"}
{"action":"plan_reconstruction","instance_id":"<UUID>"}
```

`list` and `get` read historical records, without contacting Minecraft. They do
not refresh any proof. The configured assisted player owns each record; another
player cannot read it or plan its removal. Records keep their original pinned
source catalog. Unrelated immutable catalog additions are allowed; changing or
removing captured definitions, adoption or context invalidates revalidation.

`observe` freshly reviews the adopted source and the transformed target and takes
two complete region samples separated by a 20-tick bridge wait. The bridge uses
Mineflayer client physics ticks; `sample_interval_ticks` is not an authoritative
server-tick interval. It returns:

| Observation status | Meaning |
| --- | --- |
| `matches` | Both samples agree and match the recorded constructed state, or empty region for a removed instance |
| `changed` | Stable samples differ from the expected blocks/properties or surrounding air |
| `observation_incomplete` | The exact region could not be observed, transport failed, or policy refused observation |
| `history_unavailable` | A moving piston or changing samples require history that snapshots do not retain |
| `target_mismatch` | Connection endpoint, dimension, Java version or observed feature settings differ |

The independent `revalidation` result describes current model checks. A matching
snapshot with failed model review is not removal permission. Samples do not
reconstruct hidden queues or prove absence of intervening changes. Checks do not
resume an old runtime or import moving blocks as a fresh initial condition.
The saved `assembly` and `execution_context` are the transformed initial model
inputs; `expected_snapshot` is the constructed settled state. Neither is a
replacement for a fresh live observation.

`diagnose` adds coordinate-level missing/extra/block/property differences against
a reference derived from the design and current input levels. It also assesses
reconstruction separately, retaining findings when repair is blocked. Player
damage does not require a preceding failed tool operation. It writes no world
blocks, but saves the observation and invalidates older plans through the record
revision. Read the [diagnosis contract](assembly-diagnosis.md), including reference
fallbacks and the limits of interpreting a difference as a fault.

To remove an intact layout after a permitted operation (including a flying
machine that has arrived elsewhere inside its fixed observation region), opt in:

```json
{"action":"plan_removal","instance_id":"<UUID>","removal_reference":"observed_inputs"}
```

This uses the same reference as diagnosis: replay the declared initial design,
apply the observed declared lever values in order, and settle each input. It
requires an applied instance, fresh target review, complete matching samples,
and an exact full-region match to that reference. It never imports live blocks
as hidden runtime state. Missing parts, extra blocks, changed inputs after the
preview, incomplete coverage and ongoing motion refuse the operation. Other
valid histories can end differently; such layouts require explicit diagnosis.

The preview includes `operating_removal.baseline` and modeled teardown steps.
Execution rebuilds them, checks the record revision and full baseline again,
then verifies each step through the existing server readback gates. A restart
requires a new plan; v3 instance records retain the reference and steps as
history only. v1/v2 history remains readable but cannot carry this new field.
Default `removal_reference: "constructed"` retains the original baseline rule.
Neither option proves hidden server queues empty or makes check/write atomic.

`plan_removal` with the default reference repeats those checks and requires lifecycle state `applied`.
It creates a five-minute, process-local operation with freshly simulated removal
stages. Review it through `show_operation`, then use
`invoke_operation({"operation_id":"<new operation UUID>","confirm":true})`.
Invocation repeats source/target review and observation, checks the saved record
version and freshly generated stages against the preview, then verifies the
entire region before and after every write. Reobserving the record invalidates
an older removal plan; create and preview a new one. MCP restart also requires a
new removal plan. A saved `instance_id` alone is never an executable operation.

Existing same-process `undo_operation` for a successfully applied custom
construction remains available, with the same durable journal and fresh checks.
Neither removal path restores a removed instance automatically. A new
construction needs its own reviewed empty-site plan.

## Retention and uncertain operations

Configure a durable `DUSTROUTE_STATE_DIR`; its default lives in the operating
system's temporary directory and is not a permanent storage guarantee.
`<scoped state directory>/assembly-instances/<instance UUID>.json` has schema
`dustroute.placed-assembly.v3`, a monotonically increasing record revision and
no TTL. `DUSTROUTE_PLAN_TTL_SECONDS` does not expire these files. Records are
limited to 32 MiB each. A registry file lock serializes updates and construction
attempts across MCP instances using the same state directory; another request
may receive a busy error and should retry that request.

Before the first world write, an atomic, fsynced record saves the attempt as
`needs_inspection`. Each verified stage persists its progress before the next
write. Only complete verified construction records `applied`; complete verified
removal records `removed`. Both histories survive process restart. Interrupted
or uncertain work remains `needs_inspection`, even if a later observation happens
to match. It cannot be automatically retried, rolled back or promoted to applied
from snapshots. Corrupt or unknown-schema files are errors, not empty registries.

## Reconstruction after damage or interrupted work

`plan_reconstruction` accepts an `applied` or `needs_inspection` instance and
creates a new operation from two fresh matching region samples. It compares the
observed layout with the pinned, freshly reviewed design, simulates removal of
the observed parts, and appends the ordinary construction sequence. The target
is the freshly reviewed construction result derived from the declared initial
state (open for the reference door). Missing parts are replaced using the existing command
placement capability; this is not a survival inventory/crafting workflow.

The response includes `differences`, the complete `reconstruction.baseline`,
`reconstruction.steps`, and `reconstruction_conditions`. Every remaining part
in the admitted region may be removed and rebuilt, even if only one part is
missing. Review these through `show_operation`, then use
`invoke_operation(confirm=true)` to authorize that scope. No world write occurs
during planning. As with removal, plans expire after five minutes, are consumed
when a write attempt starts, and are invalidated by record changes or restart.
It also returns a separate `diagnosis`, including when reconstruction is refused;
its current-input comparison reference can differ from the repair's initial-state
target. Classified differences do not identify ownership or modification intent.

Fresh source/target review and the whole simulation are repeated on invocation;
fresh samples must still match the reviewed baseline. Every stage has a complete
before/after readback and a durable verified prefix. A mismatch stops subsequent
writes and retains `needs_inspection`; make a new reconstruction plan after
inspection. Successful reconstruction updates the same instance to `applied`
and keeps every earlier attempt unchanged. Use `plan_removal` for subsequent
removal; reconstruction's operation ID does not support `undo_operation`.

Admission is bounded: supported, stable block states and material counts no
greater than the original Assembly (matched stable piston heads are checked
separately). Extra/different material, unsupported block state/support, moving
pistons, changing/incomplete samples and invalid model initialization are
rejected. Recognized construction prefixes are replayed through current model
commands to preserve explicit intermediate wire shapes, without relaxing the
fresh-world validator or interpreting the observation as a runtime checkpoint.
An arbitrary damaged circuit is not guaranteed to admit a reconstruction plan.

This uses the ordinary vanilla command path without a mandatory companion MOD.
Two matching client samples do not prove that server queues are empty. The
operator must let previous commands finish, review the affected blocks and keep
competing input/edits out of the region during repair. Material counts cannot
prove ownership of identical player-added blocks, checks are not atomic with
writes, and delayed old commands cannot be ruled out. These conditions are
returned with the plan; the server-guaranteed option remains
[deferred](live-operation-readiness-and-recovery.md).

The v2 record adds an explicit optional reconstruction journal containing the
observed baseline and the proposed sequence. Existing v1 records remain readable
as history and are upgraded to v2 on save, without promoting lifecycle state,
converting runtime checkpoints or changing pins. Older v1-only binaries reject
v2 rather than interpreting a reconstruction as an ordinary placement. Registry
limits and synchronization remain the same.

The [reconstruction report](evidence/reference-door-reconstruction-20260927.json)
retains the 15 passing Rust tests, 90 modeled recovery starting states, and the
isolated missing-quartz trial: 85 reconstruction stages, restart, two close/open
cycles and normal removal. It distinguishes the finite live evidence from model
and transport checks and from the deferred server-readiness guarantee.

Preexisting placements made before this registry was introduced cannot be
recovered from lost process memory. Deployment/world replacement and world save
rollback remain operator concerns: vanilla exposes no persistent world UUID, and
world writes plus local filesystem saves do not form one distributed transaction.
External concurrent edits can still occur between samples or checks; every stage
uses conditional observation and stops when a difference is detected.
