# DustRoute

[English](README.md) · [日本語](README.ja.md)

DustRoute helps an AI and a player inspect, prototype and build Minecraft
redstone circuits and supported structures together. Observe an existing site,
try changes in a model, review the plan, then apply and check the authorized work.

You can ask an MCP-connected AI:

> Explain this circuit, including what you cannot determine.

> Compare a version with a different repeater delay before changing the world.

> Build this adopted design using the materials in the bot's inventory. Show the
> construction plan and temporary works first.

## What you can do

| Goal | Current scope |
| --- | --- |
| Understand a circuit | Gaze or region observation, connections, diagnosis and bounded functional analysis |
| Try an alternative | Immutable hypothetical revisions and model checks before live changes |
| Reuse a design | Blueprint parts, types, requirements, reviewed updates and adopted Assemblies |
| Place a mechanism | Supported redstone, mixed-direction pistons, typed doors and finite flying machines |
| Author a building | Explicit parts, openings, air spaces and supported equipment composition |
| Build in survival | Bounded passive cube designs, supplied inventory, temporary works, cleanup and retreat |

Read the [capability table](docs/capabilities.md) for conditions and exclusions.
Support for observation, simulation and construction is different; recognizing a
block does not make every operation on it available.

## Start here

1. Read [getting started](docs/getting-started.md) for connection and permissions.
2. Choose a [workflow](docs/workflows.md) for observation, design or construction.
3. Use the [documentation map](docs/README.md) to reach API contracts and evidence.

The live client is the bundled Rust library Voxrig. The current MCP connection
requires Minecraft Java 1.21.11 and offline authentication. Normal block reads
use received packets and supported client reconstruction without a server MOD
or per-cell confirmation commands. Mineflayer and the product CLI are retired.

Command-based circuit/Assembly placement requires operator permissions. The
separate supported survival workflow uses inventory and ordinary player actions
without OP. World mutation is disabled by default; both paths require the
applicable policy and explicit review/confirmation.

Model validation checks declared requirements under a recorded context. Live
verification checks fresh observations; neither freezes the world nor proves
that hidden server queues are empty. Arbitrary blocks, entity-based mechanisms, unrestricted
terrain work and fully autonomous general design are outside the current scope.

## For developers

Use the [architecture and development guide](docs/development.md) for Rust
library boundaries and local checks. Detailed technical references may be
English-only. English is the standard documentation language; front-facing
pages also have Japanese versions. See the
[documentation policy](docs/documentation-policy.md).
