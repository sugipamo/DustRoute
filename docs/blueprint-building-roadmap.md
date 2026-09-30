# Blueprint building support

Started 2026-09-30 on `codex/blueprint-building`, from develop `94efa71`.
The preceding observation and construction work was fast-forwarded to develop;
merged local and remote topic branches were removed before this branch was made.

The intended interaction is a request such as “build a small enclosure here”.
The AI prepares the geometry, checks it, authors immutable Blueprint records,
plans and executes within the specified site, verifies the result and diagnoses
interrupted work. Humans decide unresolved intent, changes outside the specified
scope and unsupported mechanisms, rather than managing individual writes.

## First milestone

Generate bounded, configurable enclosures with separate floor, wall and roof
Blueprints and an explicit walk-through opening. Reuse exact block-pattern
contracts, the current physical world/runtime, source adoption, target review,
model-reviewed batches, durable instance records and fresh reconstruction plans.
The generator is authoring only: its results are not world-write permission or
live evidence. All placement passes through the existing preview and mutation
policy. Historical or uncertain attempts never become executable on restart.

| Step | Result | Acceptance |
| --- | --- | --- |
| 1 | Typed building requests and immutable component definitions | Configurable dimensions, materials, opening and roof; exact declared clearance; unknown or oversized requests rejected |
| 2 | Structural contracts and shared construction review | Complete geometry and air obligations checked, including after rotation/relocation; ordered build/removal steps and bounded batches |
| 3 | Public generation and ordinary adoption/placement | No side effects from generation; saved immutable proposals revalidated after restart; empty-site and policy gates remain active |
| 4 | Interrupted-work diagnosis and new repair plans | Partial submission stays uncertain; fresh design comparison and reconstruction work after restart without resending an old operation |
| 5 | Documentation and regression evidence | Public end-to-end transport checks and independent model tests; live evidence reported separately if a dedicated trial is run |

This first milestone uses existing admitted stateless cube materials and the
existing 256-block custom-Assembly budget. Splitting means at most 32 commands
per reviewed batch, with persistent verified progress. It does not partition an
arbitrary large building into separately owned jobs or automatically resume an
uncertain attempt. Those require subsequent work.

### First milestone result — 2026-09-30

Steps 1–5 are complete at the model and offline public-MCP level. The public
`generate_building` action authors ordinary Blueprint records; import, saved
proposal review/adoption, target placement, diagnosis, fresh reconstruction and
removal use the existing shared paths. `BlueprintRecords` is now a single Rust
authoring/import type used by both building and flying-machine generation.

Input-free structural review audits initialization microsteps to quiescence.
It does not turn undeclared input branches or missing behavioral contracts into
a pass. Exact component materials and whole-region air obligations are retained
through rotation, relocation, storage and restart.

Evidence: nine building model tests, 22 existing model/adoption regressions,
three public building lifecycle tests and three public flying-machine authoring
regressions passed (37 distinct tests). The building lifecycle was checked in
default and Voxrig-enabled builds. Tests/builds were offline, locked and serial.
Formatting and Clippy with warnings denied passed for the affected libraries and
tests in both feature configurations.
See [usage, limits and evidence](blueprint-building.md) for details.

This milestone has no new live-world building comparison and has not deployed
the changed MCP to the running server. Reconstruction retains the existing
inventory-based conflict checks and explicit review of all affected blocks;
identical materials do not establish ownership. Terrain/circuit protection and
minimum-patch repairs are not implied by the completed milestone.

## Follow-up roadmap

### Caller-authored virtual designs — 2026-09-30

Add a typed `generate_building_design` entry for LLM-authored explicit geometry:
named parts, fill/shell/individual-cell shapes, local cutouts and permanent air
spaces. Use checked existing cube materials and the ordinary immutable Blueprint
parts, exact structural obligations, current physics and shared construction.
Conflicting claims report item/coordinate; identical material claims deduplicate
physical cells. Import/adoption remain separate from successful generation.

