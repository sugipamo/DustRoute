# Public MCP feature guide

Start here for supported workflows, public tools, ID lifetimes and recovery.
[Setup](../crates/dustroute-mcp/SETUP.md) is for operators;
[the LLM guide](../crates/dustroute-mcp/README.md) describes tool selection;
[JSON contracts](mcp-api-v1.md) describe responses.

The current workflow is observation → hypothetical revision → validation →
placement proposal → preview → confirmed live change and verification. The
fixed 1×2 piston preset also supports construction, recognition, open/close and
removal. This is not unrestricted autonomous circuit design or fault recovery.
An offline Blueprint workflow also supports exact source/state records and
reviewed update proposals. Adopting a proposal saves new revisions locally;
it does not apply them to Minecraft.

Block reads use the selected backend's observation contract. Voxrig uses received
packets and supported client reconstruction; Mineflayer uses command-confirmed
server readback. Both require fresh complete observations for the applicable
live checks. Their evidence and clocks remain distinct.

## Observation backends

| Concern | Native Voxrig | Mineflayer bridge |
| --- | --- | --- |
| Selection | Build with `--features voxrig`; this build defaults to `voxrig`. Set `DUSTROUTE_BOT_BACKEND=voxrig` explicitly if desired. | Builds without that feature default to `mineflayer`. Set `DUSTROUTE_BOT_BACKEND=mineflayer` to use it in either build. |
| Setup | Pinned source is included in `vendor/voxrig`; no separate checkout or Node.js bridge. DustRoute currently accepts Java 1.21.11 and offline authentication. | Separate Node.js 22/npm bridge process. The documented and tested circuit environment is Java 1.21.11. |
| Block observation | Received packets plus supported client state reconstruction, without per-cell confirmation commands. | Client-derived candidates checked against the server's block predicates, including all properties and air. |
| Retained readback | `dustroute.client-readback.v1`, `kind=client_reconstructed`; connection, receive sequence, client frame, revisions and moving flag. | `dustroute.server-readback.v1`, `kind=server_confirmed`; request identity, bounds, checked cells and server game ticks. |
| Gaze | `targeting_geometry=block_outline`; audited static outlines include dust, levers, repeaters and comparators. | Selects cached blocks with a non-empty bounding box along the tracked player's gaze. It does not use native outline shapes and can skip non-collidable circuit parts. |
| Observation limits | Requires complete loaded cells and supported reconstruction. The scan policy defaults to 262,144 cells; tool-specific limits still apply. | The same scan policy applies, plus the command-confirmation cap of 8,880 cells, 192 commands per exchange and command-length limits. |
| Command permissions | Normal block observation needs no confirmation commands. Region previews, teleport approach, creative-mode setup and command construction still need the corresponding permissions. | Readback itself needs `execute`, `time`, `data` and `tellraw` permission, including in read-only mode; previews and writes need their corresponding permissions too. |

Native gaze skips fluids and entities. Moving geometry, missing chunks, unknown
player pose, incomplete reconstruction and unsupported context-dependent shapes
make it unavailable. Use an explicit region when gaze geometry is unavailable;
the region still needs complete observation and the relevant model checks.
Selection geometry coverage does not establish simulator support for a block.
Gaze is based on received player pose and client world state, not an interpolated
graphical camera frame.

Fresh native client evidence can support the shared placement, diagnosis and
recovery workflows after their own validation; it never becomes server-confirmed
evidence. Unsupported backend settings or operations fail explicitly, without
silently selecting another backend. A failed Mineflayer confirmation likewise
does not fall back to an unconfirmed client snapshot.

Client frames and waits are not server game ticks. Native recordings preserve
received block-state packets, not every reconstructed piston frame or hidden
scheduler event; a client-frame trace cannot establish a pulse width stated in
server ticks. Saved readbacks from either backend are historical records and
require fresh observation before a new world action. Neither matching samples
nor server-confirmed readback proves empty event queues or prevents later edits.

