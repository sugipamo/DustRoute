# Stage 4: observed-site continuation scope

Recorded 2026-10-03 UTC on source `950deef`. The stage-4 goal has been created.
The initial investigation stopped for a scope decision. The user subsequently
approved continuation after a checked idle boundary. Stage 4A was implemented
but live completion is stopped for the [new observer readiness prerequisite](survival-observer-readiness-prerequisite.md);
unresolved mining after loss of its native sessions remains a
diagnostic stop. The original investigation ran no new fault trial or compilation.

## Confirmed prerequisite boundary

The current independent mining-retirement contract holds the original miner,
observer and exact watch in one process. It registers the observer's authenticated
miner profile **before** source closure, then requires the matching removal
receipt and closed original source. A fresh login, missing visible player,
matching block snapshot or saved JSON does not reconstruct that contract.

Code evidence:

- `vendor/voxrig/src/checked_survival.rs:280`: preparation returns an in-process
  coordinator; lines 300-307 expressly disallow restoration from JSON.
- `vendor/voxrig/src/versions/java_1_21_11/client/operations/retirement.rs:124`:
  the watch needs the exact profile baseline before disconnect. Lines 194-218
  require the original source and observer connection identities and watch.
- `crates/dustroute-mcp/src/service/survival.rs:705`: the stopped executor and
  exclusive lease live in the process-local job registry.
- `crates/dustroute-mcp/src/survival_execution/journal.rs:86`: reopened evidence
  has effective `NeedsInspection` and restores no native authority.

Therefore the existing contract cannot automatically settle an unresolved
mining attempt after losing both client sessions in a process crash. This is
distinct from a deliberate stop while the old clients and watch remain live,
or a restart after a checked safe checkpoint. The stage-3 acceptance reopened
completed evidence in a new service; it did not exercise restart during mining.

The broader restart outcome must be decided explicitly. Do not silently treat
cached absence, a fixed wait or a historical connection number as a new native
retirement receipt. Changing that contract is a separate prerequisite, not a
routine deserialization change.

## Recommended next scope: stage 4A

Complete observed-site diagnosis and continuation where old effects can be
settled through existing contracts. Refuse execution from records with unresolved
old effects after an unplanned process loss, returning the exact outstanding
phase, affected cells and required intervention. Such refusal is a declared
limit, not successful crash recovery.

1. **Common typed diagnosis.** Compare fresh builder/independent observations
   with the exact adopted design, saved baseline and observed execution events.
   Classify completed targets, unbuilt targets, unchanged surroundings,
   conditional remaining owned temporary works and foreign/conflicting states.
   Preserve uncertain placements/mining as uncertain; do not infer attribution
   from an identical block or credit future drops as inventory.
2. **Checked stopping boundary.** Add an explicit settlement/checkpoint path
   to the common executor. Where its live mining intent survives, prepare the
   existing independent retirement before closure and classify the fresh target
   after retirement. Finish the already started attempt through its normal live
   executor, without replaying an old placement or duplicate FINISH. A failed
   settlement retains the exclusive source and diagnostic state.
3. **New plan from current facts.** Rebase the checked site on the fresh snapshot,
   preserve completed permanent targets and protected/foreign surroundings,
   seed the shared temporary-work ledger from conditionally verified owned cells,
   and search only remaining obligations. The final state still requires cleanup
   and retreat. Material counts come from current inventory without assuming drops.
4. **Public workflow.** Expose diagnosis and a new continuation preview linked
   to the prior job. Fresh admission checks adoption, scope, observer, position,
   world and inventory. Execution uses a new job and common executor after explicit
   approval; persistence never reconstructs a native action token.
5. **Acceptance.** Exercise deliberate placement-boundary cancellation, live
   mining interruption/settlement, foreign-change and shortage refusals,
   completion from partial work, and a separate-process restart after a checked
   checkpoint. Verify that an unresolved mining journal reopened after loss of
   its clients refuses continuation. Keep the positive checkpoint case and the
   negative unplanned-loss case separate in evidence.

This work is mainly in DustRoute's construction site/checker/search, executor
settlement, journal interpretation and public job boundary. The current dry-cube
and observation/action/search limits remain. This scope does not require a new
Voxrig cross-process retirement contract; its checkpoint prerequisites still
need implementation validation before making a readiness claim.

## Alternative prerequisite: stage 4B

If automatic continuation after losing the miner and observer during unresolved
mining is required now, investigate a native lifecycle mechanism that can establish
old-source retirement despite controller loss. Possibilities include keeping the
native operation owner/observer alive outside the restartable controller, or a
separately justified server-assisted fence. Neither is implemented or established
by the current contract. A mechanism must handle an unregistered watch, observer
loss, profile reappearance, connection-domain changes, and failure before a
retirement receipt. Making handles deserializable alone is insufficient.

This prerequisite can span Voxrig and deployment/process ownership and may be
substantially larger than DustRoute continuation planning. Its architecture must
be assessed before implementation, preserving the library boundary and without
claiming that a new generic fence already exists.
