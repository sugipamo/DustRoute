# Proposed prerequisite before adding new device kinds

Status: **approved and implemented before milestone 2** of the
[integration roadmap](device-integration-roadmap.md). The user approved this
bounded prerequisite after the original stop. Further outside prerequisites
still require stopping before implementation.

## Finding

Repeater migration exercises an existing `BlockKind`, so its spatial and placement
contracts already exist. A copper bulb introduces a new kind. The current device
definition owns callback rules, emission, conduction and full-face support, but
other consumers still use independently authored physical admission:

- `spatial.rs::SpatialLaws::compile` fixes the trait ABI to kind tags 0..15.
  `kind_tag` encodes the existing enum and is also used by piston connection and
  payload laws. Merely extending this tag domain is not an extension of every
  consumer's immutable Law ABI.
- `Block::properties`, `Block::redstone_traits` and ordinary support validation
  use the global spatial law. The callback runtime additionally reads device
  metadata through `piston_electrical::{conducts,full_face}`.
- `Block::capabilities` and snapshot classification maintain separate matches.
  Changing classification alone does not make an old executor or a saved
  placement/behavior result support a new stateful component.
- During repeater migration, a retained placement regression caught another
  conflation: signal output and dust attachment are not the same property.
  Repeater dust attaches at both ends; observer dust attaches at the output.
  Milestone 1 fixes that concrete integration issue with `WireConnectionRule`.

Adding individual kind branches remains possible. The recommendation to stop is
an architectural judgment: before expanding the supported block set, unify the
physical admission contract instead of adding more parallel sources of truth.
It is not a claim that bulbs are impossible without a wholesale rewrite.

## Proposed bounded prerequisite

1. Define checked Rust physical descriptors for shape, support requirements,
   conducting faces and dust attachment. Connect existing device descriptors to
   these traits; preserve the separation between physical properties and which
   simulation/observation/placement operations are actually supported.
2. Make admission explicit for each execution context. A registry entry must not
   grant support to historical/compatibility executors. Give changed physical
   assumptions a new revision, and reject unsupported kinds before a finite-law
   lookup instead of aliasing them to a different device's behavior.
3. Decouple the per-Law kind ABI from the shared enum so adding one kind does not
   silently expand spatial, connection and piston-payload laws together. Retain
   explicit unsupported movement until its own contract is implemented.
4. Route runtime support checks, placement checks and observation classification
   through the declared physical/admission facts where they overlap. Keep
   evidence validation and incomplete observations fail-closed.
5. Recheck all existing materials, unknown names, context archive boundaries,
   source ordering, reference-door construction and movement. Resume the bulb,
   comparator and torch milestones only after that boundary is verified.

This is a medium-sized shared-foundation change across Minecraft primitives,
translation/observation and context/admission validation. It is broader than the
planned new device bindings. It does not include a new public MCP API, entities,
inventory, fluids or expanding piston payload support.

## Work preserved at the stop

Milestone 1 uses Rust definitions for repeater neighbor, shape, tick, insertion
and removal callbacks. Dedicated repeater queue payload/delivery was removed.
The short-pulse follow-up follows the target's post-notification reservation
order. Current execution/review is v9, device programs v3, with a new repeater
callback Law v2; earlier contexts require explicit fresh review.

At the original stop, no copper-bulb kind, comparator internal state, torch
history integration or physical-admission redesign had been implemented. Validation of the repeater
change is recorded in the roadmap and its evidence manifest.

## Approved implementation

The prerequisite is implemented. `physical.rs` owns checked Rust
geometry, conducting/supporting faces, attachment requirements and dust
connection rules. Built-in device bindings reference these declarations. Current
electrical queries, construction support and observation classification share
them. Classification alone does not authorize simulation or placement.

The historical `SpatialLaws` projection remains immutable: its support and
signal-routing approximations are part of older execution contracts. It is not
used to infer the current callback adapter's conductor/support faces. Spatial,
piston connection and piston payload each own a separate kind adapter; a future
kind must be admitted explicitly to each ABI. Diagnostic spatial queries on an
unadmitted kind have no support/emission paths. Executor admission rejects such
kinds before calculation, rather than treating them as another block.

Execution and root exploration now use v10, device bindings v4 and physical
admission v1. Old v9 contexts require fresh review. Compatibility solver errors
now distinguish an unsupported block from failure to converge. No new live
Minecraft trial is part of this prerequisite.
