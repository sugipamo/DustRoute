# 3×3 piston shuttle fanout

This document records the bounded electrical control slice for the reference
3×3 piston shuttle. It is a designed control contract, not a claim of full
Vanilla redstone topology. The contract is executable through the reusable
`dustroute_translate::PistonDoorScenario` API; the integration test is only a
consumer of that API.

## Control shape

The fixture uses one external lever edge as the input boundary. The runner
drives one root source for the selected edge, then propagates through a real
wire/repeater graph:

```text
one root source → three row trunks → nine leaf channels
```

The open and close sides use independent roots so that the same mechanical
cell can be driven in both directions:

| side | root | first-level lanes (row 0, 1, 2) | leaf repeater direction |
| --- | --- | --- | --- |
| open | `(-22, 1, -6)` | `z = -7, -6, -9` | south, `z = -5..-2` |
| close | `(-22, 1, 7)` | `z = 8, 7, 10` | north, `z = 6..3` |

Each row trunk begins at `x = -18` and contains 9, 15, or 21 one-redstone-
tick repeaters. The explicit lengths preserve row order without requiring
same-tick completion rebasing. The four leaf repeaters per cell use the delay
matrix stored in the fixture; the matrix is repeated for all three rows.

The nine leaf outputs terminate at the existing reference coordinates:

- door blocks: `(x, y, 0)` for `x,y = 0..2`;
- open pushers: `(x, y, -1)`, facing south;
- close pushers: `(x, y, 2)`, facing north.

The coordinate equality is checked against
`reference_3x3_noncompact_piston_shuttle.json` by the fanout integration
test. The mechanical reference remains the source of the door geometry; the
fanout fixture adds only the bounded electrical prefix.

## Execution contract

`PistonDoorScenario::from_json` validates the versioned layout and
`PistonDoorScenario::run_cycle` is the common `closed → open → closed`
execution path. `PistonDoorScenario::translated` can move the same topology to
another origin without changing the executor.

Internally, `LeverPulseSequence` is retained as the generic external-edge boundary. It
changes the lever once, emits one high edge to the selected root, and emits a
low edge after the configured pulse width. The root edge then travels through
the ordinary wire, repeater, neighbor-update, and piston path. No piston action
is scheduled directly by the test.

Open and close are tested as separate stable phases. The test suite also checks
that a repeated stable `ON` edge is a no-op, malformed sources fail closed
before changing the lever, translated layouts use the same execution path, and
two replays produce identical world and trace ledgers.

## Observation boundary

The scenario builder derives a complete planning region from all materialized
components and expands it by the Java push limit plus one block. Coordinates
outside that region are unknown and must not be inferred as air. This preserves
the existing fail-closed contract while allowing translated layouts and the
longer row-2/close-side branches to be evaluated.

## Deliberate limits

This slice intentionally does not introduce a new scheduler or physics
meaning. It does not implement:

- same-tick multi-piston completion rebasing;
- scheduler or delay profile changes;
- QC/BUD, Slime/Honey, or moving interruption/reversal;
- complete Vanilla wire topology or an Observer-based controller;
- automatic physical correction from an observed mismatch.

If a selected door layout requires one of those behaviors, implementation must
stop at that boundary and the missing behavior should become a separate goal.
