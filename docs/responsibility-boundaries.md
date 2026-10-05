# Responsibility boundaries and readable code

This is the current readability cleanup on top of `5016ec9`. The objective is
code that makes ownership, dependencies and execution order understandable.
Line count is not an objective: additional Rust types and modules are appropriate
when they make a real boundary explicit.

## Ownership

| Owner | Responsibility | Does not establish |
| --- | --- | --- |
| MCP facade (`service.rs`, `service/requests.rs`, `service/mcp_output.rs`) | Decode public IDs and signal intent, route tools, encode replies | World mutation completion or permission to replay |
| Transition workflow (`service/transition_workflow.rs`) | Declare dependencies, acquire mutation exclusivity, retain the scenario plan and orchestrate one attempt | New physical laws or native client guarantees |
| Preparation (`transition_workflow/preparation.rs`) | Check preview/lifecycle, fresh lever and region state, validate the world, derive a scene | A consumed attempt or authority from saved results |
| Run (`transition_workflow/run.rs`) | Approach, start recording, consume the attempt, activate, wait, stop recording, restore and record the result | Successful cleanup merely because activation succeeded |
| Restoration (`transition_workflow/restoration.rs`) | Restore an attempted scenario, use the existing snapshot-write fallback when needed, verify readback | Successful restoration merely because writes were submitted |
| Comparison (`transition_workflow/analysis.rs`) | Compare recorded evidence, typed signal contracts and bounded simulation | Execution authority or a guarantee outside the model |
| Result types (`operations/transition.rs`) | Preserve diagnostic facts and expose the existing MCP response shape | A reusable plan, observation lease or native operation handle |
| Native adapter / Voxrig | Protocol, physical observation, single-operation and connection-lifecycle contracts | DustRoute plans, material allocation or job policy |

Transition code remains in the MCP crate because it depends on this adapter's
bridge, policy, transient plans and operation history. File separation does not
make it a generic core API. No new transport abstraction is introduced.
The workflow explicitly imports its dependencies instead of inheriting the
facade's entire import namespace.

## Transition execution contract

The caller admits confirmation and mutation policy before acquiring a session.
A `TransitionSession` owns the mutation guard across ID decoding, fresh-state
preparation, contract decoding, execution and final response encoding. The
workflow receives a parsed UUID and typed signal intents; MCP argument DTOs,
JSON values and `CallToolResult` stay in the facade.

Preparation constructs `PreparedRun` internally. The facade can inspect its
scene to resolve public signal-contract positions, but cannot edit its fields.
This preserves the existing refusal priority: confirmation, mutation policy,
ID, plan, preview/lifecycle/safety, fresh live state, world validation, signal
contracts, then approach and recording. A prepared value is not a saved proof;
it borrows the session that checked it, and consumes itself to run through that
same session. It cannot outlive the mutation guard.

Attempts are consumed before activation. Cleanup keeps its existing order and
appends failures rather than replacing the first failure. Recording failure
still runs lever restoration and readback. Simulation, observed trace,
restoration and overall attempt outcome remain separate diagnostic results.
Natural restoration in an explicit restore operation retains its existing
predicate; a wait failure remains in the attempt report even if final readback
matches. This cleanup does not widen or redefine those guarantees.

The explicit restore command-write fallback is distinct from automatic lever
restoration after an observed run. Their checks and outcome predicates differ;
combining them just to share lines would obscure those contracts.

## Tests as executable contracts

The former inline service tests are grouped by the contract they protect:
`mcp_boundary_tests`, `analysis_tests`, `optimization_contract_tests`,
`placement_tests`, `repair_tests`, `revision_tests` and `piston_tests`.
Small shared world fixtures live in `test_support`; tests keep their individual
scenario setup and expected outcomes visible. Existing transition failure tests
continue to cover cleanup, consumed attempts and independent result channels.

The test-only `legacy_cause_reply` adapter is removed. Its callers now assert
current diagnostic category, phase, resource and lack of execution facts directly.
They also retain checks that refusal creates no operation or store, does not
change pending state, and does not permit replay. Tests no longer reimplement a
retired JSON promotion algorithm as an expected-result generator.

Retain tests for externally meaningful behavior: admission before effects,
fresh-state validation, consumption before submission, restart/recovery,
uncertain submission evidence, cleanup and stable MCP contracts. Remove a test
or test helper when its only purpose is a retired format, an implementation
mirror or a redundant assertion already protecting the same contract.
A fake-transport test is not live Minecraft conformance evidence.

## Cleanup of the seven candidates

The transition work above is retained. The subsequent cleanup covers every
candidate from the initial inventory, with behavioral verification recorded
below. The boundaries are concrete private functions and Rust types, not a new
framework or a reduction in code size.

