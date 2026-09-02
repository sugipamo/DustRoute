# Pre-optimization performance observation

This repository keeps performance measurements separate from optimization
changes. The purpose of the observation pass is to identify the dominant phase
for a given circuit and input size before changing algorithms or data
structures.

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
