# Hypothetical circuit revisions

A `circuit_id` identifies an observed, immutable physical-world snapshot. A
`revision_id` identifies an immutable hypothetical snapshot. They are separate
namespaces: revision IDs cannot be passed as live circuit IDs, operation IDs,
repair sources, or direct placement authorization. `new_placement(revision_id)`
can instead create a separately validated placement proposal.

## API flow

`test_circuit_change` now creates and saves a revision. Supply exactly one
`circuit_id` or `revision_id` and up to 64 `changes`. An empty change list captures
an unchanged revision. Existing callers can still submit `circuit_id` and block
substitutions; the result now uses `dustroute.circuit-revision.v1` and
`analysis_mode: virtual_circuit_revision`, with validation under `validation`.

```json
{
  "circuit_id": "<observed circuit ID>",
  "changes": [],
  "simulation_ticks": 64
}
```

Edit or branch from any returned revision:

```json
{
  "revision_id": "<parent revision ID>",
  "changes": [
    {
      "position": {"x": 1, "y": 0, "z": 0},
      "block": "minecraft:stone"
    },
    {
      "position": {"x": 1, "y": 1, "z": 0},
      "block": "minecraft:repeater",
      "properties": {"facing": "north", "delay": "2", "powered": "false", "locked": "false"}
    }
  ]
}
```

Each edit replaces the entire block state. Omitted properties mean `{}`; they
are not inherited from the old block. Existing air may be replaced with a block;
`minecraft:air` deletes one. Every coordinate must be unique within the request
and inside the original observation bounds. The request is applied atomically
before structural checks: adding a support and its component together is valid.
Unchanged edits are omitted from the stored diff. Empty/no-op edits still create
a new revision ID. There is no implicit active branch or mutable current revision.

`get_circuit_revision({"revision_id":"...","include_snapshot":true})` returns
the full saved hypothetical snapshot, exact changes and stored validation.
Without `include_snapshot`, the full block list is omitted. Reading does not
rescan Minecraft or recalculate against a changed live world.

Responses also include `assembly_state`, a summary of the separate immutable
Assembly Revision when the snapshot is decodable. With `include_snapshot=true`,
`assembly_revision` contains its actual block records, known regions, pinned
Blueprint source occurrences, explicit connections and external boundaries.
Observations start unclassified: no source blueprint or connection is guessed.
Child revisions retain existing interpretations, replace actual state, and
record fresh modeled placement/connection checks under `validation.assembly`.
Logical meaning and behavioral evidence remain separate from connection types.

Blueprint Revisions are immutable source definitions. Assembly Revisions record
composed state, so a wire can change shape without rewriting its source. Native
block properties and declared wire connections survive storage without analysis
inference. The scanned cuboid is known space; absent cells outside it are unknown.
Loading rejects a modeled state that disagrees with its literal snapshot.
Legacy records and undecodable drafts may have no assembly. A draft that cannot
retain existing pinned interpretations is rejected instead of dropping them.
Assembly IDs and Blueprint IDs cannot be supplied as MCP `revision_id`,
`circuit_id` or `operation_id` values.

To retain an Assembly beyond the hypothetical revision TTL, explicitly call
`test_circuit_change` with `blueprint.action: "capture_revision"` and the saved
`revision_id` inside that object. This adds the exact record to the separate
Blueprint catalog; capture its parents first. Read it through
`get_circuit_revision(blueprint.kind=assembly)`. The [Blueprint workflow](blueprint-mcp.md)
also supports source imports and child-update proposals. Its local adoption
does not create a live placement plan or change this revision's literal snapshot.

## Identity and validation

Records contain `revision_id`, `parent_revision_ids`, `base_observation_id`,
owner, dimension, original observation completeness, snapshot, changes, and
validation. Revisions created directly from an observation have no revision
parents; derived revisions have exactly one. Two children of the same parent
are independent branches. The array form reserves room for future merge
ancestry, but multiple parents, merge and automatic conflict resolution are not
implemented.