| Area | Owners after cleanup | Contract preserved |
| --- | --- | --- |
| Assembly placement | The workflow admits and consumes attempts; `review_construction` rebuilds the proof; `checkpoint_attempt` saves each executor stage; `finish_attempt` records the final result | Fresh source/preview equality, saving before continuation, and independent persistence failure |
| Repair | The workflow saves intent and confirms readback; `recover_verification_failure` owns restoration; `analyze_completed_repair` owns post-analysis | Lost replies do not permit replay; restoration does not erase the original target failure |
| Blueprint dispatch | Command routing, write commands, proposal decisions and the catalog transaction are named stages; `CatalogActionResult` carries a separate persistence decision | An open review or refused adoption can save history without successful adoption; one lock covers load, validate and save |
| Runtime review / promotion | Fixed-geometry review separates initial placement, actual connections and occurrence requirements; runtime review separates requirement collection, initial-world review and binding exploration | Independent model contexts, declaring-consumer attribution, one review deadline and shared graph budgets |
| Survival candidate generation | `generation.rs` admits inputs and creates search state; `candidates.rs` generates checked actions; `policy.rs` owns ranking, pruning, cleanup search and completion | Original candidate order, heuristic limits, material reservation and native admission; exhaustion is not impossibility |
| Optimization | `OptimizationPlanner` searches an immutable captured circuit; the workflow saves the draft, then publishes operation history | The planner receives policy and analysis only, without bridge, store or history capabilities |
| Physical execution | `PhysicsEngine` owns scheduling, causal order, budgets, rejection and world/trace application; `BoundedDevices` prepares and dispatches existing device reactions | Model admission order, queue rollback, event phases, motion delays and checkpoint identities |

### Why these boundaries remain separate

A construction checkpoint must be persisted before the next physical stage,
whereas final publication records the result of the entire attempt. Neither
successful writes nor a successful final save imply observed completion.
Repair restoration and post-analysis cannot share a success predicate: a repair
may fail its target and still restore its baseline, or complete writes and fail
later analysis.

Catalog response success and persistence are independent. Review history is a
catalog change even when adoption is refused. The private persistence enum
replaces a boolean tuple without changing the archive schema or MCP replies.
`ProposalAction` limits the proposal handler to proposal operations, so it no
longer accepts unrelated read/write/capture commands or carries a second ID.

The fixed-geometry reviewer still uses `ValidatedWorld`; the moving-world
reviewer uses the native initial gate and monitors actual reachable states.
They share diagnostic types, not a proof implementation. `ReviewBudget` keeps
the deadline established before inspection; all bindings consume the same
remaining graph limits. No historical pass becomes adoption authority.

Construction candidate generation continues to use DustRoute policy over
Voxrig's checked hypothetical transitions. File boundaries do not turn pruning
into a native guarantee or an exhaustive reachability algorithm. Only detached
scene data enters its CPU worker; no execution handle is introduced.

The bounded event runner remains an active model, including for the fixed door.
Its device handlers are not replacements for the common electrical runtime.
`apply_event_transition` retains the existing world application and trace order;
it does not strengthen atomicity or alter what a rejected diagnostic event
records. Queue restoration stays in the scheduler.

### Tests retained

No test scenarios are deleted in the seven-candidate cleanup. Two adoption
assertions that searched diagnostic prose for retired JSON fragments now inspect
typed abstract-proof evidence. The implementation-specific 9,520-state count
is removed; universal proof selection and reuse of a four-state proof under one
four-state budget remain checked directly. The existing tests cover
externally meaningful contracts rather than the newly introduced function
names. In particular, proposal/history restart tests, repair lost-reply tests,
placement readback tests, adoption/alias tests, search resource tests and causal
scheduler tests remain in place. The initial transition cleanup removed the
obsolete JSON helper described above; it did not remove those behavioral checks.

No feature, transport, save format, physics rule or Voxrig/DustRoute ownership
change is included. If a proposed follow-up cannot clearly improve readability,
the requested stop condition applies to the whole cleanup.

## Validation (2026-10-05 UTC)

### Seven-candidate cleanup

- All 44 bounded-engine regressions passed: causal order, budget limits,
  checkpoint/retry, input admission, wire propagation, repeater delay and piston
  start/completion.
- All 57 contextual-review regressions passed across `promotion`,
  `runtime_adoption`, `review_diagnostics`, `repeated_settling_adoption`,
  `blueprint_updates`, `nested_interfaces` and `physical_periodic`.
  The first run exposed two retired diagnostic-text assertions; those now
  inspect typed proof evidence, and the affected suite passed on rerun.
- The complete MCP library suite passed: 303 passed, no failures, 10 existing
  opt-in tests ignored. It includes save/restart/replay, placement/removal,
  repair, optimizer publication and survival resource contracts.
- All-target Clippy passed with warnings denied for MCP, Minecraft and
  Translate. MCP also passed with `--no-default-features`. Workspace
  formatting and whitespace checks passed.
- Moved search methods and device handlers were compared with the original
  bodies during review. Search order, budget consumption and device refusal
  conditions remain unchanged. This source review is not a live conformance test.
- Documentation links resolve, including the historical executor reference
  updated to its current `root_behavior` destination.

### Initial transition cleanup

- The initial workflow extraction and contract-test regrouping passed the full
  MCP library suite: 302 passed, no failures, 10 existing opt-in tests ignored.
- After the final session-lifetime binding, snapshot-restoration extraction and
  admission-priority regression were added, all six transition workflow tests
  passed against the final implementation.
- All-target Clippy passed with warnings denied, both with the default native
  feature and with `--no-default-features`. Workspace formatting passed.
- Links in the changed documentation pages resolve. These are offline checks;
  no new live Minecraft conformance claim is made by this refactoring.

Commands (run serially):

```sh
cargo test --offline --locked -j1 -p dustroute-minecraft --lib time::engine::tests -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-translate --test promotion --test runtime_adoption --test review_diagnostics --test repeated_settling_adoption --test blueprint_updates --test nested_interfaces --test physical_periodic -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-mcp --lib -- --test-threads=1
cargo clippy --offline --locked -j1 -p dustroute-mcp -p dustroute-minecraft -p dustroute-translate --all-targets -- -D warnings
cargo clippy --offline --locked -j1 -p dustroute-mcp --no-default-features --all-targets -- -D warnings
cargo fmt --all -- --check
```
