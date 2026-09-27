# Expanded electrical piston comparison evidence

These are bounded observations on the existing isolated Java 1.21.11 server,
with redstone experiments disabled. They support the individual recorded
conditions, not every interaction admitted by the experimental runtime.

The subsequent [transient comparison](piston-transient-conformance.md) adds
server-observed carrier progress and callback-visible worlds for interruption,
completion boundaries and mixed interference. Its explicit post-world input
boundary is required for the measured same-world-time retract decision.

| Capture | Actual condition | Result |
| --- | --- | --- |
| mixed-electrical-20260926-b | Horizontal/up/down mechanisms, dust/repeater/conductor inputs, six applied ON/OFF writes | Complete settled region matched |
| c | Same mechanism, three inputs held ON | Complete extended settled region matched |
| d | Downward sticky piston, measured ON→OFF interval **4 game ticks** | Settled region matched; not interruption evidence |
| e | Pushed redstone block removes wire input and changes its arm geometry | Wire strength/shape and complete settled region matched |
| f | Downward sticky piston, measured ON→OFF interval **1 game tick** | Retraction and abandoned payload matched |
| g | Horizontal and downward mechanisms share a moved-block destination; ordered ON/ON/OFF/OFF | Complete settled region matched |
| h | Quasi-only power followed by actual notifying input changes | Final region and five before-next-input client samples matched; power-only changes retain the unnotified body state |
| j | All six facings, ordinary and sticky bodies, reversed ON order then forward OFF order; 24 applied inputs | Full settled region and 23 before-next-input samples matched |
| k | Dust climbs a solid step and drives an upward ordinary piston through a delay-3 repeater | Full settled region and two before-next-input samples matched |
| l | Side repeater holds a delay-4 repeater OFF and ON, then releases it while driving an upward sticky body | Full settled region and six before-next-input samples matched |

Every accepted input was paired with its actual server lever state change at
the same position and game tick. Client requested delays were not substituted
for applied times. Capture a failed this condition and remains classified as
`input_application_unverified`; it is not a model pass.
Capture i also remains unverified: its original 160-tick capture window retained
only 8 of 24 inputs. Trial j used a declared 800-tick window and retained all 24.
No successful model comparison was inferred from i's client-only input list.

Compact initial states, applied inputs, observed final states, hashes and
cleanup evidence are retained in
`crates/dustroute-translate/tests/fixtures/mixed-electrical-observed-{b,c,d,e,f,g,h,j,k,l}-v1.json`.
The replay tests use the recorded absolute coordinates and applied intervals.
Captures h, j, k and l additionally retain their client samples. Their exact
server sampling ticks are not asserted. The model must have settled before the
next applied input for a sample to be admitted by the prefix comparator.

Raw and normalized artifacts are under `.local/e2e-artifacts/` with the capture
names above. The normalizer reports **global sequence_contiguous=false**:
bounded capture evicts pre-roll records and suppresses records after its window.
That flag is preserved even when retained event sequence numbers have no gaps.
These results do not establish complete tick-internal callback conformance.
All successful primitive trials verified the test region empty after cleanup
and removed their force-load entries.

## Public construction trials

Final local checks passed: workspace formatting, Clippy across all workspace
targets with warnings denied, and the full workspace suite with **860 passed,
0 failed, 1 ignored**. The ignored test is the existing manual physical-NOT
scalability measurement. This includes electrical runtime/laws, legacy callback
replay, scheduler, saved execution contexts, electrical adoption/relocation/
construction, all ten retained observations and the MCP crate. The
[check record](evidence/piston-final-checks-20260926.json) retains commands and
log hashes; the full suite ran after the final Rust implementation changes.

The public MCP transport test covers persisted adoption, target rotation,
unadopted/unpreviewed rejection, unknown feature flags, changed baseline,
incomplete bounds, per-write checking and conditional teardown. Its bridge is
a mock and is not live physics evidence.

The first live MCP trials `mixed-assembly-mcp-20260926-a` and `-b` failed while starting
the Gradle/Java process, before the server-ready marker, bridge or actor. Java
threads remained in kernel waits (`synchronize_rcu_normal`, `expand_files`)
through the bounded startup/retry period. No construction was attempted by
these trials. Host diagnosis/recovery is outside the declared implementation
scope, so that work was stopped rather than altering the host or claiming a
passed end-to-end gate. The user subsequently reported host recovery and
authorized resuming the original task.

Trial [c](evidence/piston-assembly-mcp-20260926-c.json) passed at positive X with
R90 rotation: explicit proposal/adoption, 14 modeled/observed installation
stages, actual ON/OFF interaction, 14 verified teardown stages and empty-region
cleanup. It also rejected application without preview, a changed target
baseline and undo while the input/mechanism was changed.

Trial [d](evidence/piston-assembly-mcp-20260926-d.json) repeated the public path at
negative X with R270 rotation. It used the standard `behavior_context.piston`
request form, stopped the MCP process after adoption, and loaded the persisted
explicit context in a new process before planning/applying. Both build and
teardown verified all 14 stages. The two normal lever interactions were paired
with actual server state writes, independently of requested client timing.

Trial [e](evidence/piston-assembly-mcp-20260926-e.json) used a different candidate
with new immutable IDs: a raised circuit driving a downward body, plus constant
horizontal/upward peers. R180 placement, adoption/process restart, 14 verified
build stages, actual downward ON/OFF operation, 14 verified teardown stages and
both baseline/undo guards passed. The fixture is generated explicitly by
`tools/make_down_assembly_fixture.py`; no special-case production template was
added. All three successful public trials verified empty cleanup and removed
their force-load entries.

These public results are whole-region staged readback evidence, not complete
internal callback traces: bounded capture can omit construction records before
the first physical input. The retained compact files include operation outcomes,
applied input times, body observations, feature flags, cleanup and artifact
hashes. Those historical captures remain unchanged. Current requests use the
electrical context before storage; retired profile IDs are rejected without
implicit defaults. See [stabilization](stabilization-legacy-paths.md).
