# Java 1.21.11 interrupted outbound frames

All live 1.21.11 user packets and automatic responses use the same session
sender. Waiting for its writer lock is cancellable without affecting framing.
After acquiring the lock, the sender checks health again and arms a write guard
before invoking the existing packet writer. The guard is disarmed only when the
length prefix, frame and flush all complete. Cancellation or any write/flush
error records the first uncertain packet ID, marks the connection closed, wakes
blocked send/receive work and triggers stream shutdown through receive teardown.
Queued sends recheck health under the lock; automatic responses cannot append
another frame on the interrupted connection. No wire format or 1.16.1 sender
change is made.

`ErrorKind::UncertainDispatch` distinguishes this boundary from a rejected input
or an ordinary closed connection. It does not report how many bytes reached the
server or whether an action occurred. Encoding errors inside the armed writer
also conservatively close the connection. There is no automatic retry, reconnect
or undo. A cancelled caller has no return value; inspect its owning connection.
Login packets before the live session are still confined to connection setup:
cancellation there drops the unpublished transport instead of reusing it.

`operations.operation_history().await` works after closure. Its connection ID,
last receive sequence, interrupted packet ID, receive failure and pending
inventory/mining records are **historical diagnostics**, not a current observation or
action capability. Ordinary state/world queries and all sends refuse a closed
connection. Keep the handle until unresolved intent has been saved by the
controller. A fresh connection cannot import those records as permission to act.
Creative inventory intent is now marked before I/O, like ordinary swaps, so a
cancelled submission does not erase its uncertainty.

Six bounded tests cover complete compressed/uncompressed frames fragmented one
byte at a time; harmless cancellation waiting for the actual writer lock;
deterministically cancelled partial writes; queued user/automatic responses and
real TCP teardown; write/flush errors; explicit closure during a blocked write;
and inventory history after closure. Partial framing uses the exact live helper
with a one-byte duplex stream, without saturating kernel TCP buffers. This is
not a live-server corrupted-stream trial or a host failure injection.

The [native mining comparison](survival-mining-comparison.md) remains separate
behavior evidence. Preserving an intact transport does not resolve a delayed
server mining action: that needs its own intent/result boundary.
