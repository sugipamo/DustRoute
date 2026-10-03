# Single-bot survival construction: plan and review boundary

Recorded 2026-10-03 UTC after the user requested the investigation's stages as a
goal. DustRoute work branch: `codex/survival-single-client`, based on `0bfbd57`.
Voxrig has a separate branch with the same name. Its source is edited separately;
the immutable vendor snapshot was initially
`2b6e7bfc94e6270054eac5c7b14a74d4657a411c`. After native acceptance it was updated
through the checksum-managed vendor script to
`4b46c66cd031eacc72bdf82736eaa60c57ccb9ab`, whose implementation is unchanged from
the live-tested `bed0465` source; only acceptance documentation/evidence was added.

Normal operation is intended to use one builder/account, with an independent
observer used in verification fixtures. The overall goal is unfinished. Movement
implementation and dependent DustRoute switching are stopped for the contract
review below, under the user's instruction to handle responsibilities carefully
and report concerns before continuing dependent work.

## Stages and current status

| Stage | Completion condition | Status |
| --- | --- | --- |
| 1. Native mining continuation | Establish a same-connection release or same-profile fresh recovery without an observer retirement watch; preserve uncertainty and once-only login guards | Same-profile recovery implemented and declared native cases passed; continuous reuse remains unsupported |
| 2. Movement and shared standing | Define the basis for one-bot walk/rest/jump/land and subsequent look/place/mine checks; compare against independent test evidence | Contract review pending; no motion or standing gate has been weakened |
| 3. DustRoute execution | Use the chosen native contract for admission, placements, cleanup, final checks and durable checkpoints; expose actual evidence in MCP | Pending stage 2; production observer requirement unchanged |
| 4. Construction continuation acceptance | Complete a roofed build and separate-process continuation through cleanup/retreat, including checkpoint requested during mining | Pending; prior continuation failures remain recorded, not relabelled as passes |

The shared dependency update is independent of the movement policy choice. It
does not switch DustRoute's executor to the new recovery method, remove its
observer requirement or authorize prediction-only standing. The consumer's
diagnostic mining test fixture now initializes the new retained recovery-attempt
field. [Integration verification](evidence/survival-single-client-integration-20261003.json)
passed 29 focused survival tests (six opt-in live tests ignored), all-target Clippy
with and without the native feature, formatting and verification of all 305
managed snapshot files. This accepts the dependency integration, not observer-free
runtime construction. No new Minecraft trial or full MCP regression run was made
for this consumer update.

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

## Movement decision requiring review

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
dependent movement implementation is stopped for a concrete choice instead of
silently deleting the observer guards. The preliminary investigation did not
establish a prediction-only physical error bound or choose a movement-reconnect
policy.

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
remains undiagnosed. One-bot production would remove that dependency, not prove
the tracking issue fixed; independent verification fixtures still need readiness.
The earlier continuation evidence writer's aggregation-size problem also remains
a test concern to fix before repeating that acceptance.

See the [preliminary investigation](survival-single-client-investigation.md) for
the initial evidence inventory. Current code references:
[native standing/motion](../vendor/voxrig/src/versions/java_1_21_11/client/operations/movement/control.rs),
[hypothetical requirements](../vendor/voxrig/src/versions/java_1_21_11/client/operations/movement/scenario.rs),
[executor calls](../crates/dustroute-mcp/src/survival_execution/native.rs),
[checkpoint](../crates/dustroute-mcp/src/survival_execution/checkpoint.rs) and
[public admission](../crates/dustroute-mcp/src/service/survival.rs).
