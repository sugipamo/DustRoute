# Observation contents and acquisition identity

The live acquisition boundary shares exact immutable contents without sharing
freshness or write permission. `ContentId` is a SHA-256 digest of the versioned
canonical literal snapshot: bounds, sorted positions, native names and sorted
properties. Hash input uses fixed-width big-endian coordinates/counts and
length-prefixed UTF-8 names, property keys and values; it does not regenerate
JSON. Explicit air records remain in the hash; an omitted position is
not silently filled with air. Coverage/completeness claims still require the
transport receipt and the caller's checks.

`SharedSnapshot` can only be constructed by the bridge's `SnapshotContents`.
Cloning a stored circuit copies a reference instead of the block vector. The
interner compares actual canonical payloads before accepting a hash match;
different payloads with the same hash produce an error. Weak entries retain
the identity of every live shared payload and reclaim released payloads.
Pointer shortcuts require an upgraded weak source reference and exact pointer
equality, so allocator address reuse cannot identify a different snapshot.

`ObservationId` identifies one acquisition. Its receipt independently retains
connection, dimension, receive sequence, received revision, client revision,
local frame and capture time. Two acquisitions can have the same `ContentId`
and share storage while having different observation IDs and receipts.
Neither ID constructs a fresh capability. Archive DTOs still deserialize only
as owned records, and their JSON snapshot shape remains unchanged.

Public `circuit_id` values remain per-owner, expiring UUIDs. Content identity
does not bypass ownership, expiry, mutation policy, preview or confirmation.
Explicit `get_world` captures expose both content and observation IDs;
`test_circuit` and `get_circuit_ir` expose content identity. Mixed-IR
`analysis_id` is a typed `ValidationKey` over content IDs, model/context pins,
physical admission revision, operation scope, dimension, bounds and
completeness. Analysis still runs; this change does not cache proof results.

## Native materialization

Voxrig's new `observe_shared_client_region` API shares immutable received and
reconstructed cell arrays. Calls are serialized with packet application under
the session lock. Before cache lookup, the reconstruction advances to the
current local frame. Every acquisition constructs fresh metadata even when
its arrays are reused. The existing owned observation API remains available
as an explicit compatibility conversion.

Reuse requires exactly the same region and the same global received-world and
reconstruction revisions. Issues and pending recovery clear reuse even if the
revision did not change. Unknown cells are not retained as usable cache
entries. Block/chunk updates, unloads and dimension changes invalidate by
world revision; active carrier progression invalidates by reconstruction
revision even without new packets. Failed/disconnected sessions are checked
before acquisition.

The native region cache and DustRoute's conversion cache each retain at most
16 regions and 65,536 cells. Larger valid observations bypass retention.
Cache entries are an optimization and may be evicted. The content interner
keeps one live shared payload regardless of these evictions; an evicted
region can still need materialization again. Received and reconstructed
arrays remain separate because their provenance and moving state differ.

DustRoute reuses a converted snapshot only for the identical immutable source
array. Version, dimension, region, receive boundary, issue and recovery gates
are checked on every conversion. The complete-cell and coordinate checks
are performed before any source is admitted to the conversion cache.

## Measuring improvement

The existing opt-in request trace now includes `content_intern`,
`materialized_cells` and `cache_hits`. `scan.cells` remains the requested
volume, including cache hits. For `native_observe`, materialized cells count
the region cells decoded on a miss; for `native_convert`, they count snapshot
cells converted; for `content_intern`, they count explicit records hashed.
A hit reports zero work cells in that phase. These are per-request counts,
not receive-packet counts or independent server observations.

Offline probes compare the previous native double-decode loop and conversion
against repeated shared acquisitions, and rerun the production handler
fixture from the earlier performance investigation. They do not connect to
Minecraft. A separate 55-call live dummy-player run confirms reuse on a
stationary loaded scene: every repeated scan hit, with zero repeated
materialization, conversion or hashing and fresh explicit acquisition IDs
and advancing delayed receipts. Results and limits are recorded in the
[performance document](performance-observation.md#live-native-read-speed-with-a-dummy-player).

Remaining work includes finer dirty-region/section invalidation, sharing
overlapping regions, avoiding owned copies in remaining compatibility APIs,
and deciding whether pure model validation results should be cached with
typed keys. Global revision invalidation is intentionally conservative:
unrelated world updates can reduce the hit rate. Stationary intervals,
settling waits, model proof requirements and durable storage are unchanged.
The running older MCP process has not been replaced by this change.
