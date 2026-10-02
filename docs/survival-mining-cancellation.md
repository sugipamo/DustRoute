# Survival mining: delayed completion and cancellation review

## Initial concern and approved proposal

During the approved stationary player foundation work, the next timed-mining
stage was inspected against Minecraft Java 1.21.11. An early
`STOP_DESTROY_BLOCK` can leave a server-side delayed mining operation.
`ABORT_DESTROY_BLOCK` does **not** clear that delayed operation in the inspected
native handler. Therefore a timeout followed by abort cannot be reported as
"cancelled, no further block mutation". Starting the next construction action
or replacing the target at that point would be unsafe.

Mining implementation and live trials initially stopped at this concern,
following the user's explicit instruction to stop and report concerns. The user
subsequently approved the proposal below. Native comparison has now completed
on a separate disposable vanilla world; production mining remains unimplemented
because a new shared-sender prerequisite was found, as recorded below. The
implemented inventory/standing foundations are retained independently.

## Evidence and limits

The authority is the locally obtained target game's own
`ServerPlayerInteractionManager` in the same Java 1.21.11/Yarn 1.21.11+build.6
development oracle used for the standing comparison. The transformation only
remaps packages and widens non-private access flags; method bodies are unchanged.
The source hashes are recorded in the vendored
[foundation manifest](../vendor/voxrig/data/java_1_21_11/survival_foundation_source.json).
The inspection summary and its local-file digest are in
[the validation record](evidence/survival-foundation-20261002.json).
No decompiled body, game binary or mappings are redistributed.

Observed native control flow:

1. Starting an instantly breakable target can mutate it immediately; a start
   packet is already a mutation intent, not a reversible preparation.
2. For a matching non-air target, stop computes native breaking delta multiplied
   by elapsed **server** ticks plus one. At progress >= 0.7 it invokes completion.
3. Below that threshold, the native handler records `failedToMine`, target and
   start time. The server update loop keeps checking this delayed operation and
   attempts a break at progress >= 1.0, or clears it when the target is air.
4. The abort branch clears ordinary `mining` and the displayed breaking progress,
   but does not clear `failedToMine`. Neither an abort submission nor a transport
   acknowledgement proves that the delayed mutation disappeared.

The initial review was **native code inspection**; the later native comparison
is recorded separately below. No timing/lag fault injection was performed. Client
wall-clock ticks, periodic time packets and displayed cracks are not a server
progress fence. Received effects also do not provide a complete-list fence in
the current projection, so an empty effect map cannot establish an exact duration.

## Concrete proposed implementation

Keep the finite, stationary placement/mining milestone. Add a session-bound
mining intent and result reconciliation before exposing timed removal:

- Store target/native baseline, selected tool/slot evidence, interaction
  sequences, connection/dimension, and stage **before the first start send**.
  Separate start, finish-send-attempt, abort-send-attempt and confirmed result.
  Cancelled futures and write failures preserve the unresolved stage.
- Check standing/dry geometry, reach, target/face, known health, held stack and
  the supported mining conditions. Estimated duration is a scheduling estimate;
  it must not certify cancellation or accepted completion.
- After attempting a finish send, retain an unresolved mining marker on timeout,
  abort, cancellation, changed target, missing chunks or unknown conditions.
  Refuse another world/material/tool action while it could affect that intent.
  Resume a read-only result wait without replaying a start/finish packet.
- Publish separate results such as `observed_removed`, `pending_after_finish`
  and `requires_inspection`. A received expected world change establishes a
  result observation, not an independently attributed proof that the bot caused
  it. Do not report a safe cancellation from acknowledgement alone.
- Audit and test recovery of an unresolved delayed operation, including the
  server lifecycle on disconnect/reconnect, before authorizing another action
  at the target. A client-side disconnect alone is not a server-removal receipt.
  Until there is sufficient evidence, remain stopped and expose the intent to
  inspection. Never compensate by replacing or destroying another block.
- Feed this native state machine into DustRoute's shared durable-intent and
  operation diagnostics boundaries when the survival executor is connected.
  No creative/teleport/operator-write fallback enters the builder path.

Validate the native start/normal finish/early finish/abort paths in an isolated
fixture, with retained input and observed block/inventory evidence. The early
finish case must demonstrate that an abort is not treated as resolved while a
late break remains possible. Then implement and compare the client/result state
machine. This adds an explicit unresolved mining responsibility; it does not
expand the goal into tools, arbitrary effects, combat or general terrain mining.

The recommendation is to adopt this result-based mining boundary and keep the
overall survival construction goal. A fixed-delay dig followed by an unconditional
"cancelled" return should not be the first implementation.

## Approved comparison and current stop

After approval, an ignored, test-private Voxrig driver exercised three cases on
a dedicated vanilla Java 1.21.11 server with non-OP survival miner and observer
accounts. A separate console actor prepared the fixture; no existing user world
was reused. The normal dirt finish was observed as air at 1204 ms. An early
stone finish and abort at 50 ms still produced air at 7517 ms. After an early
finish and disconnect, the observer saw the miner removed and stone retained
through 9306 ms. The last result is a bounded observation, not a general safe
cancellation guarantee from client shutdown alone.

The [native comparison](../vendor/voxrig/docs/survival-mining-comparison.md) and
its linked raw observations, server log and provenance retain both the successful
trial and an initial fixture-control timeout before any mining input. The
[integration record](evidence/survival-mining-comparison-20261002.json) identifies
the source pin and validation. The isolated server was cleanly stopped.

Source inspection then found that an interrupted outbound frame can leave the
connection reusable for automatic responses. That failure mode was **not**
injected or reproduced. Pending mining intent cannot protect the shared framing
boundary by itself. Following the user's concern stop condition, production
mining and the sender modification initially stopped for review of the concrete
[1.21.11 sender prerequisite](survival-send-cancellation.md). The already approved
mining intent/result proposal above remains the next mining step after that
prerequisite is addressed.

## Approved sender and implemented observations

The user approved the sender prerequisite. Guarded frame dispatch and closed
operation history are now implemented. The subsequent mining slice records
START/FINISH/ABORT separately, keeps pending work on cancellation/timeout, and
uses fresh target-specific receive evidence. Its prototype native comparison
passed all three cases; see [the API scope](../vendor/voxrig/docs/survival-mining.md).

An additional native control-flow distinction prevents enabling the next
mutation: externally supplied air may precede the update which clears delayed
mining. Air observation is therefore a result, not continuation authority.
The current stricter gate blocks following mutations even after observed
removal. Further release/recovery work is stopped for review of
[the concrete continuation boundary](survival-mining-continuation.md).
