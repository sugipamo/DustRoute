# Automatic survival construction: implementation progress

Stage 2 of the [approved roadmap](survival-construction-roadmap.md). This is an
bounded planner with complete hypothetical roof generation. Actual generated-roof
acceptance remains unfinished: dependent work stopped on a new JVM crash in the
isolated server. The public MCP survival workflow is not yet implemented.

## Shared checker and library boundary

`survival_construction::CheckedPrefix` is private and branchable. It validates
the initial capture and site once, extends one candidate action through the
same native geometry and ownership checks as authored sequences, and only
produces a `HypotheticalConstructionPlan` after exact final geometry, complete
temporary cleanup, supplied materials and safe retreat are checked.

The ordinary `preview_construction_sequence` uses this same implementation.
Speculative extensions consume a cloned predecessor; a refused branch cannot
modify its sibling. Successful output has the existing non-deserializable
hypothetical plan type, not native action authority. Actual execution still
requires fresh checks through `SurvivalExecutor`.

Voxrig remains pinned at `2b6e7bfc94e6270054eac5c7b14a74d4657a411c` throughout
these caller-planning changes. No native physics or operation contract was
weakened. Movement candidates use native scenario prediction and the shared
travel/terminal admission check. The existing round-trip route API is not used
as a proof of escape after future edits.

## Generator contract

`generation::generate_construction_plan` takes a captured scene, checked site,
proposed supplied-material budget, temporary material and explicit search limits.
Async clients use `generate_construction_plan_async`, which moves detached owned
data into a bounded CPU worker so native receive/keepalive tasks can continue.
Dropping its awaiter discards the result but does not abort a started read-only
worker. Neither entry point owns a live action handle.

It returns a complete hypothetical plan with counters, or structured invalid
input, unsupported target state, initial-scene refusal, material shortage or
bounded-search failure. The material budget is not an inventory receipt.

Limits cap candidate extensions (including refusals), expansions, retained
frontier and action count. A candidate extension can invoke several native
checks, so this counter is not described as a count of physics calls. Native
edit/tick limits remain unchanged. Intermediate successor sets are also bounded.
Failures identify budget/frontier exhaustion and retain bounded refusal/progress
samples and hypothetical remaining targets; they do not prove impossibility.

Reservations include unbuilt permanent targets. Temporary placement consumes
supplied material even when permanent work uses the same material. Removal does
not refund hypothetical drops. Only already owned temporary cells are removable.

## Current candidate policy

- Greedily place reachable permanent targets through native face and reach checks.
- Generate temporary placements inside the explicit allowed footprint. Evaluate
  columns together with native movement onto them; keep every ordinary placement
  and movement in the resulting sequence.
- Derive standing and overhanging work positions from observed/scenario surfaces.
  Sample bounded walk/jump controls and require native whole-body, support and
  terminal clearance. An overhang coordinate is a candidate, not permission.
- Retain ascent, descent and level alternatives; prefer work positions separated
  from the target and choose nearby unfinished work before comparing approaches.
- Prefer top-down temporary cleanup while retaining lower tiers until descent.
  Every removal still models the existing native reconnect initialization.
- Evaluate reachable permanent work after candidate moves. If access stops making
  useful progress, try a separately bounded cleanup subgoal before transferring
  to other work. Cleanup does not add temporary blocks. Its checks and expansions
  count against the global budgets; its second frontier is independently bounded.
- Apply bounded best-first selection. Geometric scores, greedy placement, limited
  position/control samples and non-retracing motion are incomplete search policy,
  not additional Minecraft physics or an exhaustive dependency analysis.

These policies may fail on a physically constructible design. They must not
return a partial branch as a completed plan or dispatch it to the executor.
No coordinate-specific roof operation list is used by the generator.

## Checks completed so far

The related Rust suite has 21 passing tests and four opt-in live fixtures.
MCP all-target Clippy passes with Voxrig enabled. Contract tests include reserving
permanent materials against temporary use, no drop refunds and invalid budgets.

On source `c67410a`, isolated vanilla captured-scene comparisons passed for:

| Design | Generated actions | Candidate checks |
| --- | ---: | ---: |
| Two-block column | 2 | 2 |
| Supported four-block beam | 7, including movement and retreat | 548 |
| Translated two-block column | 2 | 2 |

Each plan matched a fresh complete replay through the public sequence checker.
A refused native extension left the predecessor usable. Material shortage and
one-check exhaustion also returned their declared diagnostics. These were
read-only comparisons against actual received geometry: no generated building
actions were sent, and they do not substitute for full-roof live acceptance.

## Full generated roof preflight

On `c67410a`, the original roof passed complete hypothetical generation without
an authored operation list: **115 actions, 49 permanent placements, 18 temporary
placements, 30 moves and 18 removals**. Required supplied materials are 49
cobblestone and 18 dirt (no future drop credits). Two access cleanup phases
complete, no temporary blocks remain, and the final feet are approximately
`[0.541636, -60, -1.499938]` inside the ground retreat scope.

The search used 9,963 candidate checks, 50 expansions and a peak frontier of 16
under a 12,000-check limit. This remains an incomplete bounded heuristic, not a
guarantee for arbitrary roofs. The unchanged native checks admit all actions.
[Progress evidence](evidence/survival-generation-progress-20261002.json) includes
all prior failures and the complete preflight, with source pins and hashed logs.

## Live attempts and stop condition

[Live-attempt evidence](evidence/survival-generation-stop-20261002.json) preserves
these distinct outcomes rather than replacing them with the successful preflight:

- **Live A (`c67410a`):** both clients timed out during synchronous planning on
  the fixture's current-thread Tokio runtime. At step zero, an inventory swap
  returned an uncertain write; the executor recorded `needs_inspection` and did
  not replay it. No placement/mining/motion dispatch appears in native history.
- **Async correction (`da9bdd0`):** the generator's async entry point and fixture
  planning use `spawn_blocking`. Related tests: 21 passed, four opt-in tests
  ignored; all-target MCP Clippy passed. No Voxrig change was needed. Earlier
  long read-only preflights do not demonstrate connection liveness.
- **Live B:** fresh world startup exceeded the fixture's 60-second readiness
  timeout. The server subsequently became ready and stopped normally; no Rust
  test or building operations started. The fixture startup timeout alone was
  increased to 180 seconds for the next attempt.
- **Live C (`da9bdd0`):** the isolated Java server crashed with SIGSEGV on
  `C2 CompilerThread0`, in `PhaseChaitin::post_allocate_copy_removal()`. The JVM
  identifies itself as OpenJDK `21.0.12+8-1-26.04-Ubuntu`. Detached planning still
  produced the same 115-action result; fresh executor creation refused EOF.
  No executor journal was created and native history contains no construction
  dispatch. A bounded crash-summary excerpt is retained; the full report stays
  local because it includes environment data.

The JVM crash is an observed process failure, not a determination of its root
cause. All dependent live work stopped under the user's prerequisite stop rule.
No JDK replacement, JVM flag workaround or additional crash reproduction was
attempted. Long-running transport liveness after the async correction still
needs successful live validation.

## Remaining acceptance

First agree on a narrowly scoped environment investigation: identify a supported
server JDK/build and decide whether replacement or a diagnostic JVM configuration
is appropriate. Then use a fresh isolated fixture to execute the generated roof
through the unchanged common executor, retaining exact final geometry, all
owned-temporary cleanup, independent observation and ground retreat evidence.

Stage 2 remains incomplete until that actual acceptance succeeds. Blueprint/MCP
integration, reobserved continuation and whole-workflow failure tests remain
subsequent milestones; the previously completed authored roof is separate proof.
