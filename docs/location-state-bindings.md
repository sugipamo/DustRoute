# Explicit location-state observations

The location-terminal migration now has a declaration and sampling path. A
terminal keeps its resolved coordinate while the physical world changes. This
has a [separate native exploration and review path](runtime-location-review.md)
for supported repeated-settling bindings. Sampling alone remains diagnostic.

## Observation and binding

`RuntimeView::observe_location` returns the current block, coordinate, enclosing
tick section and available carrier history. It preserves `MovingPiston` and its
carried block as separate facts. A retracting body is observed as a carrier at
the body's coordinate, not prematurely as the retracted piston it contains.
Progress, last progress, saved time and carrier lifetime remain available even
when the serialized block fields themselves have not changed.

Unknown coordinates, coarse evidence, unavailable required properties and
missing motion history never become known Air or Boolean false. A supported
predicate can return false for a fully known different state; for example, a
moving carrier is present but is not a `Solid` block at that coordinate.

`LocationPredicate` currently provides these explicit Boolean observations:

| Predicate | Meaning |
| --- | --- |
| `present` | Any known non-Air block, including a moving carrier |
| `block_kind` | The current block kind; no projection through a carrier |
| `powered` | The stored Boolean state of the selected supported block kind; not a query of power delivered from neighbors |
| `piston_state` | The state property of an actual Piston block |
| `motion_progress` | The separate runtime Zero/Half/Full progress of an actual moving carrier |

The `powered` observation supports Lever, Repeater, RedstoneTorch and
RedstoneLamp. Missing state is unknown. `piston_state` is false for a carrier
whose payload happens to be a piston. `motion_progress` is false when the actual
block is not a moving carrier. It never reinterprets the legacy 0/1 block metadata
marker as half-step motion history.

An immutable source can bind a behavioral type to explicit observations:

```json
{
  "behavior_type": "example.presence-relation.v1",
  "observed_inputs": {
    "control": {
      "observation": "location",
      "port": "input",
      "predicate": {"kind": "powered", "block_kind": "Lever", "powered": true}
    }
  },
  "observed_outputs": {
    "occupied": {
      "observation": "location",
      "port": "output",
      "predicate": {"kind": "present"}
    }
  }
}
```

The selected type supplies the named Boolean relation or autonomous requirement;
it contains no coordinate or classification-name interpretation. Each location
observation requires an explicit `BlockState` port. `signal` observations retain
the existing electrical port meanings and require their corresponding adapter.
Every type input/output must be mapped, with matching port direction. Independent
inputs need distinct named physical ports. Several named outputs may observe the
same location through different predicates. The same explicit form supports
`RepeatedSettling`, `Periodic` and `FiniteBurst` declarations; catalog validation
does not prove that a realization satisfies any of them.

## Sampling the moving world

[`LocationBehaviorBinding::resolve`](../crates/dustroute-translate/src/location_behavior.rs)
resolves a declared location-only binding through a concrete Assembly occurrence,
including its rotation and translation. Its sampler reads those fixed coordinates
from the selected piston runtime, including the standard
[electrical context](custom-piston-assembly-placement.md). It returns named
Boolean values together with the actual observations, including intermediate
carrier state. It does not advance physics, relocate a port, follow a payload,
rewrite a source or change a child reference. An undeclared reinterpretation is
rejected. Mixed signal/location observations need their own sampling adapter and
are not silently converted to block presence by this location-only sampler.

These samples are diagnostic data. They are not a proof over all input histories,
do not establish that a supplied runtime began from a particular Assembly, and
cannot authorize promotion or adoption. The separate native behavioral constructor
now establishes that relationship and retains the complete execution context.

## Compatibility and contextual verification

All catalogs, including explicit observation bindings, now use
`dustroute.blueprint-catalog.v13`. Retired v1–v12 archives are rejected; see the
[cutover guide](architecture-cutover.md). The
original autonomous and signal-only repeated-settling binding shapes retain their
JSON representation and validation rules, including rejection of BlockState.
Proposal histories containing explicit observation bindings use
`dustroute.blueprint-updates.v4`, including bindings present only in an unadopted
candidate. Older history versions cannot conceal or discard those declarations.

`BlockPattern`, explicit source Air and static type bindings keep their existing
snapshot meanings. A declared location predicate is a separate observation, not
permission to weaken those requirements or make them initial-only. Existing
fixed-geometry proof contexts report the new bindings as undetermined; no old
pass is inherited from a classification or compatible-looking relation.

The native runtime context, complete-state recurrence comparison, behavioral
exploration and whole-realization diagnostic review are now implemented for
location-only repeated-settling relations. That review observes intermediate
transitions and retains child/environment failures independently of a parent's
result. The existing 1×2 interruption case yields a replayable counterexample.
The context now enters the existing MCP proposal/adoption path with fresh checks
after restart. Native contexts require proposal-history v5. Optimization may
still relocate ports in a newly checked candidate.

State comparison must preserve more than these observations. Pending deliveries,
their order and tick section, carrier lifetimes, progress and last progress affect
the future. Any clock normalization must also preserve the special initial tick:
a newly constructed carrier has saved world time zero, so changing a later tick
to zero can change the reversal decision before that carrier is first ticked.
Discarding a retired carrier's queued delivery can also remove an external-input
boundary. The [pinned behavioral comparison](runtime-location-review.md#what-is-compared)
now retains these distinctions; exact runtime checkpoints remain separate.

## Regression coverage

The library tests cover unknown versus known-false observations, Air/carrier/solid
at one coordinate, independent motion history, carrier versus contained piston
state, exact binding coverage, direction/kind errors, immutable references and
archive downgrade rejection. Translation tests exercise resolved multi-output
observations through all four rotations and ON/OFF movement, and ensure that an
old proof context cannot certify the new binding.

Scoped verification on 2026-09-21 passed **142 tests** with zero failures across
the library, runtime, location sampling, review, proposal-history and Blueprint
MCP suites. Workspace all-target Clippy passed with `-D warnings`; translation
all-target Clippy was repeated after the history-schema addition. Formatting and
diff whitespace checks passed. The full workspace test suite was not repeated
for this declaration/observation increment; the preceding physical adapter run
is recorded separately in its documentation.

```bash
cargo test -p dustroute-library
cargo test -p dustroute-translate --test location_behavior
cargo test -p dustroute-translate --test repeated_settling_adoption
```
