# Survival interaction readiness

## Implemented and compared

The user approved the shared loading prerequisite. Java 1.21.11 now records
loading separately from local play/position readiness and sends PLAYER_LOADED
through the guarded connection sender. The generation is reset on login,
respawn and reconfiguration. Required inputs are INITIAL_CHUNKS_COMING, a player
position and the received own chunk. Ordinary teleport responses precede the
notification; dispatch is recorded separately from any later action result.

Notification attempts are retained before I/O. Missing data, cancellation and
world changes cannot authorize blind replay or a fixed-time bypass. The common
stage gates ordinary mutations and explicit fresh mining recovery. Read-only
history remains available. Recovery additionally waits for every cell in the
stationary geometry halo, including neighboring chunks, before checking player,
inventory and target conditions. Its region calculation is shared with validation.

The old mining connection remains closed to further mutations, even after an air
result. Continuation uses a new exact-UUID removal receipt on an independent
observer, closed old sender, one explicit reconnect and new validated site data.
Each fresh action still requires its own result; old job JSON grants no authority.

## Native evidence

The [source implementation and comparison](../vendor/voxrig/docs/survival-interaction-loading.md)
and [integration record](evidence/survival-interaction-loading-20261002.json)
identify the source pin, raw traces, server log and checks. On the dedicated
non-OP vanilla 1.21.11 fixture:

| Case | Observed result |
| --- | --- |
| Normal dirt mining | Air at 1,218 ms |
| Early finish plus abort | Stone still removed at 7,562 ms |
| External air and immediate stone replacement | Console input completed at 1,454 ms; replacement later became air at 7,459 ms |
| Early finish then disconnect | Stone remained through 9,300 ms; independent retirement and fresh recovery succeeded |
| Actual mining after final recovery | Started about 250 ms after connection; both miner and observer confirmed removal in 8,573 ms |

No test-private loading notification or fixed login sleep is used. The 8,500 ms
stone mining estimate is action timing, not readiness evidence. External edits
were controlled console fixture inputs; the brief intermediate air is not claimed
to have reached the bot. Its later removal supports retaining the old-session gate.

All four comparison cases completed. Development run A timed out waiting for
fixture input. Run B exposed incomplete neighbor terrain during final recovery;
run C passed after the shared halo wait and regression test were added. The
fixture server was stopped cleanly, without changing the user's server.

## Native basis and limits

The existing unchanged-body oracle is identified by
`vendor/voxrig/data/java_1_21_11/survival_foundation_source.json`.
The native client waits for INITIAL_CHUNKS_COMING and local rendering readiness,
then sends PlayerLoaded once. The headless implementation requires actually
received local terrain. It does not use the graphical client's timeout,
spectator/dead or out-of-height exceptions as construction authority.

The native server initializes a 60-update loading count; onPlayerLoaded clears
it, and canInteractWithGame checks it before mining. Client elapsed time is not
equivalent to this transition. Complete notification dispatch is still not a
server acknowledgement that an arbitrary later world action succeeded.

Tests cover delayed chunks, response order, repeated notifications, cancelled
waits/sends, reset generations and neighboring geometry. The native comparison
accepts bounded stationary empty-hand dirt/stone mining and explicit recovery on
direct unmodified vanilla. It does not validate proxies/plugins or in-session
mining reuse, survival placement/material accounting, walking, drop collection,
autonomous temporary cleanup or the complete roofed Blueprint build.

Next in the declared roadmap is nearby survival placement with received material
accounting, then walking/access planning and durable Blueprint execution.
