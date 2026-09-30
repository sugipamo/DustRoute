# Performance observation

This repository keeps performance measurements separate from optimization
changes. The purpose of the observation pass is to identify the dominant phase
for a given circuit and input size before changing algorithms or data
structures.

## Observation and differential-edit attribution (2026-09-30)

The bounded measurements and their conditions are retained in
[`measurements/observation-performance-20260930.json`](measurements/observation-performance-20260930.json).
They distinguish the running Voxrig tool, offline handler measurements, and
source-derived waits. This investigation did not write Minecraft blocks, apply
machine input or restart the running server/bot process.

The running native MCP was sampled through direct local HTTP five times after
compilation finished. `get_bot_status` had a median of **1.958 ms** and automatic
`get_world` discovery had a median of **25.598 ms** (24.081–27.662 ms). Discovery
loaded four redstone components by reading a 125-cell seed and one 4,096-cell
tile: **4,221 cells**, with 64 cells in the final reported cuboid. No call
reacquired the player. This small observation did not reproduce seconds of
delay. Connector-mediated calls took approximately 50 ms; those include an
additional transport layer and are not native processing timers.

The running tool schema does not yet support `get_world.region`. Sending that
field returned ordinary adaptive discovery, so those samples are **not** an
explicit-region benchmark. The source checkout supports it; its comparison
below runs the current handlers offline. The old native summary also counts
explicit air entries as non-air: use native block names or geometric volume,
not its `counts.non_air`, when interpreting these samples. The new scan counter
uses requested bounds and is independent of that summary defect.

The offline fixture exercises the real handlers with all cells, including air,
over a mock loopback bridge. Its waits acknowledge immediately. Its persistence
uses real file and directory sync on the normal repository state filesystem.
The six-block machine is identical in both discovery cases; only its position
relative to tile boundaries changes. Three samples ran with the unoptimized
debug test profile:

| case | scan calls / cells | measured evidence |
| --- | ---: | --- |
| adaptive, inside tile | 2 / 4,221 | handler median 134.990 ms |
| adaptive, crossing X/Y/Z tile boundaries | 9 / 32,893 | handler median 1,068.008 ms; about 7.8 times the requested cells |
| explicit 7×6×8 work region | 1 / 336 | handler median 11.706 ms |
| two-block revision | none | cold model proof 1,351.170 ms; warm proofs 5.995 and 6.114 ms |
| placement plan | 3 / 2,160 | one proof, 20 requested wait ticks |
| preview | 2 / 1,440 | 20 requested wait ticks |
| apply, mock world only | 6 / 4,320 | one proof, two writes, six checkpoints, 28 requested wait ticks |

The mock scan includes JSON construction/decoding and test readback validation.
Its durations must **not** be used as native Voxrig scan timings or as a claim
that native boundary discovery takes a second. The volume and call counts
exercise the same discovery and construction paths and do expose amplification.
The model and filesystem calls are real, but these small-scene/debug-profile
values do not establish limits for larger circuits or different storage.

Cold initialization was measured separately in a fresh test process. Piston
motion Laws took **637.901 ms**, all device programs **648.356 ms**, electrical
Laws **35.225 ms**, and spatial Laws **26.150 ms**. These public initializers
use `OnceLock`; subsequent calls were below 0.005 ms. This supports separating
first-use table compilation from the warm simulation, rather than treating
every proof as a 1.35-second operation.

For this two-write plan, the native path requests 20 stationary ticks at each of
planning, preview and application, eight construction settling ticks, and two
additional quiet ticks inside each native write batch. The current native
`wait_ticks` uses 50 ms of wall time per tick: **72 ticks = 3.6 seconds of waits**
over the complete workflow, before scan/proof/storage time. This is source and
requested-count accounting, **not** a measured live application duration or
evidence of server TPS. Preview/application retain independent freshness checks.

Application saved its intent, six stage checkpoints and final result, requiring
eight file syncs and eight directory syncs. Their combined duration was
69.114–74.047 ms in this fixture; JSON encoding took 2.543–3.044 ms. The
checkpoint duration includes its child storage phases and must not be added to
them again. Saving is therefore measurable, but much smaller than the declared
waits in this case. Durable records keep compact observation receipts, not all
the native received/reconstructed cells from each scan.

