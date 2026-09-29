# Documentation

Current contracts and reproducible evidence are kept here. Historical migration
plans, repeated goal updates and superseded build instructions have been
consolidated. Original observations remain in tracked regression fixtures and
Git history; diagnostic fixtures have not been promoted into public capability.

## Using DustRoute

| Document | Read it for |
| --- | --- |
| [Public features](mcp-public-features.md) | Tools, workflows, limits, ID lifetimes and recovery |
| [MCP setup](../crates/dustroute-mcp/SETUP.md) | Server, bot, transport and permissions |
| [LLM tool guide](../crates/dustroute-mcp/README.md) | Tool selection and execution decisions |
| [MCP subsystem reference](../crates/dustroute-mcp/REFERENCE.md) | Detailed examples |
| [JSON contracts](mcp-api-v1.md) | Response schemas and compatibility |
| [Circuit revisions](circuit-revisions.md) | Hypothetical editing, storage and live placement |
| [Blueprint MCP workflow](blueprint-mcp.md) | Immutable source/state catalogs, proposal review, explicit decisions and durable history |
| [Custom piston Assembly placement](custom-piston-assembly-placement.md) | Standard mixed-direction electrical context, adoption, new-target placement and conditional undo |
| [Fixed 1×2 piston door](piston-door-mcp-v1.md) | Supported construction, recognition, open/close and undo |

## Architecture and development

