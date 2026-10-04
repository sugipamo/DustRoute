# Four-block clock: block-effect ordering and conformance

On 2026-09-20, the fixed four-block feedback clock failed comparison against the
private Java 1.21.11 server under the original synchronous profile. The new
`dustroute.dust-single-torch-block-effects.v1` profile matches all retained
observations, including post-burnout silence and externally triggered recovery.
Its periodic check **fails** this candidate: fixing the model does not make the
four-block circuit a working autonomous clock. Its separate
[`FiniteBurst` requirement](finite-burst-behavior.md) passes: eight falling edges,
followed by OFF from tick 30. Restartability remains unverified.

The original profile and law Revisions remain unchanged. No type requirement,
observation fixture, hidden-state compression or automatic revision adoption was
changed to obtain agreement.

## Setup and observed result

The frozen server constructed these blocks in order at fresh coordinates:

| Relative position | Block |
| --- | --- |
| `(0, 0, 0)` | Stone supporting the wall torch and dust |
| `(1, 1, 0)` | Stone above the torch |
| `(0, 1, 0)` | Redstone dust |
| `(1, 0, 0)` | Initially lit wall torch facing east |

The surrounding loaded region from `(-2, -2, -2)` to `(3, 3, 2)` was checked
empty before placement. Construction did not advance game time. The autonomous
capture applies no subsequent external stimulus. Each explicit game tick is
followed by frozen server queries for torch lit/facing, dust power and all four
connection properties, and both support blocks. The dust remained a dot with
all connections `none`; comparison imports the observed shape without inference.

| Game tick | Server observation | Original synchronous profile | Block-effects profile |
| --- | --- | --- | --- |
| 0 | Torch lit, dust 15 | Same | Same |
| 2–28 | Alternates every two ticks | Same | Same |
| 30 | Eighth off edge, dust 0 | Same | Same |
| 31–189 | Off | Same | Same |
| 190 | Still off | Relights, dust 15 | Same as server |
| 190–640 | Remains off | Further bursts starting at 190, 380 and 570 | Same as server |

All 641 samples agree with the new profile. The original profile still has 48
differences, starting at tick 190; retaining this regression protects existing
pins. The original model reports a 30-tick prefix and 190-tick complete-state
cycle. That is a model result, **not a timing requirement of the periodic type**.
The type still requires only eventual nonconstant recurrence. A finite live
capture neither proves permanent silence nor infinite periodicity; it does
establish the original profile's failure to reproduce the observed trace. The
new model's closed constant-output state fails the periodic requirement.

## Bounded diagnosis

A separate 261-sample run applied a neutral neighbor notification at tick 220
by placing/removing adjacent glass within the same frozen tick. The torch
relit at tick 222 and burned out again at 252. The new profile agrees with all
261 samples when replaying that recorded stimulus. Autonomous comparison still
rejects this capture; `--recovery` explicitly selects diagnostic replay.

A third, independent 33-sample run inspected saved scheduled block ticks:

| Observation tick | Saved chunk age | Torch's saved pending callback |
| --- | --- | --- |
| 28 | 0 | Delay 2, priority 0 |
| 30 | 0 | Delay 2, priority 0 |
| 32 | 2 | Stale copy of tick 30; current queue unknown |

At the burnout boundary, the current saved queue contains a callback after two
ticks, not the original model's 160-tick recovery callback. The new profile
matches the fresh queue checkpoints and all 33 block samples. `save-all flush`
can retain an unchanged chunk's older serialization. The reader checks `LastUpdate`
against frozen game time and labels stale snapshots. Queue entries in the older
notification diagnostic lack this freshness check and are not treated as proof
of the current queue. No empty live queue was established by those stale reads.

Inspection of the pinned local runtime shows that the torch callback changes
the block with neighbor notifications before requesting burnout recovery.
The scheduler deduplicates pending callbacks for the same block position/type.
The original adapter instead completes the local law, including its recovery
request, before resolving dust and notifying torches.

These facts support this explanation: a short callback created by feedback
during the block change occupies the pending slot before the recovery request.
The live call sequence was not instrumented, so this remains an inference from
the observed queue, traces and runtime control flow. The evidence does not
justify a clock-specific exception or deleting burnout history.

## Repair and supported scope

`ExecutableLaw::advance_with_register_effects` lets the adapter deliver a
synchronous effect after a changed register assignment, before the next law
instruction. The existing atomic `advance` API retains its behavior. Under the
new profile, a changed `lit` register resolves the fixed electrical network and
delivers a support-power notification before the handler resumes. A callback
requested there wins the existing first-request deduplication rule. Time passing,
an unchanged assignment or history expiry does not itself notify neighbors.

