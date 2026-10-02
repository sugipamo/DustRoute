# Automatic survival construction: implementation progress

Stage 2 of the [approved roadmap](survival-construction-roadmap.md). This is an
in-progress bounded planner, not completion of the generated-roof acceptance or
the public MCP survival workflow.

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

On source `24952f3`, isolated vanilla captured-scene comparisons passed for:

| Design | Generated actions | Candidate checks |
| --- | ---: | ---: |
| Two-block column | 2 | 2 |
| Supported four-block beam | 7, including movement and retreat | 538 |
| Translated two-block column | 2 | 2 |

Each plan matched a fresh complete replay through the public sequence checker.
A refused native extension left the predecessor usable. Material shortage and
one-check exhaustion also returned their declared diagnostics. These were
read-only comparisons against actual received geometry: no generated building
actions were sent, and they do not substitute for full-roof live acceptance.

## Remaining acceptance

Complete generated planning for the original 49-block roof, including transfers
between access areas, removal of all owned temporary works and final retreat.
Retain unsuccessful attempts as failures and compare progress under explicit
budgets. Then execute a completely checked generated roof plan in a fresh isolated
non-OP fixture, retaining independent final-site/cleanup evidence. Only after
that acceptance is stage 2 complete; adoption/MCP integration follows separately.