See [setup](../crates/dustroute-mcp/SETUP.md#native-rust-client-java-12111),
[native evidence and limits](voxrig-rollout.md),
[current targeting and recovery trials](native-client-usability.md), and
[Mineflayer server readback](server-readback.md).

## Choose a workflow

| Intent | Entry and continuation |
| --- | --- |
| Inspect the gaze target | `test_circuit`, then `convert_from_circuit` or `get_circuit_ir` using the returned ID |
| Inspect a selected area | `set_region` twice, then `show_region` to capture current blocks |
| Create or branch a hypothetical circuit | `test_circuit_change(circuit_id)` or `test_circuit_change(revision_id)`; read with `get_circuit_revision` |
| Read sources, types, classifications or placed state | `get_circuit_revision(blueprint.kind)` |
| Author/import Blueprint data or propose a child update | `test_circuit_change(blueprint.action)`; review with `show_operation`, explicitly adopt/reject with `invoke_operation(blueprint_decision)` |
| Reflect a revision | `new_placement(revision_id)`; requires retained base evidence and fresh live validation |
| Construct an adopted custom piston Assembly | `new_placement(assembly_revision_id, assembly_target)`; fresh electrical review and construction simulation at a completely observed empty target |
| Reflect an adopted grounded Assembly | `new_placement(assembly_revision_id)` without `assembly_target`; requires complete captured ancestry, fresh review and fresh live validation at the original location |
| Install a built-in circuit | `new_placement(circuit)` |
| Manage a placed custom piston Assembly after restart | `manage_assembly` lists/reads instances, observes/revalidates, diagnoses design differences even when repair is blocked, then plans conditional removal or reconstruction |
| Repair | `new_repair`; use `get_repair_context` to resolve competing explanations |
| Optimize | `new_optimization`, or `new_macro_optimization` for a returned compatible candidate |
| Operate an existing fixed door | Observe, then `new_piston_door_operation(circuit_id, target)` |
| Test a supported live transition | `new_transition_test`; review its restoration behavior |

Plans use `show_operation` → review/confirmation → `invoke_operation(confirm=true)`.
Use `get_operation` for results and `undo_operation` only for supported recovery.
Creating a plan does not write blocks. Default policy is read-only; observations
may still move the bot or render region previews.

## Default tools (22)

| API | Purpose |
| --- | --- |
| `get_bot_status` | Connection, configured player and policy |
| `get_world` | Raw bounded physical observation |
| `set_region` | Select first/second corners using gaze |
| `show_region` | Preview selection and capture fresh `circuit_id` plus mechanisms |
| `clear_region` | Clear selection, not world blocks |
| `test_circuit` | Compact diagnosis and local interpretation |
| `convert_from_circuit` | Physical/logical interpretation, capabilities and mechanisms |
| `get_circuit_ir` | IR summary and analysis-scoped node expansion |
| `test_circuit_change` | Save hypothetical edits, or import/capture Blueprint data, search for smaller typed candidates and create update proposals |
| `get_circuit_revision` | Read hypothetical revisions or the exact Blueprint/Assembly/type/classification catalog |
| `new_placement` | Plan built-in construction, a cumulative revision diff, an adopted grounded Assembly reflection, or custom electrical Assembly construction at `assembly_target` |
| `manage_assembly` | List/get durable custom piston instances, freshly observe/revalidate, diagnose differences or plan conditional removal/reconstruction |
| `new_repair` | Rank repair proposals |
| `get_repair_context` | Evidence and questions for ambiguous repair intent |
| `new_optimization` | Plan supported physical wire-path optimization |
| `new_macro_optimization` | Plan a compatible, verified macro candidate replacement |
| `new_piston_door_operation` | Plan `open`/`closed` for the exact existing 1×2 contract |
| `new_transition_test` | Plan a supported live transition scenario |
| `show_operation` | Review the concrete operation; Blueprint updates include a diff and fresh occurrence checks |
| `invoke_operation` | Confirm a world action, or explicitly adopt/reject a Blueprint proposal locally |
| `undo_operation` | Restore where the operation kind supports it |
| `get_operation` | Retrieve status/results, including persisted Blueprint proposal history |

## Additional debug tools (7)

`DUSTROUTE_MCP_TOOL_PROFILE=debug` exposes 29 tools in total.

| API | Purpose |
| --- | --- |
| `get_visible_player` | Inspect players tracked by the bot |
| `get_player_gaze` | Low-level gaze observation |
| `resolve_looked_at_circuit` | Connected-region discovery and selection candidate |
| `get_circuit_placement` | Detailed placement/undo plan |
| `new_component_removal_plan` | Explicit component-removal proposal |
| `start_selected_region_conversion` | Start asynchronous selected-region conversion |
| `stop_operation` | Stop supported asynchronous work; does not undo world changes |

Placement and repair execution use `invoke_operation` and `undo_operation`.
`get_piston_door_state` is not exposed: mechanism interpretation belongs to the
existing observation tools. `get_operation` is available in the default profile.

## IDs and retention

| ID or record | Meaning and lifetime |
| --- | --- |
| `circuit_id` | Immutable observed snapshot; in-memory, 15 minutes, maximum 64 records with earlier eviction possible |
| `revision_id` | Immutable hypothetical snapshot; scoped state store, default one-hour TTL, survives restart with the same scope |
| `parent_revision_ids` | Zero/one parent today; sibling children represent branches, not a merge |
| `assembly_revision_id` | Separate immutable composed-state identity; retained without TTL after explicit catalog import/capture/adoption. Placement needs unique adoption and fresh review: `assembly_target` selects custom electrical construction; otherwise complete captured ancestry and the original location are required |
| Blueprint Revision IDs | Pinned source definitions referenced by assembly occurrences; catalog records have no TTL and never establish live-world evidence |
| `base_observation_id` | Original observation reference; new revisions retain its snapshot independently |
| `analysis_id`, `node_id`, `component_id` | Scoped to the originating observation/analysis; do not mix IDs from other results |
| `operation_id` | A particular plan/execution record; lifetime depends on operation kind |
| `instance_id` | Custom electrical Assembly placement attempt; durable owner/source/target/expected-state/progress record without TTL, separate from executable plans |

Saved observations are not automatically refreshed. A revision is not live
world evidence or direct execution permission. `new_placement(revision_id)`
creates a separately checked operation.

`DUSTROUTE_STATE_DIR` selects saved-state storage; `DUSTROUTE_PLAN_TTL_SECONDS`
controls its default one-hour retention. Revision reads do not extend expiry.
A child keeps its own snapshot even if a parent expires. This is working
storage, not a permanent revision archive. The separate Blueprint catalog,
its proposal histories and custom Assembly instance registry have no TTL;
configure a durable state directory because
the default is under the OS temporary directory. See [Blueprint storage and
limits](blueprint-mcp.md#persistence-and-live-world-boundary).

## Execution and recovery

| Operation kind | Retention and recovery |
| --- | --- |
| General built-in placement | In-memory, no dedicated five-minute expiry; undo checks and restores captured blocks |
| Revision placement | In-memory, five-minute pre-apply expiry; exact region/context checks before apply and undo; write attempts are consumed |
| Stationary electrical revision edit | Five-minute process-local plan; durable write intents, verified stages and operation history; after restart reobserve and replan |
| Fixed 1×2 construction | In-memory, five-minute pre-apply expiry; removal requires the exact original open layout |
| Custom electrical Assembly | Five-minute process-local construction/removal/reconstruction plans; durable instance records and stage progress; after restart use `manage_assembly` to reobserve and replan |
| Fixed 1×2 activation | In-memory, five minutes, single-use; no undo; reobserve and create a new target-state plan |
| Repair/optimization | Scoped disk state only; default one-hour retention since the last save, equally before/after restart. Expired or deleted plans cannot be previewed, applied or undone; unreadable state returns an error |
| Transition test | In-memory; inspect its restoration checks and actual result |
| Blueprint update | Persisted without TTL; explicit adopt/reject, fresh review on adoption, no history rewrite or undo; never writes Minecraft |

Most operation/undo records are lost on MCP restart, independently of persisted
revisions. After uncertain writes or failed verification, reobserve first.
Some errors return only `ok: false` and a message; absence of `needs_inspection`
does not prove nothing was written. Revision placement and fixed piston actions
do not automatically retry or roll back consumed attempts. Do not assume other
operation kinds have identical recovery guarantees.

Repair/optimization preview and successful apply/undo save the plan again and
renew its retention; ordinary reads do not. The TTL governs admission when the
plan is loaded, not cancellation of an action already in progress. Once a plan
has expired, reobserve before preparing a new action; there is no in-memory
fallback for its undo data.

## Supported scope

- Built-ins: half-adder, half-subtractor, MUX, decoder, full-adder and fixed
  `piston-door-1x2`. The door requires an empty guarded site; its origin is three
  blocks above the gaze target. Only translation is supported.
- Mechanism recognition identifies the exact known 1×2 layout. Other piston
  structures remain unidentified. Arbitrary multi-mechanism segmentation is absent.
- Revisions support addition, deletion and full property replacement: 64 edits,
  4096 block records/result blocks, 4 MiB per saved record, 1–256 simulation ticks,
  and no expansion beyond original observation bounds.
- Invalid drafts remain editable. Structural checks and initial-state simulation
  do not prove all-input behavior or exhaustive Java property validity.
- Revision placement checks the base and one-block context, shared placement
  legality and lossless export. Supported stationary piston/crop edits use the
  common callback runtime for each forward/undo stage and complete readback;
  they do not certify distant circuit effects or a flying/harvesting type.
  See [existing-machine modification](existing-machine-modification.md).
- Existing wire optimization is limited to a non-branching dust path with fixed
  endpoints; macro replacement requires a verified compatible candidate.
- Blueprint `optimize` searches the supplied Assembly or an explicit component body under one explicit
  behavioral type, with movable ports and replaceable internal interpretations.
  It counts every actual block, including support, wiring and input controls,
  once per position. Bounded search and supplied alternatives can produce
  verified smaller candidates; they do not prove global minimality or adopt
  parent changes. See [Blueprint block reduction](blueprint-block-reduction.md).
- Blueprint updates accept explicit candidate definitions and state. Parent,
  descendant and shared-occurrence checks must all pass for adoption; their
  default scope is initial placement and connections. Explicit model context
  additionally checks declared `Periodic`, `FiniteBurst`, `RepeatedSettling`, `PistonDoor` or `SingleOperation` obligations,
  without a numerical-timing, restartability or live-world guarantee. Imported
  drafts remain unverified. Full [Blueprint workflow](blueprint-mcp.md).
- The [ordinary 3×3 door type](piston-door-type.md) verifies repeated open/close
  commands after modeled completion. The reference door passes fresh adoption,
  including after restart. This adds no live readiness sensor and does not extend
  the fixed 1×2 `new_piston_door_operation` tool to arbitrary doors.
- Observer-containing custom Assemblies use v6 command preprocessing and explicit
  initialization. The [3×3 reference trial](reference-door-live-construction.md)
  passed 43 build / 43 removal stages, two ordinary cycles and restart guards.
  This finite trial does not provide a live operation-completion sensor.
- Explicit location bindings use the shared electrical piston context for new
  work, covering all six directions, dust, repeaters, conductor power and quasi
  connectivity. Review follows the declared type's input protocol (including
  changes during motion for `RepeatedSettling`) and checks retained child
  requirements at intermediate states. Adopted custom Assemblies can use
  `assembly_target` for fresh target review, ordered installation, whole-region
  readback and conditional undo. See [scope and live evidence](custom-piston-assembly-placement.md).
- The current context retains declared [slime/honey block adhesion](piston-adhesion.md):
  branches, push/pull, shared twelve-block limit and nonadhesion between the two
  materials. The current v19 context retains mature pumpkin/melon destruction
  and adds cane destruction/deferred support in declared soil and enclosed
  source water. Flowing water, growth and field construction are outside this
  scope. Other direct component destruction and entity
  carrying/bouncing/sliding remain outside scope. Earlier approvals require fresh review.
- Placement uses command writes, not survival inventory gathering/construction.
- Merge, entity handling, long-running endurance optimization and arbitrary
  fully autonomous design are outside the current scope.

## Detailed contracts and evidence

See the [documentation index](README.md), particularly
[revisions](circuit-revisions.md), [fixed piston operations](piston-door-mcp-v1.md),
[placement validation](world-validation-boundary.md), and
[diagnostic piston fixtures](piston-diagnostics.md).

The Blueprint MCP integration has Rust regression tests, formatting and
all-target Clippy checks. Rust transport tests use a mock bridge; separate
[custom Assembly trials](piston-electrical-live-evidence.md) exercise the public
MCP against isolated Java 1.21.11, including adoption/restart, target rotations,
changed-world rejection, operation and conditional teardown.
Fixed 1×2 construction/operation/removal and cumulative revision
apply/undo each have three-trial Java 1.21.11 evidence. Evidence files describe
the tested binary and scope; they are not a claim about every possible circuit.

- Blueprint `enumerate_layouts` lists and verifies torch/support patterns in component scope; see [component patterns](blueprint-component-patterns.md).

A finite flying machine can use `SingleOperation` for one launch/arrival, the
existing adopted-Assembly placement into a specified empty corridor, and shared
diagnosis. Arrival-state removal explicitly selects
`removal_reference="observed_inputs"`; no empty-site search or infinite-flight
tracking is implied. See [finite-flight lifecycle](flying-machine-lifecycle.md).

`test_circuit_change(blueprint.action="generate_flying_machine")` returns freshly checked, unadopted candidates from typed engine and body definitions, optional moving blocks, distance, reflection and rotation. Engines are `slime_relay` (default; four bodies) and `honey_direct` (compact or side blocks). Both use the shared physical runtime and adoption/placement checks. See [generation parameters and adoption workflow](flying-machine-generation.md).

Optional `harvest_targets` declare mature pumpkin/melon blocks that must disappear
during that flight while every moving part arrives. This is a single harvest pass,
not natural growth, repeated farming or item collection. See
[harvest scope and survival-construction boundary](flying-machine-practical-roadmap.md).

An explicitly imported engine can also be reviewed through the same Blueprint
workflow. The [observed-engine cane example](existing-machine-modification.md)
declares finite flight, cleared upper crops and retained roots in a fixed field;
it has model/MCP regression coverage, with no new live Java comparison yet.
Static diagnostics explicitly report that no functional flight/harvest test ran.
