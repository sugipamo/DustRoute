# Bounded search failures and MCP handoff

Roadmap stage 4 covers existing inventory-based construction search, physical
wire-path optimization, transition comparison and typed Blueprint block reduction.
This is a diagnostic change: no candidate algorithm, supported block, native
operation guarantee, adoption gate or live mutation behavior is added.

## Interpret the result before choosing another action

| Result | What it establishes | Next action |
| --- | --- | --- |
| Invalid input | The request violates an input limit or contract | Correct the request before searching again |
| Insufficient materials | Explicit supplied quantities do not cover the declared targets/reservations | Review the exact missing counts; a planning budget is not received inventory |
| Unsupported target or scene | The selected model/operation cannot accept this condition | Change the target, supported scope or initial conditions; increasing search limits does not add support |
| Search budget exhausted | This search stopped before obtaining an acceptable result | Inspect the effective limits and work counters; consider a revised scope or budget |
| Candidate/frontier/family exhausted | The generated, possibly pruned candidate family supplied no further acceptable result | Review bounded refusal samples or propose another supported design; this is not universal impossibility |
| Verification pending/undetermined | Evidence did not establish the requested contract | Inspect verification limits, interface evidence or model support; never adopt/apply an unfinished proof |
| Verified candidate found | That candidate passed the reported model checks | Review the proposal and use existing fresh adoption/execution gates; it is not globally optimal or live readiness evidence |

A successful report query can have `ok: true` while no acceptable candidate exists.
For example, Blueprint optimization reports `best: null`, and a wire preview can
have `contract_assessment.satisfied: false`. Their application/adoption gates
remain separate. Neither a query success nor a known zero counter grants action
authority. Unavailable verification remains unavailable, not a zero-difference
proof.

## Construction generation

`generation_refused.error.cause.kind` already distinguishes `invalid_input`,
`unsupported_target`, `initial_scene_refused`, `insufficient_materials`,
`no_complete_plan_within_limits` and `planning_task_failed`.
The bounded-search variant now also returns the effective `limits` alongside
`reason` and `search`. Limits include candidate checks, expansions, frontier
width and actions. Counters retain rejected/pruned candidates, action-limited
nodes, incomplete targets, bounded progress and refusal examples.

`candidate_frontier_exhausted` is the exhausted retained policy frontier. An
`action_limited_nodes` count or `pruned` count explains other limits that affected
it; it is not exhaustive enumeration of possible Minecraft construction.
Remaining targets and temporary cells are hypothetical diagnostics, never an
accepted partial plan. No job ID or native step authority is published by a
failed generation. The returned next action remains `review_search_failure`.

The API still returns a plan only after all permanent targets, temporary cleanup
and retreat have passed the existing complete-plan checks. The async worker
continues to operate on detached data with bounded computation.

## Wire-path optimization

The optimizer's public Rust error is now `PhysicalWireOptimizationFailure`,
which retains the typed local `reason`, effective `budget` and measured `search`
statistics. Previously the error path discarded these statistics even though
successful candidates exposed them. The internal stop enum preserves wire labels
`max_expansions`, `max_candidates` and `time_budget`.

A failed public search returns `ok: false`, the typed candidate `cause`,
`search.budget`, counters, `truncated` and `stop_reason`, plus
`writes_minecraft: false` and `impossibility_proven: false`. Truncated searches
use `error_code: resource_limit`; other local refusals retain `invalid_state`.
`search.candidate_budget_scope: strength_preserving_paths_per_segment` states
where `max_candidates` applies. It caps strength-preserving enumeration for each
segment, while `candidates` counts all generated paths across segments, including
the constant direct-path family used without strength preservation. It is not a
global ceiling on that counter. Expansion and elapsed limits remain shared. This
change reports the existing policy rather than changing candidate generation.
`retryable: false` means no automatic replay recommendation: inspect and change
the request before a new search. No operation ID, partial patch, saved plan or
operation history is created by this refusal.

A search can return a fully checked candidate after truncation. That remains a
candidate, with its truncation counters visible, and must pass the same contract
and later execution gates. No global optimum is claimed.

Wire verification now preserves failed original/candidate truth-table inference
in `verification.truth_table_failures`. Each side is null on successful inference
or carries the shared analysis diagnostic (`code`, message and relevant numeric
or positional evidence). Missing inputs, observation gaps and computation limits
are no longer collapsed into an unexplained `semantic.available: false`.
This evidence does not supply a missing truth table or satisfy a contract.

## Transition comparison

