# Java 1.21.11 native survival mining comparison

An opt-in, test-private native action driver compared mining on a dedicated
vanilla Java 1.21.11 server at localhost:25572. The world, non-OP survival
accounts and fixture were independent of the existing integration/user world.
The builder used empty hotbar slots. Only the separate console actor prepared
the dry stone platform, reachable dirt/stone target, teleports and empty inventory.
No builder commands, creative mode, generated held items or server mod were used.

| Case | Native input offsets from start | Received result |
| --- | --- | --- |
| Normal dirt finish | start 0 ms, finish 1101 ms | Observer received air at 1204 ms |
| Early stone finish, then abort | start 0 ms, finish/abort 50 ms | Observer received air at 7517 ms |
| Early stone finish, then disconnect | start 0 ms, finish 50 ms | Target remained stone through 9306 ms; observer no longer tracked the miner |

The second case reproduces the native delayed-mining concern: abort after an
early finish must not authorize a cancelled result or the next mutation. The
third is a bounded observation with entity removal, not a general guarantee
from client shutdown alone. It does not authorize reconnect/replay of an
unresolved mutation.

Raw evidence includes both received block snapshots and packet traces for
connected clients, own-player/ground observations, and the separate observer's
player list. See [trace](evidence/survival-mining-native-20261002-b.json.gz),
[server log](evidence/survival-mining-server-20261002.log) and
[provenance/failed trial](evidence/survival-mining-native-20261002-source.json).
No native game binary or decompiled implementation is included. The initial
attempt timed out waiting for console fixture control before any mining input;
the successful run supplied control within the same bound.

## Reproduction boundary

`client/mining_native_trials.rs` is compiled only for tests, and is ignored by
the normal suite. It accepts only the reserved localhost port 25572. Its explicit
environment variables are `NATIVE_MINING_PORT` and `NATIVE_MINING_OUTPUT`; the
output is created exclusively and refuses overwrite. Run the ignored native
test with one test thread and an attached control stdin. Each `FIXTURE` message
describes the console setup, and entering a line starts that trial. Keep the
fixture chunks loaded. Fixture input waits are bounded to 60 seconds. Action
sequences are reused between cases by this raw comparison driver; none of its
result assertions rely on acknowledgement counters. Production requests must
use the connection's monotonic interaction sequence.

The driver sends `PLAYER_LOADED` and a grounded `LOOK` after receiving complete
nearby geometry. It intentionally uses private `BLOCK_DIG` writes for the
native comparison. This is **not** a public survival mining API or a bypass
available to a production controller. The dedicated server was cleanly stopped
after the trial.

## New transport prerequisite

Inspection of the current shared protocol writer shows separate awaited writes
of the length prefix, frame and flush. Java 1.21.11 `Session::send` holds a mutex,
but does not close/poison the connection if the future is dropped or an I/O error
interrupts a frame. After the mutex is released, automatic responses can write
on that stream. Preserving a pending mining marker alone does not establish an
intact outbound framing boundary. This is source inspection, not an injected
partial-write or live-server failure reproduction.

Before exposing a mining mutation, the proposed limited change is an armed
write-attempt guard in the **1.21.11 live Session sender**: arm after obtaining
the writer, check connection health before every send, and disarm only after a
complete write. Cancellation/error closes the session to further user and
automatic writes and notifies the receive-loop teardown. It does not claim the
already sent prefix/action had no server effect. Retained pending intent must
remain available as historical inspection on the closed connection, not as
permission to resume it. Waiting for the mutex without writing must not poison
the connection. The 1.16.1 sender and packet format need no simultaneous rewrite.

Validate completed sends, cancellation while waiting for the lock, a cancelled
partial write, I/O errors, prohibition of following automatic responses, and
retained uncertainty. Then implement the approved mining intent/result boundary
and compare it against these retained native cases. The user subsequently
approved this prerequisite. It is now implemented with bounded stream and
loopback verification; see [outbound closure/history](survival-outbound.md).
The comparison above remains the original native evidence.
