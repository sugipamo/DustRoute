# Java 1.21.11 interaction loading

The native connection now sends PLAYER_LOADED through the shared guarded sender
after receiving the current world's INITIAL_CHUNKS_COMING, a player position and
the player's complete chunk. Automatic position confirmation/movement responses
are sent first. A headless client uses actual received terrain instead of native
rendering readiness; it never invents terrain or uses a fixed elapsed-time bypass.

`InteractionLoading` is exposed in player state and closed operation history.
Its generation is the receive boundary of login, respawn or reconfiguration.
It records the initial-chunks receipt and notification attempt before I/O,
then complete dispatch separately. The last previous-generation attempt survives
reset as diagnostic history. None of these fields is a server acknowledgement
of a mining/placement result or a restorable capability.

`wait_until_ready` now waits for both position readiness and complete loading
notification dispatch on 1.21.11. Cancelling that wait does not cancel or duplicate
the receive task's notification. All ordinary user mutations require the current
loading stage; a send which is interrupted or cancelled remains unresolved.
The guarded sender closes uncertain frames. A new world clears current authority
and requires new receipts. Read-only observations and history remain available.
Terrain outside the supported dimension height or missing initial-chunks/terrain
receipts stays pending; unlike the graphical client, timeout does not force entry.

Explicit mining retirement/recovery uses this common readiness path and fresh
site/player/inventory validation. It can expose fresh operations after the
notification, while the original mining connection remains closed. Each new
world mutation still needs its own target-specific result. The old temporary
`recovery_loading_pending` flag is replaced by the common typed loading stage.

The unchanged-body native oracle identifies these control-flow facts:

- `ClientPlayNetworkHandler.tick` ticks `ClientChunkLoadProgress`, calls
  `setPlayerLoaded` once when it is done, sends PlayerLoadedC2SPacket and marks
  the local loaded flag. Login/respawn reset that native flag.
- `ClientChunkLoadProgress.Start` waits for INITIAL_CHUNKS_COMING, then enters
  LoadChunks. LoadChunks normally checks local rendering readiness; native timeout,
  out-of-height, dead and spectator exceptions are not used to authorize our
  bounded construction operations.
- `PlayerManager.sendWorldInfo` sends INITIAL_CHUNKS_COMING. The server's loading
  count is cleared by PlayerLoadedC2SPacket; a later action sent on the same
  ordered connection follows it. Its actual effect still requires observation.

Two additional bounded TCP tests exercise delayed terrain, cancelled readiness
waits, teleport-response ordering, one notification per world, respawn with a
missing new initial-chunks receipt, and notification cancellation before writer
acquisition with retained history and no replay. The existing mutation-refusal
test now exercises this common stage. Native comparisons use no private
PLAYER_LOADED send and require an actual new mining result after recovery.

Recovery also waits for every native cell inspected by stationary geometry,
including the one-cell halo across chunk edges. Own-chunk readiness alone does
not establish this larger site baseline. The same region calculation serves
readiness and validation; missing neighbors wait, invalid dimension bounds refuse,
and unsupported received geometry is not converted to an empty cell.

## Native acceptance

The [immutable run](evidence/survival-interaction-loading-20261002-c.json.gz),
[server log](evidence/survival-interaction-loading-server-20261002-c.log) and
[provenance](evidence/survival-interaction-loading-20261002-source.json) record
execution revision `0e6afd4d8431a71a0226c5b5fdecbb89e871363e` on the dedicated
non-OP vanilla fixture. All four cases passed, including retirement and fresh
loading. Normal dirt mining observed air at 1,218 ms; early finish plus abort
still removed stone at 7,562 ms. External air/immediate replacement completed at
1,454 ms and the replacement was later observed as air at 7,459 ms. This does
not claim the client necessarily receives the brief intermediate air.

Early disconnect retained stone through 9,300 ms. After independently confirmed
retirement, fresh public-API mining began about 250 ms after connection start
and independently observed removal in 8,573 ms. No test-private PLAYER_LOADED,
login sleep or console target edit occurred during that fresh mining operation.
The 8,500 ms mining intent estimate is action timing, not loading authority.

An earlier trial timed out waiting for fixture input; another exposed missing
neighbor terrain in fresh recovery. The final run includes the standing-halo
fix and its offline regression. The isolated server was stopped cleanly. This
acceptance does not establish survival placement, walking or a complete build.
