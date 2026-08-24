# Logic DAG IR

The compiler now has an explicit pure-logic DAG stage between abstract logic
and physical redstone realization.

## Pipeline

```text
abstract logic
    |
    v
LogicDAG
    |
    | XOR lowering / logical transforms
    v
primitive LogicDAG
    |
    | DAG -> legacy Circuit bridge
    v
Gate / Pin / Net Circuit
    |
    v
cell mapping -> placement -> physical routing
```

`LogicDAG` deliberately contains no Minecraft concepts: no coordinates,
directions, dust, repeaters, support blocks, or routing paths.

## Core types

- `LogicNode(id, op, inputs, name)`
- `LogicDAG(nodes, outputs)`
- `DAGBuilder`
- `DAGCircuitBridge`

The DAG exposes stable topological ordering, user relationships, fan-out
counts, logic depths, evaluation, and basic statistics.

## Half-adder

The abstract half-adder contains only:

```text
a ----+---- XOR ---- sum
      |
b ----+---- AND ---- carry
```

After SOP XOR lowering:

```text
not_b = NOT(b)
not_a = NOT(a)

sum = OR(
    AND(a, not_b),
    AND(not_a, b)
)

carry = AND(a, b)
```

The original `a` and `b` producer nodes are shared; they are not copied.
Each therefore has fan-out 3 in the primitive DAG.

## Why the DAG matters

A logical fan-out is not yet a physical wire branch. The physical compiler may
later choose one shared trunk, multiple branches, or even duplicate a logical
subexpression if that gives a better Minecraft layout.

This separates:

- logical sharing / common subexpressions
- logic lowering choices
- placement
- physical fan-out trees
- redstone signal-restoration constraints

The current raw half-adder compiler now records both `abstract_dag` and
`primitive_dag` and obtains its legacy `Circuit` from the primitive DAG.

## Compatibility

The existing `Circuit(Gate/Pin/Net)` IR is retained as a bridge to the current
physical compiler. This lets routing and placement be improved independently
without coupling those changes to the new logical IR.
