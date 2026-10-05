# Single-bot survival construction: plan and review boundary

Recorded 2026-10-03 UTC after the user requested the investigation's stages as a
goal. DustRoute work branch: `codex/survival-single-client`, based on `0bfbd57`.
Voxrig has a separate branch with the same name. Its source is edited separately;
the immutable vendor snapshot was initially
`2b6e7bfc94e6270054eac5c7b14a74d4657a411c`. After native acceptance it was updated
through the checksum-managed vendor script to
`4b46c66cd031eacc72bdf82736eaa60c57ccb9ab`, whose implementation is unchanged from
the live-tested `bed0465` source; only acceptance documentation/evidence was added.
Following the approved prediction contract and native comparative acceptance,
the managed snapshot used for that acceptance was `5bace7be7cd941e1340ad94052e922db23c4f892`
(313 files), with unchanged implementation from live-tested `c491f6a`.
The [source manifest](../vendor/voxrig-source.json) records the current dependency
pin; later updates do not relabel these acceptance fingerprints.

Normal operation now uses one builder/account, with an independent observer
used only for verification comparisons. The user approved explicitly predicted
continuation. Native movement/placement and integrated construction/continuation
acceptance passed. The declared single-builder goal is complete; broader terrain,
arbitrary crash recovery and a full disturbance campaign are separate work.
Further responsibility or correctness-contract changes still require reporting
before dependent work.

## Stages and current status

| Stage | Completion condition | Status |
| --- | --- | --- |
| 1. Native mining continuation | Establish a same-connection release or same-profile fresh recovery without an observer retirement watch; preserve uncertainty and once-only login guards | Same-profile recovery implemented and declared native cases passed; continuous reuse remains unsupported |
| 2. Movement and shared standing | Define the basis for one-bot walk/rest/jump/land and subsequent look/place/mine checks; compare against independent test evidence | Explicit prediction contract implemented; native walk/jump/collision/placement comparison passed; integrated movement, placement and subsequent mining/cleanup accepted |
| 3. DustRoute execution | Use the chosen native contract for admission, placements, cleanup, final checks and durable checkpoints; expose actual evidence in MCP | One-builder integration complete; full roof and both continuation cases accepted |
| 4. Construction continuation acceptance | Complete a roofed build and separate-process continuation through cleanup/retreat, including checkpoint requested during mining | Accepted on fresh isolated worlds; prior failures retained as historical evidence |

The earlier `6ede15d` dependency update was independent of the movement choice.
It did not switch the executor, remove the observer requirement or authorize
prediction-only standing. The consumer's
diagnostic mining test fixture now initializes the new retained recovery-attempt
field. [Integration verification](evidence/survival-single-client-integration-20261003.json)
passed 29 focused survival tests (six opt-in live tests ignored), all-target Clippy
with and without the native feature, formatting and verification of all 305
managed snapshot files. This accepts the dependency integration, not observer-free
runtime construction. No new Minecraft trial or full MCP regression run was made
for this consumer update.

## Approved one-builder implementation

The native prediction contract keeps received, predicted and independently seen
positions distinct. Its 1/16-block construction reserve is a model-space policy,
not a physical error bound. The standalone non-OP trial used no observer watches
in its native runs: walk, jump/landing and wall collision/retreat all completed
and all three subsequent placements were independently compared. The observer
also received the jump rise and matching final positions. Native tests covered
correction, impulse, generation, current support and interrupted dispatch refusal.
See the vendored [contract](../vendor/voxrig/docs/survival-predicted-motion.md) and
[native evidence](../vendor/voxrig/docs/evidence/survival-predicted-motion-live-20261003.json).

Construction plans explicitly select predicted endpoints. The executor uses the
same checked contract and own received target/material/sequence evidence. Owned
temporary cleanup declares an exact-air target after confirmed removal, or an
original-or-air fresh target after an admitted inventory-only conflict; Voxrig
performs same-profile retirement/recovery. No observer is retained by the
executor or required by public planning, admission, checkpoint or continuation.
The opt-in public trials create a separate comparison client outside the service.

Motion events and completed movement steps record `outcome=predicted`, while
confirmed world edits remain `observed`. Checkpoint prefix validation requires
the appropriate outcome for each kind of step, so predicted placement cannot
invent temporary ownership. Checkpoint schema v2 retains diagnostic standing
provenance and does not restore it as native authority. Old v1 checkpoints are
not converted. Reopening still creates a new checked plan from current native
scene/materials and consumes the original checkpoint exactly once on admission.
MCP states builder-received world evidence, predicted motion, no required
independent observer, no server stop acknowledgement and no physical position
error bound. Full integrated live acceptance passed; see the final record below.