`MacroTransitionReport.unavailable_reason` is typed at its source. Contract
assessment no longer classifies explanation text. Input cardinality, inference
work/time limits, terminal mapping, incomplete observation, unsettled finite
observation and unsupported simulation have distinct categories.

For example, `TruthTableError::TooManyInputs` formats as "cannot enumerate ...";
the old text matcher did not recognize that wording and labeled it unsupported
physics. It is now `too_many_inputs` regardless of wording. A report without
typed provenance remains `transition_unavailable`; words in its message cannot
invent a category. Pending traces stay pending with no verified cases.

MCP transition views expose `reason_code` in addition to the explanation.
Timing/pulse assessments retain their existing passed/failed/unavailable states,
with the corresponding typed source code on unavailable checks. A finite
unsettled observation is not proof that a circuit never settles.

## Blueprint block reduction

The read-only report echoes the effective `budget`. `stop_reason` now contains a
stable Rust enum label instead of a prose string:

- `baseline_not_passed`: inspect the fresh baseline review; failed and
  undetermined remain distinct in that review.
- `layout_budget`, `binding_budget`, `time_budget`: the respective computation
  resource stopped the search. If baseline verification runs out of the shared
  time budget, the report retains its undetermined review and `time_budget`.
- `smaller_candidate_found`: generated search stopped early at a passing smaller
  candidate; it did not enumerate the whole family.
- `generated_family_exhausted`: no more generated layouts remain, within this
  declared incomplete family.

This intentionally changes the former human-only `stop_reason` text. Clients
should inspect codes and review statuses rather than parse old descriptions.
The always-false `global_minimality_proven` and the read-only/non-adoption response
flags remain. A best candidate from an earlier completed review can coexist with
a later exhausted budget; an unfinished candidate cannot become `best`.

## Boundaries and verification

Existing analysis truth-table diagnostics already retain typed input/work/time
limits. Existing Blueprint review keeps failed versus undetermined checks and
refuses adoption without a pass. Existing layout enumeration reports its declared
family and each candidate status. Those gates were inspected and are reused;
this work does not claim a redesign of every generation family or search policy.

Offline regressions exercise the public MCP handler/store and catalog session:
a geometry that succeeds with a larger budget refuses with a one-expansion limit
without publishing a partial operation; unavailable truth inference refuses
application both before and after restart; zero layout/binding/time budgets are
read-only reports without adoption authority. Typed transition tests retain the
source category even under misleading explanatory text. Construction reply tests
retain missing quantities and effective limits without publishing a job.

These checks cover diagnostics and the existing model gates. They do not add
live Minecraft physics evidence. The subsequent stage 5
[public live building/circuit acceptance](public-live-acceptance.md) completed
separately. See [the roadmap](failure-handling-stability.md) and
[capabilities](capabilities.md) for the supported scope.

## 2026-10-05 offline results

All commands used `--offline --locked -j1`; test commands used
`--test-threads=1`. The table lists positive test selections, not compilation-only
or empty-filter results.

| Package / selection | Passed |
| --- | ---: |
| `dustroute-optimize --lib` existing suite | 50 |
| `dustroute-optimize --lib contract::tests` after adding the provenance regression | 8 (7 overlap the preceding suite) |
| `dustroute-optimize --lib transitions::failure_tests` | 1 |
| `dustroute-optimize --test blueprint_reduction` | 12 |
| `dustroute-mcp --lib service::optimization_workflow::tests` | 6 |
| `dustroute-mcp --lib service::survival::replies::tests` | 7 |
| `dustroute-mcp --lib survival_construction::generation::tests` | 2 |
| `dustroute-mcp --lib operations::preview::optimization::tests` | 4 |
| `dustroute-mcp --lib macro_conversion_tests` | 2 |
| `dustroute-mcp --lib blueprint_optimization_moves_ports_and_reenters_persisted_explicit_adoption` | 1 |

The union covers **86 distinct passing tests**: 52 optimizer library cases,
12 optimizer integration cases and 22 MCP cases. This is relevant regression
coverage, not a new full-workspace or live Minecraft campaign.

Strict all-target Clippy passed for both default and `--no-default-features`:

```text
cargo clippy --offline --locked -j1 -p dustroute-optimize -p dustroute-mcp --all-targets -- -D warnings
cargo clippy --offline --locked -j1 -p dustroute-optimize -p dustroute-mcp --all-targets --no-default-features -- -D warnings
```

Formatting and whitespace checks passed. The changed documents' local link
targets resolved. No undeclared prerequisite, responsibility change, Minecraft
server start/write or host fault test was needed.
