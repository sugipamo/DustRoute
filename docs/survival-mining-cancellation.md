# Survival mining: delayed completion and cancellation review

## Newly identified concern

During the approved stationary player foundation work, the next timed-mining
stage was inspected against Minecraft Java 1.21.11. An early
`STOP_DESTROY_BLOCK` can leave a server-side delayed mining operation.
`ABORT_DESTROY_BLOCK` does **not** clear that delayed operation in the inspected
native handler. Therefore a timeout followed by abort cannot be reported as
"cancelled, no further block mutation". Starting the next construction action
or replacing the target at that point would be unsafe.

Mining implementation and live mining trials are stopped at this concern,
following the user's explicit instruction to stop and report concerns. This
review does not introduce a server mod or modify the live world. The implemented
inventory/standing foundations are retained independently.

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

This is **native code inspection**, not a reproduced live early-stop/abort trial.
No timing/lag fault injection or live world mutation has been performed. Client
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
