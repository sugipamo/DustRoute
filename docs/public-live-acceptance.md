# Public live acceptance within the supported scope

This is stage 5 of the [failure-handling roadmap](failure-handling-stability.md).
The opt-in trial uses the default public MCP profile, bundled Voxrig and the
existing private Vanilla Java 1.21.11 server. It exercises command construction,
not inventory-based survival construction. The separate
[continuous survival campaign](continuous-survival-validation.md) supplies the
recent bounded survival evidence.

## Declared cases

| Case | Required evidence |
| --- | --- |
| Small passive building | Generate, import, propose, review, adopt, preview, apply and exact readback |
| Invalid design or missing preview | Public refusal, no affected world writes |
| One missing floor block | Independent input confirmation, `missing` at its coordinate, no diagnosis writes or repair permission |
| Explicit fixture restoration | Reference match returns; before/after diagnosis layouts agree |
| Immutable building update | One window change; earlier placement retains its original source and material |
| Protected-cell drift | Apply and undo refuse before submission, with category, coordinate and reobservation advice |
| Observer watched-cell edit | Protected transient effect refuses in the model; admitted edit and undo match settled live states |
| Two-region lever/lamp circuit | Support layer executes first, intermediate lamp OFF, later region naturally lights the lamp |
| Region restart and cancellation | Freshly planned next/reverse stages; canceled preview cannot execute |
| No-write stage | Checkpoint verifies naturally produced state without extra writes; reverse work restores the reference |
| Cleanup and MCP restart | Entire owned region is air, processes stop normally, history loads without restoring executable plans |

## Diagnostic correction found by the trial

The initial building trial completed its lifecycle but showed that a guard-drift
refusal retained `verification_mismatch` and `world: not_attempted` without the
coordinate. The common Assembly snapshot comparison returned a string, and edit
preflight/batch execution wrapped that string into a fresh cause.

The comparison now returns `FailureCause` and callers preserve it. The existing
literal block/property equality, complete bounds check, preview, operation
consumption and submission conditions remain in force. A mismatch carries the
sorted differing positions through the existing bounded failure projection:
at most 64 coordinates, full count and an explicit truncation flag. No new
physics, native contract or persistence schema is introduced. Observation/history
reports that already expose a readable reason retain their public shape.

The original successful trial is retained separately from the subsequent
acceptance run; it is not relabeled as having passed the added coordinate checks.

## Validation and evidence

The final building and region runs at `1590f5c` passed: **98 public MCP calls**,
**13 expected refusals**, **29 checkpoints**, and **114,392 independently compared
cells**. All recorded MCP frames agree with their top-level `ok` field. Both
owned regions were restored to air; five MCP processes and both trial server
starts ended normally. No further product or fixture prerequisite was needed.

| Final run | Calls / expected refusals | Checkpoints / cells | Normal MCP exits |
| --- | --- | --- | --- |
| Building-b | 56 / 10 | 18 / 28,224 | 2 |
| Regions-a | 42 / 3 | 11 / 86,168 | 3 |

Run results and fingerprints are recorded in the
[evidence index](evidence/public-live-acceptance-20261005.md).
The fixture source is
[blueprint_iteration_live.rs](../crates/dustroute-mcp/tests/blueprint_iteration_live.rs).
Its runner records MCP requests/responses, `is_error`, timings, fixture inputs,
native checkpoints, independent console predicates and normal process exits.

The diagnostic correction passed 27 distinct offline tests with
`cargo test --offline --locked -j1 -p dustroute-mcp --lib <filter> -- --test-threads=1`:
`service::electrical_edit_tests` (9), `service::construction` (10 plus one opt-in
live test ignored), `service::assembly_placement` (6),
`flight_placement_and_operating_removal_require_the_entire_reviewed_region` (1),
and `ordinary_door_command_initialization_connects_to_full_readback_gates` (1).
These retain submission uncertainty, whole-context guards, restart history,
source ownership, partial-observation rejection and recorded report shapes.
Strict all-target MCP Clippy passed with default features and
`--no-default-features`. This is focused acceptance, not a full-workspace run.

The [reproduction instructions](blueprint-iteration-live-validation.md#reproduction)
require a stopped loopback server, explicit fixture-write permission, fresh run
ID and explicit Java executable. Add `--region-jobs` for the region circuit mode.
The JVM uses one active CPU and a 256–768 MiB heap; no forced shutdown or host
failure is part of this campaign.

## Limits

Console checks are independent comparisons of settled block states, in batches
that can span ticks. They do not turn runtime client reconstruction into an atomic
server observation, prove hidden queues empty, measure transient observer pulses
or exclude player edits between a check and a write. The observer refusal is model
evidence. Missing-floor restoration is an explicit owned-fixture input, not an
automatic repair or live reconstruction trial. Restart checks use normal MCP
shutdown, not injected crashes. An MCP successful diagnosis request can report
differences without asserting a fault, cause, ownership or functional failure.

No arbitrary building, active-circuit survival construction, new block behavior,
online authentication, entity interaction or automatic replay of uncertain native
work is admitted by these finite cases.
