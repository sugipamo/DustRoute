# Java 1.21.11 survival inventory swaps

The explicitly selected 1.21.11 connection now observes the active container,
player-screen revision and cursor, in addition to the native player slots. It
supports one ordinary SWAP click between a main-inventory screen slot (9..35)
and a hotbar index (0..8). Both occupied and empty hotbar destinations are
supported. This exchanges whole plain stacks; it does not split, craft, retrieve
from chests, create items or change game mode. The 1.16.1 API remains separate.

```rust,no_run
use std::time::Duration;
use voxrig::{Client, Result};

async fn transfer(client: &Client) -> Result<()> {
    let operations = client.java_1_21_11_operations()?;
    let submission = operations.swap_player_hotbar(9, 0).await?;
    let received = operations
        .wait_inventory_swap(&submission, Duration::from_secs(3))
        .await?;
    println!("received swap through {}", received.receive_sequence);
    Ok(())
}
```

The caller must wait for a received survival mode, player screen, revision,
empty supported cursor and known source/destination contents. Default stacks
must respect the pinned native registry stack limit. Unknown components or
unfinished creative writes refuse admission. Empty/identical slot pairs are
rejected as unnecessary, since the server need not send changed-slot packets.

A pending marker is stored before sending. The client does not swap its local
slots, increase revisions or fabricate acknowledgements from that send. A
second inventory swap, held-slot selection, creative item write or use-on-block
is refused while it is pending. The consumer must similarly avoid using other
APIs such as operator commands to interfere with unresolved inventory work.

`wait_inventory_swap` sends no packets. It requires **both** destination slots to
have received sequences strictly after submission and exact expected stacks,
with the supported empty cursor and player window still available. Its returned
record is client receive evidence, not independent server confirmation. Timeouts
and cancelled futures leave the marker pending; resuming the same read-only wait
on the owning connection can collect a late result. Waits are bounded to 30
seconds. A different connection or a reset/mismatched intent is refused. History
is serializable for inspection but is not deserializable into a new capability.

If the result remains different or unavailable, inspect it without another
click. There is no automatic compensating swap, replay or permission to restore
an old mutation across a reconnect. The controller's later recovery plan must
start from newly received state. The current API deliberately does not clear a
pending conflict merely because a later full inventory snapshot arrived.

## Native protocol audit and validation

The packet sends container 0, the received revision, clicked main slot, hotbar
button and SWAP action. It sends an empty modified-hash map and empty cursor hash,
so no predicted stack hashes suppress the actual server slot updates. Inspection
of the target's `ServerPlayNetworkHandler.onClickSlot` confirms that it retains
the earlier client baseline for unreported modified hashes and sends content
updates after the click. A **stale revision does not prevent mutation**: native
processing performs the action and then resynchronizes the inventory. Therefore
the revision is a synchronization field, not a transaction lock or retry token.

`VerifyInventoryClick.java` verifies three payloads using the target game's own
encode/decode codec, including the VarInt boundaries 0, 128 and 32767. It uses a
locally obtained development oracle JAR with package remapping and access flags
widened; method bodies are unchanged. No game server is started by this check.
Use the classpath preparation described in `scripts/export_outline_shapes.py`,
compile the verification tool against that oracle, and run it with
`data/java_1_21_11/inventory_swap_packets.json` as its argument. The Rust admission
test compares its payloads with that same independently verified fixture.

Five new tests cover malformed/truncated inventory packets and atomicity,
cursor/window and stack admission, per-slot evidence freshness, window resets,
and a real TCP transport with partial updates, timeout, refusal of duplicate
mutation, resumed confirmation, reverse swap and connection ownership. This is
an offline receive/transport test, not a live survival-building trial. Current
1.21.11 survival walking and complete Blueprint construction remain
unimplemented. [Bounded mining observations](survival-mining.md) now exist;
observed removal still does not authorize continued construction.