## Completed native mining slice

Voxrig implementation `bed0465a5e3294862511e49d9d2fe57768c7201f` adds
`prepare_mining_profile_recovery`, explicit close and once-only same-profile
reconnect. The fresh session validates loading, exact authenticated identity,
healthy dry standing, mode, dimension, inventory and the declared target condition.
The same admission helper is used by the existing independent-retirement path.

The native Java 1.21.11 login state machine waits for a registered old same-UUID
player to disappear before login success. Native player removal marks the world
entity removed and removes its list entry before removing the UUID map entry.
Configuration admission rejects a remaining same-UUID player. Closed play handlers
also refuse queued packet application. The interpretation is restricted to direct
unmodified vanilla and exclusive profile ownership; local close or a timer is not
substituted for native retirement.

`MiningRecoveryEvidence.boundary` distinguishes independent removal from native
same-profile login. A shared original-source claim is retained before login I/O,
so cancellation cannot reopen a second login through a clone or the other method.
No old mining action, permission, Blueprint or persistent capability is restored.
`OriginalOrAir` is a read-only fresh target condition for caller reconciliation;
it is not permission to remove a foreign block or replay mining.

Native comparison passed three cases: ordinary dirt finish, early stone
finish/abort, and console-controlled air/immediate stone replacement. Recovery
did not register or await a viewer retirement watch. The retained stone stayed
unchanged for nine seconds without another miner; fresh public mining then
succeeded where needed. Supplied cobblestone was placed in the original cell and
independently compared for another nine seconds. The non-OP test and isolated
server both exited zero. This is bounded comparison, not an unlimited guarantee
against later external edits/effects.

The full native library suite before module extraction passed 198 tests with
seven opt-in tests ignored. After extracting shared recovery, 28 mining tests,
two documentation tests and all-target Clippy with warnings denied passed. The
final live driver compiled and all-target Clippy passed after its added retained
stone comparison. Sources, hashes, controller and traces are retained in the
separate Voxrig repository under
`docs/evidence/survival-single-profile-live-20261003.json` and
`docs/evidence/survival-single-profile-audit-20261003.json`.

## Approved movement decision and its limits

The existing runtime's common standing basis is `Received` or
`PredictedAndObserved`. The latter derives the horizontal geometry envelope from
an independently received position's quantization and its discrepancy from the
prediction. It also requires the original observer/entity/world lifetime to remain
valid. It is not an independent measurement of stopped motion.

The detached checker embeds this requirement too:
`HypotheticalAimRequirement::IndependentlyObservedEndpoint` carries the future
packet-error and prediction-discrepancy limits. Hypothetical moves produce that
obligation; they are not actual receipts. A later ordinary look, placement or
mining relies on fresh native standing admission to meet it.

Consequently, a prediction-only runtime with the old hypothetical obligation
would be inconsistent. Copying an observer error bound into a prediction-only
basis, assigning a convenient zero/fixed bound, calling the prediction received,
or accepting the old plan's independent requirement as satisfied would claim
evidence that was not acquired. No such change has been made.

Two concrete approaches are available:

1. **An explicitly prediction-based operational contract.** Voxrig retains the
   predicted pose/contact, dispatched inputs, native received corrections and
   geometry provenance. Native Rust types distinguish this from independent
   spatial corroboration. The contract must explain that geometry is evaluated
   from the model and that a server position/error bound is unavailable; any
   planning reserve is a reserve, not a measured bound. Native correction,
   impulse, generation change, unsupported conditions and interrupted submission
   invalidate continuation. Fresh target/inventory/interaction result checks and
   exact edit scopes remain. The hypothetical checker must produce this same
   declared requirement. Test observers compare the new mode without participating
   in its admission. Recommend this direction for normal client-like operation,
   subject to explicit review of the changed spatial corroboration.
2. **A fresh same-profile connection after each finite movement.** Complete and
   stop the bounded sender, close explicitly, use the audited profile exclusion
   and obtain a new received standing pose before construction. This avoids
   pretending a prediction is a receipt, but extends native recovery to movement,
   introduces more logins/chunk loading and requires hypothetical reconnect
   obligations after movement. It has not been implemented or timed; the mining
   trial is not movement recovery acceptance. A mismatching new pose requires
   a new plan, not teleporting or modifying the saved prediction.

The first path changes the confirmation basis for continuing, even though the
library responsibilities remain the same. The second adds a broader native
lifecycle primitive and caller-side reconnect obligations. This is why the
dependent movement implementation was stopped for a concrete choice. The user
subsequently selected the recommended explicitly predicted contract. The
preliminary investigation did not establish a prediction-only physical error
bound; the approved implementation must continue to state that limit explicitly.

