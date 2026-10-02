# Java 1.21.11 bounded survival mining observations

The native operation API now admits stationary, dry, grounded, empty-hand
removal of **dirt and stone**. It uses ordinary player-action packets and never
changes game mode, flies, teleports, creates items or writes world blocks.
The caller still owns site/edit permission; the client cannot infer Blueprint
or protected-terrain permission from geometry. Removing foot support is refused.

Select a received empty hotbar slot first. `select_hotbar` records the attempted
selection before I/O and its complete dispatch separately. Server hotbar updates
are decoded with the native VarInt slot codec, validated to 0..8 and distinguished
from local submissions. A cancelled selection is not a known main-hand baseline.
Unknown inventory components, cursor/window, held contents, health, posture,
motion, dry ground, reach or first native outline hit refuse mining. Modified
known mining attributes/effect updates also refuse it. An empty effect map is not
a complete-list fence; exact server duration is never certified from it.

`start_survival_mining` stores a connection/dimension/target/native predecessor,
held-slot receive evidence and monotonic interaction sequence before START.
`finish_survival_mining` and `abort_survival_mining` retain separate attempts
before I/O. Each stage is submitted at most once; cancellation does not replay
it. ABORT and interaction acknowledgements cannot resolve a delayed FINISH.
`dig_survival_cube` is only a scheduling convenience: its 1100/8500 ms dirt/stone
waits are estimates, followed by the same finish/result boundary. Cancelling the
sleep leaves a START intent for inspection. No automatic abort or retry follows.

`observe_survival_mining` and `wait_survival_mining` send nothing. They expose
`mining`, `pending_after_finish`, `observed_removed` or `requires_inspection`.
The wait is bounded to 30 seconds; timeout returns the current status and keeps
the intent. Confirmation requires a post-intent **target-specific block/section
packet**, applied to a loaded baseline, whose current reconstructed target is
air. Unrelated packets, global revision changes, optimistic/predicted air and
cached air are insufficient. Unexpected non-air replacement, target chunk
unload/reload or world reconfiguration latch inspection rather than silently
clearing intent when air arrives later. Closed connections expose the attempt
through `operation_history`, not a current observation API.

## Observed removal is not continuation authorization

The current conservative gate refuses **all next user mutations even after
observed removal**. `MiningRemoval.continuation_validated` is false. Commands,
look/movement/flight, held selection, inventory changes, creative breaks and
use-on-block share the gate. Automatic protocol responses still use the guarded
sender, because they are necessary for the connection itself. No public bypass
or method to import history as action authority is provided.

Native `ServerPlayerInteractionManager.update` clears `failedToMine` before
its own delayed break, but if another actor supplies air it clears the flag on
a subsequent update reading that air. Those two causes cannot be distinguished
from a single received air packet. Immediately permitting replacement at that
cell can therefore leave the native delayed operation applicable to a new
block. This is source inspection, **not** a reproduced replacement race.
Continuation/recovery needs a separately audited boundary before the gate can
be opened. Following the user's concern stop instruction, that work is stopped
for review. This finite observation API is not autonomous temporary cleanup or
complete survival construction.

## Evidence and limits

[The native API trace](evidence/survival-mining-api-20261002-a.json.gz),
[dedicated server log](evidence/survival-mining-api-server-20261002.log) and
[provenance](evidence/survival-mining-api-20261002-source.json) retain normal
finish (air at 1218 ms), early finish plus abort (air at 7492 ms), and early
finish plus disconnect (stone through 9334 ms, observer saw miner removed).
Inputs use monotonic sequences 1..7. Both connected result cases returned
`observed_removed`; abort and disconnect retained pending status/history.
This run preceded the conservative continuation gate and **does not validate
that gate's eventual release**. The amended opt-in driver starts distinct
connections between cases only after independent observer removal and explicit
fresh console fixtures; that amended driver has not been rerun after the stop.
The original raw comparison remains [separate](survival-mining-comparison.md).

Six offline tests cover admission/atomic selection updates and obstruction,
every following mutation, monotonic action stages, ack/abort/timeout, ownership,
resume without replay, target freshness, conflicts, chunk/world changes and
history after closure. The result test also verifies that observing air does
not permit the next mutation. Tools, arbitrary materials/effects, item-drop
recovery, survival placement/walking and full Blueprint construction remain
outside this implemented slice.

Once an intent's removal result is observed, subsequent result reads retain that
observation as history. They do not reassert the present contents of the target;
use a new world observation for that. History never opens the continuation gate.