The supported conclusions are: small native observation is currently fast;
tile alignment and repeated full-region reads amplify observation work;
first-use Law/device compilation adds a separate startup cost; stationary and
settling waits dominate this small edit's declared native timing. Warm proof
repetition and persistence are secondary for the measured two-block edit.
The preceding user-visible operation has no per-phase timing archive, so its
exact elapsed time cannot be reconstructed from these results.

Optimization should first address bounded observation volume and redundant
reads while preserving freshness, and decide explicitly how to present or
schedule cold initialization. Reducing the independent stationary waits needs
a separate review of their safety conditions. This pass changes neither wait
durations, observation completeness, proof requirements nor storage durability.

## Content and generation sharing

The following implementation and offline comparison are recorded separately in
[`measurements/observation-sharing-20260930.json`](measurements/observation-sharing-20260930.json).
The architecture, identity boundaries and remaining work are described in
[`observation-content-sharing.md`](observation-content-sharing.md). The running
Minecraft/MCP processes were not restarted or changed for this comparison.

Voxrig now shares received and reconstructed arrays for an identical region in
the same received/reconstructed generation, advances moving carriers before
reuse, and creates fresh acquisition metadata on every call. DustRoute shares
canonical snapshot contents and converted native snapshots. Public circuit
IDs retain their independent owner and expiry checks. Proofs, stationary
intervals and durable saves keep their existing requirements.

Three offline samples per size compared 100 successive acquisitions of a
stationary loaded region in one generation, in the unoptimized debug profile:

| measured work, 100 acquisitions | previous median | shared median |
| --- | ---: | ---: |
| native received/reconstructed materialization, 720 cells | 120.585 ms | 0.757 ms |
| native received/reconstructed materialization, 4,096 cells | 700.414 ms | 3.989 ms |
| conversion plus shared contents/fresh records, 720 cells | 100.972 ms | 2.889 ms |
| conversion plus shared contents/fresh records, 4,096 cells | 681.856 ms | 16.448 ms |

Actual cell materialization and conversion went from 100 region volumes to one
volume: **99% fewer cells**, with 99 cache hits. The native reference retains
the previous separate received and reconstructed decoding only in a test
helper. The conversion reference performs the previous full coordinate
validation and snapshot creation; it excludes downstream copies from the
older owned observation flow. New conversion measurements include one full
conversion/hash admission and 100 fresh evidence records, and verify that all
records share storage. Received/local-frame boundaries are independently
tested, including concurrent requests, update/unload/reset, missing cells,
issues/recovery and movement without packets.

These figures describe repeated stationary acquisitions, not complete tool
speedups. The **first** shared conversion/admission took 2.635 ms for 720 cells
and 16.158 ms for 4,096 cells. The previous repeated conversion average was
about 1.010 and 6.819 ms per acquisition respectively, so first admission has
additional hash/interner cost. Global unrelated block/chunk changes invalidate
reuse; overlapping or different regions have separate entries. Each native
and conversion cache is bounded to 16 regions / 65,536 cells. Large valid
observations bypass retention. The hash uses a length-framed binary encoding
instead of regenerating snapshot JSON, reducing that initial cost.

The unchanged production handler fixture was also rerun. Its JSON bridge has
no native receive/reconstruction generations, so it still performs every
request and hashes fresh payloads. Median before/after times were
134.990/127.219 ms for interior discovery, 1,068.008/965.658 ms across tile
boundaries, and 11.706/11.117 ms for explicit bounds. Requested calls/volumes
remain 2/4,221, 9/32,893 and 1/336. These differences include run-to-run noise
and are not evidence of native cache reuse.

Mock plan/preview/apply medians were 87.984/81.972, 46.740/43.237 and
225.700/236.384 ms respectively. Application did **not** get uniformly faster.
Wait counts, proof counts and eight file/directory syncs remain unchanged;
the native two-write workflow still accounts for 3.6 seconds of explicit
waits. This offline comparison did not measure updated live-native tool
requests; the separate dummy-player measurements below now cover that path.

MCP's broad regression run passed 129 tests before final identity/hash
refinements; affected/new paths were then checked with targeted tests,
including five additional cases. Voxrig passed all 124 non-ignored unit tests,
including retained real-packet replay. Native/default target checks,
Clippy with warnings denied, and formatting checks validate the source
configuration. Model validation results are still computed rather than
cached; `ValidationKey` currently pins mixed-IR analysis identity.

## Live native read speed with a dummy player

