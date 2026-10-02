# Mining removal observation and continuation authority

## Completed before this stop

The user approved the 1.21.11 sender prerequisite. The shared sender now marks
interrupted writes unusable, refuses queued user/automatic packets, wakes receive
teardown and retains historical operation diagnostics. Six bounded stream/TCP
checks pass. See [sender implementation](../vendor/voxrig/docs/survival-outbound.md).

The approved mining intent/result work now provides a finite stationary,
empty-hand dirt/stone removal API, separate START/FINISH/ABORT attempts, target
receive evidence and inspection after timeout, conflict, chunk/world change or
closure. A native prototype comparison passed normal finish, late removal after
early finish+abort, and a bounded disconnect case. The retained
[API observations](../vendor/voxrig/docs/survival-mining.md) explicitly identify
that the run preceded the conservative continuation restriction. The updated
opt-in driver has not been rerun after stopping on this concern. Final offline
checks cover the stricter restriction; full survival construction is unfinished.

## Newly identified gap

Native `ServerPlayerInteractionManager.update` checks `failedToMine` first. If
its target is non-air and progress reaches 1.0, it clears that flag **before**
calling its own break. If the target is already air, it clears the flag when that
later update sees air. `ABORT` clears ordinary `mining` but not `failedToMine`.
These control-flow facts come from the existing unchanged-body Java 1.21.11
oracle, whose source is identified in the vendored foundation manifest. No new
game body is redistributed.

A received air packet does not distinguish the bot's completed delayed break
from another actor's removal before that next update. In the latter case,
allowing immediate replacement can let the delayed operation act on the new
block. This is **source inspection**, not a reproduced external-removal /
replacement race. It is a missing continuation boundary, separate from whether
the requested air result was observed.

Following the user's instruction to stop on concerns, the next-mutation release
mechanism and more live trials are stopped. The saved API returns observed
removal with `continuation_validated: false`; all following user mutations on
that connection remain blocked. There is no bypass flag, auto-retry or automatic
reconnect. Read-only results and closed-connection history remain available.
The dedicated native fixture server has been cleanly stopped and saved.

## Concrete proposed next work

Keep result observation separate from operation authority. Do not enlarge the
material/tool/geometry admission list while resolving this boundary.

1. Model the continuation stage explicitly alongside the existing result. Air,
   elapsed client time, displayed cracks, FINISH acknowledgement and ABORT must
   not individually set a validated continuation flag.
2. Audit a vanilla server-progress boundary on this target version: identify
   exactly where periodic time or other replies are emitted relative to player
   interaction-manager updates. A new received packet is not automatically a
   tick fence. If no adequate boundary exists, refuse in-session reuse rather
   than adding a fixed wall-clock sleep as proof.
3. Audit the already proposed independent server-removal + fresh-connection /
   fresh-site recovery path. Keep the original intent as history. A new client
   ID, socket closure or reconnect success alone cannot substitute for old
   player removal evidence. Do not import the old observation as a new
   construction capability.
4. Compare controlled native cases: ordinary finish, early finish+abort,
   externally supplied air while delayed mining remains possible, immediate
   replacement, and the selected recovery/fence path. Use a separate disposable
   fixture and retain target input times, received results and miner/observer
   lifecycle evidence. No host fault injection or existing user-world trial.
5. Open the shared mutation gate only from that validated evidence. Preserve
   blocked/inspection state after target replacement, unloaded chunks, unknown
   native conditions, changed dimensions, failure or missing recovery evidence.
   Add meaningful ordered receive/loopback tests and expose why continuation is
   refused through the same history/status diagnostics.

Once the boundary passes comparison, return to nearby survival placement and
material accounting, followed by walking/navigation, temporary access planning,
durable Blueprint jobs and the declared roofed non-OP construction trial.