Validation is attached to that exact version and reports before/after physical
and mixed-IR summaries, placement issues and bounded initial-state simulation.
Syntactically valid but structurally invalid or unsupported drafts are saved so
another revision can fix them. A translation error reports `unavailable` and
also preserves the draft. Incomplete observations or placement issues prevent
simulation. `structurally_valid` means only the modeled placement checks passed;
Java property validation is not exhaustive, and simulation is not proof of all
input behavior, functional equivalence or physical-world correctness. Supported
stationary piston/crop diffs additionally report common-runtime construction
checks; they do not gain functional proof from static diagnostics.

Limits are 4,096 virtual edits per call and 4,096 non-Air source/result blocks,
4 MiB per saved record and 1–256 simulation ticks (default 64), together with
existing player/dimension/region policy. Arbitrary bounds expansion and entities
are outside this API. Denied requests do not save partial edits.

## Storage and scope

Revisions use the existing scoped `PlanStateStore`, including restrictive file
permissions and atomic replacement of a newly generated UUID file. They survive
MCP process restart under the same configured state scope, independently of the
in-memory source observation. `DUSTROUTE_STATE_DIR` selects the store and
`DUSTROUTE_PLAN_TTL_SECONDS` controls retention (default one hour). Reads do not
extend expiry. This is bounded-lifetime working storage, not a permanent Git
archive: a parent can expire before its descendant, but each descendant retains
its own complete snapshot and base observation ID. There is no parent-chain
materialization dependency, revision listing, merge, or garbage-collection API.
Expired files are removed when loaded, following existing store behavior.

No Minecraft bridge call is made during revision editing or reading. Placement
proposal creation is a separate, live-validated path described below.

## Validation evidence

MCP tests cover branching without parent mutation, add/delete/property edits,
invalid support and subsequent repair, exact saved validation, persistence across
fresh service instances with no source observation, ownership, malformed input,
limits, duplicate coordinates, and rejection of revision IDs as live circuits or
operations. The tests use an unavailable bridge address so they cannot silently
fall back to observing or changing Minecraft.


## Reflecting a revision through existing placement tools

```text
new_placement({revision_id: "..."})
  → show_operation({operation_id: "..."})
  → invoke_operation({operation_id: "...", confirm: true})
  → undo_operation({operation_id: "...", confirm: true})
```

`revision_id` is an alternative to the built-in `circuit` parameter. It keeps
the original coordinates, uses the cumulative difference from the base
observation (not just the last parent diff), and disallows optimization. The
original bounds cannot expand. Empty cumulative changes are rejected.

New revisions retain `base_snapshot`, inherited unchanged by descendants.
Old records without this evidence remain readable/editable, but cannot be
placed; create a new revision from a fresh observation. This additive field
allows placement after the original in-memory observation expires, without
traversing expired parent records.

When available, the placement plan also carries `assembly.revision` and
`assembly.coordinate_origin`. Add this origin to an assembly position to obtain
world coordinates. A revision-based plan uses zero because its saved positions
are already world coordinates. Built-in plans retain their locally composed
state and use the placement origin. This is proposed circuit state, not the
surrounding overlay context or evidence of a completed Minecraft write.

Planning rescans the entire original region and one block of surrounding
context within the selected adapter and policy limits. Every observed block/property inside the original region must exactly
match the retained base. Missing/partial scans and duplicate positions are
rejected. Native literal revisions, callback-dependent edits and explicitly
scoped edits use the common electrical construction runtime. Ordinary unscoped
compatibility diffs pass the shared placement validator; unsupported devices, invalid supports and unexportable or
lossy block states are rejected. Changed blocks must provide complete properties
that the existing Java exporter can reproduce exactly. This intentionally
rejects some otherwise syntactically valid drafts, powered states the exporter
would reset, and names it would substitute. It never uses revision diagnostics
alone as placement proof.

