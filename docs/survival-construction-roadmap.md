# Survival construction: next milestones

Updated 2026-10-02. This roadmap refines the agreed stages after fixed-reference
acceptance. It does not claim implementation of the remaining stages.

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

The acceptance fixture uses an independent observer. Its operational setup and
readiness must be explicit in the public workflow; automatic generation does not
remove that dependency or prove single-client observation suffices.

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
