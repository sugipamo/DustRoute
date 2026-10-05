# Explicit development fixture adapters

Production debugging uses the typed Rust APIs; the public interface is MCP.
Retired JSON example commands are not production storage or operation APIs.
Independent expected observations and retained evidence must not be regenerated
from the simulator. These adapters only read/write explicitly selected test data.
None grants live observation, adoption, placement, or connection authority.

All adapter tests are ignored by default. An offline export requires an absolute,
new `DUSTROUTE_DIAGNOSTIC_OUTPUT` path. Existing files are rejected. Inputs below
must also use absolute paths. Normal MCP use does not require fixture files.

```sh
DUSTROUTE_DIAGNOSTIC_OUTPUT=/tmp/new-fixture.json \
cargo test --offline --locked -j1 -p dustroute-translate --test TEST_NAME -- --ignored --exact retain_fixture --test-threads=1
```

| TEST_NAME | Optional/required fixture input |
| --- | --- |
| mixed_assembly_construction_fixture | None; unadopted mixed assembly proposal |
| flying_machine_assembly_fixture | None; unadopted flight proposal |
| export_compiled_xor | Optional DUSTROUTE_XOR_SPACING_X and required DUSTROUTE_XOR_LANE_GAP when spacing is set; native numeric configuration |
| compare_external_xor_trace | DUSTROUTE_XOR_TRACE_INPUT: independent PhysicalTrace; DUSTROUTE_XOR_INPUT_A/B: numeric inputs, nonzero means true; optional new DUSTROUTE_XOR_SIMULATOR_OUTPUT |
| piston_low_layer_replay | DUSTROUTE_LOW_LAYER_INPUT: bounded 1.21.11 case fixture |
| audit_reference_door_adoption | DUSTROUTE_DOOR_FIXTURE_ACTION: prepare, live-prepare, target-live-prepare, construction, review, adopt, recheck; ordinary- prefix selects ordinary contract |

Door target-live-prepare also requires `DUSTROUTE_DOOR_TRANSFORM_INPUT`.
Door adopt/recheck require `DUSTROUTE_DOOR_ARCHIVE_INPUT` containing a typed archive.
This is test-only compatibility with retained fixture material. Revalidation is
always executed through BlueprintUpdates; saved success cannot authorize adoption.
The ordinary/live helpers compute finite model expectations, never live proof.

The native APIs remain `BlueprintUpdates`, `ElectricalConstruction`,
`review_assembly_in_runtime_context`, `PhysicsEngine`, `compare_physical_traces`,
`simulate_cell_trace`, and `compiled_xor_cell[_with_config]`.
