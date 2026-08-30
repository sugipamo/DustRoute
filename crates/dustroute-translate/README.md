# dustroute-translate

Bidirectional translation is exposed as a physical-first, staged analysis. The
canonical flow is:

`ObservedWorld -> PhysicalScene -> ElectricalNetwork -> TimedBehavior -> LocalLogic -> FunctionalCandidates`

Each higher stage is a derived view. Physical coordinates, completeness,
diagnostics, unresolved items, and provenance remain available through
`PhysicalAnalysis::hierarchy`; callers must not treat a partial functional guess
as stronger evidence than its physical source.

The public facade consists of:

- `analyze_physical_region` — builds all typed stages from a world observation.
- `derive_local_logic` — classifies truth-table-backed local behavior.
- `propose_scenarios` / `simulate_scenario` — creates and runs transition probes.
- `compare_live_trace` — compares simulator output with a live recording.
- `explain_signal_path` — follows a directed physical signal route.
- `verify_semantic_equivalence` — compares before/after behavior by truth table.
- `analyze_signal_liveness` — separates undirected physical membership from
  directed drive reachability. Sources retain their evidence kind: controllable
  input, intrinsic source, observation boundary, or inferred primary input.
  Required inputs are classified as driven, awaiting an external input,
  disconnected, or lacking a known source.
- `propose_scene_repairs_near` — enumerates local physical patches, applies each
  to a cloned observation, rebuilds connectivity, compares directed liveness,
  runs the instantaneous electrical solver, and marks patches whose components
  require subsequent temporal validation.

Union-Find membership is exposed as a physical traversal group only. It bounds
discovery and nearby-break searches; it is not used as logical identity or as
proof that a directional signal can propagate.

An inferred primary input is informational rather than a fault. Repair search
must not connect it to another net solely to reduce the number of unpowered
components; only disconnected and no-known-source findings are fault evidence.

MCP, CLI, and future optimizers should consume this facade. Natural-language
grounding, player gaze, authorization, preview/confirmation, and presentation
remain outside this crate.
