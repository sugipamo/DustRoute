# Vertical and mixed piston placement roadmap

Requested and completed on 2026-09-26 for the declared supported scope, with
implementation and evidence recorded below. The user selected dust, repeaters, conductor
power and quasi-connectivity in addition to direct lever/redstone-block inputs.

## Outcome and boundary

On Minecraft Java 1.21.11, an explicitly defined Assembly containing horizontal,
upward and downward ordinary/sticky pistons can be verified, adopted and built
at a newly selected, verified location through the public MCP workflow. Layouts
are not restricted to a built-in door template. All orientations use one world,
one queue and one world-selected set of laws. Physical facing is block state.

The electrical scope includes levers, redstone blocks, dust, repeaters, power
through admitted solid blocks and piston quasi-connectivity. Actual supported
block identities, metadata and observation coverage must be explicit. Moving
ordinary payloads, interference and the retained twelve-block push limit belong
to the mechanical scope. Unsupported mechanisms must fail explicitly rather
than being silently approximated or becoming a verification pass.

Slime/honey, entities, general block entities, destructible components, survival
building, old-checkpoint conversion and other Minecraft versions are excluded.
If an excluded mechanism or unrelated prerequisite must be implemented first,
stop that work and report the reason to the user. Live changes are confined to
an isolated test world/region with an inspected baseline and retained cleanup
evidence. Do not claim ordinary shared-world deployment from those trials.

## Milestones

| Step | Work | Completion evidence | Status |
| --- | --- | --- | --- |
| 1 | Audit target-version weak/strong emission, conductor queries, dust updates, repeater scheduling/notifications and quasi-connectivity. Specify the new profile and initial/construction boundaries. | [Source audit](piston-electrical-source-audit.md), exact provenance and regression matrix. | Complete: source/provenance audit and explicit admission/initialization contract |
| 2 | Add the electrical scope to a new unified world context, shared callback runtime and Assembly explorer. Preserve historical directional and direct-only contracts. | All orientations, all admitted sources, unknown-space rejection, notification ordering, interference/interruptions, restoration identity and context/catalog reload tests. | Complete: one electrical runtime/context, prioritized ticks, shape/prepare callbacks, moving sources, locking and preserved historical contracts |
| 3 | Compare model and isolated Java server for independent mixed mechanisms, interacting destinations, notifications, interruptions and completion boundaries. | Applied server input ticks, observations, version/settings, completeness, mismatches classified; no client-request timing substituted for applied timing. | Complete for the declared comparisons: b–h and j–l match; a/i are classified failures, not model passes; tick-internal coverage remains bounded |
| 4 | Validate explicit parent/child/environment obligations under the new context and make immutable update proposals adoptable after fresh review. | Parent-pass/child-fail and stale historical-pass rejection, unchanged child references unless explicitly proposed, persistence/restart tests. | Complete: fresh migration/adoption, retained child failure, forged-pass rejection and public MCP process-restart trials |
| 5 | Add new-location Assembly construction with coordinate/context transformation, piston-aware placement validation and preview/apply/readback/undo. | Whole destination and influence coverage, lossless properties, stable construction sequence, changed-baseline rejection, no bypass through generic validation. | Complete: target review, sequential build/teardown, whole-region checks and conditional recovery exercised by public MCP |
| 6 | Exercise public MCP against the isolated server for vertical and mixed custom Assemblies. | Plan/preview/apply/readback, normal input operation, changed-world/incomplete-observation rejection, conditional undo and cleanup evidence. | Complete: c/d/e public MCP trials, two distinct layouts, three rotations, positive/negative coordinates, 14 build and 14 removal stages per trial |
| 7 | Make the proven path the default for newly requested supported piston work and update public documentation. | New users need no directional profile choice; workspace checks pass. Historical-context reproduction was a migration gate, superseded by the subsequent retirement policy below. | Complete: standard runtime/review/MCP entry points, updated public documentation and passing final workspace checks |

These steps may be split into smaller reviewable changes. A passing test of one
step does not satisfy the later gates. In particular, local runtime support is
not live conformance, adoption is not placement, and a stored review is not fresh
authority to modify the world. Computation limits produce undetermined results.