[`measurements/observation-speed-live-20260930.json`](measurements/observation-speed-live-20260930.json)
records 55 successful `get_world` calls through an additional read-only native
MCP on loopback HTTP. Source commit `0e3ea75` was built with one Cargo worker
in the unoptimized debug profile. The existing Java 1.21.11 server and
`DustRouteBot` stayed running. A separate `dustroutetest` dummy looked down at
the existing flying-machine observer; `readspeedbot` observed it from nearby.
No blocks were placed/removed and no machine input was activated.

Each case has one first acquisition, nine repeats with 50 ms gaps, and a final
acquisition preceded by a one-second wait. The explicit bounds differ from
the automatic discovery tile, so each first call misses the region caches.
Durations include the HTTP request, MCP handler, response and JSON decoding;
block lists were disabled. All calls returned a complete scan and none
reacquired the player.

| scope / requested cells | first call | repeat median, 9 calls | repeat range | delayed call |
| --- | ---: | ---: | ---: | ---: |
| automatic discovery / 4,221 | 52.360 ms | 9.643 ms | 9.214–11.919 ms | 9.620 ms |
| explicit 7×6×8 / 336 | 5.989 ms | 3.847 ms | 3.443–4.406 ms | 3.843 ms |
| explicit 12×6×10 / 720 | 7.631 ms | 5.457 ms | 4.557–5.919 ms | 5.940 ms |
| explicit 16×16×16 / 4,096 | 31.606 ms | 11.487 ms | 11.345–13.964 ms | 12.066 ms |
| explicit 32×32×32 / 32,768 | 253.322 ms | 74.327 ms | 72.156–78.364 ms | 75.210 ms |

Every repeat and delayed native scan hit its generation cache. Repeats
materialized, converted and hashed **zero cells**. All 44 explicit captures
issued distinct observation and circuit IDs. Delayed explicit captures kept
the same content IDs while advancing capture time, local tick and receive
sequence. Automatic raw inspection does not expose those IDs, so its ID
comparison fields are null. This establishes reuse with independent
acquisition metadata on this stationary live scene; it does not establish
server-authoritative observation of hidden events.

The first automatic call also spent 21.305 ms in gaze observation. Its
repeats spent most of their handler time in discovery merging. On the large
explicit region, reused native observation/conversion took about 0.03 ms
combined in the first repeat, while the complete tool still took tens of
milliseconds. The handler still indexes/validates the literal snapshot and
builds the raw inventory over the complete region on each capture. The
current trace does not separate all those local steps, so it cannot attribute
the remaining duration to any one function.

Dummy and observer connection/ready durations were 531.461 and 395.353 ms.
After console positioning, the probe deliberately waited five seconds for
chunk streaming; that wait is excluded from the table and is not a measured
minimum loading requirement. HTTP initialization took its own recorded
duration; `get_bot_status` took 1.840 ms. The successful setup used lowercase
`readspeedbot`, matching the offline whitelist profile.

The larger regions contain 18 non-air blocks and nine redstone candidates.
These results apply to a loaded, stationary, mostly-air scene through local
HTTP. Dense circuits, moving machines, unloaded chunks, remote transport and
boundary-spanning automatic discovery require separate measurements. The
earlier older-process median of 25.598 ms and this 9.643 ms repeat median
indicate improved reuse, but different acquisition conditions prevent a
controlled before/after speedup claim. No live plan/preview/apply timing was
added, and construction waits retain their previous requirements.

The reusable harness is
[`../crates/dustroute-mcp/examples/observation_speed_probe.rs`](../crates/dustroute-mcp/examples/observation_speed_probe.rs).
Build it with `cargo build --offline --locked -j 1 -p dustroute-mcp --features
voxrig --example observation_speed_probe`. It requires `MC_PORT`,
`PROBE_HTTP_BIND` (loopback only), `PROBE_DUMMY`, `PROBE_OBSERVER`, and an
isolated `DUSTROUTE_STATE_DIR`; set `DUSTROUTE_PERFORMANCE_TRACE=1` for counters.
Use dedicated test-player names, allowlist them as required by the server,
position the creative dummy and nearby observer from the server console,
then enter a newline at its setup barrier. After `MEASUREMENT_READY`, use an
HTTP MCP client to call `get_world` with the arguments retained in the
measurement artifact. A final newline shuts down the temporary MCP and
disconnects its clients. This run removed its temporary whitelist entry and
confirmed that only the original `DustRouteBot` remained online.

