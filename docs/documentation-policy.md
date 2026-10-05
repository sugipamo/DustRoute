# Documentation policy

## Reading layers

Keep the route from intent to detail explicit:

1. Root README: purpose, a few examples and the next page.
2. `docs/capabilities.md`: supported public work, conditions, exclusions and
   the scope/date of live evidence.
3. `docs/getting-started.md`: preparation, permissions and connection.
4. `docs/workflows.md`: task sequences, completion checks and recovery choices.
5. Public MCP/operator/feature references: exact IDs, DTOs, budgets and gates.
6. Architecture, source audits, migrations and evidence: implementation and
   independently retained observations.

The documentation index maps tasks to these layers and keeps the detailed
catalog available. Avoid putting a migration log or exhaustive API list in the
root README. Link to the owner of a limit instead of copying whole contracts.

## Language

English is the standard. Front-facing pages have a sibling `.ja.md` version:
the root README, documentation index, capability table, getting-started guide
and workflow guide. Add reciprocal language links, retain the same task order,
examples, restrictions and evidence interpretation, and update both in one
change. Use Japanese navigation while the reader is in the Japanese overview.
Technical references may remain English-only; say so before linking deeper.

Exact MCP names, Rust type names, configuration keys and immutable IDs are not
translated. New detailed specifications should be written in English. Retained
historical Japanese reports do not need rewriting merely to change their language.

## Claims and evidence

- Distinguish observation coverage, admitted model behavior and public execution.
- Distinguish command construction from inventory-based survival construction.
- State unsupported/undetermined results and the action that follows them.
- A model pass is scoped to its context and declared requirements. It is not a
  fresh world observation or an all-input live guarantee.
- A stored result or adopted source is not executable authority. Explain fresh
  observation/replanning and operation-specific cancellation/undo conditions.
- Link live claims to dated evidence and identify older version/backend trials.
- Preserve original evidence, hashes and counterexamples. Mark retired setup
  instructions as historical instead of silently rewriting their trial conditions.
- Obtain version/source pin details from their owning manifest or setup contract;
  avoid another manually maintained commit or file-count claim in overview pages.

## Maintenance order

When a capability changes, update its exact reference, both capability tables
and the affected workflow pair. Change the root overview only if its summary
changes. Check relative links/anchors, language-pair scope, tool names and command
examples against current code. Documentation-only work does not need to start a
Minecraft server or run a Rust build.

Obsolete current instructions may be removed after checking the implementation.
Removing runtime code requires evidence that it is unused and appropriate
behavioral verification. Documentation drift alone is not evidence that an
active safety check is unnecessary. Responsibility changes, new prerequisites
or substantial behavioral work should be reported before implementation.
