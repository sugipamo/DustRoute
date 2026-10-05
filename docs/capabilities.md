# Current capabilities

[English](capabilities.md) · [日本語](capabilities.ja.md)

Baseline: the develop snapshot integrated on 2026-10-05. This table describes
public workflows and their conditions, rather than every internal API.
Start with the [overview](../README.md); choose a [workflow](workflows.md)
after selecting a task. Detailed references are English-first.

## Supported work and boundaries

| Area | Available | Limits / unavailable |
| --- | --- | --- |
| Observation | Gaze, selected regions and explicit coordinates; block identities, properties and evidence | Missing chunks, unsupported reconstruction or gaze shapes can refuse. Hidden server events are not observable. [Reference](mcp-public-features.md#observation-backends) |
| Circuit analysis | Connections, directed signals, physical hierarchy, expressions and opt-in bounded truth tables | Arbitrary complete large-circuit analysis; partial/budget-exhausted results are not proofs. [Reference](physical-function-model.md) |
| Hypothetical edits | Add/remove/change properties; immutable revisions, branches, simulation and scoped persistence | Revision merge; indefinite retention of ordinary circuit revisions. [Reference](circuit-revisions.md) |
| Electrical simulation | Supported dust, repeaters, comparators, torches, observers and other registered device behavior | All blocks/states or exact live timing in every case. Native client frames are not server ticks. [Reference](world-execution-context.md) |
| Pistons | All six facings in one runtime, mixed interactions, conductor power and quasi-connectivity | Unregistered payload/destruction behavior and entity interaction. [Reference](custom-piston-assembly-placement.md) |
| Adhesion, shape and support | Slime/honey branches and shared twelve-block limit; admitted dry slabs/stairs and attachment support loss | Every material, waterlogged state, panes or entity adhesion. [Adhesion](piston-adhesion.md), [stairs](stairs-runtime-readback.md), [support](support-loss-runtime.md) |
| Piston doors | Fixed 1×2 construction/recognition/open-close; reference 3×3 typed review, placement and removal | Arbitrary door recognition; a general 3×3 live operation/completion tool. [1×2](piston-door-mcp-v1.md), [3×3](piston-door-type.md) |
| Flying machines | Two registered engine families; generated finite trips, adoption, empty-corridor placement, arrival diagnosis and removal | New-engine invention, unlimited tracking, repeated travel, passengers or cargo. Generator distance is 1–16 blocks. [Reference](flying-machine-generation.md) |
| Crops | Declared mature pumpkin/melon destruction; modeled cane destruction/support loss in a fixed admitted environment | Growth, flowing water, collection or continuous farming. The cane-machine example has model/MCP regressions, not a new live Java comparison. [Reference](existing-machine-modification.md) |
| Blueprints | Pinned parts, types, ports and obligations; explicit proposals, fresh review/adoption and durable catalogs | Adoption alone does not write Minecraft or make an old pass current evidence. [Reference](blueprint-mcp.md) |
| Building authoring | Named geometry, cutouts, exact air spaces and one adopted equipment Assembly, which may contain nested devices | Six cube materials and at most 256 unique non-air blocks in the generator. No general terrain clearing or automatic equipment wiring. [Reference](blueprint-building-design.md) |
| Command placement | Supported built-ins, adopted Assemblies and captured-site revision edits; ordering, preview, readback and conditional recovery | OP commands, write policy and complete fresh context required. New-target Assembly construction requires an empty guarded volume. [Reference](mcp-public-features.md#execution-and-recovery) |
| Region jobs | Partition a captured-site revision, check stages in the whole context, diagnose progress and freshly plan forward/reverse work after restart | Up to 64 stages of 64 declared changes within the 4,096 non-air/virtual-edit budgets. Coupling may require another partition/intermediate design. [Reference](large-circuit-regions.md) |
| Survival building | Public inventory-based planning/execution, temporary works, movement, cleanup, retreat and sealed-checkpoint continuation; one non-OP builder | Dry passive property-free cubes and explicitly bounded edit/travel/temporary space. No active-circuit construction, chest supply, resource gathering or arbitrary terrain. [Reference](survival-public-construction.md) |
| Diagnosis and repair | Circuit findings, exact design differences and supported repair/reconstruction proposals | Universal recovery or proof of cause/ownership. Assembly reconstruction currently tears down and rebuilds. [Reference](assembly-diagnosis.md) |
| Optimization | Fixed-endpoint nonbranching dust paths, compatible macro replacement and bounded typed block-count search | General global optimality or guaranteed minimum block count. [Reference](blueprint-block-reduction.md) |
| Restart and cancellation | Durable source/history records; reobservation and new plans; survival continuation from sealed idle checkpoints | Arbitrary crash recovery, replay of lost native operations or unconditional rollback. Most executable plans do not survive restart. [Reference](mcp-public-features.md#execution-and-recovery) |
| Connection | Bundled Voxrig; Java 1.21.11, offline authentication; normal reads and supported survival actions need no MOD | Online authentication and other live MCP versions. Command previews/approach/construction retain their own permissions. [Reference](../crates/dustroute-mcp/SETUP.md) |

Observation coverage, model admission and public execution contracts differ.
Piston simulation and Assembly construction, for example, do not make pistons
eligible for every transition-test tool.

## What has been checked live

| Evidence | Declared result | Interpretation |
| --- | --- | --- |
| [2026-10-05 native Blueprint and region trials](blueprint-iteration-live-validation.md) | Author/review/adopt/place/edit/undo/remove and process-restart checks; 95 MCP calls, 13 expected refusals, 106,552 independent cell comparisons; sites restored | Fresh evidence for these finite workflows after internal JSON removal, not all circuit physics |
| [2026-10-03 survival trials](survival-public-construction.md#verification) | Public non-OP 115-step roof, 18 temporary removals and retreat; separate-process continuation after placement/mining checkpoints | Bounded construction and sealed-checkpoint continuation, not arbitrary crash recovery |
| [Door trials](reference-door-live-construction.md) and [flight trials](flying-machine-lifecycle.md) | Recorded construction, operation/arrival, diagnosis and teardown cases | Earlier version/backend-specific evidence; retained observations are historical, not new execution authority |

A model pass establishes declared requirements under its recorded context. A
live readback checks received/reconstructed state under that operation's
contract. Neither proves all-input behavior, atomic server state, hidden-queue
emptiness or immunity to later edits.

## Next pages

- [Getting started](getting-started.md): connection and construction paths.
- [Workflows](workflows.md): observe, prototype, adopt, construct and recover.
- [Public MCP reference](mcp-public-features.md): tools, budgets and ID lifetimes.
- [Documentation map](README.md): feature specifications and retained evidence.
