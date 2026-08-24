# Refactoring checkpoint

This checkpoint intentionally changes architecture without changing generated
Minecraft behavior.

## Structural changes

- Added `BaselineCompiler` as the single non-optimizing DAG compiler pipeline.
- Moved fixed primitive cell selection to `baseline_cells.py`.
- Added explicit typed port realization in `port_realization.py`.
- Added typed routing reservations in `routing_resources.py`.
- Reduced `raw_half_adder.py` and `dag_baseline.py` to front-end /
  compatibility wrappers.
- Kept physical-step continuity in `connectivity.py` as the authoritative
  one-edge legality check used by routing validation.
- Split the former monolithic `tests/run_all.py` into responsibility-focused
  modules.

## Regression status

76 Python regression tests pass when run by module.

The generated files for all three real-Minecraft regression packs are
byte-for-byte identical to the pre-refactor checkpoint:

- `ro_sem` low-level probes 01..20
- `ro_half_base` half-adder validation
- `ro_circuits` MUX + decoder validation

This means the refactor has not changed the command-level Minecraft artifact.
