# Continuation prerequisite: independent player readiness

Recorded 2026-10-03 UTC on implementation `e6c8fbc`. Dependent implementation
and further live trials are stopped under the user's instruction to report newly
necessary prerequisites or correctness concerns. Stage 4A is **not complete**.

## Established results

The new public `checkpoint` action sealed the placement-boundary executor after
11 completed steps: eight permanent cobblestone cubes and one remaining owned
dirt cube. The independent full site matched, native history had no unresolved
operation, and the old executor's writer and source lease were released. The
test process then disconnected normally and exited successfully.

A separate OS process, with new builder and observer connections, read the
retained record as historical evidence without restoring native authority.
External dirt added at `[8,-60,8]` was diagnosed and refused without removal;
the test controller explicitly restored that fixture cell. Removing current
cobblestone inventory produced a structured shortage of 41; the controller
explicitly restored the supplied count. These were isolated fixture operations,
not production automatic repairs or material collection.

The new checked continuation preview preserved the eight complete targets,
retained the one old temporary cleanup obligation, and planned 104 actions for
41 remaining permanent cubes plus 17 newly consumed dirt cubes. The parent
checkpoint was durably claimed for a new job after the existing block, source,
pose and inventory admission checks.

## New blocking observation

That new job stopped at **zero completed steps**, on its first planned movement:

```
native_refused: exact player UUID is not observed
```

The complete independent block scene had passed admission. This does not imply
that the observer has a spawned entity for the builder. In Voxrig's native
movement admission, the observer must first know the exact builder profile and
register a watch on that UUID's **current spawned entity**. `watch_player_motion`
refused before controls were dispatched. The native refusal remains intact;
the new job retains an uncertain movement intent and needs inspection rather
than a blind retry. The parent claim remains consumed and links to that job.

Code boundaries:

- `vendor/voxrig/src/versions/java_1_21_11/client/operations/movement/control.rs`:
  `start_control_path` checks the profile/endpoint then registers the exact watch
  before creating the run and sending controls.
- `vendor/voxrig/src/versions/java_1_21_11/client/players/motion.rs:36`:
  `watch_player_motion` requires an observed entity for the exact UUID.
- `vendor/voxrig/src/versions/java_1_21_11/client/players.rs:151`:
  exposed observations need both a spawned entity and a received profile.
- `crates/dustroute-mcp/src/service/survival.rs`: current admission checks complete
  blocks/dimension/connection and executor inventory, but not this player readiness.

The reason that the fresh observer lacks the entity is **not yet established**.
No initial-login packet trace was captured in these trials. A fixed sleep,
visible profile name, complete chunks or matching block data cannot replace the
native entity/watch requirement. Teleporting either account to force a respawn
would obscure the ordinary restart workflow and is not an acceptance result.

## Concrete next scope proposed for approval

1. Capture bounded initial-connection traces and visible-player observations to
   distinguish missing server spawn data, loading order and lost native tracking.
   Use the same isolated non-OP server, without construction until readiness is
   established. Do not assert a Voxrig decoder defect before evidence supports it.
2. Keep physical identity, profile, endpoint, entity instance and world-generation
   interpretation in Voxrig. Prefer an existing checked read-only contract; if it
   cannot express the pair's readiness, add a typed read-only readiness query on
   the separate Voxrig source branch and update the immutable vendor pin only
   after verification. Do not derive offline UUIDs or reconstruct native watches
   from JSON in DustRoute.
3. DustRoute should check/wait for that native pair readiness within a declared
   observation budget **before consuming a continuation checkpoint or dispatching
   its first action**. An unavailable observer must return a useful readiness
   diagnostic and leave the old checkpoint available. Readiness is neither a
   mining-retirement receipt nor proof of a stopped player.
4. Correct the test's evidence writer: aggregated manifests, previews and full
   diagnoses exceed the production 16 MiB single-job save bound. Preserve the
   production bounds and write separate test artifacts or a test-only aggregate
   writer. Keep failures before final assertions.
5. Repeat placement-boundary restart through final cleanup/retreat and independent
   full verification. Then exercise a checkpoint requested during live mining,
   proving that normal outcome collection and retirement precede the idle record.
   Keep unresolved lost-native-action rejection as a separate negative case.

This prerequisite may involve native login/tracking in Voxrig and startup/admission
in DustRoute. Its library boundary and initial-login correctness need investigation
before committing to an implementation size. No native source change or weakened
watch/retirement check has been made.

## Retained verification and limitations

The focused `survival` suite passed 29 tests, with six separately opted-in live
tests ignored. Both feature configurations passed all-target clippy with warnings
denied; formatting passed. The subsequent test-only shortage-variant assertion
correction passed the two public offline tests. The full MCP regression suite was
not rerun here.

Both live test controllers stopped their servers normally (exit zero). The first
trial's continuation process failed on a test assertion that expected the wrong
error shape; production correctly returned `kind=insufficient_materials`. The
second trial reached the new checked plan but failed first movement readiness;
its evidence aggregation then also exceeded the production save helper bound.
Both test failures are preserved, and neither is recorded as completed acceptance.

The mining-requested checkpoint case and successful continuation completion
remain untested. No forced process crash, unresolved-mining process-loss trial or
Voxrig modification was performed. Exact sources, PIDs, exit codes, separate job
records, controller and hashed compressed artifacts are in
[investigation evidence](evidence/survival-continuation-investigation-20261003.json).

## Following test-infrastructure correction

On the `codex/survival-single-client` branch the continuation test's aggregate
evidence now has a separate bounded writer/reader with a 64 MiB test-only limit
and the same atomic durable replacement. The production 16 MiB job and journal
limits are unchanged. The final diagnosis is still saved before the completion
assertion. Offline regression cases exercise aggregation beyond the production
bound, unchanged production rejection, and the trial input bound. This resolves
item 4 for the fixed fixture's test infrastructure; it does not resolve initial
observer readiness or prove successful live continuation. No new server trial
was performed for this correction. See the current
[single-client plan](survival-single-client-plan.md).
