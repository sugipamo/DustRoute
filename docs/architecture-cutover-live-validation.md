# Live operation checks after the architecture cutover

This follows the [offline cutover verification](architecture-cutover-validation.md).
Use the existing private Java 1.21.11 test server, fresh MCP state directories,
and new isolated coordinates. Both MCP and the JavaScript bridge use the current
mutation protocol; historical adoption results are not reused.

Status: **door and flight lifecycles plus six basic workflows passed; generic
large placement remains blocked before writing**. This is not an all-clear for
every current public workflow. Additional implementation stopped at the declared
scope boundary; the completed support-context repair is retained.

## Findings and repair

The first basic-circuit run passed five scenarios, then refused an upward dust
repair before submission. Its one-block scan margin included the neighboring
lower wire but excluded the stone below that wire. Whole-snapshot placement
validation consequently reported `InvalidSupport` for a physically supported
block. The proposal and preview had used a larger analysis region and passed.
The failed run is retained, including its original MCP responses.

`WorldEditor` now extends the observed region using the shared validator's
missing support/explicit wire-rise requirements. Each attempt rescans the entire
expanded region with server-confirmed readback. Region and volume policy apply
before every scan, and eight scans are the maximum. Remaining unknown evidence,
observed contradictions and policy limits still reject the edit before writing.

Validation decodes literal wire states. The analysis converter used previously
infers wire shapes and could erase a rise arm pointing outside a partial scan.
The explicit literal converter shares decoding with analysis but performs no
shape inference. Existing analysis behavior is unchanged. No new block behavior,
archive compatibility or mutation permission was added.

The next run passed the repaired upward-dust case, then correctly refused a
new-placement test whose temporary player footing was a barrier inside the
proposed region. Empty-region normal and optimized plans both passed an offline
public-workflow check; adding that barrier reproduced the refusal. The E2E
fixture now places its footing on the opposite side, outside the target. No
placement guard was relaxed to admit the test. This second failed run is also
retained as a fixture correction, separately from the support-context defect.

Six focused regressions cover the upward repair, missing actual support,
region/volume restrictions, removal of an existing wire's support, a rise at the
scan boundary, and the scan budget. The existing repair/placement workflows and
snapshot decoder are checked separately.

## Live evidence

The retained [manifest](evidence/architecture-cutover-live-20260929.json) identifies
source/binary hashes, logs, fresh state records and the original failed run.
Raw recordings and worlds stay under ignored local directories.

| Trial | Result | Checks |
| --- | --- | --- |
| Ordinary 3×3 door at `(410000,180,1000)`, R0 | Passed | Fresh proposal/adoption; 43 construction stages; eight read-only diagnoses; one missing quartz block; 85 reconstruction stages; two close/open cycles with whole-region and nine-cell aperture probes; 43 removal stages |
| Six-block flying machine at `(411000,180,1000)`, R90 | Passed | Fresh adoption; nine construction stages; ten-block flight; server-confirmed arrival region; diagnosis after restart; nine removal stages at the resulting layout |
| Six basic workflows, isolated run slot 1001 | Passed after repair | Normal diagnosis, reversed-repeater repair/undo, missing-wire repair/undo, transition/restore, API error contract, upward dust repair/undo |
| Guarded optimized placement, corrected fixture, slot 1002 | Blocked before writing | Planning and preview passed; invocation exceeded the readback confirmation volume |
| Physical wire optimization/undo | Not run | The runner stopped at placement; no result is inferred from previous offline or live evidence |

The door and flight trials also exercise restart after adoption, placement and
removal, refusal without preview, changed-world refusal and unavailable-client
observation refusal. Flight additionally rejects a transient removal plan after
restart. Both verify empty regions after public removal and cleanup, release
their force loads, and stop the test server normally.

These are finite operational trials. Door input probes compare client snapshots;
construction/removal and flight arrival retain server-confirmed region readbacks.
They do not establish hidden queue readiness, atomic protection from concurrent
players, exact agreement at every intermediate tick, or arbitrary-machine and
long-duration reliability. Basic transition testing checks its declared steady
state/restore contract and reports trace differences separately.

The door/flight runs precede the ordinary `WorldEditor` context repair. Their
Assembly execution path is unchanged. The manifest distinguishes their loaded
binary hashes from the repaired basic-circuit binary; it does not claim those
two lifecycles were repeated after that repair.

## Remaining blocker and next scope

The corrected half-adder trial proposes 451 changed blocks in a 51×5×29 region
(7,395 cells). Planning and preview fit the current readback limit. Invocation
validates the adjacent support/dependent region, 53×7×31 (11,501 cells), exceeding
the bridge's 8,880-cell confirmation limit. The existing one-block margin already
has this problem; the support-closure repair did not introduce it. Validation
fails before any command batch submission. Test setup/cleanup commands are
separate from application of the proposed circuit.

The current bridge deliberately confirms a supported region with one predicate
command. Splitting observations or simply increasing its cap cannot silently
preserve that contract. Next work should first align planning admission with the
actual validation region and bridge capability, then explicitly design and test
larger-region confirmation if large placements must be applied. This includes
coverage, inter-observation changes and command-size limits. No paging or weaker
readback fallback was implemented during this verification task. The server was
stopped normally after the failing trial, and both test ports were released.

## Reproduction

Build and prepare fresh inputs:

```sh
cargo build --offline --locked -j1 -p dustroute-mcp --bin dustroute-mcp -p dustroute-translate --example audit_reference_door_adoption --example flying_machine_assembly_fixture
target/debug/examples/audit_reference_door_adoption ordinary-live-prepare > .local/new-door.fixture.json
target/debug/examples/flying_machine_assembly_fixture > .local/new-flight.fixture.json
```

For the diagnosis/reconstruction door trial, add `"diagnosis_trial": true` and
`"reconstruction_trial": {"remove_position": {"x":0,"y":6,"z":-2}}` to the
newly generated door fixture. Run the existing lifecycle harness with a fresh
`--run-id`, a new isolated `--x`, `--persistence --capture-construction`, and R0
for this source-frame door fixture or R90 for the flight fixture:

```sh
python3 tools/observe_assembly_construction.py --run-id NEW_ID --x NEW_X --fixture .local/new-door.fixture.json --rotation r0 --persistence --capture-construction
```

For basic circuits, follow the [E2E setup](../crates/dustroute-mcp/mineflayer/e2e/README.md),
start the current bridge on the private server and use a fresh `DUSTROUTE_STATE_DIR`
and unused `DUSTROUTE_E2E_RUN_SLOT`. Run:

```sh
node crates/dustroute-mcp/mineflayer/e2e/runner.js normal_circuit reversed_directional_device repair_and_undo transition_run_and_restore api_error_contract vertical_dust_repair_and_undo guarded_optimized_placement focused_physical_wire_optimization
```
