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

## Remaining cleanup candidates

The initial change resolves the transition execution/facade boundary and the
large inline test block. Other candidates remain separate work:

| Area | Boundary to clarify | Contract to preserve |
| --- | --- | --- |
| Assembly placement | Review, mutation, readback, rollback and history | Source revalidation, consumption and rollback evidence |
| Repair | Admission, saved lifecycle, submission and undo | Saved lifecycle before writing; uncertainty after a lost reply |
| Blueprint dispatch | Domain command results versus persistence decisions | Explicit adoption and catalog transaction ownership |
| Runtime review / promotion | Requirement traversal versus report construction | Independent contexts and no adoption from historical passes |
| Survival candidate generation | Candidate expansion, search pruning and selection | Material, reachability and native evidence ownership |
| Optimization | Immutable candidate search versus operation publication | No bridge capability in pure optimization |
| Physical execution | Event scheduling versus device semantics | Supported model boundaries and checkpoint identities |

Do not unify independent execution models or change Voxrig/DustRoute ownership
for readability alone. A new feature, stronger recovery guarantee or cross-crate
responsibility change requires a separate review with the user before proceeding.

## Validation (2026-10-05 UTC)

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
cargo test --offline --locked -j1 -p dustroute-mcp --lib -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-mcp --lib service::transition_failure_tests -- --test-threads=1
cargo clippy --offline --locked -j1 -p dustroute-mcp --all-targets -- -D warnings
cargo clippy --offline --locked -j1 -p dustroute-mcp --no-default-features --all-targets -- -D warnings
cargo fmt --all -- --check
```
