# Architecture and development

Run commands from the repository root. For product workflows, start with the
[documentation index](README.md) and [public feature guide](mcp-public-features.md).
For Java server/bot setup, use [MCP setup](../crates/dustroute-mcp/SETUP.md).

## Workspace responsibilities

| Crate | Responsibility |
| --- | --- |
| `dustroute-codec` | Bounded versioned non-JSON storage and canonical value encoding |
| `dustroute-minecraft` | World primitives, block models, events, scheduler profiles and validation |
| `dustroute-physical` | Physical observations, evidence, ports, nets, fragments and patches |
| `dustroute-ir` | Derived signal/gate/expression/function and temporal representations |
| `dustroute-library` | Reusable specifications, provenance and compatibility evidence |
| `dustroute-translate` | Forward compilation, reverse interpretation and bounded simulation |
| `dustroute-optimize` | Candidate search and structural/behavioral verification |
| `dustroute-app` | Shared application services and placement planning |
| `dustroute-mcp` | MCP orchestration, observed/revision/operation identities, native client adapter and survival jobs |

Forward compilation follows logical intent → primitive lowering → cell mapping
→ placement/routing → legality validation → Java export. Reverse interpretation
starts with observed blocks and preserves physical evidence before deriving
higher-level labels. `PlacementCircuit` is proposed geometry, not observation.

Normal compilation retains an `Assembly` containing the connected block state,
pinned blueprint occurrences and explicit port routes. Source Blueprint Revisions
and placed Assembly Revisions have separate immutable identities. Archive loading
checks structural integrity; `validate_assembly` checks the reconstructed actual
world and connections again. See the [blueprint architecture](blueprint-architecture.md)
for persistence, shared membership and the remaining migration boundaries.

Cells retain source IDs through replacement and rotation. Optimized assemblies
can be captured with `RealizedOptimization::capture_assembly` and must pass
`validate_assembly` against the source catalog before use. MCP built-in planning
performs this check. For observation storage, `assembly_from_snapshot` retains
declared native states; the analysis-oriented `world_from_snapshot` can infer
wire shapes and must not be used to overwrite the saved actual state.

Blueprint parents retain internal connection proposals and explicit child-port
bindings. Capture copies inherited proposals into separate assembly routes;
validation checks the actual routes and every requirement behind those bindings.
Use `capture_assembly_in_catalog` when sources come from a caller-owned catalog.
Raw geometry adapters reject contracts they cannot carry through validation.

