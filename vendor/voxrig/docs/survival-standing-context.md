# Java 1.21.11 stationary standing context

This is a prerequisite for survival construction, not a walking or timed-mining
implementation. The explicit 1.21.11 adapter exposes `standing_context()` and
adds `local_player` to `player_state()`. These are client observations and
projections, not a server-current-state guarantee or an action capability that
can be saved and reused after reconnecting.

## Own-player state

The login entity ID binds native health, velocity, pose metadata, attributes and
status-effect updates to the bot. Another player's packet cannot update this
state. Health is unknown until received. Unknown metadata serializers invalidate
posture. Supported attribute/pose parsing is shared with remote-player tracking.
Native modifier order is additions, factors of the adjusted base, then factors
of the running total, followed by the attribute's native bounds.

New-world standing posture and the four supported attribute defaults explicitly
carry `NativeReset`; received updates carry their packet sequence. They are
native client initial values, **not received proof of server defaults**. Supported
attributes are scale, block break speed, mining efficiency and submerged mining
speed. The pinned data package's attribute mapper is inaccurate for several of
these IDs, so the native registry/default container is the comparison authority.
Health, velocity and effect updates are cleared on a world reset; reconfiguration
also clears the entity identity.

Explosion/vehicle-motion packets and a received passenger list containing the
bot invalidate stationary motion. Their motion consequences are not simulated;
the interruption retains packet ID/sequence and requires a fresh world baseline.
A later zero velocity alone cannot prove dismount or repair unmodeled position.
Unsupported explosion particle/sound payloads are not decoded here, and cannot
restore authority even if malformed. Other players' passenger lists do not
invalidate the bot. This is a refusal boundary, not support for riding or combat.

The new packed native velocity format is decoded, including extended scale.
Absolute position corrections retain velocity; ordinary relative velocity axes
require an existing baseline. Rotated relative updates with nonzero/unknown prior
velocity deliberately make velocity unavailable: native angle-table rotation
needs a separate implementation/comparison. They never retain a stale zero.
Native correction pitch is clamped to -90..90.

Effects retain native ID, amplifier, flags, duration **at receipt**, and sequence.
No expiration or complete-list fence is inferred. `effects_complete` remains
false; an empty update map is not proof of absence. These records do not yet
authorize a mining duration. Tools, effects and changing conditions still need
the later timed-mining admission and progress/result reconciliation.

## Derived ground and posture

`standing_context()` requires known standing posture, scale 1, a received feet
position and a resolved zero velocity, with no active flight. It rejects a known
dead player. It reads reconstructed blocks and checks the native standing body
(float width 0.6, height 1.8, eye height 1.62), clearance, and a downward contact
probe. Missing chunks, moving carriers, incomplete reconstruction and dimension
boundaries reject the query. It includes connection/receive/frame/world revision
provenance and the solid support cells. Support is recomputed after edits; it is
not an independent ground acknowledgement from the server.

The first geometry scope admits air/cave air/void air and these dry full cubes:
stone, dirt, grass block, cobblestone, oak/spruce planks, quartz block, smooth
quartz, white concrete, glass, andesite and granite. A one-cell halo refuses
unsupported protruding/context-dependent neighbors. Fluids, slabs, stairs,
climbable blocks and entities are not simulated by this bounded query. Admitted
states establish `submerged: false`; unsupported fluid geometry never becomes a
false dry reading. Every admitted block state was checked against native dry
full-cube collision geometry.

Survival `look()` rechecks this context under its session lock and sends the
derived ground bit. Unknown/moving/unsupported context refuses **before** any
packet or rotation change. A known unsupported-in-air context sends false; it
does not simulate falling or establish a stable work position. Creative flight
retains its separate behavior. Initial teleport confirmation also remains
ungrounded; a correction alone, often received before chunks, cannot establish
standing contact. Sending a ground bit is not server acceptance.

Low-level `use_on_block` is still a packet submission API. This change does not
make it a validated survival placement or add a survival building executor.

## Validation

`scripts/VerifySurvivalFoundation.java` invokes the target game's own APIs for
standing dimensions, the four attributes' IDs/defaults/limits, all admitted cube
states, packed velocity encode/decode, look encode/decode, and native voxel-shape
contact results. It generates `data/java_1_21_11/survival_foundation.json`;
`survival_foundation_source.json` records source/tool/output hashes. Run against
the locally prepared oracle/classpath from `export_outline_shapes.py`, with the
fixture path followed by `--check` to verify rather than regenerate. Only
package remapping and access flags differ; native method bodies are unchanged.
No Minecraft binary, mappings or decompiled source is redistributed.

Seven Rust tests cover native numeric/packet comparison, atomic own-player receive
and remote isolation, resets and unsupported posture, native attribute modifier
order, edge/floating/body-obstruction contact, removed support, negative/chunk
boundaries, fluid/motion/reconstruction refusal, and a real loopback TCP look
packet with a no-send refusal, plus unmodeled impulse/own-vehicle invalidation.
These offline tests and native API comparisons
are not a live Minecraft survival construction trial.
