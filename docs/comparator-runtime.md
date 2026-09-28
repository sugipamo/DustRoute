# Circuit-input comparator in the shared runtime

The current Java 1.21.11 profile executes comparators through the same checked
Rust definitions, queries and ordered effects as repeaters and bulbs. Compare
and subtract are explicit construction settings. Manual mode switching is not
an admitted Use callback in this profile; no player, inventory, item-frame or
other entity inputs are simulated.

## Source and behavior

The cached Yarn build.6 target JAR was inspected with `javap -p -c`:
`ComparatorBlock`, `ComparatorBlockEntity`, `AbstractRedstoneGateBlock`,
`World`, `ServerWorld` and
`RedstoneWireBlock`. The finite signal law accepts rear and side levels 0–15.
Compare emits the rear level when it is at least the side level; subtract emits
saturating rear minus side. This is a distinct Law from the historical
compatibility comparator, whose input domain and boundary semantics differ.

Rear input includes emitted power and raw wire strength. A directly adjacent
comparator-readable block overrides that input, including a zero readout. If
normal input is below 15 and the rear block conducts, readout can come through
that one block. Side inputs include raw dust, redstone blocks and direct strong
power. The comparator has dust connections on every horizontal side; repeater
connections retain their axis rule. Both gate kinds can lock a repeater and
participate in the output-alignment priority query.

Neighbor updates compare the desired output with the stored signal and POWERED.
They schedule a two-game-tick update unless a tick is in the collected batch.
Priority is HIGH for a misaligned output gate, otherwise NORMAL. The tick writes
the internal signal before any POWERED write, drains that write's shape callbacks,
and then notifies the output. Compare mode notifies even when signal is unchanged;
subtract mode skips the remaining update when its stored signal is unchanged.
This also retains the unusual fresh `powered=true,mode=subtract`/zero-input case:
the Boolean can remain true while the actual emitted signal is zero.

Bulb flags-3 writes now route `World.updateComparators` after ordinary neighbor
callbacks and before shape callbacks. Each direction is selected from the then
current world after the previous receiver returns. Command postprocessing also
routes readout notifications for the installed block. Direct and one-conductor
readout routing use the common notification infrastructure, including checkpoints.

## Internal state and evidence boundary

A bounded output register belongs to the synchronous world runtime, separate
from `Block.power_level` and observed block properties. Zero is the explicit
fresh-construction value. A different block identity clears the old register;
ordinary POWERED changes preserve it. Writes are transactional, range/identity
checked and recorded in `RuntimeRecord.output_changes`. Checkpoints, exact keys
and root-exploration comparison states include it. The immutable checkpoint and
behavior-state APIs remain process-local; this is not a new disk import format.

An electrical query built from only a `World` cannot infer comparator output;
it reports an error when emission needs that missing runtime state. The fresh
runtime constructor is deliberately a construction model, not resumption of a
running world captured as block states. Resumption requires its checkpoint or
behavior state. Snapshot roundtrip preserves facing/mode/powered but carries no
block-entity output. Snapshot equality and ordinary live readback therefore do
not prove equality of internal comparator output. No NBT reader or public MCP
operation was added, and no new live parity claim is made.

The shared command exporter reads the typed mode setting and never emits an
invented `power`/`outputSignal` block property. Existing historical generic export
and compatibility execution retain their contracts. Moving a comparator as a
piston payload remains rejected. Initial block-only observations do not authorize
inventory/entity behavior.

## Revisions and verification

Execution/root exploration is v12, device definitions v6 and physical admission
v3. The synchronous runtime record is v4 and root comparison v2. Earlier saved
contexts require fresh review; no checkpoint conversion is provided.

Tests cover all 512 arithmetic input combinations, nonbinary circuits, four
orientations, side locking, priority/collected-tick decisions, direct/read-through
readout precedence, insertion/removal, range/identity rejection, every microstep
of bulb-driven transitions, hidden-state comparison and checkpoint restoration.
Translation tests cover exact block-state roundtrip and construction/teardown.
These tests and the target-bytecode audit are model evidence, not new live trials.
