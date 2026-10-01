# Shared runtime worlds and incremental construction checks

This phase improves the existing physical execution and construction-proof
paths. Public previews, saved jobs, operation history and complete per-command
expected snapshots retain their formats and meaning. It adds no blocks,
physics rules, placement permissions or observation shortcuts.

## Architecture and preparation

A repository-wide cleanup is not required. The prerequisite cleanup is local:
make the world-write boundary and the post-event observation boundary explicit.

- Private runtime `State` owns an `Arc<World>`. Input admission, queue changes,
  read-only events, checkpoints and behavior-state branches share it. A
  nonempty world delta detaches the staged state's world before writing.
- `State::apply_world_delta` is the sole runtime write boundary. Empty deltas
  still validate their shape and moves without detaching the world. The
  adapter receives a read-only world.
- `WorldDelta::apply` validates **all** source states, duplicate coordinates and
  movement representations before any write. Infallible `World::set` calls
  then apply changes directly, removing the previous second world clone.
- The outer staged `State` transaction remains. A later carrier, output,
  history or queue failure discards the entire staged update, including its
  detached world. Time, IDs and pending work do not advance on rejection; the
  runtime still marks the accepted prefix failed.
- `ConstructionObserver` separates full initial admission from checks after
  every committed microstep. `ScopeGuard` uses the committed runtime record's
  delta positions, including deletion and creation in previously Air cells.
  It checks before the next callback can conceal a transient protected change.
- Moving-block orphan checks use changed world positions plus both old and new
  active/staged carrier positions. Retiring metadata without deleting the
  moving block remains an error. All carrier and staged-payload validations
  remain in place.

The incremental check relies on a validated initial state, a previously
validated committed prefix and the single world-write boundary. Runtime records
are observations, not authority to replay arbitrary decoded changes. World
identity, exact state equality, behavior-state serialization, checkpoint adapter
matching and fresh live readbacks are unchanged; pointer identity is never used
as a physics or authorization check.

This is a local change across the runtime, delta application and construction
observer. It introduces one ownership type and one internal observer interface,
without a new index invalidation system or a shared mutable cache.

## Costs and remaining limits

Let N be non-Air context blocks, E processed events, W events with nonempty
world deltas, K the number of positions changed by an event, C active/staged
moving positions, and D requested placement positions.

| Work | Before | This phase |
| --- | --- | --- |
| Runtime world copying | Whole world for every staged event; a further copy on delta application | No world copy for read-only events; one whole-world copy on a nonempty delta, O(WN) |
| Protected-state inspection | Whole baseline/current world after every event | Full initial admission, then K positions per event, including Air transitions |
| Moving-block orphan inspection | Whole world after every event | Union of changed positions and old/new moving metadata, with ordered-set construction and world/map lookups |
| Queue, history, output and carrier state | Staged copies and exact comparisons | Unchanged; these costs can still grow with event/metadata counts |
| Shape/state hashes and exact state comparison | Whole-world work when requested | Unchanged; sharing does not make hashing or content equality constant-time |
| Expected snapshots retained by a proof | O(DN) records | Unchanged |
| Live context capture and readback | Whole observed region | Unchanged |

Inspection counts are not constant-time total-cost claims: ordered map/set
lookups and explicit scope-region checks still apply. Nonempty deltas may also
contain unchanged values; W is defined by submitted deltas, not net motion.

Saving expected states as shared/delta values is a separate phase. The current
full `MinecraftSnapshot` is consumed by ordinary placement, repair, buildings,
doors and flying machines, including saved instance records. A migration must
first define an immutable Rust expected-state type, explicit materialization
boundaries, serialization compatibility and retention accounting. It should be
measured against this phase before changing saved forms or removing them.

## Verification and measurement

See the phase's measurement/evidence records for source fingerprints, paired
standalone debug runs and regression results. The passive cost fixture keeps
64 forward and 64 inverse commands and complete expected states at every N;
it is not a functional proof of an arbitrary large active circuit or an
end-to-end placement benchmark.

Three paired standalone debug sweeps alternated before/after order on the same
host. Each sweep used separately compiled unchanged-baseline/current binaries
with no concurrent compiler or test server. Median proof times were:

| Context blocks | Before, seconds | After, seconds | Reduction |
| --- | ---: | ---: | ---: |
| 64 | 1.323 | 1.306 | 1.3% |
| 256 | 0.586 | 0.276 | 52.9% |
| 1,024 | 2.531 | 1.061 | 58.1% |
| 2,048 | 5.211 | 2.117 | 59.4% |
| 4,096 | 10.874 | 4.327 | 60.2% |

The 64-block row includes cold registry initialization. At 4,096 blocks, the
three before samples ranged from 10.229 to 14.700 seconds and after samples
from 4.281 to 4.982 seconds. Standalone process peak RSS medians were 71,220
KiB before and 69,608 KiB after (about 2.3% lower). Both retained exactly
520,192 per-command expected block records. This demonstrates a speed gain for
this fixture, with modest memory improvement; it does not certify other
workloads, release-profile speed or whole-service memory use.

The [paired measurement record](measurements/runtime-state-sharing-20261001.json)
retains all six sweeps, sample ranges, execution order and source/binary/log
fingerprints. Compilation and live placement are excluded from these timings.

Offline regression checks passed 317 Minecraft library/integration cases,
12 existing construction/partition/batching cases and the new incremental guard
case, 36 building/door/flight/construction integrations, nine public job cases
and ten public edit/presentation cases (385 distinct cases). The 26 runtime
transaction/checkpoint tests also passed after strengthening late-failure
sharing assertions. The guard regression compares incremental and full checks
until the first violation for protected Air creation/deletion, then demonstrates
that a subsequent restored endpoint cannot erase the earlier violation.

Minecraft all-target, translate library/scaling-example and native MCP all-target
Clippy passed with warnings denied; the three packages passed format checks.
