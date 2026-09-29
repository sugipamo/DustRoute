# Java 1.21.11 client piston reconstruction

The Java 1.21.11 adapter can now apply ordinary and sticky piston block actions,
retain independent moving body/head/payload state, and update dry stairs and
supported mounted attachments and redstone wire geometry. This is a bounded client implementation. It is not
a server simulator or a server-confirmed observation API.

## API and ownership

- `Client::observe_region` still returns only received block states. Its behavior
  and the original stale-stair evidence remain available for diagnostics.
- `Client::observe_client_region` returns that received snapshot alongside client
  states, per-cell provenance, moving-block metadata, dimension, local frame and
  reconstruction revision. Both views are sampled under the same session lock.
- `StateOrigin::ClientUpdate` retains the causal block-action receive sequence.
  `ReconstructionIssue` and absent cell states expose incomplete observations.
  Callers must check both the issue and cell availability.
- Native updates supersede local effects at their coordinates and retire obsolete
  carriers. Delayed local completion cannot resurrect a server-removed block.
- World/configuration resets discard reconstruction. Replacing or unloading a
  chunk used by local effects invalidates reconstruction conservatively. The
  `recovery_chunks` field identifies the dependency chunks whose fresh full
  snapshots must arrive before recovery. New actions during recovery require a
  fresh baseline for loaded chunks. Other unsupported behavior still requires a
  fresh world/connection; fresh chunks cannot clear unsupported clock semantics.

`reconstruction/rules.rs` declares supported block classes. `motion.rs` handles
block events and carrier lifetimes; `reconstruction.rs` owns the overlay,
provenance, bounded shape updates and local frame advancement. Received chunk
storage is not mutated by reconstruction. No DustRoute dependency was added.

The native state codec resolves complete properties back to the pinned registry;
every bundled 1.21.11 state is round-tripped in tests. Block IDs used by block
actions remain distinct from state IDs used by state updates.

## Movement semantics

Each carrier has its own carried state, facing, extension flag, role, progress,
previous progress, completion-wait count and causal sequence. Body retraction
uses a carrier at the body coordinate. Payload carriers are independent.

Moving-piston chunk NBT is decoded and validated against native state properties.
These carriers have `chunk_sequence` and no `action_sequence`; local completion
uses `StateOrigin::ChunkUpdate`. The received `progress` is native serialized
previous progress, used as both local progress values as in the native reader.
Absent NBT remains unavailable, and malformed NBT cannot commit a partial chunk.
Fresh chunk arrival is a passive recovery mechanism; vanilla does not provide a
general client request to resend arbitrary dependency chunks.

Progress advances through 0, 0.5 and 1. In the inspected client implementation,
five additional completion checks wait before materialization. Forced completion
materializes a payload but removes a source carrier. Retraction event 2 skips
pulling; event 1 also leaves a still-extending matching payload behind after
forced completion. Six directions and both piston types are exercised in tests.

Local frames use elapsed monotonic time at 20 Hz. They advance when receiving a
packet or requesting a client observation. At most eight steps are needed to
settle existing carriers; this implementation does not run server redstone timers.
Packet records retain the frame at application so frame/packet interleaving is
replayable. These frames and progress values are **not server tick measurements**.
Non-default tick rates, frozen worlds and positive tick-step requests invalidate
reconstruction. The normal initial zero-step packet is accepted.

State updates and block actions use their distinct notification behavior. Native
state packets are not treated as arbitrary shape-change notifications. Piston
effects run bounded, synchronous shape updates in the inspected order. Unknown
consumed rules, missing chunks or resource limits reject the local transaction;
no partial result is published as usable. The received cache remains readable.

## Current scope

