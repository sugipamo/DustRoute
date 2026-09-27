# Ordinary 3×3 piston-door type

Status: **implemented, freshly verified and adopted in isolated test catalogs**.
The authoritative requirements remain in
[Blueprint architecture](blueprint-architecture.md#agreed-next-type-ordinary-3x3-piston-door).
This implementation does not change the Minecraft physics or the meaning of
`RepeatedSettling`.

## Function and binding

`TypeContract::PistonDoor { requirement }` describes repeated completed open/close
operations. JSON uses `{"kind":"piston_door","requirement":...}`. The requirement
contains one `closed_input` name and a 3×3 `aperture` matrix of `{air, solid}`
observation names. `PistonDoor::three_by_three()` supplies the canonical names;
the reference type ID is `dustroute.type.piston-door-3x3.v1`.

Bind these names using `BehaviorBinding::Observed`. Each cell must observe the
same fixed position using exact Air and Solid kind predicates. The resolved
matrix must form a complete 3×3 plane with perpendicular unit axes. Duplicate
cells, scattered points, presence-only predicates, heads and moving pistons
cannot substitute for that geometry. Relocation and rotation resolve before
validation. The input is a declared lever with explicit Powered predicate
polarity; false means open and true means closed in the logical contract.

| Binding | Reference realization |
| --- | --- |
| Control | Lever at relative `(0,11,0)`; OFF opens and ON closes |
| Aperture | `x=0, y=6..8, z=-1..1` |
| Initial state | Fresh declared open/retracted world |
| Closed material | Smooth quartz; the general type requires Solid occupancy |
| Geometry | Exact original 43 blocks, including original material names |
| Execution | Java 1.21.11 v6 electrical runtime and explicit Law requirements |

## Completion and proof boundary

Hold the input and explore the complete physical state until it repeats. The
recurrent region is complete only if every aperture observation at its boundaries
and inside its transitions has the requested values. Every phase of that region
allows the next command. Apply either logical input without resetting, find its
held-input recurrent region, and continue until the permitted operation graph
closes. A wrong recurrent region disproves the contract; incomplete exploration
is **Undetermined**, never a pass.

Initial input comes from the actual initial lever and its polarity. Initial
settling must preserve the declared aperture throughout; the first command is
issued from its certified recurrent region. The reference has two completed
physical states, open and closed, with **388 reachable states / 388 evaluated
steps** across the closed graph. Parent, child, environment, port and Law
obligations are checked throughout the same physical execution, including
intermediate callbacks. Correct door outputs cannot hide a failed child.

All future-relevant runtime state remains in the equality key: pending roots,
relative ordering/delays, carrier history and every block in the declared region.
There is no aperture-only hash, reset, sampled operation count or fixed wait.
Unrelated periodic activity may continue; its complete cycle phases are explored.
The predicate is conservative: it does not claim that the earliest matching
visible aperture is already ready, and budgets may prevent proof for larger worlds.

This is a certificate in the selected model. It is not a live sensor or a promise
that a block snapshot exposes pending game events. A live readiness interface
remains unimplemented. The previously measured 20/100-tick waits do not become
minimum operation intervals.

Input during an unfinished operation is outside this type's guarantee. The
simulator still executes that input normally, including leftover quartz. No
input queue, filter, rejection policy or automatic recovery was added. Verified
interruption tolerance requires a separate stronger obligation.

## Registration, persistence and adoption

The explicit reference candidate registers the type and introduces
`reference-door.mechanism.v3`, `reference-door.parent.v3` and
`reference-door.state.v3`, forked from literal v1. Its isolated proposal is
`reference-door.ordinary-adoption.v1`. The historical unrestricted v2 proposal
and its type/evidence keep their original meanings. No user catalog or reference
is automatically updated.

Catalogs containing the new type require `dustroute.blueprint-catalog.v11`, even
before a binding exists. Downgrading to v1–v10 is rejected. Proposal history uses
the existing v5 native-context format with that v11 catalog. Saved reports never
grant adoption: reload and adoption perform fresh physical review. Public MCP
uses the existing import → propose_update → show_operation → adopt tools, and
rechecks after service restart. No new public tool or special door execution
model is needed. The fixed-geometry contexts cannot certify this contract.

See the [ordinary adoption evidence](reference-door-ordinary-adoption.md),
[historical unrestricted audit](reference-door-adoption.md),
[normal live comparison](staged-piston-motion.md) and
[short-input live comparison](reference-door-short-input-comparison.md).

## Live construction and remaining readiness work

The [v6 command-construction trial](reference-door-live-construction.md) passed
all 43 build stages, two complete open/close cycles with exact full-region and
aperture readbacks, MCP restarts, and all 43 public removal stages. It also
refused changed-world and incomplete-observation removal requests. This repairs
the earlier stage-38 observer initialization failure without changing geometry,
physical Laws or the completed-operation contract.

A live readiness interface remains unimplemented. The finite trial waits and
matching snapshots do not expose all pending server work or establish a minimum
safe operation interval.
