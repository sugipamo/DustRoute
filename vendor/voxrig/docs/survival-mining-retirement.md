# Java 1.21.11 mining retirement and fresh recovery

Fresh recovery uses the [common interaction-loading layer](survival-interaction-loading.md)
and new site/player/inventory observations. The original connection is never
reopened by an air result or by recovery.

Air is a result observation, not authority to reuse a delayed miner. All user
mutations on the original mining connection remain blocked. This slice instead
provides an explicit retirement/reconnect path for **direct, unmodified vanilla
1.21.11**, with two separately authenticated connections to the same endpoint.
Proxies, tab-list plugins and arbitrary server implementations are not admitted
as equivalent removal semantics. No bot becomes OP; console writes belong only
to the disposable comparison fixture.

1. `prepare_survival_mining_retirement(intent, observer)` registers an in-process
   watch while the observer knows the miner's exact login UUID and name. It does
   not disconnect, resend mining or mutate the world. An absent entity, player
   name match, previously received removal or cached profile absence is not proof.
2. Explicitly close the original bot. `wait_survival_mining_retirement` waits
   read-only for a **new PLAYER_REMOVE containing that UUID** and local source
   closure. It returns `pending`, `retired` or `requires_inspection`. Timeout or
   cancellation preserves the watch. The observer must remain live and in its
   registered world context. Entity destruction, other UUIDs, wall time, mining
   acknowledgements and ABORT cannot substitute. Profile reappearance before
   reconnect latches inspection. One observer retains one watch; registering a
   later miner after a completed receipt supersedes the old token.
3. `reconnect_survival_mining` is an explicit call using the original endpoint,
   name and version, with a declared expected target (air or the original dirt/
   stone state). It checks retirement again, opens a new connection and waits
   within its ordinary readiness timeout for new health, inventory and target
   baselines. It exposes fresh operations after validating the authenticated UUID,
   survival mode, original dimension, healthy dry grounded stationary geometry,
   supported inventory, unmodified known mining conditions and exact expected
   target. Missing or conflicting observations refuse recovery. No retry or
   inherited old context/job is provided. The reconnect attempt is recorded before
   I/O; cancellation or a parallel call cannot authorize another login from the
   same watch. Cancellation drops the unexposed new
   connection; the original mining history remains.

The result contains fresh operations and `MiningRecoveryEvidence`: old closed
history, independent removal receipt, new connection ID, stationary context,
target and receive boundary. A caller must create a new permission-checked plan.
It cannot replay a Blueprint job from the retired operation's JSON. The old
`continuation_validated` flag remains false because that connection is not reused.
`interaction_ready` records completion of the common loading notification; it is
not acknowledgement of a later game action. Required standing geometry includes
the one-cell halo, so recovery waits for neighboring chunks as well as its own
chunk. Unsupported received geometry still refuses admission. This is not yet
autonomous temporary cleanup, walking or Blueprint execution.

## Native control-flow audit

The existing unchanged-body development oracle and foundation source manifest
identify the primary Java 1.21.11 implementation. Only observations of its
control flow are retained; no decompiled game bodies are shipped.

* `MinecraftServer.tickWorlds` sends periodic time before each world's tick.
  `ServerWorld.tick` updates game age and chunks before iterating entities;
  `ServerPlayerEntity.tick` calls its interaction manager near its beginning.
  One post-air time packet therefore does not establish an intervening miner
  update. This implementation does not use time as in-session release authority.
* `ServerPlayNetworkHandler.cleanUp` calls player disconnect and
  `PlayerManager.remove`. The latter removes the player from its world and player
  collections **before** broadcasting `PlayerRemoveS2CPacket` with its UUID.
  The independent receipt establishes retirement of that native player instance.
* Queued `PacketApplyBatcher.Entry.apply` checks listener `accepts` before packet
  application. The native default calls `isConnectionOpen`; the play handler
  rejects a closed connection. Pending packets do not justify reopening the old
  sender after retirement.

Four offline TCP receive tests cover exact profile/UUID registration, stale and
unrelated receipts, entity destruction, local-closure-only refusal, timeout,
observer context loss/closure, rejoin conflicts, history and invalid reconnect
configuration, and cancellation during an actual login handshake followed by
refusal to open a second connection, and refusal of all fresh-session actions
while loading is unknown despite usable read-only geometry/history. The opt-in
API comparison also exercises ordinary finish, early
finish+abort, early disconnect and console-controlled air/immediate replacement,
then exact retirement and fresh-connection operation. Live acceptance is recorded
separately; availability of that ignored test is not evidence of its result.

## Historical comparison and the loading prerequisite

The [native run](evidence/survival-mining-recovery-20261002-a.json.gz),
[server log](evidence/survival-mining-recovery-server-20261002-a.log) and
[provenance/limits](evidence/survival-mining-recovery-20261002-source.json) identify
immutable execution revision `eda7f09d032247f6e8eaf22a2d8614a285556319`.
Ordinary finish, early finish+abort and early disconnect each reached exact
retirement, a fresh site/player baseline and an ordinary hotbar selection on the
new connection. The external-input case also recovered, but input arrived after
25.671 seconds; it is **not** proof of the delayed-miner replacement race.
The later single-attempt guard, conservative fresh-session loading gate and
extended lifecycle trace driver are covered by offline checks, not by that older
native run. That checkpoint driver stopped before further cases when its gate was pending
and reported `all_cases_executed: false`; that older run is not full acceptance.

Additional source inspection found that native `canInteractWithGame` rejects
game actions while `remainingLoadingTicks > 0`. The native loading stage starts
at 60 player updates and is cleared by `PlayerLoadedC2SPacket`; that checkpoint
client's `ready` flag did not establish it. The original-mining fixture sent a
test-private PLAYER_LOADED, whereas recovery compared only a hotbar send. That
send is not acceptance of a world interaction. Accordingly, production survival
interaction readiness and a following mining/placement are **not validated**.
The user subsequently approved the shared loading stage. Its implementation and
new comparison evidence are described in [interaction loading](survival-interaction-loading.md);
the historical limitations above do not claim the current fresh-session gate is closed.
