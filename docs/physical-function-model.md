# Physical function model

Efficient Minecraft circuits frequently reuse one dust network, support block,
or torch path for more than one logical purpose. DustRoute therefore does not
require every physical block to belong to exactly one NOT, AND, or OR gate.

When truth-table inference is requested, reverse translation now produces a
physical function model containing:

- the exhaustive input/output observations;
- one Boolean expression and truth column for each inferred output;
- every physical signal component's reachable input and output sets;
- a `shared_role` marker when a component depends on multiple inputs or can
  influence multiple outputs.

The output expression is the primary functional claim. Recognized local gates
remain useful explanations and debugging landmarks, but they are not an
exclusive partition of the circuit.

For example, the compact compiled XOR is recovered as `in0 ^ in1` from its
observed truth table even though its physical network contains shared paths and
many lower-level primitive cells. This same representation can describe a
future compact hand-built XOR without forcing its shared blocks into artificial
gate boundaries.

The MCP reverse-analysis response exposes this as `physical_function_model`
when `include_truth_table` is enabled. Open-boundary or unsupported circuits
still return an unavailable functional result. Large circuits are no longer
rejected solely by their component count when an exhaustive table is explicitly
requested; row, tick, estimated-work, solver-iteration, and elapsed-time
budgets bound the attempt and report `budget_exceeded` when necessary. A
runtime-limited attempt never returns its partial rows as a complete table.

## Macro replacement search

`dustroute-optimize` can search the verified component catalog using the
functional model. Matching uses the complete truth table, including input and
output port permutations, rather than local gate labels. Candidates must:

- have physical layout metadata;
- carry Minecraft E2E evidence;
- explicitly support the requested edition and version;
- improve occupied-block count or bounding volume.

The result includes the observed-to-macro port mapping and estimated block and
volume savings. It remains proposal-only: surrounding placement, routing,
steady-state equivalence, and every relevant input transition must be verified
after realization before a mutation plan is allowed.