Supported movement materials include slime/honey groups, stone, cobblestone, quartz block, smooth quartz,
glass, redstone block, observer, redstone lamp, dry stone/cobblestone/quartz/smooth
quartz stairs, all sixteen wool colors, and unextended pistons. Air variants, bedrock, obsidian, piston
heads, moving pistons and levers have explicit roles. A lever in a pushed line
can be destroyed; dry stair shape changes and supported lever attachments have
client shape rules. Piston-head attachment checks are included. Lever support on
a piston head is not claimed and returns an unsupported-state issue.

Wire, repeater, comparator and listed button states have explicit destruction
and support callbacks. Wire geometry distinguishes full supporting faces from
solid-block conduction, preserves dot/cross behavior, and propagates diagonal
changes through the old/new wire prepare callbacks. Power, gate delay/locking,
observer pulses and scheduled redstone remain owned by received server updates.
Torch center support, trapdoor wire climbing and other unlisted shapes remain
unsupported; no general shape coverage is implied by the material table.

Not yet implemented: other block callbacks,
waterlogged movement, fluid/entity effects, non-default client ticking, and full player movement
or inventory for 1.21.11. Unknown moving carriers yield unavailable client state.
These limits apply to reconstruction, not the broader native-state receive codec.

## Source and validation

The behavior was inspected in the locally mapped Minecraft Java 1.21.11 / Yarn
build.6 classes `PistonBlock`, `PistonBlockEntity`, `PistonHeadBlock`, `StairsBlock`,
`World`, `ClientWorld` and `Block`. In particular, movement writes and packet
writes use different flags; client completion differs from server completion.
Decompiled source is not included in the repository. The implementation does not
copy DustRoute's server runtime or import its dependency graph.

On a fresh run of the isolated official server (offline-mode, loopback 25577),
three retained trials passed independent final-state checks for **all 770 cells**:

| Trial | Input spacing (wall time) | Received piston events | Result |
| --- | --- | --- | --- |
| b | Ordinary piston, 800 ms | Extend, retract | Final region matches server |
| c | Sticky piston, 800 ms | Extend, retract | Pulled payload and final region match |
| d | Sticky piston, 70 ms | Extend, drop retract | Payload left behind; final region matches |

The received stair stays `inner_left`; the reconstructed stair becomes `straight`
and matches the server. Body/head/payload intermediate states were sampled.
Saved packet/frame interleavings replay the sampled states and final regions.
This does **not** establish continuous equality with server moving-block state or
with a graphical vanilla client's frame timing. Vertical directions are covered
by source-based unit tests, not these three horizontal live trials.

The diagnostic server function checks all cells/properties, including air, with
short early-return predicates in one invocation. It performs no block writes and
is used only for independent validation. It does not enable command-free adoption
in DustRoute. `scripts/prepare_motion_confirmation.py` reproduces these functions.

Retained failures and limits:

- Trial a rejected the server's initial `STEP_TICK=0` as unsupported ticking.
  This was corrected and regression-tested before trials b/c/d.
- The first verification function chained 770 conditions into one command and
  caused a **Java command-parser StackOverflowError** while loading the diagnostic
  function. The server continued running. Short per-cell checks corrected this;
  the failed diagnostic and server log remain local/retained evidence. This is not
  a claim of a host kernel fault or a piston implementation failure.
- Trial d logged setup lag before capture. Wall-time waits are not claimed as
  precise server tick input schedules.
- The later missing-carrier guard initially also rejected completion of a known
  carrier after a moving-state packet. The targeted regression caught this and
  carrier retirement was corrected before final validation.

[Evidence manifest](evidence/client-motion-20260929.manifest.json) includes all
four captures, checksums, server log, successful confirmation functions and
cleanup status. Full captures retain received and reconstructed views; compact
replay fixtures select relevant intermediate cells and preserve the full final
region. The isolated fixture was cleared, all four owned force-loaded chunks
released and the server stopped normally.

DustRoute's active bridge/readback is unchanged. The earlier unresolved 1.16.1
two-bot movement trial is still documented in [version validation](version-adapter-validation.md).

