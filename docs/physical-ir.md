# Physical-first IR

DustRoute treats observed Minecraft state as the source of truth. Signal, gate,
expression, and functional views are derived interpretations and must retain
traceability to physical evidence.

## Layering

1. `PhysicalScene`: observed blocks, typed ports, verified transfers, observation
   boundaries, fragments, and physical diagnostics.
2. `SignalView`: directed propagation derived from verified physical transfers.
3. `GateView`: partial or complete local gate recognitions with physical origins.
4. `ExpressionView`: expressions composed from recognized gates and unresolved
   placeholders.
5. `FunctionalView`: optional candidates such as half adders and multiplexers.
6. `PhysicalPatch`: reversible world changes whose higher-level effects remain
   predictions until observed after application.

Upper layers never replace or mutate the physical observation.

## Facts, derivations, and hypotheses

- Facts are block positions, block states, observed power, loaded regions, and
  unavailable boundaries.
- Derived facts use versioned Minecraft rules, such as repeater direction or a
  dust arm connecting to an adjacent block.
- Hypotheses include missing connections, recognized gates, circuit functions,
  and repair effects. They carry confidence, evidence, and conflicts.

## Core physical API

`dustroute-physical` will expose:

- `PhysicalScene`, the root for an observed area containing zero or more circuit
  fragments.
- `Observation`, `ObservedRegion`, and `ObservationFrontier`, so an incomplete
  scan is distinct from an unknown circuit.
- `PhysicalComponent` and typed `PhysicalPort` values.
- directed `PhysicalConnection` values between ports, supported by
  `PhysicalEvidence`.
- `PhysicalNet` and `PhysicalFragment`, built only from verified connections.
- diagnostics and reversible `PhysicalPatch` values.

Proximity never creates a verified connection and never participates in the
Union-Find used for nets. It only produces a diagnostic or repair hypothesis.

## Identity and traceability

A component is identified by dimension and block position within an observation.
Compact numeric IDs may be used inside one scene, but exported results include
the physical position. Every derived gate, expression input, diagnostic, and
repair references component, port, or net IDs that resolve back to positions.

## Observation completeness

An observation records why it is incomplete:

- a verified connection continues outside the scanned region;
- a chunk was unavailable;
- a scan or policy limit was reached.

Functional classification is provisional whenever relevant signal paths meet an
open frontier. Local physical and gate results remain usable.

## Derived IR API

`dustroute-ir` will expose three views:

- `GateView`: `AND`, `OR`, `NOT`, `XOR`, and other local recognitions, including
  partial and conflicting candidates.
- `ExpressionView`: named or anonymous signals expressed through recognized
  gates, preserving unresolved subgraphs.
- `FunctionalView`: ranked optional classifications with missing features and
  conflicts.

Recognition status is separate from gate kind: complete, partial, conflicting,
or boundary-limited.

## Final names

- `PhysicalScene` is the canonical observed-world root.
- `VerifiedTopology` is its lower-level verified component/connection topology.
- `PlacementCircuit` is the forward compiler and optimizer's proposed cells and
  routes; it is never an observation.

No compatibility alias named `PhysicalCircuit` is exported, preventing observed
state and proposed placement state from being confused at API boundaries.

## MCP contract

`analyze_looked_at_circuit` returns, in order of authority:

1. observation completeness and focused physical component;
2. local signal role and recognized gates;
3. expressions with unresolved portions retained;
4. optional functional candidates;
5. physical diagnostics and non-mutating repair proposals.

The MCP server must not turn `unclassified` into “not a circuit.” It explains
recognized local structure and the reason higher-level classification is absent.
World mutation continues to require preview and explicit confirmation.