The example passed its native build and Clippy with warnings denied. The
live assertions verified complete scopes, no reacquisition, cache counters,
unique IDs and advancing delayed receipts. Production read semantics were
not changed for this measurement.

## Opt-in request phase tracing

Set `DUSTROUTE_PERFORMANCE_TRACE=1` on the **next normally scheduled MCP start**
to emit one `dustroute.performance.v1` JSON line to stderr for each completed
tool call. It is disabled by default; this setting was not applied to the
running bot. MCP response schemas and the stdio protocol are unchanged. The
bounded counters are request-local and never combine concurrent requests.
Positions, block states, player identities and arguments are not logged.

`elapsed_ms` measures wall time within the tool router, before delivery to the
client. Each phase reports calls, inclusive elapsed time and applicable cells,
bytes, requested ticks or submitted commands. Durations include awaits, not
just CPU work. Missing phases mean no instrumented call ran. Phase coverage is
focused on common observation and the existing-world electrical edit workflow;
unreported work remains in total elapsed time.

- `scan` includes selected transport, reconstruction and validation.
  Native `native_observe` includes client-state lock acquisition, frame advance
  and cell materialization; `native_convert` covers the adapter conversion.
  Both are children of `scan`, not additional costs to add to it.
- `discovery_merge` and `discovery_neighbors` separate local flood-fill work.
- `static_validation`, `model_proof`, `model_queue` and `mutation_queue`
  distinguish computation from worker/operation lock waits. Electrical proof
  captures explicitly propagate into `spawn_blocking` and restore worker-local
  context after completion or unwind.
- `wait` records explicit bridge tick requests. `write` includes native batch
  submission and its internal quiet wait; these are separate from `wait`.
- `checkpoint` includes durable callback work; `store_read`, `store_encode`,
  `file_write`, `file_sync`, `file_rename` and `directory_sync` explain its
  persistence cost. `response_encode` measures MCP JSON text encoding.

The bounded offline probes are opt-in and never connect to Minecraft:

```shell
cargo test --offline --locked -j1 -p dustroute-mcp --features voxrig --lib \
  profile_observation_and_edit_phases -- --ignored --nocapture --test-threads=1
cargo test --offline --locked -j1 -p dustroute-mcp --features voxrig --lib \
  profile_law_initialization -- --ignored --nocapture --test-threads=1
```

Run the Law probe alone in a fresh process to retain a meaningful cold sample.
Both probes print `PERFORMANCE` followed by a JSON object. The workflow probe
checks scan volumes, model passes, requested waits, stage counts and successful
outcomes rather than imposing environment-dependent timing assertions.

## Reproducible commands

Run the commands from the repository root with a release build:

```shell
cargo bench -p dustroute-translate --bench connectivity_scaling
cargo bench -p dustroute-translate --bench analysis_scaling
cargo bench -p dustroute-translate --bench reverse_observation
```

The first two benches sweep synthetic wire-line sizes. `reverse_observation`
prints one JSON object per built-in circuit and measures compilation,
connectivity extraction, reverse analysis, liveness, and truth-table
enumeration separately. Its output is JSON Lines, so it can be saved and
compared without parsing human-readable logs:

```shell
cargo bench -p dustroute-translate --bench reverse_observation \
  > /tmp/dustroute-reverse-observation.jsonl
```

The same bench includes `mux_2_to_1_padded_538`, a three-input mux padded to
538 sparse world blocks with remote non-conductive supports. This isolates the
cost of cloning and scanning a 538-block observation without changing the mux
signal topology; its `graph_edges`, inferred terminals, and truth-table rows
remain comparable to the unpadded mux.

For a coarse process-level memory measurement, wrap a single run with the
platform's resource tool:

```shell
/usr/bin/time -v cargo bench -p dustroute-translate --bench reverse_observation
```

The benchmark intentionally does not run in CI and does not alter production
analysis behavior. The JSON fields are counts and wall-clock milliseconds;
`truth_table_ok=false` is still useful evidence because an unsupported or
incomplete circuit must not be treated as a successful verification.

## Reading the result

Compare phases using the same release profile and machine. In particular:

- `extract_ms` shows the fixed-neighborhood graph extraction cost.
- `analyze_ms` includes scene construction, directed components, interface
  inference, and diagnostics.
