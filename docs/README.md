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
| [Fixed 1×2 piston door](piston-door-mcp-v1.md) | Supported construction, recognition, open/close and undo |

## Architecture and development

| Document | Read it for |
| --- | --- |
| [Development](development.md) | Workspace boundaries, local checks, CLI and examples |
| [Physical IR](physical-ir.md) | Observations, evidence and derived representations |
| [Physical function model](physical-function-model.md) | Shared circuitry and bounded functional inference |
| [Component library](component-library.md) | Candidate provenance and promotion requirements |
| [World validation](world-validation-boundary.md) | Immutable placement proofs and recovery distinctions |
| [Update model](minecraft-update-model.md) | Event/transition identity, scheduling and checkpoints |
| [Multi-fault repair](multi-fault-repair.md) | Composite repair conditions |
| [Performance observation](performance-observation.md) | Reproducible benchmark and bridge measurements |

## Validation and diagnostic evidence

| Document | Read it for |
| --- | --- |
| [Differential physics](physics-differential-testing.md) | Comparing model and client-visible observations |
| [Vanilla instrumentation](vanilla-instrumentation.md) | Server-side artifact and capture-integrity contract |
| [Piston low layer](piston-low-layer-validation.md) | Completion checks and validated horizontal subset |
| [Piston diagnostics](piston-diagnostics.md) | Retained 3×3/single-cell models that are not deployable |
| [Observed 3×3 recognition](observed-3x3-piston-door.md) | Read-only geometry/evidence recognition |
| [Live harnesses](../crates/dustroute-mcp/mineflayer/e2e/README.md) | Private-server test procedures |

GitHub Actions configuration was removed intentionally. Run the local checks in
[development](development.md) before integrating code; this repository no longer
provides an automatic push/PR check workflow. `.cursorignore` and `.gitattributes`
were also removed; `.gitignore` continues to exclude local environments and
runtime artifacts.
