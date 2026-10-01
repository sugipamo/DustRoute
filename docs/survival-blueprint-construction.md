# Survival Blueprint construction

Started 2026-10-01 on `codex/survival-blueprint-construction`, from develop
`4ba023c`. The previous diagnostics and Voxrig pin work was fast-forwarded into
develop and pushed. Both merged topic branches were deleted locally and on
origin. Main remains at `7f74b37`.

## Intended result

Given a Blueprint, all required building materials and temporary access blocks,
the bot builds a bounded structure in survival mode through ordinary player
actions. Materials are supplied directly in **the bot's inventory**, as confirmed
by the user. The human should specify the design and site, rather than place each
block. Completion requires checking the final structure and declared air spaces,
accounting for consumed materials and removing the bot's temporary access works.

The first target is a small passive structure on an observed level site, with a
roof high enough to require temporary access. The blueprint and material list
must be fixed before testing; a successful single nearby placement is only an
earlier milestone. Existing circuit analysis and adoption do not establish
survival constructibility.

The temporary-block question is still unanswered at this audit. The proposed
first implementation uses ordinary, admitted solid blocks. Minecraft's dedicated
`scaffolding` block requires its own placement/support/climbing audit before it
can be selected. This assumption has not authorized any live writes.

## Existing mechanisms and gaps

This is a source audit of `4ba023c`, not new live Minecraft evidence.

| Mechanism | Current evidence | Needed for this goal |
| --- | --- | --- |
| Blueprint authoring, immutable pins, adoption, physical review | [design workflow](blueprint-building-design.md) | Reuse them; keep design validity separate from the ability of a player to construct it |
| Literal site observations, policy, diagnostics and durable uncertain attempts | [construction executor](../crates/dustroute-mcp/src/service/construction_executor.rs), [diagnostics](operation-diagnostics-progress.md) | Reuse the boundaries and evidence kinds; add player actions and material state |
| Normal block-use packets | [1.21.11 `use_on_block`](../vendor/voxrig/src/versions/java_1_21_11/client/operations.rs) | Check actual held item, target face, reach, obstruction, placement rule and received result |
| Received inventory | The same native module receives player slots and treats unsupported components as unavailable | Retain window/state revisions and cursor knowledge needed for transactions; reconcile material consumption |
| Inventory transfer | `select_hotbar` selects only an existing hotbar slot; `set_creative_hotbar` creates items in creative mode | Add ordinary, received-result-checked main-inventory/hotbar transfers in 1.21.11 |
| Player movement | 1.21.11 has `move_flying`, gated by server flight permission | Add survival control ticks, collision, gravity, grounded state and server correction handling |
| Survival locomotion donor | [1.16.1 client](../vendor/voxrig/src/versions/java_1_16_1/client.rs) has `set_control`, `jump` and a control/physics loop | Audit reusable algorithms and expose them through the explicit 1.21.11 adapter; do not redirect a 1.21.11 connection to 1.16.1 |
| Timed mining donor | That 1.16.1 client has `dig_block`; 1.21.11 exposes `dig_creative` | Add survival mining for declared temporary materials, tool constraints and uncertain-result handling |
| Current physical placement | [native physical bridge](../crates/dustroute-mcp/src/voxrig_bridge/operations/physical.rs) switches to creative, teleports and creates held items | Introduce an explicitly selected survival executor that cannot fall back to those actions |
| Current building execution | The shared executor constructs `CommandWrite` batches | Its command schedule is not a player action plan; reuse review, policy, persistence and diagnostics rather than relabeling it |
| Ground and temporary access | Building patterns require an outer air guard, and target placement requires a completely empty region | Define an observed, protected site context and explicit temporary edit scope without silently weakening immutable building requirements |

Voxrig's public `Client` keeps the two protocol adapters separate. The root
`Bot`/`ControlState` APIs belong to 1.16.1; the presence of those exports does not
mean 1.21.11 already supports them. At the audited pin, the 1.21.11 position
receiver reads but discards velocity and has no corresponding survival control
loop. Transport acknowledgements and cached local movement are not proofs of
completed world placement or accepted position.

## Architecture and order

