# Survival operation prerequisite: cancelled outbound frames

The user approved the unresolved mining intent/result proposal in
[the mining review](survival-mining-cancellation.md). Native comparison then
reproduced ordinary finish, delayed destruction after early finish plus abort,
and a bounded disconnect case on a new isolated vanilla world. The retained
[Voxrig comparison](../vendor/voxrig/docs/survival-mining-comparison.md) and
[provenance](../vendor/voxrig/docs/evidence/survival-mining-native-20261002-source.json)
record the exact scope and failed initial fixture-control attempt.

## New concern requiring a stop

`write_packet` in the current Voxrig source awaits the packet length prefix,
packet frame and flush separately. Its Java 1.21.11 live `Session::send` holds a
mutex, but has no cancellation/error guard that prevents reuse of a stream whose
frame may be incomplete. Dropping the future releases the writer lock; a later
user request or automatic response may write into the unfinished frame.

This is a source-derived possibility, not a reproduced partial-write failure.
An unresolved mining marker prevents intentional next construction actions,
but cannot prevent automatic keepalive/teleport/etc. responses from continuing
the stream. Thus retaining mining intent alone is an insufficient cancellation
boundary. The shared sender should be addressed before production mining.
This initially stopped sender and mining work. The user subsequently approved
the proposed sender prerequisite; it is now implemented and checked. The
completed comparison evidence remains historical. Mining observations now
exist, but the separate continuation boundary described below remains stopped.

## Concrete proposed change

Keep the first change in the 1.21.11 live session sender. Preserve the version
adapter and wire format, and do not simultaneously migrate the 1.16.1 runtime.

1. Acquire the writer lock and check session health. Waiting for the lock has
   not written bytes, so cancellation there must remain harmless to the stream.
2. Arm a write-attempt guard before the first possibly effective write. Disarm
   only once the whole framed packet write completes successfully.
3. If the future is dropped or the write fails while armed, mark the connection
   unusable for **all** later sends, notify receive-loop teardown, and shut down
   the stream. Check the unusable flag again under the writer lock so queued
   user requests and automatic responses cannot append another frame.
4. Preserve pending inventory/mining intent as historical diagnostics. A
   read-only history query must work after connection failure and explicitly
   identify the closed connection and uncertainty. It must not turn stale
   player/world history into a current observation or restored action authority.
5. Expose failure as uncertain dispatch/connection loss. Disconnect does not
   undo bytes already delivered, and no automatically retried mutation follows.
   Any continuation uses the separately audited server-removal and fresh-site
   observation boundary, rather than the previous connection's capability.

Validate full successful frames, cancellation before lock acquisition,
cancellation after a partial write, write errors, queued/automatic sends after
failure, and pending history retention. Use bounded stream/loopback tests; no
host fault injection or live-world corruption trial is required. A fragmented
receiver must still decode exactly the completed frames and never receive an
additional frame on the poisoned connection.

After this prerequisite passes its tests, return to the approved mining state
machine: intent before start, separate finish/abort attempts, timeout retains
unresolved work, read-only result waits, and no following world/material/tool
mutation until the result is resolved or sufficient recovery evidence exists.
Nearby validated placement, walking, access planning and the full survival
Blueprint executor remain later parts of the active construction objective.

## Implementation and new stop

The live 1.21.11 sender implements the guard and common closure boundary without
changing the 1.16.1 sender or packet format. Six bounded tests verify fragmented
successful packets, harmless lock-wait cancellation, interrupted framing,
write/flush errors, refusal of queued automatic/user responses, real TCP teardown
and retained inventory history. `operation_history` works after closure;
`UncertainDispatch` reports an interrupted attempt without asserting undo.
The native mining intent/result API also records held-slot and mining attempts
before I/O, and exposes pending/removed/inspection observations.

Native comparison then exposed a separate distinction: received air does not
prove that an externally affected delayed mining state has already cleared.
Following the user's concern stop instruction, all next mutations remain
blocked even after observed removal. The concrete
[continuation proposal](survival-mining-continuation.md) and
[validation record](evidence/survival-mining-implementation-20261002.json) identify
what is implemented and what is still unvalidated.
