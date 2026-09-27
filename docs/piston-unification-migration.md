# Piston unification: migration baseline

The old directional/direct-only runtimes and their reproduction harnesses
have since been removed; [stabilization](stabilization-legacy-paths.md) supersedes
the preservation policy recorded here. Raw captures and source audits remain.

This document preserves the migration-baseline milestone and its acceptance
plan. The stage statuses below describe that milestone, not current capability.
For the completed electrical/runtime/adoption/placement work, see the
[general placement roadmap](piston-general-placement-roadmap.md); for subsequent
restart and removal support, see [placed Assembly management](placed-assembly-management.md).

As of 2026-09-26, the first migration milestone is implemented: retained
directional behavior and serialized contracts are fixed by regression captures,
both restoration paths enforce execution identity, and Java 1.21.11 power and
notification differences have been audited. The subsequent
[isolated direct-input unified profile](unified-piston-runtime.md) now implements
the second milestone. The retained separate horizontal/vertical profiles still
reject a world containing both axes.

## Retained behavior and restoration

At this milestone, horizontal and vertical v1 profiles were preserved as historical execution contracts.
Their movement, power queries, law IDs/programs, initial gates and serialized
contexts were unchanged by that milestone. A later unified profile must use a new world
context; it must not reinterpret an old pass or resume old pending work.

The new regression evidence records:

- [60 execution cases](../crates/dustroute-minecraft/tests/fixtures/legacy_piston_callbacks_v1.jsonl):
  six facings × normal/sticky × five input histories. ON is applied at external
  tick 1; OFF at tick 1, 2, 3 or 8; the fifth history adds ON at tick 9 after OFF
  at tick 8. Each delivered invocation retains time, section, kind, target and
  result, along with complete world deltas and carrier-history changes. Large
  notification/continuation payloads are summarized; final blocks and pending
  count are retained. These are model captures, **not live observations**.
- A 6 × 6 direct-source matrix with redstone blocks freezes the old asymmetry:
  horizontal bodies ignore power above/below; vertical bodies inspect all six
  sides. Both exclude the front. A powered side cannot hide a later unknown
  side. This is a legacy expectation, not the expected unified behavior.
- [Two saved contracts](../crates/dustroute-library/tests/fixtures/legacy_piston_contracts_v1.jsonl)
  freeze behavior-context JSON, world-context JSON, all thirteen selected law
  roles and full immutable law records for each directional profile. Reloading
  the archived law catalog must resolve to the same records. Existing bounded
  v1 piston fixtures remain unchanged.

`PistonRuntime::from_behavior_state` and
`VerticalPistonRuntime::from_behavior_state` now return `Result<_, RuntimeError>`.
They use the same identity check as checkpoint restoration: delivery-runtime
profile, adapter Revision string and concrete Rust adapter type must all match.
The behavior explorer propagates restoration errors. The check does not rerun
fresh-construction validation, which would reject valid moving carriers.

Tests reject cross-profile restoration even for piston-free worlds, reject each
identity mismatch independently, and resume both vertical directions through
interruption at every microstep (exact checkpoint) and complete root boundary
(behavior representative). Horizontal continuation tests remain in place.

Checkpoints and behavior states are opaque, process-local values, not durable
JSON save files. The existing comparison identifier and normalization are
unchanged; its full key already includes the runtime and adapter identities.
No conversion of old checkpoints is provided. Continue an old run under its
old profile; begin unified verification from explicit fresh initial conditions.

## Target-version comparison

The audit uses the locally available named Minecraft Java **1.21.11** artifact,
Yarn **1.21.11+build.6**, with redstone experiments disabled. Its SHA-256 matches
the [previous source audit](../crates/dustroute-minecraft/tests/fixtures/piston_runtime_source.meta.json).
The [new audit manifest](../crates/dustroute-minecraft/tests/fixtures/piston_unification_source.meta.json)
records class hashes, inspected methods and bytecode offsets. It is static
implementation evidence, not a server measurement. To reproduce, use that exact
mapped artifact and `javap -p -c -classpath <jar> <class>` for the listed classes.

