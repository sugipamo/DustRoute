# Java 1.21.11 ordinary survival placement

`place_survival_cube(support, face)` submits one stationary main-hand block use.
`observe_survival_placement` / `wait_survival_placement` inspect the retained
intent without resending. The API does not grant site or Blueprint permissions.
It requires common loading, received survival mode, healthy dry standing contact,
a received plain selected stack, the current first native outline hit on an
admitted passive cube, adjacent received air and clearance from the player's body.
Hit cursor coordinates come from the same native rotation/outline calculation.

The admitted materials are stone, dirt, cobblestone, oak/spruce planks, quartz
block, smooth quartz, white concrete, glass, andesite and granite. Grass block is
valid ground but not a state-free placement material. The local native oracle
checks item/block identity, single-state defaults, inherited passive placement
methods and all six face packet encodings. Shape admission alone is not the
placement contract. There is no item creation, teleport or mode switch.

The intent is stored before I/O and survives cancelled or failed dispatch. While
unresolved, all user mutations are blocked. A read-only successful result requires:

- A new target-specific block/section receipt matching the expected cube.
- A new selected-slot receipt showing exactly one consumed material, including
  the last item becoming empty. Local prediction never changes received counts.
- The native processed interaction sequence, which alone proves no successful
  placement. Native passive BlockItem use is synchronous; no delayed miner state
  is created by this packet.

World/generation/position/mode/selection changes, unsupported inventory, affected
chunk replacement and conflicting block or material updates latch inspection.
A later matching update cannot erase an intermediate conflict. Timeout or
cancellation is pending, not rollback or permission to retry. Closed history
retains the attempt. Only observed completion releases this one-shot gate, and
each subsequent placement rechecks its own predecessor and current geometry.
Raw `use_on_block` in survival refuses callers in favor of this checked path;
creative packet submission retains its existing contract.

The result reports observed outcomes, not attribution to an actor or a guarantee
against future external edits. Server-side entity collisions, protection and
other rejection can leave an unresolved attempt; they never become assumed
success. No general entity simulation, walking, stairs/slabs, replaceable plants,
fluids, automatic recovery/replay or autonomous Blueprint construction is added.

## Native source basis

The unchanged-body oracle provenance is in
`data/java_1_21_11/survival_placement_source.json`. Original helper
`scripts/VerifySurvivalPlacement.java` invokes native registry/reflection and
packet codecs. No decompiled game method bodies are distributed.

Source audit shows `ItemPlacementContext` chooses the adjacent face cell for a
nonreplaceable clicked cube. `BlockItem.place` checks placement/collision, applies
the block synchronously and decrements the stack once outside creative mode.
`ServerPlayNetworkHandler.onPlayerInteractBlock` processes the one-shot use and
sends clicked/adjacent block updates; its later tick emits the accumulated sequence
response. This processed marker is used with the two independent received results,
never as success by itself. The dedicated native comparison records actual
inventory synchronization rather than assuming the server echoes a packet.

## Native comparison

The [raw comparison](evidence/survival-placement-20261002-a.json.gz),
[server log](evidence/survival-placement-server-20261002-a.log) and
[provenance](evidence/survival-placement-20261002-source.json) pin execution to
`c230027c02ed1e7b5deae3ddd05b7fce642f3906`. A non-OP survival bot received three
dirt in main inventory, moved them to its hotbar with an ordinary verified swap,
and placed three blocks on the same connection (two top faces, then one side).
Every placement had an independent observer result and selected-stack decrement,
ending with both main and hotbar slots empty. Empty-hand retry was refused.
Observed completion took 81, 80 and 100 ms respectively on this localhost fixture;
this is not a throughput benchmark for real construction. The fixture is stopped.

Six added offline tests cover admission, independently encoded packets/materials,
receipt order, partial outcomes, processed sequence, last-item consumption,
conflict latching, chunk/mode changes, cancelled dispatch and retained history.
The full source suite passes 161 tests (4 opt-in tests ignored). This milestone
has no walking, access planning, durable Blueprint job or complete build acceptance.
