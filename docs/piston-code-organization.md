# Piston implementation cleanup

Scope: the supported Java 1.21.11 v6 runtime, exact command construction and
public custom-Assembly lifecycle. Minecraft observations and their regression
fixtures remain the reference. This cleanup adds no physics, device support,
readiness detector or saved-data migration.

## Inventory and removal

The horizontal, vertical and direct callback adapters and their compatibility
aliases were already removed in the [earlier inventory](stabilization-legacy-paths.md).
The remaining old construction-order experiment, `audit_reference_door_construction`,
was deleted: it independently implemented rank/support ordering and teardown for
an earlier hypothesis. Current construction is inspected through
`audit_reference_door_adoption ordinary-construction`; the old command failure is
covered by `command_placement_regression` using retained Java commands/readbacks.
Its original evidence, raw outputs and source fingerprints remain historical
records, rather than new acceptance criteria. The old reproduction instructions
now point to the production planner and regression.

These distinct, used paths remain:

| Path | Reason |
| --- | --- |
| Bounded `PhysicsEngine` piston/direct-source/propagation runners | Existing low-level model contracts, conformance cases and the fixed 1×2 door use these entry points. They are not aliases for the callback runtime. |
| Fixed-geometry `RedstoneTickSimulator` | Active tracing, scenario and optimization consumers with their own device/model coverage. |
| Grounded revision reflection and general placement | Existing-world changes and their provenance/recovery checks differ from empty-target construction. |
| Catalog readers, registry and public response fields | Active persisted/API contracts; historical context rejection remains explicit. |
| Recorded model failures and live captures | Necessary evidence distinguishing old predictions from actual Minecraft behavior. |

Removing the retained active contracts would require a separate consumer/model
migration. That is not a prerequisite for this internal cleanup. No replacement
feature was started.

## Responsibilities

| Boundary | Responsibility |
| --- | --- |
| [Runtime command entry](../crates/dustroute-minecraft/src/time/piston_runtime/command.rs) | Requested-state preprocessing, command writes, add/remove callbacks and their continuation order |
| [Electrical callbacks](../crates/dustroute-minecraft/src/time/piston_runtime/electrical.rs) | Electrical queries, scope checks, wire/repeater callbacks and notification construction |
| [Native devices](../crates/dustroute-minecraft/src/time/piston_runtime/devices.rs) | Lamp/observer Law effects; shared requested-observer tick reservation for commands and carrier arrival |
| [Construction planner](../crates/dustroute-translate/src/piston_construction.rs) | Simulate each step, require the exact reviewed final world, then simulate full teardown |
| [Candidate ordering](../crates/dustroute-translate/src/piston_construction/order.rs) | Existing support/watch precedence and deterministic removal selection |
| [Exact states](../crates/dustroute-translate/src/piston_construction/snapshot.rs) | Lossless export and explicit powered observer initialization |
| [Mismatch diagnosis](../crates/dustroute-translate/src/piston_construction/diagnostics.rs) | Explain a constructed world that does not match its declared result |
| [MCP planning](../crates/dustroute-mcp/src/service/assembly_placement.rs) | Public routing, plan ownership/preview and instance management |
| [MCP validation](../crates/dustroute-mcp/src/service/assembly_placement/validation.rs) | Source/adoption identity, shared typed basis decoding and target contract |
| [MCP observations](../crates/dustroute-mcp/src/service/assembly_placement/observation.rs) | Complete snapshot comparison and explicit observation/history limits |
| [MCP execution](../crates/dustroute-mcp/src/service/assembly_placement/execution.rs) | Consume one attempt, verify every write and persist partial progress/final status |

`ValidatedAssemblyPlacement` remains the fresh model capability between review
and orchestration. The registry remains the persisted attempt record. Neither a
plan nor a saved pass replaces fresh review or live observation.

## Preserved boundaries

- Runtime/exploration profiles remain v6 and Laws remain unchanged. The event
  enum, queue identities, callback order and restoration boundaries are retained.
- Public tool names/JSON fields, error messages, saved formats, source pins,
  placement limits and preview/consumption/partial-failure behavior are retained.
- Construction/removal still follow the same deterministic order, exact command
  states, wait margins and complete expected snapshots. The public
  `piston_construction::electrical_snapshot` API remains available.
- Typed full-field equality replaces serializing construction steps/transforms
  solely to compare them. It does not omit any field or authorize a saved plan.
- Historical evidence is not rewritten to claim another live trial. The prior
  [43-stage live build/removal and two-cycle result](reference-door-live-construction.md)
  supplies the measured baseline. No Minecraft server is started for this cleanup.

## Verification

Before editing, the relevant sources and full production construction JSON were
saved under `.local/e2e-artifacts/piston-cleanup-20260927-*`. Final validation passed:

- The complete before/after construction JSON is byte-identical: 43 build and
  43 removal steps, including command states, wait margins and expected snapshots.
- 96 Rust tests passed, covering retained physical observations, checkpoints,
  construction, adoption and public MCP readback/persistence/removal guards.
- Both Python command-placement evidence tests passed.
- The MCP binary and production audit example built successfully. Clippy passed
  with warnings denied for all targets of the three affected crates; workspace
  formatting and diff whitespace checks also passed.

Rust checks used one build job and tests ran serially. Results and hashes are recorded in
[the cleanup verification record](evidence/piston-code-cleanup-20260927.json).
