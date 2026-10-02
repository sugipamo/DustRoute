# Survival interaction readiness: review boundary

## Finding and why work stopped

The user approved mining continuation/retirement work, with the standing
instruction to stop and report concerns. The native retirement path has now been
implemented, but an additional prerequisite was found while auditing the fresh
connection: **play/position readiness is not native game-interaction readiness**.
Loading-stage implementation and further native trials are stopped for review.

The existing unchanged-body Java 1.21.11 oracle is identified by
`vendor/voxrig/data/java_1_21_11/survival_foundation_source.json`. Source inspection
of `ServerPlayNetworkHandler` establishes:

- Its constructor calls `markRespawned`, which initializes a loading count of 60.
- `tickLoading` decreases that count once per player update. Client elapsed time
  is not an equivalent fence when the server is stalled, slow or reconfigured.
- `onPlayerLoaded` handles the ordinary serverbound notification and calls
  `markLoaded`, clearing the count.
- `canInteractWithGame` refuses interactions while that count is positive or the
  player is dead. `onPlayerAction` checks this before applying mining.

Voxrig's ordinary `ready` establishes local play/position availability. It has
not yet integrated the native loading transition as an operation stage. The
comparison driver sent test-private PLAYER_LOADED for original mining cases;
the new connection was tested only with an ordinary hotbar send, which is **not
proof that the server accepts its next mining/placement**.

The final code conservatively leaves both old mining and fresh recovery mutation
gates closed. Fresh recovery exposes new read-only site/player observations and
history with `recovery_loading_pending: true` / `interaction_ready: false`.
There is no bypass, fixed-sleep proof or public history-import capability.

## Concrete proposed change

Keep this in the shared Java 1.21.11 connection/operation layer, rather than
adding a special sleep or hidden packet in each construction/mining helper.

1. Audit the native client's loading conditions and packet order. Define separate
   local play readiness, complete required observation baselines and the loading
   notification stage, scoped to the current login/respawn/world generation.
2. Implement the ordinary PLAYER_LOADED transition using the guarded sender.
   Record its attempt before I/O and complete dispatch separately. Do not call
   dispatch a server acceptance acknowledgement. Interruption/cancellation and
   world changes must retain the right stage and refuse blind replay.
3. Connect ordinary mining/placement admission and explicit fresh mining recovery
   to that shared stage. Old observed air or retired job JSON must still not
   authorize an old connection or replay a construction plan.
4. Test ordered receive/send behavior, resets, cancelled sends, delayed baseline
   arrival and duplicate attempts with bounded offline loopback fixtures.
5. On the dedicated non-OP vanilla fixture, demonstrate an actual subsequent
   mining/placement on the fresh session without a test-private loading packet.
   Rerun the timed external-air/immediate-replacement case: the previous input
   was delivered too late and is not accepted as delayed-miner race evidence.

No new material/tool/terrain admission, walking, or Blueprint mutation is needed
for this prerequisite. If the native client audit reveals a larger dependency,
report it before implementation under the same stop condition.

After this boundary passes, return to nearby survival placement/material
accounting, walking/access works, durable planning and the declared roofed build.