## Boundaries retained in either approach

Voxrig owns version-specific protocol, identity, loading, physics, evidence,
single-operation guards and native connection recovery. DustRoute owns design,
route selection, supplied material accounting, edit/travel/retreat permission,
temporary ownership, durable intent, checkpoint claims and new continuation plans.
MCP reports effects, uncertainty and readiness separately. Prediction, own
receipts and independent comparison remain distinguishable.

No native gate is reconstructed from JSON. An uncertain action is not blindly
resent. Foreign blocks are not automatically removed. A checkpoint is consumed
once. The existing sender cancellation/closure guard and dry full-cube scope
remain. Entities, new terrain/tools, gathering and helper MODs are outside this
goal. Native recovery interpretation must not be reimplemented in DustRoute.

The old [observer-readiness blocker](survival-observer-readiness-prerequisite.md)
remains undiagnosed. One-bot production removed that dependency; it did not prove
the tracking issue fixed. Independent verification fixtures still need readiness.

The continuation trial now uses its own bounded aggregate writer/reader instead
of the production single-job helpers. The trial-only limit is 64 MiB, providing a
separate budget for multiple job records and previews in this fixed fixture; it
is not a production job size or a guarantee for arbitrary traces. It keeps the
existing atomic durable replacement and saves the final failure before checking
completion. Production job/journal limits remain 16 MiB. Offline regressions cover
round-tripping aggregate evidence above 16 MiB while both production helpers
continue to refuse it, and refusing trial input above 64 MiB. This prepares the
test infrastructure. At this historical checkpoint, the failed continuation
trial had not been repeated or reclassified as accepted. Subsequent fresh trials
passed as recorded in the integrated live acceptance below; the original failure
remains unchanged. The two offline regressions and native all-target
Clippy passed; formatting passed. See the
[verification record](evidence/survival-continuation-evidence-writer-20261003.json).

See the [preliminary investigation](survival-single-client-investigation.md) for
the initial evidence inventory. Current code references:
[native standing/motion](../vendor/voxrig/src/versions/java_1_21_11/client/operations/movement/control.rs),
[hypothetical requirements](../vendor/voxrig/src/versions/java_1_21_11/client/operations/movement/scenario.rs),
[executor calls](../crates/dustroute-mcp/src/survival_execution/native.rs),
[checkpoint](../crates/dustroute-mcp/src/survival_execution/checkpoint.rs) and
[public admission](../crates/dustroute-mcp/src/service/survival.rs).

## Integrated offline verification (2026-10-03)

The single-builder integration passed the complete MCP library suite (210 passed,
10 explicitly ignored live/profile tests), focused survival tests (31 passed,
6 ignored), and the new persisted prediction/world-edit separation regression.
Both default and Voxrig all-target Clippy passed with warnings denied. Formatting
and the managed 313-file vendor check passed. Logs and source hashes are retained
in [the offline verification record](evidence/survival-single-builder-offline-20261003.json).
At this offline checkpoint, integrated roof construction, cleanup, retreat and
two-process continuation were still pending. They subsequently passed the
separate live acceptance below; native movement acceptance alone was insufficient.

## Integrated live acceptance (2026-10-03 UTC)

Immutable DustRoute source `e4206c2cb463b6e611a28e44bf1736b4093cbc19` and the
managed native pin above passed three fresh isolated non-OP vanilla trials. The
production service had no configured observer; the separate test viewer compared
results only. Every final scene matched all 3,120 cells and the independently
received final position. Each trial removed all owned temporary blocks and
satisfied retreat; every test and server exited zero.

- Normal public construction: 115 completed steps, 30 predicted movement runs,
  67 received placements and 18 temporary removals/same-profile recoveries.
- Placement checkpoint: 11 completed steps saved with one owned temporary block;
  a different process generated and completed a 104-step plan. The saved standing
  basis remained explicitly predicted.
- Mining-requested checkpoint: request observed `mining_started`; the normal
  outcome and one same-profile recovery completed before saving 31 steps and six
  owned temporary blocks. A different process completed a new 85-step plan.

Both continuation cases independently verified that external changes were refused
without mutation, material shortage was reported, historical reads restored no
native authority, and a second continuation claim was refused. They are planned
settled-boundary continuation, not arbitrary interrupted-operation recovery.
Controllers, original traces, process identities, executable/JAR/JVM hashes,
commands and source pins are in
[the live verification record](evidence/survival-single-builder-live-20261003.json).
The test servers were stopped normally; no user world or host configuration was
changed. The public contract is documented in
[the updated workflow](survival-public-construction.md).
