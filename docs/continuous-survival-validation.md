# Continuous survival construction validation

This is stage 3 of [supported-scope failure handling](failure-handling-stability.md).
The goal is to verify supported ordinary sequences and useful stops after changed
prerequisites. It does not add terrain, entities, same-connection mining reuse,
server-side atomicity or arbitrary interrupted-operation recovery.

## Declared cases

All trials use a fresh isolated, unmodified Java 1.21.11 server on
`127.0.0.1:25572`, offline authentication, forced survival and an empty operator
list. The builder receives 49 cobblestone and 32 dirt through fixture setup.
Production admission/execution uses one builder; a separate non-OP client only
compares results. The existing player server and worlds are not edited.

| Case | Input and acceptance |
| --- | --- |
| Normal roof | Public MCP authoring/adoption, complete plan, movement, permanent/temporary placement, temporary mining and fresh same-profile recovery. Compare the complete final site, cleanup and retreat; confirm the handed-back source can capture a fresh scene. |
| Site changed after recovery | Add dirt at `[8, -60, 8]` after the first completed temporary removal. Stop with a typed site mismatch, preserve the known prefix and the foreign cell. |
| Materials changed after recovery | Clear the remaining cobblestone. Stop with the actual material/native placement refusal and preserve the prefix; do not manufacture material availability from the old plan. |
| Position changed after recovery | Teleport within the declared fixture to `[7.5, -60, 7.5]`. Preserve the actual geometry/motion/native refusal and known prefix; model predictions do not override a received correction. |
| Connection changed after recovery | Issue a normal server kick for the builder. Preserve the actual native/motion/mining refusal and known prefix; do not automatically acquire a new source or replay the job. |

The change trials wait for the first removal's completed prefix and confirm at
least one recovery in the public detailed record before requesting the console
input. They **do not pause the production executor**. The controller records the
input request time and commands; command submission is not a server application
acknowledgement. An already admitted operation may finish or become uncertain
while the input arrives. The final count is the actual confirmed prefix, not an
assumed exact stop step. Failure checks therefore accept declared typed causes
from the affected native or orchestration boundary rather than parse prose.

After stopping, the tests compare the public prefix and typed stored journal,
require `next_action=inspect_execution`, retain source quarantine and refuse a
second `start`. Stable journal bytes prove no scheduler/journal continuation in
that interval, **not** that hidden effects of earlier dispatch are impossible.
These are diagnostic stops, not repaired or resumable jobs.

## Test and controller boundaries

The existing opt-in `native_public_roof` test covers ordinary completion. The
new opt-in `native_public_continuous_change` test runs the real public MCP job;
it adds no executor hook, native physics entrypoint or bypass of authorization.
Its four cases are selected by the explicit test environment variable
`DUSTROUTE_CONTINUOUS_CHANGE`. Console setup and changes affect only that trial's
fresh world. The test runner and Java server run serially; Java uses one active
processor and a 768 MiB maximum heap. The controller issues a normal `stop` after
each test exits. No host configuration, process SIGKILL or fault trial is used.

Explicit MCP/trial JSON captures are diagnostic fixtures. Production job/status/
journal records retain the existing typed non-JSON storage codec. A trial capture
is bounded to 64 MiB and cannot authorize execution or restore a native token.

## Results

The normal trial on baseline `c981fe1` completed 115 steps and 18 same-profile
recoveries. The comparison client checked 3,120 final cells and the builder's
position. The production service had no observer configuration. Both the Rust
test process and isolated server exited zero. Mid-work change validation is in
progress; this baseline alone does not establish those outcomes.

## Stop conditions

Stop before a prerequisite changes Voxrig/DustRoute responsibilities, adds a new
native guarantee or physical mechanism, introduces a server MOD, or turns a
diagnostic stop into universal automatic recovery. Retain and report bounded
failure evidence rather than weakening the existing execution contract.
