# Expanded piston electrical execution: source audit

This is step 1 of the [general placement roadmap](piston-general-placement-roadmap.md).
The target is Java 1.21.11, Yarn 1.21.11+build.6, with redstone experiments
disabled. The inspected local named artifact has SHA-256
`5b4aa12131a872a25dc32e7f017373d6e83502f6919154769296dd3de6af8c1b`.
Class hashes and methods are retained in the
[audit manifest](../crates/dustroute-minecraft/tests/fixtures/piston_electrical_source.meta.json).
This is static implementation evidence, not a new server observation.

The support-destruction exclusion below describes the initial scope. The current
v14 runtime adds [attachment support loss](support-loss-runtime.md) for admitted
components, with separate live evidence; direct piston crushing remains outside
that extension.

## Differences that must be implemented together

| Area | Target behavior | Required change |
| --- | --- | --- |
| Emission | Lever weak signal in all directions, strong toward its support. Repeater weak and strong toward its output. Redstone block weak 15, strong 0. | Use explicit source identity and receiver-to-source query directions, not a generic powered flag. |
| Conductors | A solid source supplies max(its weak output, strong input from its six immediate neighbors). Strong queries do not recurse through conductors. | Validate every consumed position and admitted block identity. Physical full-cube shape is not an electrical conduction proof: piston and redstone block use `solidBlock(Blocks::never)`. |
| Piston queries | Six adjacent queries excluding actual front, own-position DOWN query, then five queries around the space above excluding DOWN. | One direction-independent query path. Quasi power only affects behavior when a real callback requests reevaluation; do not add global polling. |
| Dust strength | External input is calculated with dust emission disabled, including dust strong output. Compare it with neighboring dust strength minus one. | Replace the historical horizontal-arm-gated approximation. Include above/below neighboring wires using the target's solid-block/clearance rules. |
| Dust shape/output | DOWN query is zero; UP returns wire strength; horizontal queries use the recomputed opposite arm. Both weak and strong output follow that rule. Dot/cross and line completion matter. | Preserve exact shape metadata; recompute geometry after shape callbacks. Unknown shape or consumed space is an error. |
| Dust notifications | On a strength change, notify around self and six adjacent positions in Java HashSet iteration order. Each batch then follows ordinary six-way order. | Preserve synchronous nested callbacks and coordinate-dependent ordering. The old nearby-wire set is insufficient. |
| Repeater callbacks | An unpowered repeater receiving its scheduled tick turns ON even if the input already fell, then schedules OFF. Locking is checked again at delivery. | Add a new callback law; the old stale-expected-level event law must remain unchanged. |
| Repeater scheduling | Different priorities for target orientation, ON and OFF; one pending block/position tick; due-tick membership suppresses duplicate requests. | Extend delivery state explicitly, including restoration and behavioral comparison. Do not keep deduplication/priority in an uncheckpointed cache. |
| Repeater notifications | Notify output neighbor, then that neighbor's five other adjacent cells. | Add conductor-aware output notification batches; a single output callback is insufficient. |

The Java `BlockPos` hash is `(y + z * 31) * 31 + x` with 32-bit arithmetic.
For the seven-element default HashSet, Java's spread hash and bucket order affect
the notification sequence. Consequently a translated or rotated Assembly needs
a fresh review at the actual target coordinates. A source-local pass cannot be
reused as placement authority.

## Implementation contracts

The electrical world profile uses fresh initial conditions, source-derived
electrical laws, one callback queue and the shared motion laws. The temporary
horizontal, vertical and isolated-direct profiles have since been removed.
Body direction does not select an adapter.

Fresh inputs require exact stable piston/head metadata, explicit device states,
and a complete rectangular known region containing every consumed coordinate.
The initial admitted identities are the existing ordinary passive materials,
glass, normal/sticky pistons and heads, levers, redstone blocks, dust and
repeaters. Unsupported components and moving snapshots without exact history
are rejected. A destroyed support or component is outside the declared scope;
reject before committing such a transition, rather than silently preserving it.

Construction must establish an explicit initial state and account for callbacks
during installation. New-location plans must transform blocks, known space,
input positions and bound terminals consistently, then validate the complete
destination environment and construction sequence. Initial placement support,
behavioral proof and live readback are separate gates.

## Required regression and comparison matrix

- Six body directions × normal/sticky × direct source side, front exclusion,
  conductor source and source orientation; alternate powered sides remain active.
- Dust above/below, dots/crosses/lines, climbs/drops, two sources, source removal,
  conductor input and no dust feedback through strong-source sampling.
- Repeaters at delays 1–4, short pulses, input reversal while pending, locking,
  unlocked delivery, shared due ticks, priority, deduplication and replacement.
- Quasi-only ON/OFF without notification, then a real triggering neighbor
  notification; body-above and front-source distinction; unknown upper cells.
- Independent and interfering mixed axes, simultaneous and reordered inputs,
  interruptions, carrier materialization and completion-triggered notifications.
- Exact checkpoints at microsteps, behavior states at roots, new/old rejection,
  persisted context/catalog round trips and immutable law resolution.
- Original vs translated/rotated world reviews, negative coordinates, chunk
  boundaries and coordinate-dependent dust notification order.
- Public MCP adoption and placement reject old proof, unknown destination,
  changed baseline, unsupported construction and unverified partial results.

## Progress

Four additive executable law programs cover source emission, conductor
combination, piston query acceptance and repeater callback decisions. The new
standard `dustroute.piston-electrical-callbacks.java-1-21-11.v1` profile and
`dustroute.piston-electrical-root-exploration.v1` behavior context select them.
Read-only world queries and callback execution now cover all six facings,
conductor and dust/repeater input, short repeater pulses and delayed quasi
reevaluation. Prioritized guarded block ticks retain their priority and
deduplication in checkpoint/behavior state; old FIFO scheduled ticks keep their
historical behavior.

The declared [live comparison matrix](piston-electrical-live-evidence.md) and
custom public MCP construction trials now supplement these source findings.
They establish the recorded conditions, not exhaustive Minecraft conformance.
Wire shape callbacks now preserve dot/cross history, visit connected-arm
above/below diagonals in `prepare`, and retain old.prepare/new.shape/new.prepare
ordering. Repeater output notifications precede the write's shape pass; lever
flags-3 notifications precede its explicit source/support batches. A separate
payload program admits moving redstone blocks and treats stable piston heads
as blocking collisions; the old payload program is unchanged.

Construction and teardown use explicit command roots and checkpointed method
continuations. Each proposed live step has a complete expected readback; a
nested wire write during onBlockAdded suppresses the outer write notifications
when the requested state no longer remains, as in World.setBlockState.
Execution and review use the electrical profile. Retired directional/direct-only
profile IDs are rejected without checkpoint conversion or a saved-profile default.
The raw historical captures remain unchanged.