Target-coordinate review is required: the target Java dust updater has
coordinate-dependent notification order. Translation and rotation cannot reuse
a local-coordinate behavior pass, even when all blocks fit in empty space.

## Existing baseline

- [Migration baseline](piston-unification-migration.md): directional execution,
  laws and serialized contexts frozen; restoration identity enforced.
- [Direct-input unified runtime](unified-piston-runtime.md): mixed orientation
  execution and exploration, deliberately excluding the expanded electrical
  scope. Keep its profile immutable.
- [Existing fixed door MCP](piston-door-mcp-v1.md): useful placement/operation/
  conditional-removal machinery, but no arbitrary Assembly construction proof.
- [Blueprint MCP](blueprint-mcp.md): immutable revisions, proposals and persistent
  adoption; the retained grounded placement route uses original observed coordinates.

No live conformance, adoption migration or general placement gate has been
completed merely by registering this goal.

## Recovery and current validation

The original host fault stopped work before custom live construction. No host
repair was performed by this task. The user reported recovery and authorized
resumption; trials c/d/e then passed. Their retained evidence is in the
[evidence report](piston-electrical-live-evidence.md), alongside ten accepted
primitive comparisons and the unsuccessful captures excluded from conformance.

New execution uses `new_piston_runtime`; new review uses
`RuntimeBehaviorContext::fresh_pistons` or public `behavior_context.piston`.
Those entry points select one electrical profile regardless of body direction.
Proposals store the resolved explicit profile. Subsequent stabilization removed
the directional/direct-only runtimes. Their profile IDs are rejected without
conversion; see [retirement policy](stabilization-legacy-paths.md).

At completion of this placement milestone, workspace validation passed: **860 tests passed, 0 failed, 1 ignored**.
The ignored case is a pre-existing manual physical-NOT scalability measurement,
not a piston gate. Workspace formatting, all-target Clippy with warnings denied,
script syntax checks and diff whitespace checks also passed. The
[check record](evidence/piston-final-checks-20260926.json) retains exact commands,
results and log hashes. All seven milestones are complete for the outcome and
boundaries above; future unsupported mechanisms require a separate scope.

## Requirement-to-evidence audit

| Requirement | Concrete evidence |
| --- | --- |
| Historical migration preservation (subsequently retired) | The original milestone froze 60 model records; stabilization removes that execution obligation and tests rejection of retired profile IDs. Original captures remain unchanged |
| One physical world for all directions and admitted electrical sources | Electrical law/world/runtime tests; captured b–h/j–l observations; j operates ordinary/sticky bodies in all six directions in one world |
| Compare actual applied timing, classify insufficient evidence | Retained input packet/state-write pairs and full settled regions; 36 additional prefix samples across h/j/k/l; failed stimulus a and incomplete capture i excluded |
| Revalidate parent, children, environment and saved adoption | `electrical_piston_adoption` retains original revisions, rejects a child failure and forged stored pass; public d/e reopen adoption in a new process |
| Transform actual physical state and review the new destination | `electrical_piston_relocation` covers moved input/terminal frames, inverse transform and overflow rejection; public c/d/e use R90/R270/R180 at positive and negative X |
| Plan/build/read back/undo custom Assemblies | `electrical_piston_construction` and the public transport test; live c/d/e each verify 14 build and 14 teardown stages, baseline/undo guards and cleanup; e has distinct candidate definitions/layout |
| New users need no directional profile selection | `new_piston_runtime`, `fresh_pistons`, and request-only `behavior_context.piston`; public d/e exercise the shorthand, explicit stored context and restart |

Evidence proves the declared admitted scope and recorded trials. It does not
prove all Minecraft interactions, survival construction, concurrent shared-world
mutation safety, unsupported binding types or every possible assembly. Unknown
space, unsupported transitions and exhausted exploration remain non-passing.

## Follow-on: durable placed instances

The separate [persistent Assembly management goal](placed-assembly-management.md)
is now complete for this admitted construction path. It adds TTL-free placed
instance records, fresh observation and source/target review after MCP restart,
and previewed conditional removal with durable progress. Its own
[check report](evidence/placed-assembly-final-checks-20260926.json) and
[isolated live trial](evidence/placed-assembly-mcp-20260926-a.json) retain the
evidence; the original physical-conformance scope above is unchanged.
