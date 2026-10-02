# Bounded survival navigation

DustRoute selects routes; Voxrig predicts and executes native control ticks.
The implementation is [survival_navigation.rs](../crates/dustroute-mcp/src/survival_navigation.rs).
This is a planning/execution foundation for the survival Blueprint roadmap, not a
public MCP building command, Blueprint adoption, or a durable construction job.

A `RouteRequest` supplies an inclusive goal volume for the feet, a reviewed travel
volume containing the whole body, and a prediction budget. The planner uses
native standing dimensions/uncertainty from the received context rather than
maintaining a second physics model. It tries eight headings, short/long walking
and jumping primitives, and released ticks until stopping. Each candidate is
predicted by Voxrig from the same initial player/world context; a changed baseline
aborts. The search yields between candidates so packet receivers can apply queued
updates. It sends no player input or world command.

The bounded best-first frontier uses distance to the goal and control duration,
with endpoint bins of 1/16 block. Search is not complete or optimal. A request
admits at most 32 blocks along each travel axis and 2–4096 predictions. Each
outbound candidate uses at most half of the native 120-tick run budget, reserving
the rest for a predicted return. Unknown/unsupported paths are refused, and a
structured `NoRouteWithinLimits` retains examined counts and representative
refusals. It does not assert that no Minecraft route exists.

A selected route requires:

- A native admitted terminal standing position with the conservative margin.
- Every predicted body position within the declared travel bounds.
- A separately predicted round trip ending within 0.35 horizontal blocks and
  0.125 vertical blocks of the original standing area, with terminal clearance.
- An unchanged connection, generation, dimension, world revision, player context
  and initial position across the successful candidate predictions.

The return is tested through physics, not assumed from reversed headings. It is
only evidence for the original world snapshot. `preview_return` re-evaluates the
return after a placement/world edit and checks its origin, footprint and endpoint
again. It does not authorize an old return path through changed geometry.

`SurvivalRoute::start` calls native `start_previewed_survival_motion`: prediction
is recomputed under the send-intent lock, and mismatched context/frames reject
before any control packet. Existing finite-run interruption/correction handling,
independent observer evidence and result checks remain intact. Route JSON is
diagnostic and cannot be deserialized as action authority. Calling code still
owns site permissions, durable intent, Blueprint references, resource accounting,
placement sequencing and cleanup. No command/creative/teleport fallback exists
in this module.

## Validation scope

Search tests use a deterministic obstacle model to check exploration, return
validation, budget reporting and observation changes. They do not prove native
physics. Voxrig's real connection tests cover multi-heading output and stale
preview refusal, in addition to its native method oracles and earlier live tests.
The ignored `native_route_around_wall_place_and_return` test uses the isolated
non-OP two-client fixture and separately records selected controls, predicted and
observed endpoints, ordinary placement/material results, revalidated return and
received packet traces. Its source alone is not evidence of a successful trial.
