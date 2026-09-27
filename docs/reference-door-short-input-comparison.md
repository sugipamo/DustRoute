# Reference door short-input comparison

On 2026-09-27, six fresh Java **1.21.11** captures matched the unchanged v5
simulator. Short inputs leave quartz behind in the actual game too. The
unrestricted input contract remains unsuitable for this candidate; the observed
failures are not a simulator-only difference in these cases.

| Measured ON duration (game ticks) | Observed after holding OFF | Initial world restored | Tick-end worlds | Palette writes | Callback/carrier boundaries |
| --- | --- | --- | ---: | ---: | ---: |
| 1 | Nine quartz blocks fill the aperture | No | 82 | 130 | 541 |
| 3 | Quartz remains at relative `(0,6,0)` and `(0,7,0)` | No | 84 | 190 | 819 |
| 12 | Aperture clear | Yes | 93 | 267 | 1,089 |
| 14 | Quartz remains at relative `(0,6,0)` and `(0,7,0)` | No | 95 | 244 | 1,016 |
| 20 | Aperture clear | Yes | 101 | 262 | 1,060 |
| 100 | Aperture clear; normal-cycle control | Yes | 181 | 262 | 1,059 |
| **Total** | **All six match the model** | | **636** | **1,355** | **5,584** |

Requested and applied durations happened to agree in every capture. The
comparison nevertheless uses the measured lever writes and their paired
interaction packets, not client-side waiting durations. Continuous server-world
heartbeats establish that every interaction happened after a world tick.

## Method and coverage

Each case uses a separate initially empty region in the existing private
instrumented server, at x=52000 through 57000, y=180, z=1000. The imported 43
blocks retain exact materials and states. Strict saved-state placement and 100
client ticks of warmup establish the initial world, which is fully read back and
compared with the archive before applying ON then OFF. This initialization does
not validate ordinary construction.

The retained comparison window starts immediately before ON and continues to
80 server game ticks after OFF. An independent complete-region client readback
after 100 client ticks agrees with the reconstructed final world. The failure
rows describe this measured observation interval, not a proof of permanent
behavior for every possible future input.

All retained tick-end block identities/properties and ordered palette writes
match. The callback projection includes piston events, moving-carrier tick and
finish boundaries, and ordinary notifications to pistons, heads, moving pistons,
dust and repeaters, with their complete visible worlds. It does not claim every
shape callback, all observer/lamp callbacks, entity collision or client animation.
Both suspended checkpoint replay and normalized behavior-state replay reproduce
each model run's future.

The actor removed each test region, read it back as empty and removed its
force-load tickets. All six private-server runs stopped normally. No user
Blueprint catalog, adoption record or shared Minecraft server was changed.
No simulator Law, scheduler, input contract or physical runtime was modified.

## Implication and next boundary

The user subsequently accepted ordinary operation with the next command supplied
after the previous open/close completes. Interruption tolerance is optional;
the [ordinary-door type specification](piston-door-type.md) now records that
choice. The retained incomplete-reopening observations remain correct physical
results and are outside this ordinary type's operating assumption.

The new evidence supports the [adoption audit's rejection](reference-door-adoption.md)
of unrestricted interrupted input. The previous 33-case model probe remains a
model probe: only these six measured cases now have new live evidence. In
particular, 12 ticks succeeds while 14 ticks fails, so these samples must not be
converted into a claimed minimum safe interval. They also do not prove arbitrary
repeated sequences or the behavior explorer's separate root-boundary witness.

The selected next design task is to express that operating assumption in the
type system. The precise completion condition, validation, fresh adoption and
persistence/restart handling remain implementation work. Any managed operation
interface must define how out-of-protocol requests are handled; the simulator
continues to execute their physical effects. No circuit redesign or stronger
interruption-tolerance requirement is needed for the agreed function. No new
contract has been implemented or adopted. This stops before the substantial new
implementation requested to be reported to the user first.

Live sequential construction and public MCP placement/readback/undo remain
separate unverified steps for this door.

## Evidence and reproduction

[The evidence manifest](evidence/reference-door-short-live-20260927.json) records
applied input times, all comparison counts, cleanup, tool/source versions and
artifact hashes. Raw captures, complete client readbacks, server logs and model
traces remain under `.local/e2e-artifacts/reference-door-short-*-20260927.*`.

The retained regression fixture
`crates/dustroute-translate/tests/fixtures/reference-door-short-live-v1.json`
contains all six live-derived tick/write/callback references. It also retains
the first capture's raw events and minimal client record to test rejection of
missing heartbeats, mismatched input times and inconsistent readback. Offline
tests reproduce all comparisons without launching Minecraft; the existing
four-input normal-cycle comparison is also rerun.

```sh
cargo build --offline --locked -j 1 -p dustroute-translate --example compare_electrical_pistons
python3 -m unittest discover -s tools -p 'test_reference_door*.py'
```

For a fresh capture, use a new artifact prefix and a verified empty coordinate.
Create a fixture from `reference-3x3-bobiloosky-v1.json`, keeping only its first
ON/OFF pair and setting the second step's `wait_ticks`. Then run sequentially:

```sh
python3 tools/observe_reference_door.py --fixture /absolute/pulse.fixture.json --run-id new-pulse-id --x 58000 --interrupted
python3 tools/compare_reference_door_interruption.py --prefix .local/e2e-artifacts/new-pulse-id
```

The explicit `--interrupted` mode records a negative circuit outcome without
weakening trace integrity checks or the original four-input capture mode. A
successful comparison certifies agreement for that captured window, not adoption
or arbitrary-input correctness.
