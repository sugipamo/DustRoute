# Staged piston writes and reference-door conformance

The reference 3×3 door now reproduces its retained Java 1.21.11 operation in
one electrical world. The four applied lever inputs at offsets 0, 100, 200 and
300 produce matching **381 tick-end worlds, 524 ordered palette writes and
2,118 callback/carrier boundaries**. Both closed states fill the nine-cell
aperture and both open states restore the initial region. Exact suspended
checkpoints and normalized behavior-state resumption preserve the result.

The previous [v4 failure](evidence/reference-door-native-comparison-20260927.json)
remains recorded. User authorization to proceed with the movement refactor
resolved the earlier stop point; no door-specific timing adjustment is used.

## Write and entity lifecycle

The current world/exploration profile is **v6**, generic synchronous delivery
is **v3**, and electrical payload law **v3** adds observers. The thirteen law
roles and one world queue remain. Saved v1–v5 contexts are rejected instead of
reusing previous acceptance or automatically changing child requirements.

`PistonBlock.move` writes destinations from far to near with flags 324. Each
write drains shape callbacks before `addBlockEntity`. The head follows the
payloads; remaining source cells are cleared with flags 82 before the explicit
source notifications. Retracting bodies and the old pulled head use flags 276.
The implementation retains that ordering in checkpointed `MotionPlan` steps.

A bare moving block is an exact intermediate state, with no registered entity.
`CarrierEffect::Stage` records its captured future payload separately from the
queryable block; callbacks cannot read that payload as an installed entity.
Registration checks that the saved payload has not been substituted. A stage
cannot survive a completed synchronous root, admit outside input, or be imported
as a fresh world. Its block, continuation and pending work survive an exact
checkpoint. Atomic `WorldDelta::moves` is not used to misrepresent a displacement
that spans several deltas; the continuation retains its captured source blocks.

Movement support/destruction gates remain enforced. The existing narrow
back-face attachment exception applies only to a source body retracting in place.
Slime/honey branching, entities/collisions, inventories, general support-block
destruction and moving lamps are outside this implemented increment.

## Moving observers

Native observers use the existing observer callback law and common tick queue.
Removal with a pending powered tick emits its output-neighbor updates. Movement
preserves facing and captured state, while ticks remain at their original cells.
Arrival post-processing may schedule a new local pulse; a powered arrival without
an existing local queued tick resets during `onBlockAdded`. If that nested reset
changes the requested state, the enclosing write's shape/ordinary pass is skipped,
matching the target. Retired source ticks are checked against the current block.

Three isolated captures at x=46000, 47000 and 48000 cover:

- Unpowered push/pull, followed by a short pulse and interrupted extension.
- Movement of a powered observer with its off tick still at the old coordinate.
- Movement while the source observer's initial pulse is pending.

Applied offsets were respectively **0/12/24/25**, **0/2/14**, and **0/0/13**.
Each capture matches final readback, ordered palette writes and the retained
ordinary/event/carrier projection; restore checks reproduce the same future.
The regions were emptied, force-load tickets removed, and each server stopped
normally. Six-direction push/pull and pulse timing also have runtime tests.
These small captures supplement the door run, in which no observer itself moves.

## Evidence and reproduction

- [Complete-door comparison](evidence/reference-door-staged-final-20260927.json)
  records counts and hashes for the model, raw trace, inputs and client readback.
- [Retained callback worlds](../crates/dustroute-translate/tests/fixtures/reference-3x3-callbacks-a-v1.json)
  are derived exclusively from the server trace, with deduplicated world states.
- [Checks and source/capture hashes](evidence/staged-piston-motion-checks-20260927.json)
  record exact commands and results. The three observer fixtures retain their
  contiguous raw windows and can be replayed without launching Minecraft.

After building `compare_electrical_pistons`, the Python suites
`test_reference_door_comparison.py` and `test_piston_transients.py` reproduce the
comparison, including restoration, from retained evidence. The latter includes
17 earlier piston captures and three new observer captures. Corrupt input timing,
missing heartbeats and incomplete trace coverage are rejected.

Validation passed: **265 Rust tests** (221 Minecraft, 18 library, 13 translation,
13 MCP), **10 Python tests**, workspace/all-target Clippy with warnings denied,
formatting and whitespace checks. The full workspace test suite was not rerun.

The callback projection covers piston events, carrier tick/finish boundaries and
ordinary notifications to pistons, heads, moving pistons, dust and repeaters, with
the complete visible world at each boundary. All palette writes are additionally
compared in exact order. This does not claim every shape invocation, client
animation, collision or arbitrary 3×3 design has been proven. Public construction,
Blueprint adoption and MCP placement of the reference door require their own
construction and environment checks; the live reference used explicit `strict`
initialization, not a certified placement plan.

The subsequent [adoption audit](reference-door-adoption.md) leaves the candidate
unadopted: the public exploration exhausts its budget, and a separate directed
replay disproves the unrestricted input contract in the current model.
Its sequential construction failure has since been corrected by placing watched
blocks before observers; exact model construction and teardown now pass. Live
construction and public placement of this door remain unverified.

The later [short-input comparison](reference-door-short-input-comparison.md)
matches six measured Java pulses (1, 3, 12, 14, 20 and 100 ticks), including the
failed OFF aperture in three cases: 636 tick ends, 1,355 palette writes and
5,584 observed callback/carrier boundaries. This supports the rejection of
unrestricted operation; it does not grant adoption or live construction approval.

The user subsequently accepted completed-operation inputs for the ordinary door
and made interruption tolerance optional. [The agreed type specification](piston-door-type.md)
records that functional acceptance separately from the historical unrestricted
candidate's rejection. Its protocol-aware contract and verifier are now implemented;
see the [fresh ordinary-type adoption audit](reference-door-ordinary-adoption.md).
