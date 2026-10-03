# Java 1.21.11 same-profile mining recovery

This native lifecycle slice supports one account with explicit sequential
connections on a direct unmodified vanilla endpoint. It does not establish
continuous reuse of a mining connection or observer-free movement. The caller
exclusively owns the profile. Proxies, plugins and other server implementations
are outside the audited contract.

`Client::survival().prepare_mining_profile_recovery(intent)` creates an in-process
coordinator without I/O or edits. Close its original source explicitly, then
call `reconnect(config, target_condition)` once. Local closure alone never
produces retirement evidence. A successful same-profile native login supplies
the lifecycle boundary, followed by the same fresh loading, health, inventory,
stationary geometry, dimension and target checks used by independent recovery.

`MiningRecoveryTarget::Exact` admits only declared ordinary air or the original
baseline. `OriginalOrAir` permits those two received outcomes for caller-side
reconciliation. Neither authorizes re-mining, transfers an old intent, reserves
resources or edits blocks. The caller checks ownership, permissions, whole-site
differences and whether a new plan is needed.

All recovery methods share one claim retained on the original mining record
before login I/O. A cancelled or failed same-profile login cannot be retried
through a clone or the independent-retirement method, and vice versa. Diagnostic
JSON cannot reconstruct the private coordinator/watch or release that claim.
The original sender remains closed and its removal's continuation flag remains
false.

`MiningRecoveryEvidence.boundary` explicitly distinguishes `independent_removal`
from `same_profile_login`; an observer receipt is never fabricated for the
latter. Capability discovery separately reports `same_profile_mining_recovery`.
The existing `ObservedDryCubeV1` movement still requires an observer.

## Native source audit

The existing unchanged-body Java 1.21.11/Yarn build.6 oracle was inspected with
`javap -c -p`, with a 256 MiB tool heap. Its `PlayerManager` output matches the
previous retained retirement audit byte for byte. Only control-flow facts and
input hashes are retained, not native game bodies. See
[audit manifest](evidence/survival-single-profile-audit-20261003.json).

1. `ServerLoginNetworkHandler.tickVerify` asks `disconnectDuplicateLogins(UUID)`
   to disconnect matching registered players. When duplicates exist, it enters
   `WAITING_FOR_DUPE_DISCONNECT`; its `tick` sends LoginSuccess only after
   `hasPlayerWithId` is false. With no duplicates it sends success directly.
2. `hasPlayerWithId` uses `PlayerManager.getPlayer(UUID)`, the UUID map. The
   duplicate helper checks both the player list and map.
3. `ServerPlayNetworkHandler.cleanUp` calls `PlayerManager.remove`.
   `remove` marks the player removed in its world and removes it from the player
   list before removing the UUID map entry and broadcasting PlayerRemove.
   The world entity tick loop refuses removed entities. The delayed mining state
   is owned by that old player's interaction manager.
4. Configuration `onReady` executes on the main thread and refuses a remaining
   same-UUID registered player before completing spawn. Normal connect adds the
   new player to the list and UUID map. Respawn replaces the old world entity;
   the admitted healthy normal session does not restore the old miner.
5. `ServerNetworkIo.tick` handles disconnection instead of ticking a closed
   connection. Queued packet application checks listener acceptance; a closed
   play handler rejects application. Old user frames remain guarded on the
   client and are never reopened by the new login.

Under these exact native semantics and exclusive profile ownership, successful
new same-profile login follows exclusion of the old registered player. This
reuses the removal ordering underlying independent retirement; it does not
infer native cleanup from a local socket close, a timer or an unrelated packet.
Fresh authenticated UUID/name/endpoint and survival scene checks still follow
login success. Receiving only a new TCP connection is insufficient.

The in-session alternative is not established here. Air can come from another
actor before `ServerPlayerInteractionManager.update` clears delayed mining.
Early STOP can set the delayed flag; ABORT does not unconditionally clear it.
A world-time receipt is emitted before the world/player tick and is not by
itself an intervening player update. This slice uses the audited fresh-login
boundary rather than introducing a time-based release.

## Verification scope

Three focused loopback/offline tests cover refusal before closure, foreign
attempt/profile and invalid target rejection before I/O, cancellation during
an actual login handshake, clone refusal and cross-method once-only claims.
The independent-retirement regression suite remains applicable to the shared
fresh admission and original method. Test outcomes are recorded separately.

The opt-in `native_survival_same_profile_mining_recovery` driver declares three
cases: ordinary dirt finish, early stone finish/abort and externally supplied
air followed immediately by original stone. It closes and reconnects without
registering or waiting for an observer retirement watch. Where the fresh target
remains stone it first compares that retained stone for nine seconds without
starting another mining action, so a later miner cannot hide an old delayed
effect. Then it performs fresh public mining, recovers again and places supplied
cobblestone in the original cell. The test-only viewer compares that replacement
for nine seconds. This is bounded comparative evidence, not a universal absence
of future server effects. No fixture edit occurs between recovery and replacement
verification. A driver is not a passed live trial; live evidence must identify
the immutable source and full case results before acceptance is claimed.

There is no route, Blueprint, scaffold ownership, material reservation, job
continuation or observer-free standing policy in this native lifecycle API.

## Accepted native comparison (2026-10-03 UTC)

All three cases passed on immutable implementation
`bed0465a5e3294862511e49d9d2fe57768c7201f`, with an empty operator list and the
ordinary common loading stage. See [results, provenance and artifacts](evidence/survival-single-profile-live-20261003.json).
The early case closed at 51 ms and admitted the new connection at 342 ms; the
external-input case closed at 203 ms and admitted it at 478 ms. Neither waited
for an independent retirement receipt. Each retained stone comparison and each
replacement comparison has 174 samples over approximately nine seconds.
Both retained-stone cases then successfully mined through fresh public operations,
recovered again and placed the supplied cobblestone at the original target.
The test exited zero, and the isolated server saved and stopped normally.

This accepts the declared native recovery slice, not an observer-free build or
safe continuous reuse. Movement still uses `PredictedAndObserved`. DustRoute's
production vendor pin and observer requirement have not been switched here.