One optional pinned equipment Assembly is transformed and wrapped through the
same helper as door/enclosure composition. All retained child requirements are
freshly checked in the combined world. Motion space must be explicit and cannot
silently erase structure or weaken permanent-Air contracts. Public MCP requires
unique source adoption. Separate active-source joint proof, new materials/physics,
terrain preparation, automatic wiring and large job partitioning are outside
this step. If one becomes a prerequisite, stop and report before implementing it.

Acceptance: independent geometry/material/air checks, L-shaped and windowed-room
designs, meaningful conflict correction, source immutability and retained child
failures, nonzero transform anchors, fresh adoption after restart, and an offline
public generation→placement→readback→diagnosis→removal lifecycle with occupied
target rejection. No live-server changes or new live-world evidence are implied.
See [structured design contract](blueprint-building-design.md).

The entry is implemented at the model and offline public-MCP level. Six new
model tests and two new public lifecycle/adoption tests pass, alongside twelve
existing building/door model cases and four public building regressions (24
distinct tests). The two new public cases also pass with Voxrig enabled.
The public lifecycle covers error correction, no generation side effects,
restart/adoption, an occupied-target rejection, placement/readback, diagnosis
and removal. Clippy with warnings denied passes for affected libraries/tests in
both feature configurations. Cargo jobs are offline, locked and serial.
This evidence uses a transport stub, not a Minecraft physics oracle. No new live
trial has run and the changed MCP has not been deployed to the running server.

### Typed door composition — implemented 2026-09-30

Compose a bounded enclosure with an existing uniquely adopted `PistonDoor`
Assembly, preserving its immutable occurrences, bindings and required Laws.
The first supported component is a one-block-deep 3x3 mechanism mounted in the
north-wall frame. A declared source-frame motion region owns its cells; fixed
building patterns cover every other cell of the combined known rectangle.
Generation must close the complete operation graph while auditing fixed
structure at every committed microstep, then verify shared build and removal.
This introduces no independent building physics or isolated-child pass reuse.

Acceptance includes exported input/aperture aliases, source immutability,
restart/adoption, malformed or insufficient reservations rejected, and public
MCP placement/readback/diagnosis/removal in the offline transport fixture.
Existing live-door evidence is not evidence for the combined building. No live
trial, external controller routing, moving-time commands, terrain replacement,
new block physics or automatic clearance inference is included in this step.
If one of those becomes a prerequisite, stop and report before implementing it.

The initial model feasibility check passed for a 9x3x8 enclosure with 43 existing
door cells (168 non-air cells total). Its construction and teardown reuse the
ordinary electrical construction path. The implementation adds typed requests
and a new action on the existing public `test_circuit_change` tool.

Model and offline public-MCP acceptance checks passed: 39 model regressions and
four building lifecycle tests, 43 distinct tests. Combined generation,
source immutability, exported aliases, relocated review, restart/adoption,
insufficient reservations and retained child invariants are checked. The new
MCP case requires source adoption, freshly generates/reviews the building,
places all 168 cells through the common transport path, restarts, diagnoses and
removes it. Existing interruption/reconstruction tests also pass. This is not
live-building evidence and the running MCP/server has not been replaced.

1. Expand passive building materials through checked Rust constant declarations,
   with recording, construction and behavior capabilities kept explicit.
2. Add bounded composition of multiple structures and protection of existing
   circuits/terrain, based on complete observations and declared edit scope.
3. Introduce durable multi-part jobs, fresh replanning and intent-aware conflict
   reporting. Keep immutable completed designs separate from execution progress.
4. Extend special block mechanisms only as needed by actual designs.

Survival inventory building, entities, fluid simulation, falling-block execution,
block-entity contents and arbitrary terrain clearing are outside this milestone.
If another prerequisite should be implemented first, stop and report its scope
and reason before implementing it. Existing unrelated warnings are recorded,
not used to expand this task.
