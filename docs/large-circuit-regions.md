# Region-based circuit work

The practical target is to prototype and revise circuits containing thousands
of blocks in a completely observed context, with bounded, separately reviewed
work regions. A hierarchy summary is not a whole-circuit behavioral proof.

Migration order:

1. Expose observation limits/evidence at the backend boundary. Native Voxrig
   reads reconstructed client state; the optional Mineflayer bridge checks
   command predicates. Its 8,880-cell command limit must not govern Voxrig.
2. Make explicit coordinate capture independent of gaze. Still resolve the
   assisted player's actual dimension and require every requested cell loaded.
3. Persist region work intentions, immutable before/target states and verified
   progress. Keep only the current region's executable model proof in memory.
   Plan, preview, apply and read back each region against the **whole** context.
   After restart, reacquire observations and generate a new operation.
4. Verify cross-region support, observer precedence, notifications and power,
   drift, ambiguous writes, restart and superseded previews. Measure realistic
   workloads before raising remaining model or transport limits.

The initial job path uses literal Circuit Revisions at their captured site:
at most 4,096 non-Air blocks, 4,096 virtual changes, 64 disjoint work regions,
and 64 changed coordinates per region. The full live context includes the original
one-cell guard and must fit both policy and adapter limits. Regions cover every requested change;
all other observed cells are protected unless explicitly admitted by edit_scope.
Support/watch dependencies determine a candidate region order. A dependency
cycle needs a different partition; ordering alone never proves construction.
Each current region gets fresh forward and inverse common-runtime verification
in the full context, including intermediate protected states. Later regions
remain unverified until planned. A coupled circuit may require changing the
partition or declared intermediate design; the tool does not guess such a design.

Jobs are durable intentions/history, not restored validation capabilities.
Failed or uncertain attempts stop with needs_inspection. There is no automatic
retry, rollback, chunk loading or world lock. Undo must proceed in reverse region
order and receives a fresh reviewed operation, including after restart. A
cancelled job retains its history and makes forward previews unusable. Its
verified prefix can still receive fresh inverse cleanup plans; cleanup does not
reenable forward work.

Fresh-target adopted Assembly construction, building generators, whole-circuit
functional budgets, entities, survival inventory and arbitrary unloaded worlds
retain their existing separate limits. This migration does not certify arbitrary
large circuits or atomic server execution.

## Adapter audit

| Contract | Result |
| --- | --- |
| 8,880-cell command confirmation | Kept exclusively as the Mineflayer observation capability; native Voxrig reports its 262,144-cell loaded-region limit. Both are distinct from user policy. |
| Gaze required even for explicit coordinates | Removed. Native coordinate capture uses received player/dimension context, without a raycast or guessed eye pose. Compatibility capture ignores the incidental target. Gaze/discovery options cannot be combined with explicit region. |
| 256-block generic gaze option versus native 64-block raycast | Adapter range is reported explicitly and checked before dispatch. It no longer restricts coordinate capture. |
| Dense transport arrays consuming a 4,096-block revision limit | Removed: native Air records are normalized before counting non-Air content. Revision base states and job intentions are saved sparsely with complete known bounds; raw observation receipts/content IDs remain unchanged. |
| 64 virtual changes | Replaced with a 4,096-change offline revision budget. Live work is still bounded to 64 changed coordinates per separately reviewed region. |
| Approximate generic baseline ignoring dynamic properties | Native literal revision edits now use exact full-context common-runtime state comparisons, including passive geometry. Mineflayer coexistence retains its ordinary unscoped compatibility route. |
| Predicate stair corrections and server clock evidence | Stay in the compatibility adapter. Native records remain client reconstructions; server confirmation is not fabricated. |
| Native packet pacing and local wait timeout allowance | Already native-specific; retained. Local waits use a client clock, not a server tick guarantee. |
| Whole-context readback, 32-write idle batches, stationary samples, protected microsteps, consumed attempts | Retained as construction/observation safeguards, independent of Mineflayer. Removing them would need separate physics and recovery evidence. |
| Fresh Assembly/building size budgets and exhaustive analysis budgets | Retained as separate model/generator limits. Region jobs do not silently change adoption, child pins or functional requirements. |

For a coupled power/support/watch layout, a region boundary is a work boundary,
never a cut in the physical world or queue. Model refusal is an unsupported
partition or intermediate target, not evidence of a broken Minecraft circuit.
An interrupted partial prefix is deliberately not reconstructed from its command
count: examine the full observation and author a new explicit repair revision.

## Verification

Offline tests cover cross-region support/watch precedence, protected power
changes and coupled power partitions, cycles, coverage/overlap limits, large
native-Air revision input, ownership/read-only enforcement, superseded previews,
explicit unchanged-baseline recovery, partial-write refusal, process restart and
reverse undo. Large-model cases and isolated native trials are recorded separately
from the transport fixture; the fixture does not serve as a Minecraft oracle.

The large passive model test adds 2,048 stone blocks across all 32 regions of
64 changes, generating and dropping each whole-context forward/inverse proof
in sequence. All six partition tests passed in 116.08 seconds in the debug
profile. This is construction evidence for a passive layout, not exhaustive
functional verification of a 2,048-block active circuit.

The six public job lifecycle tests also pass, including permanent forward
cancellation with restart and inverse cleanup. Electrical edit, revision and
bridge regressions pass. The full default MCP library regression run passed
144 tests with two opt-in tests ignored; its two outdated tool-count assertions
were updated and passed on individual reruns. Native MCP all-target Clippy and
translate-library Clippy pass with warnings denied. The unrelated translate
example's existing dead-code warnings are outside this migration.

The opt-in `tools/verify_blueprint_iteration_live.py --region-jobs` trial uses
a stopped, private Vanilla Java 1.21.11 server and two dummy native clients.
Its declared owned fixture checks gaze-independent 32,768-cell capture, two
40-change regions with a floor lever depending on support in the other region,
MCP process restarts, protected drift refusal, reverse undo and cancellation.
Console predicates independently confirm complete stable checkpoints and the
final empty fixture. These sequential predicates are test evidence, not an
atomic server observation or a production fallback from native reconstruction.
