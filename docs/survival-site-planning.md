# Grounded survival construction and future geometry

## Implemented grounded design foundation

`GroundedBuildingDesignRequest` is a distinct authoring request around the
existing `BuildingDesignRequest`. Its bottom known plane is a declared existing
flat ground layer. Geometry above it, named permanent air and all other clearance
requirements retain their existing meanings. An old empty-site pass is never
reinterpreted. Components are refused in this first passive survival scope.

`generate_grounded_building_design` reuses the ordinary geometry expansion,
immutable sources, whole-Assembly review and update/adoption persistence. The
predecessor already includes the ground; the resulting exact pattern requires
both ground and structure. An explicit permanent-air declaration overlapping
ground is refused. Declared ground remains an assumption until live observation
matches it, and creates no edit permission.

Construction modeling uses `ElectricalModification::new_scoped`, with ground
outside editable space and explicitly protected. Only permanent structural
changes count toward materials. Forward physical callbacks must produce the
complete target and inverse callbacks must restore the original ground and air.
This uses the existing 64-change stage bound. It is **not** a player construction
schedule; the result separately reports player and live verification as false.
No public MCP entry or creative/command fallback is added here.

The tests declare a five-by-five cobblestone roof at y=6, four columns at y=0..5,
and exact interior air. Ground is 49 existing stone cells; structure is 49 new
cobblestone cells. At the normal ground standing height, the roof requires
elevated access. This is a test design and not yet the complete adopted live
acceptance fixture with inventory, temporary works and a durable job.
Tests also cover adoption across serialization/restart, missing ground, an
obstructed interior, conflicting permanent air, and unchanged ordinary authoring.

[Validation evidence](evidence/survival-grounded-authoring-20261002.json): 20
targeted tests passed; formatting and Clippy for translate/MCP libraries and
tests passed. The broader all-targets Clippy failed on unchanged unused helper
functions in the `flying_machine_assembly_fixture` example; its failure log is
retained and it is not claimed as a pass.

## Concern found before access planning (2026-10-02 UTC)

The route milestone predicts the **currently received** world. In Voxrig,
`Operations::preview_survival_path` locks the live `State`, obtains the current
standing context and calls `geometry`, which reads
`state.reconstruction.cell(&state.world, position)`. Terminal clearance likewise
uses that state. The 1.21.11 movement module exposes no captured-world/future-edit
prediction API. The placement preparation also checks current standing, current
geometry and actual held inventory.

This prevents the caller from proving, before placing an access block, the route
on that future block and the escape path after later removal. A static
world-modification proof does not establish player reach or retreat. Reading
today's geometry as if it were tomorrow's would reject valid stairs or accept
routes through blocks that construction will occupy. Building first and finding
the exit afterward does not meet the roadmap's reviewed access/cleanup plan.

Following the user's instruction to stop on concerns, the prerequisite below is
initially proposed without implementation. The user subsequently approved it;
implementation progress is recorded below. Completed grounded authoring remains
reviewable, independently of access planning.

## Concrete next prerequisite for approval

1. Extract the existing admitted dry-cube geometry access used by native motion,
   standing/terminal checks and placement hit validation behind a read-only
   bounded context. Keep the same movement equations and native shape admission;
   do not introduce a DustRoute physics copy.
2. Capture an immutable, complete local scene with native player parameters and
   provenance. An explicitly hypothetical scenario may apply bounded passive
   block edits and chain predicted standing positions. Missing geometry,
   unsupported cells, state conflicts and scope/budget violations refuse.
3. Give hypothetical predictions a distinct Rust type from live movement
   previews. They cannot be passed to execution or become received observations.
   No writes to the live reconstruction, own position, inventory or session
   bookkeeping are allowed. Real execution keeps fresh observation and current
   intent checks before each action.
4. Adapt bounded route selection to use that same predictor for proposed stages.
   Check placement support/visible hit, full body clearance, terminal rest and
   retreat for access placement and removal. Temporary inventory reservations
   must not count unobserved future item drops as resources.
