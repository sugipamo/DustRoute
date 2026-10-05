# Getting started

[English](getting-started.md) · [日本語](getting-started.ja.md)

Read the [overview](../README.md) and [capabilities](capabilities.md) first.
This page explains what to prepare; the [operator setup reference](../crates/dustroute-mcp/SETUP.md)
contains server registration, transport and full policy settings.

## Choose how work reaches Minecraft

| Path | Prepare | Permissions and scope |
| --- | --- | --- |
| Inspect and prototype | Running server, bot connection, configured assist player for gaze; no materials | Default read-only policy. Native block reads need no OP; gaze reacquisition/region previews can use privileged commands. Offline Blueprint authoring needs no live connection. |
| Command construction | A supported design and fully observed site | Bot OP permissions, authorized region/player/dimension, mutation policy and preview/confirmation. Inventory is not consumed. |
| Survival construction | Adopted grounded passive design, materials in bot inventory, temporary material and explicit work/travel/retreat space | Ordinary non-OP player actions and mutation policy. No second observer is required. Active circuits and chest supply are outside this path. |

Command-based new-target Assemblies require an empty guarded volume. Survival
building uses its declared grounded site and movement contract. These paths have
different admission checks; a plan for one is not permission to use the other.

## Prepare the connection

Use a trusted private Minecraft Java 1.21.11 server with offline authentication.
Register the exact bot/player names with its whitelist. The default bot name is
`DustRouteBot`; do not use its account concurrently from another client.
Server hosting requires Java 21. No companion MOD, Node.js bridge or separate
Voxrig checkout is needed. Offline-auth servers should not be publicly exposed.

Build from the repository root with Rust/Cargo and the locked dependencies:

```bash
cargo build --locked -j1 -p dustroute-mcp
```

After dependencies have been fetched, `--offline` can be added. The default build
includes Voxrig. The following is a read-only stdio launch template; replace the
player name and state directory with your own values:

```bash
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_ASSIST_PLAYER=YourMinecraftName \
  DUSTROUTE_MC_AUTH=offline \
  DUSTROUTE_MC_VERSION=1.21.11 \
  DUSTROUTE_READ_ONLY=true \
  DUSTROUTE_STATE_DIR=/private/path/to/dustroute-state \
  target/debug/dustroute-mcp
```

Configure your MCP client to launch that executable with these environment
variables. Keep stdio for MCP messages. Alternatively, the operator reference
describes loopback-only HTTP at `/mcp`; remote HTTP exposure is unsupported.
For a new server, follow its [private test-server setup](../crates/dustroute-mcp/SETUP.md#prepare-the-vanilla-server).
That example configures creative command trials; survival work needs a survival
builder and inventory instead.

## Check before your first task

1. Ask the AI to call `get_bot_status` and report the connection, player and policy.
2. Observe a small known area with `get_world`, or look at a circuit and use
   `test_circuit`. Gaze needs the configured assist player online and tracked.
3. Confirm the dimension, bounds, completeness and evidence source. An unavailable
   observation is not an empty site; unsupported blocks are not ordinary solids.
4. Choose a [workflow](workflows.md). For live changes, explicitly configure
   `DUSTROUTE_READ_ONLY=false` and appropriate allowed region/player/dimension
   limits in the operator setup, then review the tool's actual plan.

## Know what is retained

Use a durable `DUSTROUTE_STATE_DIR` for adopted catalogs, Assembly instances and
job history; the default directory is temporary. Ordinary observed `circuit_id`
records are process-local and short-lived; circuit revisions default to one-hour
retention. Catalog/history persistence never restores old executable authority.
After restart, observe again and create a new plan. See
[ID retention](mcp-public-features.md#ids-and-retention) and
[recovery](workflows.md#inspect-or-recover).

Continue with [workflows](workflows.md), or use the [documentation map](README.md)
for API and feature-specific detail.
