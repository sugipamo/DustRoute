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
Materials are checked at the affected placement: intervening admitted movements
or temporary works can complete before the next missing-material refusal.

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
each test exits. No host configuration, process SIGKILL, kernel/filesystem fault
trial or deliberate server crash is used.

Explicit MCP/trial JSON captures are diagnostic fixtures. Production job/status/
journal records retain the existing typed non-JSON storage codec. A trial capture
is bounded to 64 MiB and cannot authorize execution or restore a native token.

## Results

The normal trial on baseline `c981fe1` completed 115 steps and 18 same-profile
recoveries. The comparison client checked 3,120 final cells and the builder's
position. The production service had no observer configuration.

All four change cases on `e5bc761` stopped with useful evidence. All five accepted
Rust trial processes and their isolated servers exited zero. The change cases
verified public/stored prefix agreement, preserved their actual typed causes,
returned `inspect_execution`, retained the source and rejected a second `start`.

| Accepted case | Confirmed steps | Recovery count | Result | Total seconds, including setup/shutdown |
| --- | ---: | ---: | --- | ---: |
| Normal-a | 115 | 18 | Completed, complete final-site/position comparison | 262.28 |
| Site-a | 31 | 2 | `recovery_site_changed`; foreign dirt preserved | 127.95 |
| Materials-b | 45 | 7 | `material_unavailable`; 33 cobblestone removed by the fixture input | 141.76 |
| Position-b | 31 | 2 | `native_refused`, native `InvalidInput`; received recovery start differed | 105.25 |
| Connection-b | 31 | 1 | `native_refused`, native `Protocol`; explicit server kick | 119.89 |

The native categories above are the values supplied by Voxrig; error text is not
used to infer a different category. The position case exercised a changed
reconnect start, not an exhaustive in-flight motion disturbance campaign.

The first materials-a attempt is **excluded** from change acceptance: its console
command landed while the player was absent during recovery. The server returned
`No player was found`, the stock was not changed and construction completed. The
test exited 101 because the intended stop did not occur; its server exited zero.
No product failure is inferred from that unapplied input. The corrected fixture
controller waits for positive server command application. It retries only an
external fixture input explicitly rejected for absent player, after a new login;
an unknown result never permits resending a construction operation. Connection-b
exercised this rejected-input path once. All three corrected player-targeted
trials record the actual successful server receipt and its time.

The focused offline survival run passed 16 tests, with three explicit live cases
ignored. Both strict all-target Clippy configurations, workspace formatting and
diff checks passed. These tests validate declared sequences and stops; they do
not establish universal crash recovery, instant interruption or empty server
queues. No production execution or Voxrig code change was needed.

[Hashed trial captures, controllers and excluded-input evidence](evidence/survival-continuous-20261005.md)
retain the exact source, binary/JAR/runtime hashes, commands and outputs. The
subsequent stages also completed: [search failure diagnostics](search-failure-diagnostics.md)
records stage 4, and [public live acceptance](public-live-acceptance.md) records
stage 5. Those results are separate from this stage 3 survival campaign.

## Stop conditions

Stop before a prerequisite changes Voxrig/DustRoute responsibilities, adds a new
native guarantee or physical mechanism, introduces a server MOD, or turns a
diagnostic stop into universal automatic recovery. Retain and report bounded
failure evidence rather than weakening the existing execution contract.