5. Compare unchanged captured scenes with the existing live preview, exercise
   future stair placement/removal and blocked return cases, test isolation and
   stale-world refusal, then compare a declared small access sequence on the
   isolated non-OP server. The full roofed build remains the later acceptance.

This is a medium native/client boundary change involving movement, standing and
interaction geometry, followed by caller planning. It adds hypothetical planning
only for already admitted passive blocks. Entity physics, new shapes, survival
resource gathering and relaxing observed-action checks are outside this proposal.

## Approved implementation in progress

The user approved the prerequisite. Voxrig `d2db52a` now has a bounded immutable
scene capture, detached hypothetical branches, shared native movement/standing/
placement/removal geometry and a distinct hypothetical prediction type. Actual
operation admission still reads current live state. A compile-fail test prevents
direct use of hypothetical predictions as live movement previews. See the
[native contract](../vendor/voxrig/docs/survival-hypothetical-scenes.md).

DustRoute's `plan_hypothetical_route` now uses the same bounded search kernel as
live route selection. Its result has no live `start` method; advancing a scenario
requires the exact immutable branch used by the search. Edits invalidate that
branch identity. No hypothetical prediction is converted to `StandingContext`
or `SurvivalMovementPreview` to bypass the type boundary.

Native TCP fixtures pass captured/live motion and placement comparisons,
isolation/staleness checks, step-up/retreat/removal and foot-support refusal.
These do not constitute live temporary-access acceptance. Access-work selection,
placement/removal dependency scheduling, resource reservations, durable jobs and
the complete roofed construction/cleanup remain unfinished.

The [pinned live comparison](evidence/survival-scenario-live-20261002.json) passed
on the isolated non-OP vanilla 1.21.11 fixture. Captured-world route search took
191 ms including capture; the live search took 206 ms. Both selected the same
56-tick outbound frames. Adding hypothetical dirt at `[1,-60,1]` changed the
predicted frames and invalidated the original branch's route, while the live
capture remained valid. The actual unchanged-world detour, ordinary dirt
placement/material decrement, revalidated return and independent observations
all passed. The hypothetical obstruction was never written to Minecraft.

Validation: native 183 tests and two doc tests passed (five opt-in tests ignored);
root navigation three tests passed; native and root all-targets Clippy passed.
The server was stopped with exit 0. Source, receive traces and check logs are
pinned in the record. This comparison does **not** yet exercise building a real
temporary platform, climbing it and retiring/recovering mining during cleanup;
that declared access sequence is the next live milestone.

## Actual temporary access verified (2026-10-02 UTC)

The [declared finite access trial](survival-temporary-access-trial.md) passed on
an isolated non-OP vanilla 1.21.11 server. Three ordinary dirt placements consumed
3 -> 2 -> 1 -> empty. The bot climbed the platform, retreated to supported ground,
removed all three cubes and completed three explicit mining retirement/recovery
cycles. Each movement matched the earlier hypothetical frames before dispatch;
independent observations confirmed placements, endpoints, removals and the final
stone floor with exact air above it.

The [pinned evidence](evidence/survival-access-live-20261002.json) retains complete
received traces, connection changes, inventory outcomes and server/check logs.
Tested DustRoute source is `94c975c`; Voxrig is
`be55a64bc32da1ff0265ff0bdeb591f13d623137` (280 managed files). Native tests:
183 passed, five ignored; native and MCP all-targets Clippy passed; root formatting
passed. The opt-in live test passed in 47.31 seconds including the fixture gate.
The dedicated server saved and stopped with exit 0. Item drops were not credited
as resources and their collection is not part of this acceptance.

This verifies the declared access sequence, not automatic access-layout selection
or the complete adopted roofed build. Next are access/dependency/material planning,
durable survival execution and full construction/cleanup acceptance. The broad
survival construction goal remains active.
