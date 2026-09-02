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
- `truth_table_ms` includes one world clone and settled simulation per input
  row; the number of rows is `2^inputs`.
- `liveness_*`, `undriven_inputs`, and the diagnostic counts make it possible
  to distinguish a large healthy graph from a large graph with disconnected or
  unobservable branches.

Optimization work should start only after recording these values for the target
world. A change is not considered an improvement if it changes the observable
contract, the inferred terminal counts, or the truth-table result.
