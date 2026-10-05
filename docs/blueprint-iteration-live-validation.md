# Live validation of Blueprint iteration

The three changes in the [iteration roadmap](blueprint-iteration-roadmap.md)
passed their declared live workflow cases on Vanilla Java 1.21.11 through the
default public MCP profile and Voxrig. Product source is `58f400f`; the
[retained evidence](evidence/blueprint-iteration-live-20260930.json) records
binary/source fingerprints and the original responses. No product runtime fix
was needed. The new files provide an opt-in reproducible trial and evidence.

The fixture is deliberately small: three stone floor blocks, a glass window,
an exact air gap and a separate observer. Its update changes only the window to
tinted glass. This covers the iteration and edit contracts, rather than repeating
all earlier door, flight and large-building trials.

| Case | Result |
| --- | --- |
| Invalid structured geometry | Structured `missing_air_guard` error; named part and coordinate; live world remains empty |
| Equipment moves into retained structure | `verification_not_established`; failed requirements retain expected/actual blocks, input ON and committed model time; no live blocks written |
| Generate, import, propose, review and adopt | Passed; fresh target construction and exact readback pass; invocation without a preview is refused |
| Immutable design update | One-block diff; unchanged floor/air pins retained; wrong previous design refused; old instance still references the old Assembly and remains glass, new instance is tinted glass |
| Direct write outside declared editable space | Refused before execution |
| Protected glass externally changed to stone after preview | Apply refused; editable stone remains unchanged |
| Explicit scoped edit | Stone changes to smooth quartz; protected glass remains unchanged |
| Protected glass externally changed before undo | Undo refused; editable quartz remains unchanged |
| Restore guard and undo | Passed; original stone/glass arrangement restored |
| Observer protected while its watched cell changes | Model detects powered-state change at tick 2 and refuses the plan; actual watched cell remains air |
| Observer and watched cell explicitly editable | Apply and undo pass; live settled states match the model |
| Public removal and owned fixture cleanup | Both registered buildings removed through fresh removal plans; observer fixture removed explicitly; all 1,568 cells are air |
| New MCP OS process with the same saved state | Undone history retains its scope; removed instances reload; old executable undo plan is refused; live region remains empty |

The final run performed **57 public tool calls** and **13 checkpoints**. Each
checkpoint reads the complete 1,568-cell owned region through the native client.
Separate console predicates compare every reported cell, including air and
literal properties, against the running server: **20,384 cell comparisons** in
total. The server uses the existing accepted private-server configuration,
whitelisted test actors and a fresh MCP state directory. Existing machines and
the assistant player's ordinary work region are not edited by this trial.

There are four retained fixture failures preceding the final successful run.
The first lacked a gaze target; the second captured before remote targeting
updates arrived; the third attempted a debug-only helper in the default tool
profile; the fourth omitted the policy margin needed by observer validation.
All stopped before the affected operation wrote blocks. Already constructed
fixtures were then removed using saved instances and newly reviewed public
removal plans. The owned observer from the fourth trial was separately checked
and cleaned. Original failures and recovery fingerprints are preserved in the
manifest; none is relabeled as a successful full trial.

The final harness uses only the default public tools and waits for a bounded,
read-only `get_world` targeting barrier. Only the two explicit targeting errors
are retried during capture. Refused apply/undo calls are repeated only in the
declared test case after deliberately restoring the guard; apply also requires
a fresh preview. There is no blind write retry or injected process fault. Capture bounds
leave room for the existing surrounding-world validator; mutation policy remains
restricted to the declared test area. All trial servers stopped normally, and
the final run closed both MCP processes normally.

Native runtime readbacks remain **client reconstructed**. Independent console
checks establish agreement at these stable checkpoints, in batches that can
span server ticks. They do not make runtime scans server-confirmed, recover hidden
queues, prove atomic protection against players, or compare every intermediate
observer pulse. Failure diagnostics are model evidence, not observed server
failure traces. Design adoption does not automatically upgrade an old placement.

## Reproduction

Use only the stopped, loopback-only private Vanilla test server. The script checks
the known JAR fingerprint, existing EULA acceptance, creative/flight settings and
test actor whitelist/OP entries. It does not install a MOD, change permission
grants, or restart a running shared server. The declared area
`(1280,179,1200)..(1307,185,1207)` must be completely empty on first readback.

```sh
cargo build --offline --locked -j1 -p dustroute-mcp --bin dustroute-mcp
cargo test --offline --locked -j1 -p dustroute-mcp --test blueprint_iteration_live --no-run
python3 tools/verify_blueprint_iteration_live.py --run-id NEW_UNIQUE_ID --server-dir .local/minecraft-server-1.21.11 --java-executable .local/jdks/jdk-21.0.12.1+1/bin/java --allow-owned-fixture-writes
```

Raw request/response JSONL, console predicates, process logs and fresh saved
records remain under `.local/e2e-artifacts/NEW_UNIQUE_ID.*`. On a failed trial,
inspect those records and use the explicit `--recover-run-id FAILED_ID` option
with a new run ID. Recovery creates new public removal plans; it does not restore
or replay old executable plans. Owned external fixture cleanup requires its
literal current state to match the retained last write. A mismatch is left for
inspection rather than silently removed.

The runner requires an explicit Java executable and records its fingerprint.
Its JVM uses one active CPU and a 256–768 MiB heap, with inherited Java option
variables removed. Shutdown uses `stop`; a timeout is retained as a failure,
without forced termination. These limits concern the private trial, not product
resource limits. The current building probe also diagnoses one missing floor
block before and after explicit fixture restoration, checking that diagnosis
does not write or grant repair permission.

Live fixture clients are explicit ignored integration tests under
`crates/dustroute-mcp/tests/`, rather than example commands. They compile without
connecting; only `retain_fixture` with `--ignored --exact --nocapture --test-threads=1`
and the prepared private-server environment runs a trial. Preserve all operator
barriers and owned-world restrictions. The Blueprint runner selects the exact Cargo
test artifact and hashes it. Its MCP restart bookkeeping remains native in memory;
retained JSON recovery fixtures are decoded only at the test boundary, followed by
fresh public removal plans and exact observed external-state checks.


## Native JSON migration revalidation (2026-10-05)

The migrated ignored-test runner passed Blueprint iteration (53 tool calls,
13 checkpoints, 20,384 cell comparisons) and region jobs (42 tool calls,
11 checkpoints, 86,168 cell comparisons) on the existing private Vanilla server.
All declared negative cases refused as expected. Both trials restored their owned
regions to air, closed all MCP processes normally, and stopped the server normally.
Product source is `189cfc7`; no product runtime repair was needed.
The fixture launcher uses `--format=terse` so libtest does not prefix the first
operator barrier. Original evidence above is unchanged; this is a separate run.
See [the retained summary](evidence/json-boundary-live-20261005.json) for fingerprints,
raw paths and the finite stable-checkpoint scope.
