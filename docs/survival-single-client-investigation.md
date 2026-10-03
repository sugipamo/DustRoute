# Preliminary investigation: one survival builder and observer

Recorded 2026-10-03 UTC. Reviewed DustRoute `0bfbd57` and its immutable Voxrig
snapshot `2b6e7bfc94e6270054eac5c7b14a74d4657a411c`. This is source and retained
evidence review. No production implementation, compilation, server start or new
live trial was performed. Stage 4A continuation acceptance remains incomplete.

## Conclusion and intended scope

Normal construction and observation can plausibly use one bot. The builder
already receives block states, inventory, health, attributes, position
corrections and operation results. The additional bot is required by current
movement and mining continuation contracts, not by an inability to observe
blocks on the builder connection.

Recommend one builder in normal operation and an independent observer in
verification fixtures. This is a proposed architecture, not an implemented or
accepted construction mode. Independent test observations must not become a
hidden prerequisite for the production mode being tested.

Two targets have different requirements:

* **One bot/account**, allowing explicit sequential reconnection: a possible
  native same-profile recovery boundary needs investigation.
* **One continuously reused connection**: mining also needs a validated
  in-session continuation boundary. Removing the second account alone does not
  establish this stronger target.

Neither target requires a physics/planner rewrite based on the reviewed code.
Neither is achieved by passing the builder as its own observer or disabling
existing checks.

## What the observer currently contributes

| Responsibility | Existing builder evidence | Current extra observer dependency | Migration assessment |
| --- | --- | --- | --- |
| Block and material observation | Received loaded blocks, target-specific updates, plain player inventory | Another connection reads target/site | Reuse the builder projection; disclose loss of independent comparison |
| Placement result | Processed interaction sequence, fresh exact target update and exact selected-stack decrement | DustRoute additionally waits for matching target state | Native result checks are reusable; independence is additional assurance, not an ACK meaning success |
| Position after walking/jumping | Bounded prediction, submitted controls, own corrections and received terrain | Exact spawned-player watch and later position corroboration | Needs an explicit prediction-based standing contract and validation |
| Safe continuation after mining | Air result and retained mining attempts, including uncertainty | Exact profile removal after source closure, followed by fresh recovery | Main unresolved prerequisite; current API intentionally refuses reuse even after observed air |
| Start, idle checkpoint and final completion | Builder captures the scene and validates native operation history | Independent whole-site comparison | Must record the new evidence basis and reject missing/changed state; cached data is not a fresh server acknowledgement |

The source changes would span Voxrig's checked contract, movement completion,
common standing/aim admission and mining lifecycle, then DustRoute's executor,
admission, checkpoint evidence and MCP descriptions. Treat this as a medium to
large cross-layer change, with mining feasibility determining the scope. No
timing or throughput improvement was measured in this investigation.

## Movement: possible, with a different stated basis

The native ordinary movement success path does not supply an own-position
receipt for every accepted step. Current `StandingPositionBasis` has `Received`
and `PredictedAndObserved`; it has no prediction-only construction basis.
`start_control_path` explicitly requires a distinct connection/profile, and
standing checks retain the observer's exact entity/world lifetime.

A one-bot basis would need to retain predicted pose, velocity/contact, complete
dispatch, terrain provenance and an explicit uncertainty policy. Corrections,
impulses, generation changes, partial writes and unsupported conditions must
invalidate continuation. Fresh clearance, support, reach and aim checks must
use that basis throughout the common operation layer. An uncertainty bound for
this new basis is not established by the present observer quantization bound.

The existing observer does not prove server-measured rest either: its ground bit
comes from the moving client's ground flag. It does provide additional spatial
corroboration. Removing it changes the basis for proceeding and can miss a
disagreement that the second client would have exposed. Silence and elapsed time
must not be renamed position confirmation.

The established dry full-cube model, control sender, continuous conservative aim
checks and bounded planner are reusable. Preserve current terrain, effects,
fluid and entity exclusions during this migration.

## Mining: the first feasibility decision

The current checked API distinguishes result from continuation permission:
`MiningRemoval.continuation_validated` remains false, and the shared mutation
guard rejects any retained mining record, including `ObservedRemoved`.
DustRoute finishes each cleanup through closure, independent retirement and a
fresh connection. This is an intentionally conservative API contract; it is
not evidence that every ordinary Minecraft mining action inherently requires
reconnection.

The retained native trial establishes a real reason for the guard. External air
and immediate replacement completed at 1,454 ms; the replacement was later
removed at 7,459 ms. Observing air, an ABORT, a processed interaction sequence
or a fixed delay does not by itself establish that the delayed miner is cleared.

Existing native audit and cached disassembly distinguish the relevant paths:

* `ServerPlayerInteractionManager.update` clears `failedToMine` when it reads air,
  or before its own delayed break attempt. An externally supplied air receipt
  does not establish that this update has occurred.
* Early STOP can set `failedToMine`; ABORT clears ordinary `mining` without an
  unconditional clearing of that delayed flag.
* `PlayerManager.remove` removes the player before broadcasting exact UUID
  removal. That receipt is the present independent retirement boundary.
* Closed play handlers reject queued application; local socket closure alone
  does not establish that the old native player has been removed.

Two candidate investigations follow from this evidence. Neither is implemented:

1. **In-session release.** Audit whether the admitted ordinary mining path and
   available same-connection receipts can establish that the interaction manager
   has no remaining action. Any proposed tick boundary must prove its ordering
   relative to that player's update; one post-air world-time packet does not.
   Preserve separate treatment of early FINISH, external removal and interrupted
   actions. Do not infer success attribution from received air.