| Area | Java 1.21.11 implementation | Retained model and unified requirement |
| --- | --- | --- |
| Direct piston power | `PistonBlock.shouldExtend` queries all six adjacent coordinates except the body's facing. Request and block-event delivery both call it. | `geometry::powered` selects a four-side or six-side scan by the body's axis. Unified execution needs one six-side query for every body, reading each body's facing as state. Change it only in the new profile. |
| Front exclusion | The front is omitted from the direct query; it is not removed from the world. | The old adapter temporarily replaces the front with Air in a clone. The unified query should skip that input edge, retaining the real front for conductor and other spatial queries. |
| Directional emission | `RedstoneView.getEmittedRedstonePower` reads the source's weak output and, for a solid source, received strong power. Java's query direction is receiver-to-source. | The old piston connection law uses source-to-receiver facts and does not model solid-mediated input. Do not treat source presence or an undirected powered bit as a complete input law. |
| Vertical dust | `RedstoneWireBlock.getWeakRedstonePower` returns zero for a DOWN query, power for UP, and checks the opposite connection arm for horizontal queries. | The old law treats all wire directions through the arm map; synthetic wires without a map are accepted. A map with horizontal arms cannot express the vertical rule. A new directional input law and exact-observation tests are required. |
| Additional power checks | `shouldExtend` also queries the body coordinate with DOWN and five neighbors of the coordinate above, excluding DOWN. The latter is the quasi-connectivity path. | Neither directional profile performs these checks. Six-side direct input alone cannot claim general Java piston power. Conductor transfer and quasi-connectivity require a separately declared scope decision before implementation. |
| Ordinary / shape order | `NeighborUpdater.UPDATE_ORDER`: W, E, D, U, N, S. `AbstractBlock.DIRECTIONS`: W, E, N, S, D, U. `SixWayEntry` delivers in the former order. | The geometry law and notification code already use these separate six-way orders. Preserve the order and synchronous continuation boundaries while combining axes. This does not certify all Java write callbacks. |
| Lever notifications | `LeverBlock.updateNeighbors` emits one six-way batch around the lever and another around its supporting block. | `PistonEvent::Input` calls `wire_jobs`, which emits adjacent jobs around the lever and selected nearby-wire work, but no general support-centered piston notification batch. Cover this together with conductor input; merely expanding the input scan will not fix it. |
| Mixed movement | Movement uses each body's facing; carrier state and callbacks are world-owned. | The common adapter already computes 3D positions. Axis rejection is at the old entry gates. A new world-level adapter/context is needed; selecting the old adapter per piston would retain inconsistent power rules. |

This milestone deliberately stops at identifying the deferred electrical work.
It does not add quasi-connectivity, general conductor transfer, destructible
payloads, slime/honey, entity motion, block-entity transport or new experimental
redstone semantics. The old limited behavior is preserved for reproduction.
If unified scope remains direct-input-only, its explicit support boundary and
handling of layouts that depend on omitted paths must be established before
verification or placement can call those layouts supported.

## Required unified cases

Keep the frozen legacy rows separate from new source-derived expectations and
from live observations. These are the acceptance cases for the next stages:

| ID | Case and required evidence |
| --- | --- |
| U01 | Six body facings × six direct source sides, normal and sticky: eligible source powers all five non-front sides. Use direct levers/redstone blocks first; record the actual lever support. |
| U02 | Directional repeaters and exact dust observations, including dust above/below and front sources. Distinguish Java query direction from internal source-to-receiver direction. Missing direction/shape and unknown consumed coordinates must not become false OFF. |
| U03 | Two powered sides, then one OFF; a known powered side plus an unknown side; request-time ON followed by delivery-time OFF. Preserve complete boundary validation and delivery-time power checks. |
| U04 | One horizontal, one upward and one downward mechanism in one region and one queue. Check independent ON/OFF, simultaneous inputs and world-level context identity. No dispatch to the old directional profiles. |
| U05 | Shared destinations, a moving carrier blocking another push, and stable/moving heads beside the other axis. Check both input orders, body identity, retained payload restrictions and explicit unsupported results. |
| U06 | OFF before delivery, during extension, at the materialization boundary and after settling; ON during retraction. Cover all axes, normal/sticky variants and independent body/payload carriers. |
| U07 | Completion A notifies piston B before an unrelated completion C; nested notifications preserve their enclosing tick section. Verify ordinary and shape orders, head support and checkpoint continuation inside each sequence. |
| U08 | Lever support-centered notifications and conductor-mediated power. Separate power evaluation from the event that causes reevaluation. This case requires the deferred electrical scope to be resolved. |
| U09 | Quasi-connectivity-only source with/without a later neighbor update, plus unknown cells above the body. Requires an explicit scope extension or unsupported-case handling; never infer conformance from a direct-input test. |
| U10 | Linear limit 12/13, normal/sticky settling and retained payload gates. Add vertical piston payloads only if their existing horizontal-only payload restriction is deliberately revised under a new law. |
| U11 | Old-to-new and new-to-old checkpoint/behavior-state restoration rejected, including worlds without pistons. Same-profile continuation and saved context/law catalog round trips preserve the selected contract. |
| U12 | Revalidate parent, child and environment requirements under the new context. An old pass cannot authorize adoption; changed requirements/references become explicit Revision proposals. Recheck after saved history reload. |
| U13 | Adopted Assembly plan, preview, apply, readback and undo in an isolated world. Changed blocks or insufficient observations must reject before apply; this is separate from behavior adoption. |

Live captures for U04–U07 must retain the server-applied input tick and section
when available, observation tick, positions/properties, target version/settings,
capture completeness and comparison result. Client-requested intervals alone
are insufficient. Begin with independent mixed layouts, then add interference,
notification, interruption and completion cases. Classify mismatches as input
timing, observation coverage, unsupported scope or model disagreement before
changing a law. No new live captures were made in this milestone.

## Historical roadmap and completion gates

| Stage | Gate | Status |
| --- | --- | --- |
| 1. Migration baseline | Freeze legacy execution/laws/serialized contexts; check restoration identity; audit six-way input/notifications and declare differences/tests. | Implemented here. Deferred electrical scope is explicit above. |
| 2. Unified execution | One direction-independent context, selected laws, runtime and explorer. Horizontal/up/down share a world and queue. | Implemented for the [isolated direct-input subset](unified-piston-runtime.md). U02 devices are rejected; U08/U09 influence is conservatively excluded at construction and before writes. Broader electrical support remains pending. |
| 3. Mixed live comparison | Retain applied-input/observation correspondence for declared U04–U07 cases; classify discrepancies. | Pending. |
| 4. Blueprint validation/adoption | Fresh parent/child/environment review, explicit immutable updates, same decisions after reload. | Pending. |
| 5. MCP placement | Piston-capable placement validation and U13 in an isolated world. | Pending. General `ValidatedWorld` still rejects pistons; a fresh adopted-Assembly review does not bypass that gate. |
| 6. New-use default | Execution, review and placement use the unified context by default; documentation agrees; historical contexts still reproduce their old behavior. | Pending; do not switch defaults before prior gates pass. |

Law selection remains world-wide. The selected programs consume block facing
and physical state; a per-piston law selector is unnecessary. Immutable child
references and terminals never move merely because runtime blocks move.

## Historical verification

At the baseline milestone, 167 distinct tests passed, including the 60 recorded
legacy callback cases and serialized legacy contracts. Those model-freezing
tests and the `audit_legacy_piston_*` generators are retired. Their JSON/JSONL
captures and metadata remain as historical evidence; they are not regenerated
from the current electrical runtime. No live placement was claimed by this
baseline milestone. Current commands and results are in
[stabilization](stabilization-legacy-paths.md).