| Document | Read it for |
| --- | --- |
| [Development](development.md) | Workspace boundaries, local checks, CLI and examples |
| [Architecture readability audit](architecture-readability-audit.md) | Remaining responsibility, type and module boundaries; evidence and recommended refactoring order |
| [Architecture migration](architecture-migration.md) | Migration progress, preserved contracts, deferred concerns and regression evidence |
| [Architecture cutover](architecture-cutover.md) | Typed workflow boundaries, retired formats/APIs, migration impact and current checks |
| [Cutover verification and format retirement](architecture-cutover-validation.md) | Additional regressions, one current update/store format and preserved evidence |
| [Stabilization and legacy paths](stabilization-legacy-paths.md) | Retired piston profiles, removed fallbacks, saved-data impact and remaining active paths |
| [Physical IR](physical-ir.md) | Observations, evidence and derived representations |
| [Physical function model](physical-function-model.md) | Shared circuitry and bounded functional inference |
| [Component library](component-library.md) | Candidate provenance and promotion requirements |
| [Blueprint architecture](blueprint-architecture.md) | Single Blueprint specification, current boundaries, migration sequence and stop conditions |
| [World execution contexts](world-execution-context.md) | Shared law selections, retained model boundaries, initialization/input policies and state ownership |
| [World validation](world-validation-boundary.md) | Immutable placement proofs and recovery distinctions |
| [Update model](minecraft-update-model.md) | Event/transition identity, scheduling and checkpoints |
| [Synchronous world runtime](synchronous-world-runtime.md) | Versioned callback/continuation boundaries, tick context and independent carrier histories |
| [Piston callback runtime](piston-callback-runtime.md) | Unified execution and links to motion evidence; directional runtimes retired |
| [Piston unification migration](piston-unification-migration.md) | Historical migration baseline, source audit and original acceptance cases |
| [Unified piston runtime](unified-piston-runtime.md) | One electrical world/queue for all facings, explorer integration and retired-profile rejection |
| [General piston placement roadmap](piston-general-placement-roadmap.md) | Scope and completion evidence for electrical support, mixed live comparison, adoption and custom construction |
| [Persistent placed Assemblies](placed-assembly-management.md) | Durable instance records, fresh observation after restart and conditional removal |
| [Shared diagnostic system](diagnostic-system.md) | Common findings, repair handoff and pinned Assembly/Blueprint references |
| [Assembly diagnosis](assembly-diagnosis.md) | Locate design differences regardless of cause, account for current inputs and report repair blockers |
| [Electrical piston live evidence](piston-electrical-live-evidence.md) | Applied input times, observed mixed/interference/quasi results and public construction trials |
| [Piston transient conformance](piston-transient-conformance.md) | Measured input boundaries, carrier progress and callback-visible state comparisons |
| [Movable piston bodies](piston-payload-conformance.md) | All-facing ordinary/sticky payloads, shared chains, double-extender evidence and subsequent redstone integration |
| [Slime and honey adhesion](piston-adhesion.md) | Ordered branching movement, material relations, scope and verification |
| [Downloaded 3×3 reference door](reference-3x3-door-audit.md) | Static inventory, exact coordinates and implementation/validation history |
| [Typed device definitions](typed-device-runtime.md) | Rust constants, compile-time contracts, atomic properties, analog signals and concrete variant selection |
| [Device integration roadmap](device-integration-roadmap.md) | Repeater integration, waxed bulbs, comparator internal state and torch history milestones |
| [Data-driven block execution](data-driven-block-runtime.md) | Declarative queries, finite tables, ordered effects, lamp/observer migration and stone-button extension |
| [Native lamp and observer callbacks](native-device-callbacks.md) | Shared scheduler integration and source-backed device callbacks |
| [Staged piston movement](staged-piston-motion.md) | Complete reference-door comparison, moving observers and resumable movement writes |
| [Ordinary 3×3 piston-door type](piston-door-type.md) | Implemented completed-operation contract, shape validation, exact-state verification and optional interruption tolerance |
| [Ordinary reference-door adoption](reference-door-ordinary-adoption.md) | Fresh v3 candidate, MCP adoption/restart, placement-path checks and remaining live work |
| [Reference door live construction](reference-door-live-construction.md) | Retained counterexample, v6 command repair, verified 43-stage build/removal and two live cycles; readiness remains separate |
| [Reference door relocation](reference-door-relocation.md) | Two-coordinate, four-rotation trial matrix and static placement-dependency audit |
| [Live readiness and recovery](live-operation-readiness-and-recovery.md) | Recovery-first scope using ordinary commands; stronger server readiness/conditional execution deferred |
| [Piston code organization](piston-code-organization.md) | Removed obsolete experiment, current module responsibilities and behavior-preserving cleanup evidence |
| [Reference door adoption audit](reference-door-adoption.md) | Historical unrestricted candidate, corrected construction and saved-proposal revalidation; distinguished from the agreed ordinary-door requirement |
| [Reference door construction proposal](reference-door-construction-proposal.md) | Cause of the placement pulse, isolated experiments and implemented observer-front dependencies |
| [Reference door interruptions](reference-door-interruptions.md) | Historical stronger-contract counterexample and post-world-tick pulse probes; ordinary-operation assumption now agreed |
| [Reference door short-input comparison](reference-door-short-input-comparison.md) | Six live Java pulses match the simulator, including incomplete reopening; evidence and the agreed ordinary-operation assumption |
| [Expanded electrical source audit](piston-electrical-source-audit.md) | Weak/strong power, conductors, quasi-connectivity, dust notification order and repeater callback differences |
| [Location-state bindings](location-state-bindings.md) | Explicit observations of fixed coordinates, moving-state samples and contextual verification |
| [Moving-world behavior review](runtime-location-review.md) | Complete-state recurrence, motion-time input exploration, intermediate observations and independent child requirements |
| [Multi-fault repair](multi-fault-repair.md) | Composite repair conditions |
| [Performance observation](performance-observation.md) | Reproducible benchmark and bridge measurements |

## Validation and diagnostic evidence

