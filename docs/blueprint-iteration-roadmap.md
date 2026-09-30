# Blueprint iteration and edit boundaries

Authorized order: structured review diagnostics, existing-design updates, then
explicit editable/protected space. Work starts from `codex/blueprint-building`
at `c7a33ec`. Current public generation/review/adoption/placement paths remain
the shared implementation. Each step is verified before proceeding to the next.

1. Preserve review failures as structured evidence: failed versus undetermined,
   occurrence and pinned revision, check kind, type/terminal, coordinate,
   expected/observed block, available input levels and committed runtime time.
   Preserve verifier reports/counterexamples directly rather than parsing detail
   strings. Return bounded findings with explicit omission counts. Use the same
   diagnostic view in generation, proposal review and refused adoption. No proof
   result or persisted evidence grants mutation/adoption authority.
2. Generate explicit immutable revision-update proposals from a selected adopted
   design and revised structured geometry. Preserve unchanged component pins;
   show structural/type/space changes and recheck the combined candidate. Retain
   the relation to a placed instance without silently replacing its saved design
   or updating the world. Site modification requires a new reviewed operation.
3. Declare editable and protected regions, validate complete known observations
   and reject writes or modeled effects outside the declared change scope.
   Reuse differential electrical modification and existing preview/readback
   gates. Do not infer ownership from identical materials or treat a snapshot
   as recovered server queue/history. Record exactly what protected-state checks
   establish. Persistent placed-instance and source/version boundaries must hold.

The initial work uses supported block mechanisms, current single-context input
contracts and bounded operations. Entities, survival hand/inventory building,
automatic redstone routing, new block physics and server-side atomic editing
are not prerequisites to silently implement. No live server/MCP restart or
world mutation is part of offline architecture verification. If an undeclared
prerequisite or major blocking contract is found, stop and report its concrete
case before proceeding.

Cargo jobs remain offline, locked, one compiler job at a time; test threads are
also serial. Known example-only warnings are reported separately from affected
library/test checks. Old records may be kept as history with fresh review;
diagnostic serialization never restores a runtime or a reusable proof.

Stage 1 is implemented: shared structured findings reach proposal review,
refused adoption and all building generators. Native runtime requirements retain
the actual failure time/input observations; behavior reports retain their
counterexamples. Offline checks cover failed versus undetermined, omission
counts, saving/reloading and public generation without catalog mutation.

Stage 2 is implemented through `generate_building_design_update`. The base must
match the supplied previous structured input, including retained air/type and
nested equipment requirements. Unchanged direct-child pins remain unchanged;
the candidate descends from the actual selected base. Public tests establish
unique-adoption gating, non-publishing generation, diff/review/adoption after
restart and preservation of the old Assembly. This does not upgrade a placed
instance. Assembly-record loading now resolves ancestry independently of input
order, including updates across different namespaces.

Stage 3 adds typed editable/protected regions to the existing captured-revision
and grounded-Assembly edit path. All other observed cells are protected;
forward/undo proofs inspect every committed model microstep. Public planning
retains preview/target/stationary/readback gates and persisted scope, without
inferring ownership or upgrading placed source identities. Fresh ungrounded
construction remains an empty-target operation. See
[world edit scope](world-edit-scope.md) for the supported bounds and live limits.

Offline validation completed with serial offline/locked Cargo jobs and serial
test threads:

- Model regressions cover building/design/door generation, repeated design
  updates, moving-world evidence/adoption, permission boundaries, transient
  protected pulses, shared electrical/adhesive/stair/flight construction, and
  immutable Blueprint records/connections/updates.
- Library Assembly/Blueprint regressions: 16 passed.
- Default MCP library suite: 139 passed, 2 opt-in performance tests ignored.
  After the later incremental-bundle adjustment, the affected public update
  workflow was rerun and passed.
- Voxrig-enabled virtual-design workflows: 4 passed; scoped workflows/store
  regression: 3 passed. These use transport fixtures, not a live Minecraft world.
- Clippy for affected libraries/tests passed with warnings denied, both default
  and Voxrig enabled. Formatting and whitespace checks passed.

No live placement, server restart or world changes were performed. No unplanned
block physics, entity or survival-building prerequisite was introduced. Model
protection does not establish server atomicity or recover hidden pending work.