The common placement preview is required. The plan is owner/dimension-bound and
expires after five minutes before application. Immediately before writes,
execution repeats the live placement checks and exact comparison of the full
captured region plus guard, including cells absent from the diff. The server
version/dimension must still match. Each forward attempt is consumed before
writes; an uncertain/partial write or verification failure cannot be replayed.
No automatic rollback is performed.

Post-write checks require the entire captured context to match the target,
including all properties. Physics-induced state changes not represented in the
revision therefore fail verification rather than being silently accepted.
Undo is only available after verified application; it checks the full expected
target context and then verifies restoration of the base context. Undo attempts
are also consumed before writing. These in-memory placement/undo records do not
survive MCP restart, independently of the persisted revision records.

Native literal revisions (including passive geometry), supported stationary
piston or cane/fixed-environment diffs, and scoped edits use
`ElectricalModification`. The original context is retained and only changed
positions are written. Forward and undo sequences use the common electrical
construction runtime and each command must settle to a complete expected state.
The model assumes an empty initial queue; snapshots cannot reconstruct hidden
pending events. Plans remain ephemeral, while write intents, verified prefixes
and readback evidence survive restart as historical `get_operation` records.
They do not restore executable authority or imply Blueprint adoption.

`validation.electrical_modification` describes the last draft edit only. Static
diagnostics and the older general validator can still report unsupported
pistons. `new_placement` independently checks the cumulative diff and the entire
fresh surrounding context; a passed stored check never grants placement
permission. See the [workflow and limitations](existing-machine-modification.md).

This proves modeled placement and the observed local result, not functional
equivalence, all input transitions, or distant circuit effects beyond the
captured one-block context. Stationary piston edits are limited to identities
and states supported by the common physical runtime. Unknown devices and
unmodeled crop/fluid behavior remain rejected. Cane ages and declared soil/enclosed
source water are retained exactly; support loss must be reflected in the complete
target state. Water-source or containment changes, natural growth and irrigation
construction are not supported. Merge remains outside this
increment.

Placement evidence (2026-09-07): 62 MCP tests passed and all-target Clippy
passed. The real Java 1.21.11 MCP harness passed three independent trials of a
parent repeater-property revision plus a child block-addition/deletion revision.
The cumulative diff was applied and exactly undone through common placement
tools. Both apply and undo rejected context drift. Cleanup was verified. The
tracked `revision-placement-mcp-summary.json` includes the tested binary hash.

## Region construction jobs

`new_placement({revision_id,work_regions:[{min,max},...]})` creates durable
work at the captured site, with at most 64 disjoint input regions. Oversized
regions are subdivided and bounded support/watch cycles combined, resulting in
at most 64 stages of 64 declared changes. The whole observed context, including
its guard, permits at most 4,096 non-Air blocks. Only the current stage receives
fresh forward/inverse physics in that complete context. Its boundary includes
natural updates, with one bounded temporary output initialization when needed;
input settings and the immutable final target are unchanged. Later stages remain
unverified. A naturally satisfied stage still requires a reviewed, confirmed
checkpoint, with no block writes.

After each reviewed application, use
`manage_construction_job({job_id,action:"plan_next"})`. After restart, `get`
returns history and `observe` reports fresh coordinate differences; new
`plan_next` or reverse `plan_undo` operations require new previews. An old
operation is never restored. `cancel` permanently disables forward work while
allowing fresh inverse cleanup of the verified prefix. Uncertain writes require
inspection; explicit `plan_recovery` only accepts the unchanged original stage
baseline. A partial prefix needs a separately authored repair revision.

v2 jobs retain immutable sparse intentions and verified boundary deltas beyond
the revision TTL. v1 history cannot be resumed; recapture a new v2 job instead of
projecting checkpoints from final properties. Large responses are summarized;
`get` with `include_intention:true` explicitly expands saved history. Jobs
do not adopt Blueprints, upgrade existing references or establish whole-circuit
functional behavior. See [region migration](large-circuit-regions.md) for
bounded scope, coupled-layout restrictions and evidence.
