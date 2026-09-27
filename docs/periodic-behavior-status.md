# Autonomous periodic behavior

Implemented on `codex/blueprint-architecture`, 2026-09-20. Periodic types,
complete-state recurrence checking, and contextual Blueprint review/adoption
are available for bounded dust/torch execution profiles. The first live clock
comparison exposed a divergence in the original synchronous profile. The new
single-torch block-effects profile matches the observations and **rejects the
four-block candidate's periodic requirement**. See the
[observations, repair and scope](periodic-clock-conformance.md). A working
autonomous clock candidate remains subsequent work.

## Approved first contract

`TypeContract::Periodic { requirement: Periodic { output } }` describes one
named Boolean output and no external control inputs. Starting from the declared
initial state, after some finite startup, a nonconstant waveform must repeat
indefinitely. Both ON and OFF must occur in the recurring waveform.

The type fixes no numerical period, high/low duration, startup deadline, phase
or port coordinates. Bursts followed by burnout/recovery pauses are allowed if
the entire pattern recurs. A constant output or transient pulses followed by
permanent silence fail. Different realizations may have different timing and
geometry while satisfying the same type.

The environment is explicit: actual blocks, known surrounding space, fixed law
Revisions, initial memory assumptions, output binding and execution profile.
The current adapter rejects assemblies exposing an external input boundary for
periodic verification. It does not silently freeze such an input. Physical
levers without an external input binding remain at their declared initial level.
Control inputs, multiple periodic outputs and numerical waveform constraints
remain later extensions. The existing `RepeatedSettling` contract retains its
arbitrary-input, repeated-use semantics.

## Verification and evidence

[`verify_periodic`](../crates/dustroute-translate/src/periodic.rs) follows the
autonomous deterministic model until its **complete execution state** recurs.
The state includes torch registers, local input levels, off-event ages and
relative pending callbacks. Output samples alone cannot establish recurrence.
No history compression or approximate equality is used.

The report separates:

- `startup_steps`: the prefix before the recurring complete-state cycle; this
  is sufficient startup, not necessarily the earliest output repetition.
- `state_period_steps`: the complete-state cycle length.
- `output_period_steps` and `waveform`: a minimal output period at cycle entry.
  The output period can be shorter than the complete-state period.

A closed cycle with both output levels passes; a closed constant-output cycle
fails. Unknown execution, invalid bindings, electrical nonconvergence or exhausted
state/step/time budgets cannot pass and remain undetermined. Budgets limit
computation, not the circuit's allowed startup or period. Closure reached exactly
at the state/step budget boundary is accepted.

`PhysicalBehaviorModel::from_fresh_assembly` accepts the new type with an empty
input map and exactly its named output. `verify_periodic` returns diagnostics
including the actual Assembly/type/law records and bindings. The explicit initial
condition is fresh construction: declared torch lit state, empty histories and
no pending callbacks before initial notification. A running-world block snapshot
does not establish that hidden-state assumption.

The original `dustroute.dust-torch-synchronous-game-tick.v1` remains unchanged.
Explicitly select `dustroute.dust-single-torch-block-effects.v1` for synchronous
block-change notifications before local callback processing resumes. Both use
selected executable laws and the fixed electrical topology. The new profile
rejects multiple torches and neighbor handlers that directly change `lit`.
See [physical behavior](physical-behavior.md) for supported blocks and limits.

## Blueprint obligations and adoption

A Blueprint Revision can declare an obligation on a named physical output:

```json
"behavior_bindings": [
  {"behavior_type": "clock.type.v1", "output_port": "pulse"}
]
```

This is a requirement, not a certificate. The Type Revision may name its output
`signal` while the physical port is named `pulse`. Port coordinates are resolved
from each actual occurrence, including nested aliases, translations and rotations.
The binding requires a signal output port and either a periodic or
[finite-burst](finite-burst-behavior.md) type. Consumer inputs can also include
these Type Revisions in
`required_source_types`; the connected producer output is then checked.

`review_assembly_in_context` and `validate_assembly_in_context` evaluate these
obligations in the **complete actual Assembly**, including shared geometry and
connections. Every declaring occurrence retains its own result. A passing
parent cannot hide a failed or undetermined child, and no check rewrites source
Revisions. Without an explicit execution context, behavioral requirements stay
undetermined. Legacy cell projections that cannot retain obligations refuse them.

An execution context pins these assumptions:

```json
{
  "profile": "dustroute.dust-single-torch-block-effects.v1",
  "initial_condition": "fresh_construction",
  "dust_law": "dustroute.law.dust-strength.v1",
  "torch_law": "dustroute.law.torch.java-1-21-11.v1",
  "max_electrical_iterations": 128
}
```

`PromotionCandidate::validate_in_context` checks a grouping candidate without
publishing it; explicit adoption checks the captured dependencies and runs fresh
validation. Child-update requests retain `behavior_context`; review and adoption
both execute fresh checks. Persisted reports, including forged historical passes,
cannot authorize adoption. Failure leaves old pins and the catalog intact.
Verification budgets are shared across obligations within one review; identical
checks may be reused only within that exact review/context.

Catalog v4 preserves periodic types and bindings. Catalogs containing a
finite-burst type use v5, which also preserves periodic contracts. All versions
v1 through v5 remain readable; labels earlier than v4 cannot contain periodic
types. Update archives retain their existing envelope and an explicit context
where needed. No new MCP tool or automatic adoption path was added: existing
Assembly reads and proposal/review/decision tools carry these fields. See the
[MCP contract](blueprint-mcp.md#periodic-obligations).

## Regression evidence

The four-block feedback fixture contains a wall torch, its support, a block above
the torch and dust on the support. Under the original synchronous model, it closes
after 220 distinct states: a 30-game-tick startup followed by a 190-game-tick cycle.
The cycle contains 16 ON samples and 174 OFF samples, including the burnout
pause. All hidden memory is retained. Tests replay three complete modeled cycles;
these are compatibility tests, not live clock evidence. The new effects profile
matches 641 autonomous, 261 external-notification and 33 queue-diagnostic block
samples, then rejects the constant-output recurrence as a periodic implementation.
The separate finite-burst contract passes with eight falling edges and OFF from
tick 30; this does not alter the original periodic requirement.

Changing the torch law or removing the feedback block fails a fresh periodic
check. Other regressions cover constant/transient outputs, a shorter output
period than state period, unclosed repeated samples, exact resource boundaries,
unknown context, input-boundary refusal, rotation, consumer requirements,
parent-pass/child-fail adoption, immutable dependencies, archive restoration,
forged saved success, and the existing MCP tools across a server restart.

```bash
cargo test -p dustroute-library --test periodic
cargo test -p dustroute-translate --test periodic --test physical_periodic
cargo test -p dustroute-mcp periodic_blueprint_uses_existing_tools_and_reverifies_after_restart
```

The existing temporal IR's `ClockCandidate` remains a structural feedback
heuristic, independent of this proof. Finite traces and pulse assessments also
remain observations. The older physics engine's absolute-time state key is not
used for cycle equality here.

`periodic_clock_observation` requires agreement with retained live samples under
the block-effects profile and preserves the old profile's known mismatch. It
does not certify the fixed four-block candidate as an autonomous clock. Periodic
recurrence also does not solve arbitrary-input history explosion. A separate
[conservative history verifier](abstract-behavior-verification.md) now closes
the NOT repeated-settling graph; periodic traversal remains exact. Automatic
clock discovery, block-count search, numerical timing types and live placement
remain separate work. There is no portable guarantee that a clock will pass in
a changed physical environment without rechecking it.
