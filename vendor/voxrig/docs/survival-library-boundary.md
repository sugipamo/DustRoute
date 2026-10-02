# Survival library boundary consolidation

Approved by the consumer project's user on 2026-10-02. Keep the two Java version
adapters distinct and preserve the established 1.16.1 API. This work prepares a
client library for the caller's survival construction goal; it does not move
Blueprints, route selection, access layouts, resource reservations or durable
construction jobs into Voxrig.

## Order and acceptance

1. Separate hypothetical aiming uncertainty from conservative standing clearance.
   Share endpoint observation limits with actual motion admission, expose the
   future observation requirement as a distinct Rust type, preserve real action
   gates, and compare an isolated non-OP edge-move/place/retreat sequence.
2. Consolidate the version-selected survival API entry point and capability
   descriptions. The caller must be able to determine the supported checked
   contract without selecting Java module paths throughout its planning code.
   Unsupported adapter capabilities must refuse explicitly; existing 1.16.1
   features are not evidence of equivalent checked 1.21.11 behavior or vice versa.
3. Consolidate explicit mining retirement/recovery orchestration at the client
   boundary. Retain before-I/O intent, original history, independent exact removal,
   one-shot reconnect and fresh baselines. Expose resumable inspection and actual
   phase evidence. Do not imply safe same-session continuation or silently retry.
4. Update version capability and ownership documentation and integrate a validated
   immutable source snapshot into the consumer. Consumer plans continue to select
   routes, materials and permissions and to persist/replan their own jobs.

Work on support for new terrain, crouching, tools, gathering, crafting, arbitrary
servers or entities is outside this consolidation. An unexpected correctness
concern is reported before extending the approved implementation scope.

## Declared edge trial

Dedicated direct vanilla 1.21.11 on localhost:25572, survival, empty operator
list and the two existing whitelisted fixture profiles. A fresh flat fixture
world avoids older dropped-item interference. The console sets a stone floor at
y=-61, a single stone cube at `[0,-60,0]`, air above, the builder at
`[0.6,-59,0.5]`, an independent viewer at `[0.5,-60,4.5]`, and one dirt in the
builder's main inventory before the exercise. No later fixture world edits.

Before inventory/motion mutation, the driver captures native geometry and tries
1..8 eastward walking ticks, each followed by twenty released ticks. It must find
a conservatively supported endpoint with x=1.08..1.23 and unchanged feet y=-59,
then predict east-face dirt placement at `[1,-60,0]` and a westward retreat onto
the original support. It retains the complete hypothetical sequence first.

Actual motion previews must match those frames and both runs need independent
endpoint observation. Ordinary placement requires exact target/cursor agreement,
received one-item consumption, processed interaction sequence and independent
block readback. The final small region must contain only the declared stone,
one new dirt and exact air. Traces and failures are retained; no ambiguous action
is retried. This is edge-operation acceptance, not full construction or cleanup.

The initial x=0.5 fixture had no candidate in the declared integer-tick family:
three active ticks stopped at x=1.02826, while four failed terminal clearance.
No inventory, movement or placement action began. A diagnostic repeat retained
all rejected candidates. The next fixture starts at x=0.6 on the same support;
this changes only fixture geometry, not predictor admission or aiming limits.

## Consolidated boundary checkpoint

The aiming correction passed 185 native tests. The adjusted non-OP edge trial
passed with independent endpoint and placement observations, exact hypothetical
frames/cursor/target, one dirt consumed and an exact final region. See
[evidence and all attempts](evidence/survival-edge-20261002-source.json).

`Client::survival()` now selects the checked contract; `checked_survival`
provides its public data and operation surface. The existing legacy `survival`
module and 1.16.1 API remain intact. Capability discovery is static and explicitly
refuses that adapter's use of the new contract. The new `MiningRetirement` handle
binds source, observer and watch, with explicit close, resumable read-only waits,
original history and a once-only fresh reconnect. See [API ownership](survival-api.md).
No route selection, Blueprint, access layout, permission, material reservation
or durable job has moved into this library.

Native all-target tests: 185 passed, six live tests ignored. Four documentation
tests (including compile-fail boundary checks), all-target Clippy with warnings
as errors, formatting and whitespace checks passed. Exact retirement receipts,
local closure, rejoin conflicts and cancellation during a real loopback TCP login
are exercised through the new façade; native lifecycle guards remain shared.
The earlier successful edge run used the native public API before façade import;
its source revision is retained rather than relabeled as a façade live run.
