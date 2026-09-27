# Native lamp and stationary observer callbacks

This records the original device-law implementation and its evidence. Current
execution uses the [v7 declarative device programs](data-driven-block-runtime.md);
the original v1 lamp/observer laws and raw observations remain unchanged.

The common electrical piston runtime implements lamps and stationary/moving
observers. The supplied 3×3 door now matches the retained Java 1.21.11 run:
**381 tick-end worlds, 524 ordered palette writes, and 2,118 callback/carrier
boundaries**, including repeated closing/opening and checkpoint restoration.
[Movement staging and evidence](staged-piston-motion.md) describes the completed
refactor. Public construction/adoption of this reference layout is not certified.

## Implemented boundary

- Lamp: immediate lighting from ordinary neighbor notifications; delayed off
  after four game ticks, rechecking current power when the tick arrives.
- Observer: facing-side shape notifications while unpowered; two-game-tick
  start delay and pulse duration; directional weak/strong output in all six
  directions. A shape notification, including one caused by a same-identity
  state update, is the trigger. Generic cached-state comparison is not used.
- Observer writes drain their shape callbacks before scheduling the off tick
  and emitting output-neighbor updates. Command addition of a powered observer
  without a queued tick resets it using the target's skip-shape behavior.
- The shared scheduler distinguishes pending ticks from the remaining ready
  batch. Due callbacks do not count as `isQueued`; repeaters additionally use
  `isTicking`. Current execution is removed from the ready batch before its
  callback. Position/concrete-block guards and priority ordering are preserved.
- Lamp is a conductor and full support. Observer is full support but explicitly
  **not a conductor**, as registered by `Blocks.OBSERVER.solidBlock(Blocks::never)`.
  Dust connects to its output side. Internal observer facing is its output,
  opposite the imported Vanilla `facing` property.
- Native scheduled events and suspended post-write work are checkpointed.
  Behavior-state restoration admits the new queued event variants. A fresh
  powered observer snapshot is rejected because it lacks the pending history;
  explicit command addition and checkpoint resumption are distinct operations.

The world/exploration profile advances to **v5**, generic callback delivery to
**v3**, with two new immutable laws:
`dustroute.law.lamp.callback.java-1-21-11.v1` and
`dustroute.law.observer.callback.java-1-21-11.v1`.
The world pins thirteen law roles. Saved v1–v4 contexts are rejected instead of
silently rebinding saved validation. Existing immutable programs remain unchanged; the newly selected electrical
payload v3 law also admits observers. Public reference-door construction/adoption is not certified.

A narrow existing support guard also now allows a back-face attachment on a
source piston retracting in place. Java does not notify that attachment of a
shape change while the source body is a carrier; the stable body is restored
before its final shape notifications. The electrical full-face query for moving
pistons remains false. Moving an attachment's support as payload, component
destruction, and arbitrary temporary unsupported placements remain rejected.

## Evidence and tests

[Source and capture hashes](evidence/native-device-callbacks-20260927.json) pin
the target 1.21.11 classes and the added read-only `ObserverBlockMixin` in the
companion instrumentation project. Capture `reference-3x3-native-20260927-b`
uses the exact reference layout, translated to x=44000, with strict saved-state
initialization. Actual input offsets are **0, 101, 200, 300**. Both live open
cycles restore the complete initial region; 381 tick ends, 524 palette commits
and 812 carrier records are retained. Cleanup emptied the region, removed
force-load tickets, and the server stopped normally.

The [retained probe window](../crates/dustroute-translate/tests/fixtures/reference-3x3-observer-probes-b-v1.json)
contains 498 observer records: 266 shape notifications, 62 pairs of scheduling
entry/exit records, and 108 delivered ticks. Twenty-six shape records expose
`isTicking=true` with `isQueued=false`. The local Law test checks recorded
admission and palette writes, including calls that do not schedule a pulse.
**This checks local decisions given real callbacks; it does not establish that
the movement adapter delivers those callbacks correctly.**

Six focused runtime tests cover all six directions driving a lamp, short input
pulses, lamp repower at its delayed-off boundary, electrical traits, powered
command addition, source-body attachment retention, shared ready/pending queue
semantics, and restoration at queued and suspended boundaries. Existing piston,
repeater and recorded transient cases are regression-checked separately.

## Resolved movement-start mismatch

The previous v4 comparison recorded 238 mismatched tick ends: the first observer
pulse began at tick 5 instead of Java's tick 3. The user subsequently authorized
the movement refactor. Destinations now receive shape callbacks before their
moving entities are registered. The source/body writes and explicit notifications
retain their source order; suspended plans and unregistered writes are exact
checkpoint state. This removes that delay without special-casing the door.

The [old failing comparison](evidence/reference-door-native-comparison-20260927.json)
and the original native-device check manifest are retained as historical evidence.
Current checks and passing comparisons are linked from
[the movement completion report](staged-piston-motion.md).

Moving observers use the same native scheduler. Arrival post-processing can
schedule a pulse at the new coordinate; powered arrivals without a pending
local tick reset during onBlockAdded. Old scheduled ticks stay at their original
coordinates and are guarded by the block identity there. Three additional
isolated captures cover idle push/pull and reversal, a powered payload, and an
unpowered payload with a pending source tick. Each matches callback-visible
worlds and ordered palette writes, with checkpoint/behavior-state resumption.