The profile accepts **at most one torch**. Its law interface has
`neighbor_update` and `scheduled_tick` handlers and one `scheduled_tick` pending
slot; neighbor handlers cannot themselves write `lit`. Multiple-torch ordering
and nested block changes are rejected as unsupported, not assigned an invented
order. This is a bounded adapter, not a complete Vanilla scheduler. Geometry
and dust propagation still use the existing fixed-point electrical model.

Direct callers select it through `from_fresh_assembly_with_profile`; existing
`from_fresh_assembly` callers retain the old profile. Existing MCP review and
proposal tools accept the new profile in `behavior_context`. The four-block
candidate with its periodic obligation fails fresh review/promotion/adoption
under this context, including
after saving and restarting. Selecting the old context still reproduces its
old model result; neither result authorizes changes to the live world.

The new profile also matches all 3,888 existing isolated torch observations.
The interpreter tests independently check notification-before-tail scheduling,
unchanged assignments and failure rollback. No law program changed, so existing
law Revisions remain immutable inputs to either explicitly selected profile.

## Retained evidence and reproduction

Fixtures under `crates/dustroute-translate/tests/fixtures/`:

- `periodic_clock_1_21_11.json`: autonomous observation, 641 samples.
- `periodic_clock_recovery_1_21_11.json`: external-notification diagnostic,
  261 samples; saved queue freshness unknown.
- `periodic_clock_queue_1_21_11.json`: queue freshness diagnostic, 33 samples.
- `periodic_clock_1_21_11.meta.json`: original capture reasons, source filenames,
  SHA-256 hashes and the historical synchronous-profile comparison summary.

The pinned official server JAR has SHA-1
`64bb6d763bed0a9f1d632ec347938594144943ed` and SHA-256
`f83b8e093865806f931c7e34aae41b177d4c076335263dd124c75d6d65dd1726`.
Raw console logs and exact historical probe versions remain in ignored
`.local/e2e-artifacts/`; their hashes are retained in the observation metadata.
No third-party runtime bytecode or world files are copied into these fixtures.
All three runs removed their blocks and force-load tickets, stopped the server
and restored its properties.

Offline comparison uses the typed Rust API in
`dustroute_translate::periodic_clock_observation`. `compare(&ClockCapture)`
defaults to the repaired profile, `compare_with_profile` retains the historical
profile comparison, and `compare_recovery` checks the diagnostic stimulus. The
JSON command-line example has been retired; independent Minecraft capture files
are decoded only at the regression-test input boundary.

The typed `ClockComparison.model_proof` retains the periodic result (`failed`
for this circuit). For autonomous captures, `finite_burst_model_proof` reports
`passed`, with eight falling edges and OFF from step 30. Complete-state recurrence
starts at step 91 with period 1 (92 distinct states). Recovery comparison reports
no finite-burst proof (`None`); `restartability_verified` remains false.

```bash
cargo test --offline --locked -j1 -p dustroute-translate --test periodic_clock_observation -- --test-threads=1
```

The regression compares Rust results directly: exact agreement for the repaired
profile, the original 48-sample counterexample for the historical profile, and
refusal of incomplete or unsuitable captures. Additional fields in placement,
placement coordinates or stimuli are rejected, preserving the previous exact
scope gate. Finite sample agreement does not establish infinite live recurrence
or full world conformance. The returned hidden-state diagnostics cannot restore
an executable simulator state or authorize placement/adoption.

New captures require the existing stopped private server with its EULA already
accepted. Choose an unused coordinate and a new output filename for every run:

```bash
python3 tools/observe_periodic_clock.py \
  --server-dir .local/minecraft-server-1.21.11 \
  --output .local/e2e-artifacts/clock-new.json --x 23104 --ticks 640
```

For a separate notification diagnostic, add `--diagnose-recovery` and use at
least 252 ticks. For queue inspection without stimulus, use e.g. `--ticks 32
--queue-checkpoint 28 --queue-checkpoint 30 --queue-checkpoint 32`. The read-only
NBT helper uses the already installed Mineflayer `prismarine-nbt` dependency.
Capture tools refuse occupied/unloaded regions and existing output/log files.

## Next work

The baseline repair and separate finite-burst contract are complete for the
declared scope. Restart triggers and repeated-use guarantees need their own
requirement before implementation. To obtain a reusable
autonomous clock, first select and observe a physical candidate that actually
sustains a waveform; this four-block candidate no longer passes the new model.
Multiple-torch/device scheduling needs its own explicit execution model and
evidence before extending this profile. Rotated placements and output consumers
also need live comparison. State compression, block-count optimization and
broader architecture migration remain separate work.