| Document | Read it for |
| --- | --- |
| [Finite flying-machine trial](flying-machine-short-course.md) | Fixed-corridor ten-block flight, absolute-coordinate live comparison and independent arrival checks |
| [Finite-flight lifecycle](flying-machine-lifecycle.md) | Single-operation type, adopted empty-corridor placement, arrival diagnosis and reviewed removal |
| [Flying-machine generation](flying-machine-generation.md) | Typed body recipes, attachments, finite travel, reflection and rotation through shared validation |
| [Declarative flying-machine engines](flying-machine-engines.md) | Typed engine definitions, shared verification and live lifecycle evidence |
| [Harvest pass and practical roadmap](flying-machine-practical-roadmap.md) | Pumpkin/melon destruction, generated harvest contracts and the remaining survival-construction prerequisites |
| [Coauthoring architecture](coauthoring-architecture.md) | Checked Rust policy tables, typed observation/diagnosis and the boundary between reports and fresh operation evidence |
| [Differential physics](physics-differential-testing.md) | Comparing model and client-visible observations |
| [Executable torch law](torch-laws.md) | Blueprint rule execution and server-observed burnout/recovery regressions |
| [Executable repeater laws](blueprint-architecture.md#executable-repeater-laws) | Retained queue/event models, immutable law data and the short-pulse/locking boundaries |
| [Executable comparator law](blueprint-architecture.md#executable-comparator-law) | Compare/subtract programs, sampled inputs, commit timing and retained compatibility boundaries |
| [Executable observer law](blueprint-architecture.md#executable-observer-law) | Observation differences, pending pulses, deadline effects and retained event ordering |
| [Executable lamp laws](blueprint-architecture.md#executable-lamp-laws) | Delayed compatibility updates and immediate bounded updates, each with its original state boundary |
| [Executable piston laws](blueprint-architecture.md#executable-piston-laws) | Bounded movement/state rules, complete input validation and retained atomic completion checks |
| [Physical behavior checks](physical-behavior.md) | Executable dust law, pinned circuit/type bindings, retained execution state and proof limits |
| [Passive shapes and conduction](passive-shapes-runtime.md) | Checked slab/glass state declarations, directional support, exact placement export and independent live comparisons |
| [Behavioral state reduction](behavior-state-reduction.md) | State inventory, possible reductions and required proof obligations before implementation |
| [Torch settling proof investigation](torch-settling-proof.md) | Electrical independence, exact continuations from observed prefixes and the boundary for a new proof path |
| [Abstract behavioral verification](abstract-behavior-verification.md) | Conservative history transitions, universal cycle checks and arbitrary-input NOT model proofs |
| [Repeated-settling adoption](repeated-settling-adoption.md) | Multiport bindings, actual input controls and fresh Blueprint/MCP adoption proofs |
| [Blueprint block reduction](blueprint-block-reduction.md) | Type-directed block-count search with movable ports, immutable candidates and explicit adoption |
| [Finite-burst behavior](finite-burst-behavior.md) | At least two falling edges, eventual permanent OFF, contextual adoption and restart limits |
| [Periodic behavior status](periodic-behavior-status.md) | Autonomous periodic types, complete-state recurrence and contextual Blueprint review/adoption |
| [Clock conformance](periodic-clock-conformance.md) | Block-effects profile, retained observations, legacy divergence and distinct periodic/finite-burst results |
| [Vanilla instrumentation](vanilla-instrumentation.md) | Server-side artifact and capture-integrity contract |
| [Piston low layer](piston-low-layer-validation.md) | Completion checks and validated horizontal subset |
| [Piston motion source audit](piston-motion-source-audit.md) | Java 1.21.11 reversal, carrier and callback findings and preserved preflight diagnostics |
| [Live piston interruption conformance](piston-live-interruption-conformance.md) | Isolated Java 1.21.11 ON/OFF trials, server-applied timing and model comparison |
| [Piston diagnostics](piston-diagnostics.md) | Retained 3×3/single-cell models that are not deployable |
| [Observed 3×3 recognition](observed-3x3-piston-door.md) | Read-only geometry/evidence recognition |
| [Live harnesses](../crates/dustroute-mcp/mineflayer/e2e/README.md) | Private-server test procedures |

GitHub Actions configuration was removed intentionally. Run the local checks in
[development](development.md) before integrating code; this repository no longer
provides an automatic push/PR check workflow. `.cursorignore` and `.gitattributes`
were also removed; `.gitignore` continues to exclude local environments and
runtime artifacts.

- [Component patterns and external equipment](blueprint-component-patterns.md): separate NOT bodies, direct device outputs, and freshly verified torch/support placements.
