# Shared diagnosis and Blueprint references

Both diagnostic methods use `dustroute_translate::diagnostic::report::Diagnosis`
and serialize the common schema `dustroute.diagnosis.v1`:

| Method | Evidence | Public location |
| --- | --- | --- |
| `connectivity` | Observed connections, reachable sources, incomplete coverage and unsupported semantics | `test_circuit.diagnostic`, `new_repair.diagnostic`, `get_repair_context.facts.diagnostic` |
| `design_comparison` | Literal differences against an explicitly selected design reference | `manage_assembly(action="diagnose").diagnosis`, reconstruction planning's `diagnosis` |

The existing method-specific health, counts, reference modes and comparison
statuses accompany this shared core. The two methods are complementary; an
unregistered circuit need not acquire a fictional design to receive connectivity
diagnosis, and a literal design difference does not become a proven fault.

## Common result and repair handoff

Every report contains `schema`, `method`, `findings`, `repair`, `design`,
`world_writes: false` and `functional_test_performed: false`. Each finding has a
report-local `finding_id`, a tagged `basis`, its method's evidence and
`design_occurrences`. Both evidence variants expose `position` (possibly null
for connectivity). Design differences retain observed/target block states,
categories and property differences; connectivity findings retain confidence,
status, reasons, physical components and inferred-input distinctions.

`repair` has a shared status, reason, strategy, finding IDs to review,
`review_required: true` and `permission_granted: false`. Connectivity diagnosis
starts as `not_assessed`; `new_repair`/`get_repair_context` report `plan_available`
or `no_candidate` after generating candidates. Their candidates carry
`diagnostic_finding_ids` linked through changed cells and physical gap evidence.
These IDs are local to the accompanying report, not durable block identities or
proof that a candidate resolves every finding. Assembly diagnosis assesses
reconstruction through the existing admission checks; its bounded sequence size
and reset target are in `repair_details`.

The strategies are `partial_patch` and `teardown_and_rebuild`. Their existing
planners and world mutation/verification procedures remain separate. Sharing a
diagnostic result grants no execution permission, imports no runtime state and
does not weaken either procedure's preview, policy or revalidation checks.

## Connection to the Blueprint system

A placed instance retains its exact Assembly Revision. A freshly rebuilt
placement proof supplies the transformed Assembly's existing `AssemblyView`
membership index; diagnosis uses that index rather than deriving source identity
from observed blocks. `design.assembly_revision_id` identifies the parent state,
and `design.occurrences` lists the pinned Blueprint occurrences. Each finding's
`design_occurrences` retains all declarations at that coordinate, including
overlapping and nested occurrences, with their paths, revisions, origins and
rotations. Unclassified cells have an empty list.

Read the definitions through the existing tools:

```json
{"blueprint":{"kind":"assembly","id":"<design.assembly_revision_id>"}}
{"blueprint":{"kind":"blueprint","id":"<occurrence.revision>"}}
```

These are arguments to `get_circuit_revision`. The comparison target remains the
Assembly's actual physical state (or its modeled operating reference), not a
copy of each Blueprint's source layout. Source membership records interpretation;
it neither proves ownership nor traces a block moved by a piston. The report
states this coordinate-based limitation. Failed fresh review retains the saved
Assembly ID with `fresh_review_passed: false`; it does not fabricate trusted child
links or repair permission.

This supports following a diagnostic location back to its declared design.
Assigning an arbitrary existing world region to a chosen Assembly is not yet a
public operation. It would need an explicit reference/coordinate mapping and
observation bounds. Diagnosis does not replace revisions, change child pins,
alter requirements or adopt an update automatically.

## Implementation and compatibility

Literal snapshot indexing lives in `snapshot::index_literal_snapshot`; both
revision editing and design comparison use it without normalizing wire shapes.
The difference engine lives in `diagnostic::difference`. The former MCP-only
`assembly_diagnosis` module has been removed. Connectivity analysis produces the
same shared finding envelope and repair assessment as design comparison.

Current Assembly diagnosis changes from `dustroute.assembly-diagnosis.v1` to the
shared schema. Its assessment moves from `reconstruction` to `repair`; executable
reconstruction plans/journals still use `reconstruction`. Stored older reports
remain historical JSON and are not silently upgraded into current evidence.
Circuit diagnostic wrappers keep their established tool-specific fields and
schema versions; the embedded diagnostic core adds the shared fields. Read the
reported schema and rerun diagnosis when fresh common evidence is needed.

## Verification

The [verification record](evidence/diagnostic-unification-20260927.json) retains
10 passing Rust tests, Clippy with warnings denied, MCP build, formatting and
JavaScript syntax checks. Coverage includes both methods through the public MCP
transport, repair-context/proposal agreement, partial repair and undo, saved
revision editing, failed-reference diagnosis, rotated nested/shared source
membership, public design reads and interrupted Assembly reconstruction.

The physical runtime and mutation executors are unchanged. No live trial was
rerun for this refactoring. The earlier [door diagnosis trial](evidence/reference-door-diagnosis-20260927.json)
remains historical mechanical evidence; it predates the common report schema.
