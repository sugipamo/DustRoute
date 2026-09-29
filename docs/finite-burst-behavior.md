# Autonomous finite-burst behavior

`FiniteBurst` is a circuit behavioral requirement: from the declared initial
state, one Boolean output must complete **at least two ON-to-OFF transitions**
and eventually remain OFF forever. It has no external control inputs. This
classifies observable behavior; it does not require a physical burnout mechanism.

```json
{"kind":"finite_burst","requirement":{"output":"signal"}}
```

An initially ON output counts when it actually falls. No edge is invented before
the initial sample. An always-OFF output or a single pulse fails. There is no
exact pulse count above the minimum, stopping deadline, duty, phase or fixed
port coordinate. A different implementation may satisfy the same requirement.
Restartability is a separate requirement and is **not verified** here.

## Complete-state proof

[`verify_finite_burst`](../crates/dustroute-translate/src/finite_burst.rs) and
[`verify_periodic`](../crates/dustroute-translate/src/periodic.rs) share exact
autonomous traversal. Verification follows the deterministic model until its
complete execution state repeats. It retains all future-relevant state,
including torch off-event ages and relative pending callback delays.

For a finite-burst pass, the recurring cycle must contain only OFF outputs,
and the preceding trace must contain at least two falling edges. Internal state
may continue cycling while the output stays OFF. A long observed pause alone
cannot prove permanent silence. A cycle containing any ON output fails;
unclosed exploration, unsupported execution or exhausted budgets is
`undetermined`, never a pass. Budgets limit computation, not circuit timing.

The diagnostic `cessation` records the observed falling-edge count, earliest
`off_from_step`, complete-state cycle start and cycle length. These are results
of that implementation, not additional type constraints. It is also present
when an OFF cycle is found with too few falling edges; always inspect `status`.

The physical adapter accepts an empty input map and exactly the named output.
`PhysicalBehaviorModel::verify_finite_burst` retains the actual Assembly, type,
law records, bindings, execution profile and initial-condition assumptions in
its report. The current context requires fresh construction: declared initial
torch lit state, empty histories and no pending callbacks before the initial
notification. A running-world block snapshot does not establish this state.

## Blueprint review and adoption

Use a new immutable Type Revision and bind it to an existing signal output:

```json
"behavior_bindings": [
  {"behavior_type":"burst.type.v1","output_port":"pulse"}
]
```

The type's output name and the physical port name may differ. Consumer inputs
can also request this type through `required_source_types`. Classification
labels and catalog membership do not establish the requirement.

Fresh contextual review runs the obligation in the complete actual Assembly,
including connections and shared geometry. For the observed single-torch scope,
select this context explicitly:

```json
{
  "profile":"dustroute.dust-single-torch-block-effects.v1",
  "initial_condition":"fresh_construction",
  "dust_law":"dustroute.law.dust-strength.v1",
  "torch_law":"dustroute.law.torch.java-1-21-11.v1",
  "max_electrical_iterations":128
}
```

Without context, a behavioral obligation remains undetermined. Promotion and
adoption require every applicable check to pass: a passing finite-burst parent
cannot hide a child's failed periodic obligation. Validation never rewrites a
child or follows a newer Revision automatically. Legacy projections that cannot
preserve these obligations refuse the source. Existing MCP Assembly reads and
proposal/review/decision tools carry the contract; no burst-specific endpoint
was added. Saved diagnostics cannot authorize adoption; checks run again after
restart. See the [MCP workflow](blueprint-mcp.md#periodic-obligations).

All catalogs now use `dustroute.blueprint-catalog.v13`; retired v1–v12 archives
are rejected. The MCP response envelope is unchanged. Every update archive now
uses v5; old update versions and archives containing retired catalogs fail loading.
See the [cutover guide](architecture-cutover.md).
Contextual MCP reports for a catalog containing a finite-burst type
use the additional scope value
`placement_connections_and_declared_behavioral_obligations`; the existing
periodic scope value remains unchanged for earlier catalogs. Neither scope
certifies the live world.

## Recorded four-block circuit

The [retained clock comparison](periodic-clock-conformance.md) uses a wall torch,
its support, an upper block and dot dust. Its requirements now have distinct
results under the explicit block-effects profile:

| Requirement | Model result | Reason |
| --- | --- | --- |
| `Periodic` | Failed | The recurring output is constant OFF. |
| `FiniteBurst` | Passed | Eight falling edges, then OFF from game tick 30. |
| Restartability | Not verified | No restart contract has been defined. |

The complete-state cycle begins at step 91 and has length 1; 92 distinct states
are retained. No burnout history was discarded to reach that result. The old
synchronous profile instead has recurring bursts and fails `FiniteBurst`.
Its known mismatch with the server remains a compatibility regression.

The repaired profile still matches all 641 autonomous, 261 notification-recovery
and 33 queue-diagnostic samples, as well as 3,888 isolated torch samples.
These finite observations support the model comparison; they do not prove
permanent silence in every live world. The recorded external neighbor
notification can restart this circuit, but that diagnostic does not establish
a typed signal input or a repeated-use restart guarantee.

```bash
cargo test -p dustroute-library --test finite_burst
cargo test -p dustroute-translate --test finite_burst --test physical_periodic
cargo test -p dustroute-translate --test periodic_clock_observation
cargo test -p dustroute-mcp finite_burst_blueprint_uses_existing_tools_and_reverifies_after_restart
```

## Subsequent work

Before adding restartability, define the allowed trigger, applicable stopped
states and required behavior after each trigger. External neighbor notifications
must not silently become wired signal inputs. A separate
[history abstraction](abstract-behavior-verification.md) now verifies
repeated-settling NOT behavior; finite-burst checks retain exact traversal.
The separate [movable-port block-count search](blueprint-block-reduction.md) now
uses fresh contextual proofs to select smaller candidates. Broader internal-state
types, concrete-state compression, multiple-torch scheduling and new live-world
deployment remain later work. No existing requirement or old Revision is weakened
to make this candidate pass.