For catalog-driven candidates, use `CellLibrary::from_blueprints` with explicit
gate-to-classification bindings, then `BaselineCompiler::compile_with_library`
or `realize_staged_optimization_with_library`. Source catalogs are retained and
the composed result passes `validate_assembly_occurrences`. Gate simulation is
separate from physical type checks. For arbitrary multi-port/nested replacements,
use `plan_blueprint_replacement` and
`materialize_macro_replacement_in_assembly` with explicit source context and
port mappings. These return proposals; they do not adopt immutable revisions.
See [candidate selection](blueprint-architecture.md#catalog-driven-candidate-selection-and-replacement).

For initial-state blueprint promotion, use `PromotionCandidate::prepare` →
`validate` → explicit `PromotionReview::adopt` in `dustroute_translate::promotion`.
The review checks parent and descendants separately, including shared parts;
failed or undetermined required checks block adoption. `group_as_blueprint` only
prepares unverified data. Captured parent layouts do not rewrite child sources.
Behavioral and live evidence still require their existing verification paths.

`dustroute_translate::blueprint_update::BlueprintUpdates` adds explicit child
update proposals over that gate. Supply the base parent/state, selected child,
new parent/intermediate definitions and candidate Assembly Revision. Use
`create` → `diff` → `validate` → `adopt` or `reject`. `adopt` reruns verification;
serialized review history never grants adoption authority. The typed archive,
stored with the versioned non-JSON codec, retains candidates and decisions
without modifying historical sources or states. The
[workflow reference](blueprint-architecture.md#explicit-child-update-proposals)
describes path conventions and persistence. MCP exposes this workflow through
the existing revision and operation tools, without adding tool names; see the
[Blueprint MCP contract](blueprint-mcp.md). The `blueprint_mcp` adapter stores a
player-scoped catalog and proposal history with a lock across read/review/write,
atomic replacement and no plan TTL. Tests in `service_blueprint_tests.rs` exercise
actual MCP tool calls and restarts, including a forged saved pass that cannot
authorize adoption. They need no live Minecraft connection.

`World` is mutable data. General placement requires `ValidatedWorld`; edits
invalidate that proof. Persisted plans are proposals and must be checked against
fresh live state. The fixed 1×2 piston preset has a separate exact-contract
proof and does not widen general placement support. See
[validation boundaries](world-validation-boundary.md).

## Toolchain and local checks

The behavioral-type extension exposes `RepeatedSettling`, autonomous
single-output `Periodic` and `FiniteBurst` types, the completed-operation
`PistonDoor` type, and complete-state checkers in
`dustroute_translate::behavior_type`, `periodic`, `finite_burst` and
`piston_door_type`. These are model-level
checkers. `physical_behavior::PhysicalBehaviorModel` connects actual
Assembly Revisions, explicit port bindings and pinned dust/torch laws, including
hidden torch history and pending recovery. Declared obligations can
use explicit contextual review/adoption through the existing MCP tools;
[repeated-settling adoption](repeated-settling-adoption.md) checks complete port
mappings and actual input controls using universal conservative abstraction.
The [periodic foundation](periodic-behavior-status.md)
describes the supported initial condition and limits. The [extension status](blueprint-architecture.md#behavioral-types-and-executable-laws-current-extension)
records the accepted guarantees. See [physical behavior](physical-behavior.md)
for execution assumptions, proof limits and the manual reachability measurement,
and [torch laws](torch-laws.md) for server-observation evidence.

The [Blueprint block-count search](blueprint-block-reduction.md) uses the same
fresh contextual gates and returns new immutable candidate data. Its regression
checks a 6-to-2 component NOT reduction (7-to-3 including its external lever), both moved terminals, shared-block counting,
changed nesting, law pinning and explicit parent adoption after MCP restart.

```bash
cargo test -p dustroute-translate --test behavior_types
cargo test -p dustroute-translate --test physical_behavior
cargo test -p dustroute-library --test periodic
cargo test -p dustroute-translate --test periodic --test physical_periodic
```

The workspace manifest declares Rust edition 2024 and `rust-version = "1.85"`.
Use a current stable toolchain for the existing formatting/lint/test workflow.
Live integration additionally uses Java 21 with the pinned
Minecraft Java 1.21.11 environment. Rust-only tests do not need Minecraft.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The GitHub Actions workflow has been removed. These are local integration
checks, not checks automatically run on push or pull request. CI configuration,
`.cursorignore` and `.gitattributes` are intentionally absent; `.gitignore`
continues to exclude build products, local worlds, dependencies and secrets.

Run narrower tests for a bounded change, and the workspace checks before
integration. Do not restart live servers merely to validate documentation.

## Rust diagnostics and export APIs

The CLI has been retired. Product work uses MCP; offline debugging calls typed
Rust APIs directly. `DustRouteService::built_in_circuit` provides logical
evaluation, and `DustRouteService::analyze_world` accepts a world and a bounded
`ReverseRequest`. `compile_builtin` and `compiled_circuit_datapack` provide
circuit export; `semantics_datapack` provides semantics export. Datapacks can
still be written as ZIP files with `write_zip`; removing the CLI does not remove
these library capabilities or add equivalent MCP tools.

MCP leaves exhaustive truth tables opt-in. An incomplete interface or exhausted row/time/
solver budget must return unavailable/error evidence, not partial rows labeled
as a complete function. See [function modeling](physical-function-model.md).

The retained 3×3 piston scenarios are diagnostic-only. Their API regression tests
and limits are in [piston diagnostics](piston-diagnostics.md); successful diagnostic
execution does not authorize construction.

## Snapshot format

The example below is public MCP/explicit fixture JSON, not an internal storage
format. Rust code passes typed observations; persisted records use the non-JSON
codec. See [JSON boundaries](json-boundary-migration.md).

`properties` retains observed Java block-state values. An absent cell means air
only inside a complete declared observation; boundary completeness is separate.

```json
{
  "min": {"x": 0, "y": 100, "z": 0},
  "max": {"x": 20, "y": 110, "z": 20},
  "blocks": [{
    "pos": {"x": 1, "y": 101, "z": 1},
    "name": "minecraft:repeater",
    "properties": {"facing": "west", "delay": "1", "powered": "false", "locked": "false"}
  }]
}
```

## Evidence and benchmarks

Built-in cell layouts are frozen blueprint data. Use typed Rust APIs and the
explicit test-only [fixture adapters](development-fixture-adapters.md), with the
reproducibility checks in [blueprint architecture](blueprint-architecture.md),
when changing those definitions. Runtime lookup must not call their generators.

Sanitized regression observations are tracked under
`crates/dustroute-translate/tests/fixtures` and
`docs/evidence/legacy-mineflayer`. Runtime recordings, server JARs,
worlds, logs and player lists remain in ignored `.local/` directories.
Historical failure fixtures are retained deliberately; they are not current
feature status or a request to reimplement obsolete goals.

Use [differential testing](physics-differential-testing.md),
[server instrumentation](vanilla-instrumentation.md), and the
[native rollout](voxrig-rollout.md) for current evidence capture. The former
[harness guide](evidence/legacy-mineflayer/README.md) is historical context. Timing claims must preserve the distinction between client packet
order and observed server internals.

Use [performance observation](performance-observation.md) for reproducible
release benchmarks. Measurements and verified candidate contracts remain
separate from general performance claims.