Keep the final Blueprint independent of a site-specific execution plan. A plan
contains the observed baseline, protected environment, temporary access scope,
material reservations, player actions and escape/cleanup route. Temporary blocks
do not become permanent Blueprint geometry. They may not occupy a declared
permanent-air region merely because they will eventually be removed; any needed
execution-time allowance must be explicit and reviewed separately from the
immutable final-state obligation.

Voxrig owns version-specific packets, received player/inventory state and player
physics. DustRoute owns bounded navigation, construction sequencing, access-work
planning, edit permissions and Blueprint integration. Preserve Voxrig's separate
contribution history and update the unmodified vendor snapshot only from a
validated source commit, as described in [the vendor policy](../vendor/README.md).

| Order | Work | Acceptance before proceeding |
| --- | --- | --- |
| 1 | Native 1.21.11 survival state and inventory transactions | Ordinary stacks in main inventory can move to the hotbar and back. Received revisions, cursor, mode and connection changes are checked. No generated items or optimistic inventory counts. Unsupported components produce an explicit refusal |
| 2 | Nearby survival placement and bounded timed mining | On a prepared, reachable site, ordinary interactions place exact admitted cubes and remove declared temporary cubes. Validate the hit and player occupancy; independently observe block changes and inventory consumption. Acknowledgement alone is insufficient. No mode switch, flight, teleport or block-write commands |
| 3 | Native survival locomotion and bounded navigation | Walk, stop, jump and settle on admitted static geometry; honor corrections and missing chunks. Validate collision, ground support, reach and an escape path. Health/death or unsupported movement conditions stop work; adversarial entity simulation is not part of this milestone |
| 4 | Site-aware Blueprint and temporary access planning | Revalidate the final design in its observed ground context, preserve all protected terrain and permanent air obligations, and plan support/place/access/remove dependencies. Include action order and last safe retreat. Material estimates distinguish permanent consumption, peak temporary inventory and resources dependent on later recovery |
| 5 | Durable survival job execution | Preview the exact site, materials and temporary footprint. Execute one checked player action at a time through shared mutation/policy/error boundaries. Save intent before mutation, read back the result, reconcile inventory, and checkpoint verified progress. Restart requires fresh observation and replanning; never replay an uncertain action blindly |
| 6 | End-to-end isolated survival acceptance | With a non-OP survival bot given inventory and an adopted passive Blueprint, build the declared roofed structure using access works. Observe exact completion and cleanup. Exercise missing materials, blocked access, unexpected edits and disconnects; demonstrate diagnosis and a newly planned continuation |

Steps 1 and 2 deliberately permit a stationary, reachable trial before the larger
locomotion change. They do not satisfy the full building goal. The end-to-end
trial must still run with operator permissions absent: using commands to set up
an isolated fixture by a separate test actor is not the builder's execution path.

Start with a finite admission list of passive cube materials and plain inventory
stacks. Placement-state recipes for stairs, slabs and directional equipment need
independent player-interaction verification before expansion. Resource gathering,
crafting, chest retrieval, terrain excavation, arbitrary fluids, combat and
arbitrary survival construction are subsequent scope, not implied acceptance.
Access-block removal does not imply recovered item drops: actual recovery needs
its own received inventory evidence. If recovery is necessary to finish with the
supplied inventory, the plan must establish it or stop before spending resources.

## Current stopping point

Branch consolidation and this audit/roadmap are complete. **No survival runtime
changes or live trial have been performed.** No compiler or test process was
started for this documentation-only change.

The first substantial prerequisite is in Voxrig's 1.21.11 player/inventory
implementation, rather than a table or a flag in DustRoute's existing placement
executor. Locomotion, inventory transactions and timed mining affect the client
receive/control lifecycle, and site-aware ground/access context also affects
construction proofs. Together this is a medium-to-large change across the two
projects, with native comparison work required before autonomous elevated builds.

Following the user's earlier instruction to stop and report a large blocking
prerequisite, implementation is paused at this review point. The recommended
next concrete milestone is steps 1 and 2: a supplied main-inventory stack becomes
a verified nearby survival placement, with a verified temporary-block removal.
Then add locomotion and the site/access planner in that order. Do not claim the
existing creative building trials validate these new mechanisms.
