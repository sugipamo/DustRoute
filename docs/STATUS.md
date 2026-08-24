# Project Status

## Confirmed baseline

Python regression at the refactoring checkpoint: **76 tests PASS**.

Java Edition real-world confirmation:

- low-level semantics/connectivity probes 01–20: PASS
- half adder truth table: PASS
- 2:1 MUX truth table: PASS
- enabled 1-to-2 decoder truth table: PASS

The refactor that introduced `BaselineCompiler`, `PortRealization`, and `RoutingResources` was verified to generate byte-for-byte identical Minecraft Data Packs to the pre-refactor checkpoint.

## Next technical target

The next major target is routing scalability rather than basic semantics. Full-adder and 2-bit ripple-carry-adder DAGs can be represented, but the baseline router does not yet find legal layouts reliably/efficiently at that scale.

## Definition of compatibility

A change is considered compatible only when it preserves:

1. Python logical/electrical/routing regression tests,
2. static physical route legality,
3. the Java Edition compatibility suite where the touched layer can affect real behavior.
