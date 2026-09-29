# Native client rollout

The seven-stage rollout is authorized: moving-piston restoration, adhesion,
reference callbacks, broader live comparisons, DustRoute observation/interaction,
remaining operation APIs, then replacement of verified bridge paths. Java 1.16.1
and 1.21.11 coexist in Voxrig; DustRoute's physical context remains 1.21.11.
Entities in the circuit simulator remain deferred. A new large prerequisite
outside this scope stops the task for a report.

Voxrig is developed in the sibling `../Voxrig` checkout on
`codex/dustroute-client`, with separate local commits for later upstream review.
No remote submission has been made. The Cargo `voxrig` feature uses this explicit
path while the branch is unpublished. A clean checkout needs that sibling; this
is not a crates.io dependency or an unpinned fetch at build time.

## Observation boundary

`BotBridge::connect_voxrig` connects the native adapter.
`scan_client_region` returns a `ClientRegion` with exact native states and
`dustroute.client-observation.v1` / `client_reconstructed` evidence. The original
received cache, connection ID, packet sequence, frame, dimension, origins and
moving carriers remain attached. Frames/revisions are not server ticks.

Incomplete reconstruction, pending chunk recovery, unavailable cells, a wrong
dimension or incomplete coverage rejects the scan. The Rust type has no
Deserialize implementation, so saved data cannot create a fresh capability.
It also cannot construct `ValidatedRegion` or `ServerReadback`: existing confirmed
placement/repair paths retain their current contract until separately migrated.
The adapter does not run DustRoute's simulator to manufacture observations.

At this increment only the explicit observation API is connected. Existing
Mineflayer RPC remains selected by `BotBridge::new`; operations on a native
bridge that have not been implemented reject explicitly. MCP runtime selection,
operation APIs and the final evidence-policy migration are subsequent steps.

## Native evidence already retained in Voxrig

- Mid-motion joins restore real moving-piston chunk NBT and provenance.
- Slime and honey branch trials each match 770 final native cells.
- Wire/gate support and diagonal wire callbacks match 770 final native cells.
- The reference door completes repeated close/open cycles. Closed, reopened and
  post-reload open regions each match 770 native cells. Unload explicitly makes
  observations unavailable and fresh chunks restore them.
- The initial door run exposed incorrect support for a retracting piston body;
  the failure, fix and six-direction regression are retained.

These comparisons use an isolated vanilla 1.21.11 server and independent command
assertions as test instrumentation. They do not claim arbitrary circuits, native
server progress equality for every client frame, or public placement completion.

Observation increment validation: `cargo check --offline -j1 -p dustroute-mcp
--features voxrig` resolved only the new local dependency and required feature
packages; subsequent commands use `--locked`. Eleven bridge/adapter tests pass,
including stale evidence, invalid coverage and native provenance rejection.
All-target feature-enabled Clippy with warnings denied, package formatting and
diff checks pass. Logs are `.local/voxrig-adapter-{check,tests,clippy}.log`.
End-to-end native operations and runtime selection are not claimed by this increment.