- `liveness_ms` isolates directed reachability and required-input checks.
- `truth_table_ms` measures the complete truth-table call from the benchmark
  side. `truth_table_rows_requested` is `2^inputs`, while
  `truth_table_rows` is the number of rows returned (always equal on success).
  `truth_table_settle_ticks_executed` reports the actual `advance_tick` calls;
  early settling can make it lower than
  `truth_table_rows_requested * settle_ticks`.
- `truth_table_solver_iterations` is the cumulative instantaneous fixed-point
  iterations charged by the simulator, and
  `truth_table_execution_elapsed_ms` is the same inference duration measured
  inside the translation crate. Comparing it with `truth_table_ms` exposes
  benchmark/serialization overhead.
- The phase fields `truth_table_world_clone_ms`,
  `truth_table_input_drive_ms`, `truth_table_wire_shape_update_ms`,
  `truth_table_simulator_init_ms`, `truth_table_settle_ms`, and
  `truth_table_output_read_ms` split that duration by operation. The remaining
  `truth_table_unattributed_ms` covers budget checks, state comparisons, row
  allocation, and other work not assigned to a phase. These are aggregate
  values across all rows, not per-row averages.
- `liveness_*`, `undriven_inputs`, and the diagnostic counts make it possible
  to distinguish a large healthy graph from a large graph with disconnected or
  unobservable branches.

MCP exhaustive inference is opt-in and bounded. Its preflight estimate is
`2^inputs * observed_blocks * (settle_ticks + 1)`. The default limits are 256
rows, 2,000,000 estimated work units, 1,000,000 cumulative instantaneous
solver iterations, and 120,000 ms elapsed time. Protocol hard limits are 16
inputs, 65,536 rows, 256 settle ticks, 100,000,000 work units, 10,000,000
solver iterations, and 300,000 ms. A large circuit is therefore still
structurally analyzed. Functional inference reports `budget_exceeded` before
simulation when the static estimate is too large, or while simulating when a
runtime iteration/time budget is exceeded. In every case the partially
enumerated rows are discarded and never exposed as a complete truth table.

Optimization work should start only after recording these values for the target
world. A change is not considered an improvement if it changes the observable
contract, the inferred terminal counts, or the truth-table result.

## Live Mineflayer bridge measurements

The visible-bot bridge keeps a bounded cumulative counter set and exposes it
through the `metrics` member of the `status` response (and therefore through
the MCP `get_bot_status` tool). The counters reset when the bridge process is
restarted. Request and response byte counts cover the serialized JSON payload,
excluding the JSON-lines delimiter.

- `requests_total`, `errors_total`, and `requests_by_method` show call volume
  and protocol failures without allowing arbitrary method names to grow the
  map.
- `request_bytes` and `response_bytes` show payload pressure on the local
  JSON-lines connection.
- `total_duration_micros` and `max_duration_micros` measure bridge-side time
  from request parsing through response serialization. Divide the total by
  `requests_total` for an approximate average; compare it with the Rust-side
  analysis timer to separate Mineflayer/bridge work from translation work.
- `scan_requests`, `scan_volume_blocks`, and `scan_non_air_blocks` quantify
  how much world scanning was requested and returned. A large volume with a
  small non-air count indicates that the selected box, rather than the circuit
  itself, is driving the scan cost.

Record these values before changing the transport. If bridge duration and
payloads are small while Rust analysis dominates, a custom Minecraft client
would not address the measured bottleneck. If repeated scans dominate, the
next bounded intervention is batching or connection reuse while retaining
Mineflayer as the world-facing client.

As a pre-instrumentation reference, a read-only sample against the local
1.21.11 test server (2026-09-02, one fresh TCP connection per request) measured
the following. The first status call includes connection/setup cost; the
repeated status values are the median and p95 of 25 calls.

| request | round-trip | response payload | result |
| --- | ---: | ---: | --- |
| `status` (first) | 3.307 ms | 229 B | connected |
| `status` (25-call median / p95) | 0.147 / 0.493 ms | 229 B | connected |
| `get_block` | 0.200 ms | 85 B | available |
| `scan_region` 16³ | 6.382 ms | 87 B | 0 non-air blocks |
| `scan_region` 32³ | 34.876 ms | 91 B | 0 non-air blocks |
| `scan_region` 48³ | 121.104 ms | 86.5 KiB | 1,037 non-air blocks |

This is a baseline rather than a stable benchmark: chunk-cache state,
coordinates, and server load affect it. The new cumulative bridge metrics
should be collected after the next normal bridge restart and used for the
decision about batching; no custom client is justified by this sample alone.
