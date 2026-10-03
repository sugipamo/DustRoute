# Survival construction: next milestones

Updated 2026-10-03 UTC after single-builder acceptance. Bounded stages 2, 3 and
sealed idle-checkpoint stage 4A are accepted. Broader interrupted-operation recovery
and stage 5 remain separate milestones.

## Product objective and baseline

A human supplies a Blueprint, building materials and temporary blocks to the
bot's inventory, and identifies the construction area. The AI plans and performs
the work, reports progress and asks for intervention only when a concrete
obstacle cannot be handled within the authorized scope.

The [fixed reference](survival-roof-execution.md#fixed-reference-completed-2026-10-02-utc)
is complete: a 49-block roof/column building, 119 authored actions, all 19
temporary blocks removed, ground retreat and 3,120 independently checked cells.
This proves the shared executor on that sequence. It does not yet establish
automatic sequence generation or a public end-to-end survival building service.

The initial rollout stays within currently admitted dry passive cubes, bounded
observations, explicit edit/temporary/travel/retreat scopes and supplied inventory.
Current limits include 64 permanent changes, 512 proposed actions and native
256-edit/4,096-motion-tick scenario budgets. Reaching a limit is a diagnostic,
not a reason to silently raise it or claim that construction is impossible.

## Current stage status (2026-10-03 JST)

Stage 2 is complete for its declared bounded acceptance cases. A generated
115-action roof plan completed on the isolated non-OP server using project-local
Temurin 21, including all 18 temporary removals, retreat and 3,120 independently
checked cells. The JVM crash did not recur; the user requested no further
root-cause investigation in that case. Source and complete execution evidence
are in [implementation and evidence](survival-construction-generation.md).
Stage 3 is now complete for its declared bounded acceptance cases. Public MCP
authoring, adoption, planning, start and progress produced a completed 115-action
roof, independent verification and historical diagnosis from a new service.
Separate non-OP cases refused actual inventory shortage and a changed site before
construction, preserving all 3,120 observed cells. See the
[public workflow](survival-public-construction.md) and
[hashed acceptance evidence](evidence/survival-public-acceptance-20261003.json).
Stage 4A is now complete for sealed settled-boundary continuation under the
explicitly approved one-builder prediction contract. A normal 115-step public
roof and separate-process continuation from placement and mining-requested
checkpoints completed cleanup and retreat on fresh isolated non-OP worlds.
The service did not configure an observer; a separate comparison client verified
3,120 cells and final position in each case. Both continuation trials refused
foreign changes and material shortages and prevented duplicate checkpoint claims.
See [the current public contract](survival-public-construction.md) and
[hashed single-builder evidence](evidence/survival-single-builder-live-20261003.json).

The earlier continuation's missing observer entity remains a recorded historical
failure, not a diagnosed tracking issue. The new production path removes that
dependency. Motion remains explicitly predicted, with no physical position error
bound or server stop acknowledgement. Arbitrary crash recovery and stage 5's
broad disturbance campaign are not completed. The compiler stop below is also
historical: serial compilation succeeded after the user's memory increase;
the original crash cause remains undetermined.

## Ordered milestones

| Stage | Player-visible outcome | Completion evidence |
| --- | --- | --- |
| 2. Generate complete construction sequences | Given a bounded supported design and site, select placement order, access, temporary works, cleanup and retreat without authored action lists. | Original roof plus translated and genuinely different geometry pass the shared checker; a generated original-roof plan completes on the isolated non-OP server with independent final checks. |
| 3. Connect Blueprint adoption and public MCP | An AI can request planning, inspect required materials and footprint, start authorized construction, and inspect progress or a stopped job through the public tools. | Public-path integration test from an adopted revision to a completed building; stale revisions, changed sites, insufficient materials and missing permissions are reported before dependent actions. |
| 4. Continue from the observed stopped site | After interruption, identify completed work, differences, remaining temporary works and materials; produce a new checked plan for what remains. | Placement/mining interruption and process restart cases reobserve the world, settle old operation effects through existing retirement contracts and continue without duplicate uncertain actions or damage to foreign changes. |
| 5. Validate the whole workflow under failure | Expected disturbances result in a useful diagnosis, preserved evidence and an explicit next action, without falsely reporting completion. | End-to-end material shortage, obstruction, external edits, inventory changes, disconnect, cancellation and persistence failures are exercised with defined expected outcomes. |

Stage 4 depends on the planner from stage 2 and the job/diagnostic interface from
stage 3. Focused negative tests accompany every stage; stage 5 expands coverage
across the complete workflow rather than postponing basic failure handling.

## Stage 2 implementation sequence

1. **Search contract and diagnostics.** Define caller-owned budgets, counters and
   failures: invalid request, unsupported input, material shortage and no complete
   plan found within limits. Include relevant cells, remaining work and native
   refusal examples. Count rejected predictions against the search budget.
2. **Placement and access candidates.** Derive supports, visible faces and work
   positions from target geometry and captured cells. Use dependency ordering to
   prioritize candidates. Native movement, reach and collision checks decide
   whether a candidate can actually be performed; a topological order alone
   cannot establish a viable player construction sequence.
3. **Temporary works and escape.** Search placement and cleanup together, carrying
   exact scenario state, owned temporary blocks and cumulative consumption.
   Model the existing mandatory reconnect after removal. Account for the changed
   world when checking retreat; an earlier unchanged-world return route is not
   enough. No future drops count as supplied resources.
4. **Complete-plan checks and variants.** Submit candidates to the existing common
   checker. Cover the reference roof, translation, changed dimensions/layouts,
   forbidden temporary footprints, shortage and search exhaustion. Coordinate
   templates and partial plans do not satisfy this milestone.
5. **Generated-plan live acceptance.** Use the existing common executor and
   isolated fixture, retain the generated plan, journal, source pins, outcomes
   and independent site/cleanup/retreat checks. Keep authored acceptance as a
   regression reference, not a substitute for generated-plan success.

Automatic access and cleanup selection is the main algorithmic uncertainty.
Bounded search may legitimately fail on a supported design. Report the budget
and unresolved candidates; do not imply completeness or physical impossibility.

## Shared boundaries and measurements

- **Voxrig:** received observations, native geometry/physics, single-operation
  admission and retirement, hypothetical prediction and typed reconnect boundary.
- **DustRoute:** Blueprint requirements, permissions, planning/search, material
  and ownership accounting, job lifecycle, persistence and user diagnostics.
- **MCP:** expose those results and their evidence accurately. A serialized plan
  or an adopted geometric design never grants native execution authority.

At each live milestone separate planning, observation, movement, placement,
mining/reconnect and final verification time. Record preview counts, completed
actions, supplied consumption, remaining temporary blocks and intervention
causes. The previous 215.57-second roof test includes setup and preflight; it is
not a pure construction throughput measurement. Set performance targets after
generated-plan measurements, while preserving the existing correctness gates.

Acceptance fixtures use independent observers for comparison only. Normal
operation uses the builder's received world and the explicitly approved modeled
motion contract. This is not independent spatial corroboration; correction,
interruption and fresh-state admission guards remain. The public workflow states
the absence of a measured position error bound and server stop acknowledgement.

## Stop conditions and deferred scope

Stop dependent implementation and report when a new correctness prerequisite or
necessary out-of-scope work is discovered. Explain the failing evidence, affected
milestone, proposed change and expected scope. Do not replace a failed check with
a weaker success criterion. Routine candidate rejection or budget exhaustion is
an expected search outcome, not itself a reason to stop all development.

New block/shape admission, entities, resource gathering, chest supply, hazardous
terrain, survival placement of active circuits, large-building partitioning and
changing mining retirement policy are separate follow-up decisions. The earlier
general simulator's capabilities do not automatically establish survival player
placement support. Prioritize those expansions only after the bounded workflow
above is usable and its limitations have been measured.

## Stage 3 draft and compiler stop (2026-10-02)

Historical record; the 2026-10-03 JST memory-increase retry supersedes this stop.

The public-path goal is active. The working branch contains a draft, **not an
accepted or deployment-ready implementation**:

- `blueprint.action=generate_grounded_building_design` exposes the existing
  protected-ground authoring contract through the normal import/proposal/adoption
  flow. The survival planner checks the regenerated specification against the
  exact adopted Assembly, context and referenced definitions.
- `survival_construction` exposes `plan`, `start`, `get`, and `cancel`. Only an
  in-memory checked plan can start; persisted manifests/journals are diagnostic.
  Background execution uses the existing common executor. An independent native
  observer is explicit deployment configuration.
- A native bridge lease excludes other mutations and replaces the source
  connection after successful completion. Read-only planning cancellation returns
  the unchanged source; an executing/uncertain job retains exclusive ownership.
- A public MCP test for adoption/observer/mismatch gates and an opt-in full-roof
  public integration fixture are added but have **not run successfully**.

An earlier draft passed `cargo check --offline --locked -j 1 -p dustroute-mcp
--features voxrig --all-targets`. After adding tests and refining cancellation,
`cargo test --offline --locked -j 1 -p dustroute-mcp --features voxrig --lib
service::survival::tests::public_grounded -- --test-threads=1` failed inside
**rustc with SIGSEGV** during test compilation. The stack includes
`rustc_mir_build::check_unsafety::UnsafetyVisitor`. The compiler suggested
`RUST_MIN_STACK=16777216`; this is a troubleshooting hint, not an established
stack-overflow diagnosis. Cargo exited 101; no test body or Minecraft server ran.
[Preserved check/failure evidence](evidence/survival-public-progress-20261002.json)
separates the earlier check from the unverified current draft.

Per the prerequisite stop rule, compilation and dependent validation stopped.
No compiler environment change, toolchain replacement, cache deletion, host
operation or new reproduction was performed after the failure. No relationship
to the earlier JVM or PVE incidents has been established.

Proposed next scope: inspect the compiler failure and the added test/macro
expansion, then perform at most one serial build with the compiler-suggested
16-MiB thread-stack setting if appropriate. If that passes, continue the focused
public tests, regression/Clippy/feature checks and non-OP Temurin live acceptance.
If rustc still crashes, stop and report rather than repeatedly retrying or
changing the toolchain. This investigation/retry needs user agreement under the
existing stop rule. Source/site/material/permission refusal coverage, observer
freshness, lease cancellation/handback and persistence handling still require
review and validation before stage 3 can be declared complete.

## Stage 3 acceptance after memory retry (2026-10-03 JST)

The user increased guest memory to 8 GiB and authorized revalidation. The first
unchanged serial compilation completed in 95 seconds; rustc SIGSEGV did not
recur. Ordinary integration failures were then corrected: MCP root-object schema,
test adoption arguments, obsolete tool-count assertions and feature-gated tool
registration. No compiler stack override, toolchain replacement or cache deletion
was used. Retry success is not a diagnosis of the earlier compiler failure.

The final implementation also keeps progress polling off full journal reads,
checks persisted manifest identity/ownership, reports failed execution tasks,
persists expired-plan refusal and validates complete independent admission
observations. Source bridge ownership is returned only after read-only admission
refusal or verified completion; uncertain executors retain their lease.

Validation on source `3221cacf25d9b2430b53f76476a4b583f92c71b8`, with unchanged
Voxrig pin `2b6e7bfc94e6270054eac5c7b14a74d4657a411c`:

- Broad native-feature regression: 198 passed, three obsolete count assertions
  failed, nine explicitly ignored. After fixes, all three failures and the two
  public survival offline tests passed (five passed, one native test ignored).
  The broad suite was not repeated after the focused fixes.
- Clippy `--all-targets -- -D warnings` passed with and without `voxrig`.
- Public live roof: 49 permanent cobblestone blocks, 18 temporary placements and
  removals, 30 moves, 18 checked reconnects, no remaining temporary blocks and
  ground retreat. Independent final verification covered 3,120 cells.
- Public admission with 48 of the required 49 cobblestones returned
  `supplied_materials_missing`; a dirt block introduced after preview returned
  `snapshot_mismatch`. Both dispatched no construction, left the independently
  observed 3,120 cells unchanged, and refused replay of the consumed job.
- Lifecycle gates used forks of the already checked process-local preview to
  verify cancellation/expiry before dispatch. Reopened diagnosis used a new MCP
  service instance with a dead bridge, not an OS reboot or crash injection.
- All three isolated Temurin tests and servers exited normally. The normal test
  took 221.15 seconds including setup synchronization, authoring, planning,
  execution and diagnosis; controller startup/shutdown included took 246.48
  seconds. First-to-last completed-step polls span 167.25 seconds; this narrower
  interval is not total construction time.

No operating-system faults were injected or JVM failure investigation resumed.
Full logs, generated plan, journal, manifests, runtime identities and checksums
are retained in the [acceptance index](evidence/survival-public-acceptance-20261003.json).
Next is stage 4: freshly diagnose a stopped site and create a new checked
continuation plan without restoring or replaying old native authority. That
capability is not implied by successful historical diagnosis.
