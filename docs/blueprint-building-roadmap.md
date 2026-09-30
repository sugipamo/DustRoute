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
