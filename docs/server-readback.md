# Server-confirmed block readback

This document records the **retired Mineflayer backend** (removed 2026-10-03). The native Voxrig backend
has a separate [client-observation contract](voxrig-rollout.md): its normal scans
do not issue confirmation commands and cannot return `ServerReadback` or satisfy
the explicit `scan_region_confirmed` API. Shared workflows retain the selected
source in their durable evidence and do not silently change backend on failure.

The former bridge and device trials shared `mineflayer/readback.js` at `9b62dcf`.
Their [retained measurements](evidence/legacy-mineflayer/README.md) remain historical facts.
It confirms a client-derived candidate with Java 1.21.11 block predicates before
returning it as a current world observation. It does not take expected states
from construction plans or simulator output.

## Contract

- Every cell in the cuboid is checked, including omitted Air. Native block names
  and **all** properties are validated against the connected version's registry.
  `execute in <dimension> if loaded ... if block ...` checks the server state.
- Contiguous Air cuboids use a direct check of one cell followed by native
  `if blocks ... all` comparisons that expand the checked prefix. Each cuboid's
  entire proof stays in one command. Both comparison regions are inside that
  batch; no external reference area or template world is created. The tests
  change each cell in turn to ensure compressed predicates still reject it.
- Commands reply privately to the bot with a random nonce and a final reply
  fence. Only system messages with that nonce count. Permission errors, missing
  replies and disconnects fail the observation.
- Each successful predicate runs native `time query gametime` in the same
  command; its success flag and tick are returned from owned scratch storage.
  If predicate ticks differ, the entire confirmation is retried, up to three
  attempts. Missing or contradictory state also fails. Client ticks are not
  used as server timestamps.
- Stair shape disagreements can be resolved by independently querying the five
  native shape values while retaining the other properties and identity. Exactly
  one must match; the resulting **whole region must then be confirmed again**.
  A failed region confirmation probes at most 32 candidate stairs.
  Other identity/state disagreements remain incomplete observations. This is a
  bounded state query, not general reconstruction of arbitrary unknown blocks.
- The original client observation is preserved in live trial artifacts. A shape
  correction records the client state and the server-confirmed value; it does
  not edit the Mineflayer chunk cache or trigger Minecraft neighbor updates.

The receipt `dustroute.server-readback.v1` contains the requested region,
dimension, cell count, server time interval, request identity, command nonce,
snapshot SHA-256, per-predicate ticks and batch size, attempt summaries and independently
confirmed shape corrections. Earlier retained trial receipts used enclosing
native clock reads; they remain historical evidence of those earlier trials.
The Rust bridge requires a receipt matching its fresh request and exact region;
plain replies from an older bridge are rejected. The digest identifies the
original JSON-encoded bridge snapshot for audit, not remote cryptographic
attestation. It hashes the original object/property order; it is not a canonical
hash of an arbitrarily reserialized Rust snapshot.

## Public operations and persistence

`scan_region` and `get_block` now return confirmed data through the common bridge.
Existing callers therefore cannot silently fall back to client-only data when
checking a placement, repair or removal. No public MCP tool name is added.
Assembly construction stores baseline, before-write and after-write receipts in
the durable attempt; instance observations retain both receipts from the existing
two-sample stability check. A failed confirmation leaves the existing refusal or
`needs_inspection` behavior in place.
The requested sample interval is explicitly tagged as client ticks; the actual
server tick interval is retained separately. A backwards server clock fails the
combined observation.

The literal `MinecraftSnapshot` geometry format is unchanged. New receipt fields
on saved attempts default to empty for otherwise-compatible older records. An old snapshot or saved
receipt is historical evidence, never a fresh capability to write: existing
plans and restarted processes must obtain fresh confirmation. RPC request IDs
prevent accidentally treating a receipt from an earlier request as a new one.

## Operational limits

The bot needs command permission for `execute`, `time`, `data` and `tellraw`.
Readback changes no world blocks and does not freeze ticks. It temporarily writes
an owned random key in `dustroute:readback` command storage to return the native
time, then queues removal of that key. This is command-assisted observation;
there is no new companion MOD or automatic server configuration change.

The policy maximum scan region remains 262,144 cells. Command confirmation has
a separate conservative limit of **8,880 cells**, **192 commands per exchange**
and **30,000 characters per command**, all checked before sending commands.
The compressed complete region is checked in one predicate command. Dense or
fragmented states can hit the command-length limit and need a smaller region.
Earlier multi-command implementations rejected even matching sparse regions
when commands crossed server ticks; retained captures distinguish those versions.
These are explicit operational limits, not evidence
that the region is empty or that the mechanism is invalid. Large scans that
previously used client-only observations may now need a smaller selection.

Checks occurring in one game tick do **not** constitute an atomic world snapshot
or prevent another player from changing the world between commands or after the
read. The receipt also does not reveal pending block ticks, piston events,
inventories or motion history. Existing completion and concurrent-edit limits
remain; no stronger write transaction or safe automatic rollback is claimed.

## Evidence status

Ten declared stair cases and two readback rechecks match the simulator's declared
projection. The public 4,785-cell Assembly trial passed fresh adoption,
construction, three process restarts, stale stair correction and removal,
retaining 62 durable step receipts. The original failed capture and unsuccessful
readback implementations remain separate failure records. See the
[stair validation and evidence](stairs-runtime-readback.md) for exact versions,
the final correction-accounting regression, test results and scope.