Final checks passed: 85 unit tests and all example targets, one doctest,
all-target Clippy with warnings denied, formatting, package-file listing and
pinned registry/packet generation verification. Cargo ran sequentially with one
build job and one test thread. Package listing is not a package build.

## Native chunk recovery follow-up

The next implementation adds the chunk NBT and recovery contract described above.
An isolated ordinary sticky-piston clock (8 ticks on / 8 ticks off, 20 TPS, no MOD)
was observed through fresh connections. The third corrected connection received
native half-progress retracting body and payload NBT and continued observing
without an issue. All packets and client frames replay the retained observations.
An initial failure showed that `Direction.INDEX_CODEC` writes Byte NBT; the decoder
and tests were corrected to accept this native representation. Failures are kept
alongside successful captures in the [recovery manifest](evidence/client-recovery-20260929.manifest.json).

Dependency refresh, unload during refresh, interleaved actions, clock errors,
truncated/duplicate/invalid NBT and source roles are unit-tested. This does not
claim a live comparison of every unload/reload scenario or continuous server
current-progress equality. Owned schedules and force-load were cleared, the
fixture removed, and the server stopped normally.

Follow-up validation passed 92 unit tests, all example targets, one doctest,
formatting, all-target Clippy and package listing. Logs are retained locally as
`.local/voxrig-recovery-{all-targets,doc,clippy,package}.log`.

## Adhesion follow-up

The ordered planner now follows side attachments and chains behind adhesive
blocks, separates slime from honey, admits at most twelve moved blocks and
reorders colliding branches. Retraction can leave a group behind when its path is
blocked. Carrier installation precedes source clearing; source shape updates
follow the native position-map order. Unsupported rules still invalidate the
transaction instead of guessing a result. Entity transport is not included.

Tests exercise six directions, both adhesive materials, reverse chains,
obstructions, the 12/13-block boundary and all 128 occupied subsets containing
the root of a 2x2x2 slime volume. Two isolated native trials add an adhesive
upper branch and opposite-material neighbor to the earlier stair fixture.
Both final regions match **all 770 cells** independently on the server. The
retraction also pulls a stair newly adjacent to the moved adhesive group.
The original and compact captures are in the
[adhesion manifest](evidence/client-adhesion-20260929.manifest.json); deterministic
replay checks the client intermediate samples and final cells. These trials do
not establish every interacting-piston or flying-machine configuration.

## Mixed reference circuit and live reload

`client-reference-door-20260929.manifest.json` records the user-supplied one-wide
3x3 reference door (10 pistons, vertical and horizontal) in the same isolated
vanilla server. Two close/open cycles fill/clear all nine aperture cells; both
open samples restore the exact initial region. Separate closed and reopened
trials, each after reconnect, and the final post-reload open trial each pass
independent server checks of all 770 cells. Intermediate packet/frame samples
replay; they do not establish continuous native server progress equality.

The initial trial failed on the lever above a downward piston: moving blocks
had been assumed to provide no support. `PistonBlockEntity.getCollisionShape`
retains the stationary extended base during source retraction. Its back face
therefore remains a full supporting face, and at progress one the head fills
the remaining quarter. That rule is now covered in six directions. Full-cube
payload faces at half progress are handled separately. Unsupported intermediate
non-cube payload support still returns an explicit issue. During native
setBlockState callbacks before carrier installation, collision is empty.

After teleporting the client 256 blocks away, its dependency chunk is unloaded:
all 770 requested cells become unavailable, with the required chunk explicit.
Returning delivers a fresh chunk and restores availability. The capture includes
that failure/recovery interval and is replayed with the live 1024-chunk cap.
A first replay used the earlier 64-chunk harness cap and failed; it is not a
client data-loss result. Other unrecoverable issue classes remain documented.

The fixture uses strict initialization solely to establish the reference input
state. It does not certify live construction order. Setup, command assertions,
cleanup and normal server shutdown are recorded. The probe now retains compact
intermediate samples and full initial/settled/final regions to avoid large
duplicate air-cell allocations. No server MOD was used.
