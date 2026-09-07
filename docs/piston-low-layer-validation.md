# Piston low-layer validation

This document summarizes the current implementation and retained evidence.
For user-facing construction and operation, use the
[fixed 1×2 contract](piston-door-mcp-v1.md). The observations below do not promote
general piston placement or arbitrary door layouts.

## Validated progression

The [coordinate fixtures](../crates/dustroute-translate/tests/fixtures/piston-low-layer)
cover the following bounded cases:

| Case | Coverage |
| --- | --- |
| `01-normal-stone` | Horizontal normal piston pushes one ordinary block |
| `02-sticky-stone` | Sticky extension and return of the ordinary payload |
| `03-sticky-piston` | Moving an explicitly retracted, unpowered horizontal piston |
| `04-moved-piston-actuation` | Actuating the previously moved piston |
| `05-single-piston-passage` | One-block passage and settled mechanical states |
| `06-two-row-passage` | Opposing independent rows forming a 1×2 panel |
| `07-single-input-two-row` | Single lever and supported dust fanout driving both rows |

The last fixture is the basis of the fixed public 1×2 door. It was compared on
Java 1.21.11 in three independently rebuilt closed/open/reclosed trials, with
stable-state and forward-walking checks. This is settled block-state evidence,
not exact tick equivalence or entity simulation support.

## Deferred completion contract

`PistonComplete` revalidates its local dependencies in the current world and
constructs a fresh delta before applying it. Checks cover the moving carriers,
piston body/head, legacy payload/destination cells, and the empty source read
by a no-payload sticky retraction. Conflicting destinations, shared retraction
payloads, and altered carriers are rejected.

Independent movement or distant changes no longer fail solely because another
operation changed the global shape after start. Direct `WorldDelta` application
still checks the original parent shape and exact before states; it has not
become a general rebase operation. The known planning region remains static.

Regression entry points:

```bash
cargo test -p dustroute-translate --test piston_completion
cargo test -p dustroute-translate --test piston_payload_push
cargo run -p dustroute-translate --example piston_completion_isolation
```

## Event budget and normalization

The case-07 diagnostic requires 67 events for ON, 383 for OFF, and 67 for the
next ON: 517 across that sequence. The former 512-event stop was a cumulative
diagnostic-budget mismatch. The replay supplies a finite budget of 512 times
the declared action count; the engine's cumulative accounting is unchanged.
A recorded 20-cycle offline probe used 9000 events with an empty queue after
every edge. It is bounded-settling evidence, not a performance or endurance
certification.

The normalized low-layer comparison reconciles numeric string dust power from
Mineflayer with numeric simulator power. The public exact door contract
separately retains all raw Java block-state properties, including dust shape,
lever attachment and piston/head metadata.

## Reproduction and evidence

Use [E2E instructions](../crates/dustroute-mcp/mineflayer/e2e/README.md) for the
private Java server and live harnesses. Offline replay:

```bash
cargo run -p dustroute-translate --example piston_low_layer_replay -- crates/dustroute-translate/tests/fixtures/piston-low-layer/07-single-input-two-row.json
```

Retained evidence includes:

- [Completion isolation](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-completion-isolation-summary.json).
- [Budget diagnosis](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-event-budget-summary.json).
- [Single-input success](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-single-input-success.json).
- [Normalized stable observations](../crates/dustroute-translate/tests/fixtures/piston-low-layer-single-input-observation.json).
- [MCP observation/open-close](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-door-mcp-summary.json).
- [MCP placement and undo](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-placement-mcp-summary.json).

Earlier failure JSON files remain regression evidence; their historical status
is not the current product status. Full transient client recordings belong in
ignored `.local/` artifacts. Exact Vanilla same-tick ordering, interruptions,
short pulses, larger doors, slime/honey and general placement remain outside
this bounded validation.