2. **Same-profile fresh recovery.** The existing `PlayerManager` disassembly
   contains `disconnectDuplicateLogins(UUID)`, collecting matching old players
   and calling their disconnect handlers. This helper alone does not prove that
   new-login completion follows old-player cleanup. The login/configuration
   callers, tick removal and queued old handlers must be audited before a new
   session can supply retirement evidence. This may permit one account with
   sequential reconnections, without proving safe continuous reuse.

The cached review did not establish either complete boundary. The existing
cache has no login/configuration handler disassembly in the inspected directory;
no new disassembly was generated. This is an investigation gap, not a conclusion
that vanilla makes one-bot operation impossible. A server MOD has not been shown
necessary and is not included in the recommended scope.

## Library boundary and reusable work

Voxrig should own packet/native lifecycle interpretation, physics, standing
evidence, single-operation guards, checked continuation and connection recovery.
DustRoute should continue to own designs, scope, route selection, supplied
materials, temporary ownership, durable jobs and explicit continuation plans.

Expose evidence and supported semantics through Rust types and checked API
contracts. Avoid a series of `observer_optional` bypasses in placement, mining
and checkpoint callers. The current `ObservedDryCubeV1` explicitly promises
independent endpoint observation; do not silently reinterpret old evidence as
acceptance of a new mode. Historical JSON remains diagnostic and cannot restore
native authority. Preserve the framed-write cancellation/closure guard and
single-use checkpoint claim.

MCP results should distinguish an observed effect, unresolved old action and
readiness to continue, and state whether position is received, predicted or
independently corroborated. A successfully captured local cache is not proof
that the server has freshly revalidated every unchanged cell. Two observers do
not provide an atomic world lock against human edits either.

## Recommended order and acceptance

1. Establish or reject a one-bot **mining continuation boundary** first. Audit
   the two candidates above and propose the exact checked contract before
   implementation. Without it, placement-only success would leave scaffold
   cleanup and the roofed construction goal unsupported.
2. Define and implement the **one-bot movement/standing evidence** in Voxrig,
   reusing the existing model. Keep the observer in test fixtures to compare
   predictions and resulting interactions. Test walk/rest/place, jump/land/place,
   changed terrain and corrections without a runtime observer dependency.
3. Migrate **DustRoute admission, execution and durable evidence** to the chosen
   native contract. Verify placement/mining cycles, material changes, cleanup,
   retreat and settled checkpoints before declaring one-bot construction usable.
4. Repeat the **separate-process continuation** trial through final completion,
   including a checkpoint requested during mining. Retain an independent final
   audit as test evidence, not production authority. Preserve the unresolved
   lost-action refusal case.

If neither mining candidate establishes the boundary within declared vanilla
semantics, stop and report it before adopting a weaker workaround or introducing
a helper MOD. World locks, general entities/terrain, material gathering and
restoring capabilities from persisted records are outside this proposal.

The previous missing spawned-player readiness issue remains undiagnosed. A
one-bot mode would avoid that production dependency, but would not prove the
tracking problem fixed. Independent fixtures still need reliable readiness.
Their evidence aggregation also still needs its recorded size-bound fix.

## Evidence and source pointers

* [Shared checked API and declared contract](../vendor/voxrig/src/checked_survival.rs).
* [Motion standing and observer admission](../vendor/voxrig/src/versions/java_1_21_11/client/operations/movement/control.rs).
* [Mining result and retained uncertainty](../vendor/voxrig/src/versions/java_1_21_11/client/operations/mining.rs)
  and [shared mutation guard](../vendor/voxrig/src/versions/java_1_21_11/client/operations.rs).
* [Mining retirement audit](../vendor/voxrig/docs/survival-mining-retirement.md),
  [audit manifest](../vendor/voxrig/docs/evidence/survival-mining-recovery-20261002-source.json)
  and [replacement-race evidence](../vendor/voxrig/docs/evidence/survival-interaction-loading-20261002-source.json).
* [Implemented movement contract](../vendor/voxrig/docs/survival-motion-controls.md),
  [earlier evidence audit](survival-movement-evidence.md) and
  [native motion audit manifest](../vendor/voxrig/docs/evidence/survival-motion-evidence-source.json).
  Earlier unimplemented checkpoints are superseded where these documents say so.
* [Executor](../crates/dustroute-mcp/src/survival_execution.rs),
  [native calls](../crates/dustroute-mcp/src/survival_execution/native.rs),
  [checkpoints](../crates/dustroute-mcp/src/survival_execution/checkpoint.rs) and
  [public admission](../crates/dustroute-mcp/src/service/survival.rs).
* [Current continuation acceptance blocker](survival-observer-readiness-prerequisite.md)
  and [retained trials](evidence/survival-continuation-investigation-20261003.json).

Read-only cached disassembly used from `/root/Voxrig/.local/survival-continuation/`:
`PlayerManager.javap` SHA-256
`ed8035f2f17d67b93863c77b9846d9cba8877e5198a5b1c3cf182df6cf2b82a7`;
`ServerPlayNetworkHandler.javap`
`e705cd3f36856d4b47fcb2e6e141252605efe43c38b94cb4bb59bf77b17ff642`;
`PacketApplyBatcherEntry.javap`
`094c595f969e755369d89b4fc0c170cc6fc4e0b8c4e0c7dac8e90954ef8b653a`.
These match the retained retirement manifest. The reviewed
`ServerPlayerInteractionManager.javap` hash is
`21844711c1dc3d6c98467f42dde0f7aeaef94f9807461644123aa206bc87e5a3`;
that file is not listed in that manifest. No native game method bodies are
copied into this report. Same-profile login completion ordering remains unaudited.
