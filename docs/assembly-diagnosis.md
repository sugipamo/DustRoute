# Diagnose a registered Assembly

Use `manage_assembly` without changing the Minecraft world:

```json
{"action":"diagnose","instance_id":"<UUID>"}
```

The design is the pinned Assembly retained when the tool placed the instance.
Diagnosis does not require a failed tool operation: missing parts, player edits
and other stable changes use the same comparison. It does not infer a design
for an unregistered circuit or determine who caused a change.

See the [shared diagnostic contract](diagnostic-system.md) for common findings,
repair handoff and links to the Assembly and its pinned child Blueprints.

## Findings and reference

Two complete, matching observations are required. Each finding includes its
position, observed and target block, and changed property values. Categories are
`missing`, `unexpected`, `different_block`, `orientation`, `configuration`, and
`state`. One position can have several property categories. Unsupported blocks
can still be described; importing the damaged layout into the simulator is not
required to report differences.

Fresh source/target review supplies the reference design. Its declared initial
state is simulated to rest, then each declared lever is set to its observed
level, in declaration order, settling after each input. The resulting
`reference.mode: observed_inputs` avoids comparing an ordinary closed door with
its initially open layout. It is one modeled reference, not recovery of actual
history: a history-dependent circuit can have other valid states at the same
input levels. A difference alone therefore does not prove a fault or its cause.

If a lever is missing or cannot supply an input level, the report falls back to
`declared_initial` and includes the reason. If source/target review fails,
`saved_initial_unverified` permits historical comparison but blocks repair.
Removed instances are compared with an empty region (`removed_instance`).

| Diagnosis status | Meaning |
| --- | --- |
| `matches_reference` | All observed blocks/properties match the selected reviewed reference |
| `differences_found` | Literal differences were found; check reference mode and modification intent |
| `reference_unverified` | Only a saved historical reference could be compared |
| `observation_unavailable` | Incomplete, changing, moving or wrong-target observation; no damage findings are asserted |

The shared report schema is `dustroute.diagnosis.v1`, with
`method: design_comparison`. `summary` counts differing
positions and categories; `findings` and `reference_snapshot` retain the detail.
An `ok: true` response means the request completed, including a report that says
observation is unavailable. `observe` retains its existing comparison with the
initial construction result, so its `changed` status can coexist with a normal
closed door's `diagnosis.status: matches_reference`.

## From diagnosis to reconstruction

`repair.status` separately reports `plan_available`, `blocked`,
`not_needed_for_reference_match`, `requires_new_placement`, or `not_assessed`.
For example, a player-added chest is reported at its coordinate even though
unsupported/extra material blocks automatic reconstruction. Existing
`plan_reconstruction` responses now retain the diagnosis when repair planning
fails. Diagnosis creates no executable operation and grants no write permission.

When repair is admitted, request `plan_reconstruction`, inspect `show_operation`,
and explicitly confirm invocation. The current strategy tears down the admitted
layout and rebuilds the declared initial construction result. It can affect
matching blocks too, and its target is independent of the current-input reference
used for diagnosis. Review the complete affected region, including intentional
human modifications. Material counts cannot establish ownership.

Diagnosis saves the latest observation and advances the registry revision, just
like `observe`; any older removal/reconstruction plan must be regenerated. It
does not promote an uncertain attempt to `applied`, reconstruct pending queues,
run a live functional test, or guarantee that a subsequent write is atomic with
the observation. Missing block-entity data and changes outside the declared
observation region are outside this block/property comparison.

## Validation scope

Model tests compare normal open/closed references at all four Y rotations and
replan from every settled repair-stage boundary for missing quartz and three
premature-input outcomes. These are settled boundaries, not arbitrary instants
during motion. Public transport tests cover player damage without a preceding
tool failure, unsupported extra material, missing controls, unverified reference,
incomplete/moving observations, and an applied write whose reply is lost followed
by restart and fresh reconstruction planning. Live evidence is recorded
separately from these model and transport checks.

The [verification report](evidence/reference-door-diagnosis-20260927.json) retains
18 passing Rust tests, Clippy/build/format checks and an isolated Java 1.21.11
trial at `(69008,180,1000)`, R0. Its eight diagnoses cover initial open, unavailable
client chunks, player-added chest, missing quartz, and two full close/open cycles.
The chest finding survives refused repair planning; missing quartz admits the
85-stage reconstruction. All eight before/after block snapshots are unchanged
by diagnosis. Complete construction, reconstruction and removal readbacks pass,
as do four explicit operating-state/aperture comparisons. Four MCP restarts,
contiguous server capture, empty final region, removed force loads and normal
server shutdown are retained. This finite trial does not extend the limits above.

The later [public acceptance trial](public-live-acceptance.md) checks three
diagnoses of a small passive building: intact, one confirmed missing floor block,
and explicitly restored. Missing damage is reported at its coordinate; stable
before/after layouts confirm that diagnosis does not write. Restoration is an
owned fixture input, not automatic repair. The earlier door evidence remains
separate; this case adds no new door operating-state guarantee.
