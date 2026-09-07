# Piston door MCP v1

The operations are restricted to the verified Java 1.21.11 single-input
1x2 door. The preset can also be installed through `new_placement`. It does not change global piston placement capabilities or construct
a `ValidatedWorld`. Its private `VerifiedDoor` value proves only an exact
versioned door layout and settled state, sufficient for the dedicated lever
operation.

## Contract and flow

The compiled `crates/dustroute-mcp/src/piston_door_v1.json` retains complete
live block properties for open and closed states, captured in three matching
trials. The lower east-facing piston defines origin `(0,0,0)`, the upper
west-facing piston is `(4,1,0)`, and the lever is `(2,0,4)`. The complete frame,
wall, floor, roof, dust power and connection shapes, and piston/head metadata
must match. Translation is allowed; rotations or substitutions are rejected.
The scan must cover `(-3,-2,-4)..(7,3,6)` relative to that origin. Non-air blocks
outside the template, missing support blocks, mixed panel states, duplicate
coordinates, incomplete bounds, or another Minecraft version are rejected.

1. Select both region corners and call `show_region` to capture an immutable
   complete `circuit_id`.
2. Call `new_piston_door_operation` with that ID and target `open` or `closed`.
   The proposal is non-mutating, player/dimension-bound, and expires in five
   minutes. No plan is persisted across a server restart.
3. Call `show_operation` for the preview. Normal confirmation and mutation
   policy still apply; default read-only policy continues to deny execution.
4. `invoke_operation(confirm=true)` takes the shared mutation lock, checks
   version, dimension, owner, expiry and preview, approaches the lever, and
   rescans the full contract immediately before activation.
5. A plan is consumed before the activation request. If already at the target,
   return a verified no-op. Otherwise activate once, wait 30 ticks, rescan, and
   require the complete target state to match before reporting success.

## Failure behavior

A changed or unrecognized pre-state is rejected without lever activation.
After an attempted activation, a communication error or mismatching scan
returns `needs_inspection`, including available observed state and error data.
The consumed operation cannot be retried, even when the activation reply was
lost. There is no automatic reverse toggle, rollback, or destructive block
restoration. Re-observe and create a new operation if the door is still in a
recognized stable state; otherwise diagnose the mismatch first.

This is client-observed stable-state verification, not a server transaction.
The shared lock prevents this MCP instance from overlapping its own mutations,
but does not prevent a player or another client changing the world between a
scan and activation. Such concurrency and entities in the passage are not
certified by the block-only contract. Arbitrary piston circuits and generic
placement/repair/transition eligibility remain unchanged.

## Validation

Contract tests cover both states, translation, every missing required block,
version, guard, duplicate coordinates, wire power/shape and lever support
metadata. Fake-bridge operation tests cover preview/confirmation requirements,
read-only policy, expiry, changed baseline, post-state mismatch, uncertain
activation, verified no-op, and consumption/replay rejection. The real MCP
harness is `crates/dustroute-mcp/mineflayer/e2e/piston-door-mcp-live.js`; it uses
the real stdio tool router and Mineflayer bridge against the isolated test
world, not a direct simulation shortcut.


Real MCP validation passed on 2026-09-07: three independently rebuilt trials,
each closed/open/closed, plus missing-preview rejection, consumed-ID rejection,
a verified already-closed no-op, and rejection after a guard block was added
between preview and invoke. The lever stayed closed in that last negative
case. See the [live summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-door-mcp-summary.json).
All 56 MCP tests, four Node tests, and MCP all-target Clippy passed. The live
region was cleared; the test bridge and private server were stopped.

`undo_operation` deliberately rejects this operation family. Requesting the
opposite target requires a fresh snapshot and a new previewed operation.
The live harness constructs only its isolated test fixture; automatic
construction is not exposed by the MCP door operation.

## Reverse observation

Observation uses existing tools, with no door-specific read endpoint:

- `show_region` scans current selected blocks, returning `source: fresh_scan`,
  a new immutable `circuit_id`, and `mechanisms`.
- `test_circuit` and `convert_from_circuit` return `mechanisms` alongside their
  existing physical/logical interpretation. Passing `circuit_id` interprets that
  exact snapshot; it never silently refreshes it. Recapture to inspect changes.
- `get_world` remains the raw-block observation interface.

Each mechanism has `kind`, `recognition`, `state`, `contract`, and
`candidate_assessments`. Only an exact stable match is identified as
`piston_door`. Other observed piston structures are
`unidentified_piston_mechanism`, with null state/contract and diagnostic candidate
assessments; failure to match one contract does not make an arbitrary structure
an invalid door. Regions without piston blocks have an empty mechanism list.
This initial recognizer assesses the entire observed region, not independently
segmented mechanisms within a larger scene. It never authorizes mutation.

The v1 candidate observation schema reports:


| State | Evidence / meaning |
| --- | --- |
| `open`, `closed` | All block identities and properties exactly match the pinned stable contract. |
| `moving` | A `moving_piston` marker exists at a contract movement cell. Payload, progress and eventual completion are not inferred. |
| `indeterminate` | Cells or signal levels fit intermediate/mixed states, but the complete stable contract does not match. A mismatched pair of rows alone is not evidence of motion. |
| `configuration_mismatch` | An unexpected block, property, or missing required structure was observed. |
| `observation_incomplete` | Required coverage or properties are missing, coordinates are invalid/duplicated, or a fresh scan failed. |
| `unsupported_version` | The observed version differs from Java 1.21.11. |

Issues include absolute positions where available and a suggested next action.
Known configuration errors take precedence over incomplete metadata or movement
markers; the report does not claim the mechanism will settle. Sparse snapshots
interpret omitted cells inside a complete scan as air.

The report is read-only interpretation, not an operation capability. Proposals
still require exact stable verification; execution still rescans and verifies
immediately before activation. Pre/post-operation responses include observation
when a scan was obtained; post-operation scan failure reports incomplete
observation. An uncertain activation remains failed and its token consumed even
if the subsequent scan sees the requested state. Recovery requires a new scan,
proposal and preview.

Scope remains the existing translated 1x2 contract. Other layouts, block-entity payload inference and endurance optimization are
excluded. The fixed initial placement is described below. Physical MCP open/close validation above remains the execution evidence;
movement-marker classification is covered by synthetic snapshots, not a claim
that an in-flight live server marker was captured.

Reverse-observation validation (2026-09-07): all 57 MCP tests passed and
MCP/translate all-target Clippy passed. The real MCP harness repeated three
close/open/close trials: all nine post-operation observations and fresh scans
using each trial's original snapshot ID matched current blocks. A modified
guard reported `configuration_mismatch` and rejected activation. Cleanup was
verified. The tracked MCP summary includes the tested binary and contract hashes.

Endpoint consolidation: the dedicated door read tool was removed. The existing
observation/conversion endpoints now expose mechanism interpretation. Historical
fresh-scan evidence above predates this consolidation; subsequent validation is
recorded separately.

Consolidated endpoint validation (2026-09-07): 57 MCP tests and all-target MCP
Clippy passed. The real MCP harness passed three close/open/close trials;
`show_region` and `convert_from_circuit` identified the known door and matching
state after all nine activations. Unit coverage also confirms old circuit IDs
keep their old state, ordinary non-piston circuits have no piston mechanism,
unmatched structures remain unidentified, and the removed read endpoint is
absent from both tool profiles. Cleanup was verified.


## Initial placement through the common API

`new_placement({"circuit":"piston-door-1x2"})` creates an immutable, in-memory
placement plan. No new MCP endpoint is added. Use `show_operation`, then
`invoke_operation(confirm=true)`. The optional `max_blocks` limit applies;
`optimize=true` is rejected because it would alter the pinned configuration.

The lower piston origin is three blocks above the gaze target. This keeps the
entire guard above the targeted ground block; the plan returns both anchor and
origin plus absolute bounds. The exact guard volume must be empty, including
cells absent from the write list. There is no overwrite or collision acceptance.
Only translation of the captured open state is supported.

A private, non-deserializable `ValidatedDoorPlacement` verifies the fixed
versioned template, ordinary support/state checks, and complete empty baseline.
It does not produce `ValidatedWorld` or broaden general piston capabilities.
Forward execution rechecks owner, policy, version/dimension, five-minute expiry,
preview, and every guard cell under the shared mutation lock. It consumes the
attempt before writing. Blocks are ordered support-first, lever-last through
the existing command-based `write_blocks` bridge (bot operator rights required).
After 30 ticks, the full open-state contract must match. This is not survival
inventory placement or pathfinding-based block construction.

`undo_operation(confirm=true)` is supported for a successfully applied
placement only when a fresh scan still exactly matches the original open
configuration. If closed, open the door through a new ordinary door operation
first. Changed structure is rejected. Undo removes the known placed blocks in
reverse order and verifies the full guard is empty; the ground anchor is outside
that region. Applied records remain available for undo beyond proposal expiry,
but all records are lost on process restart.

A write/wait/verification failure marks the attempt `needs_inspection`. No
automatic retry or rollback is allowed, including uncertain undo. Partial builds
require inspection; this API does not provide arbitrary recovery edits. This
keeps uncertain writes from acquiring a second execution token.

Placement validation (2026-09-07): 59 MCP tests passed. The private Java server
passed three independent `new_placement` / preview / apply trials. Each newly
built door was recognized by existing observation/conversion tools, operated
close/open/close and reopened, then removed by `undo_operation`. Changed guard
and closed-state undo were rejected. The tracked
`piston-placement-mcp-summary.json` records results and tested binary/contract
hashes; cleanup was verified. General `ValidatedWorld` still rejects the same
piston template, as asserted by the proof test.
