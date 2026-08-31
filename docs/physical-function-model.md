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
when `include_truth_table` is enabled. Large or open-boundary circuits continue
to skip exhaustive truth-table inference; their physical observations and
partial local roles remain available without making a complete function claim.
