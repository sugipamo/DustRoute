# Native client usability

Approved order (2026-09-29): improve player targeting, validate recovery through
the public MCP process, then make setup and dependency selection reproducible.
The purpose is collaborative circuit prototyping; entity physics and automated
survival construction remain deferred. A large prerequisite outside this scope
stops the work for a report.

Before implementation, DustRoute's previous work was fast-forwarded into develop
at `89e0d1d` and pushed. The old feature branch was deleted after its commits were
retained in develop. Voxrig's local develop retains `b98785e`, with upstream main
unchanged. Both repositories now use `codex/native-client-usability`.

1. **Player targeting.** Separate native outline selection from collision
   geometry, preserving an explicit source/geometry type and complete observation
   checks. Audit the version's native shape/raycast implementation, exercise thin
   circuit components and occlusion, and retain independent native comparisons.
   Unknown context-dependent geometry must remain unavailable rather than air.
2. **Recovery.** Exercise real MCP OS-process restart and connection interruption
   on the owned isolated vanilla server. Durable instances may be reopened, but
   stale observations/plans must not authorize mutations; diagnose/replan/review
   is the recovery path. Do not perform host fault injection or touch user worlds.
3. **Setup.** Pin the tested Voxrig source and document reproducible installation
   and backend choice. Remove dependence on an unrecorded sibling checkout while
   keeping the separate upstream contribution history available. Validate the
   documented route from an isolated checkout.

Existing validation at the baseline is recorded in [the native rollout](voxrig-rollout.md).

Player targeting is implemented. Voxrig exports audited state-only native
outline/auxiliary shapes for 22,899 states and retains 25,394 independent native
raycast cases plus 2,304 native rotation cases. The complete native state identity
mapping is verified. All 118 Voxrig tests, its doctest, Clippy and reproducible
data generation checks pass. DustRoute chooses `block_outline`; the explicit
Voxrig collision API remains available.

The [isolated target trial](evidence/voxrig/native-outline-a-20260929-manifest.json)
selects dust, lever, repeater and comparator from received player gaze, respects
stone occlusion and rejects unsupported light-block geometry. The final 648 cells
match an independent server predicate. This is static client-world selection,
not a graphical camera-frame or server receipt.

The recovery harness now has an explicit OS-process mode. Set
`MCP_PROCESS_BIN` to the built `dustroute-mcp` executable when running
`voxrig_assembly_probe`. `PROBE_INTERRUPT=1` routes only that trial MCP client's
Minecraft connection through an owned loopback proxy, closes its streams after
the independent actor observes the first world change, then checks durable
`needs_inspection`, old-operation rejection and newly reviewed reconstruction.
The listener endpoint remains stable across child restarts. Processes stop by
closing MCP stdio, and their clean exits/PIDs are recorded. Voxrig connection
counters are process-local, so these records identify each owning MCP process
as well; archived receipts are never used as fresh observation capabilities.

The [door process trial](evidence/voxrig/native-process-door-a-20260929-manifest.json)
passes through four distinct MCP PIDs: interrupted placement, newly reviewed
reconstruction, later missing-block repair and normal close/open, then removal
and another restart. All four processes exit cleanly. Independent server
predicates match each of the four complete 770-cell snapshots. This covers the
declared TCP interruption and graceful OS-process restarts, not host crashes.
The native adapter tests, durable registry test and all-target MCP Clippy pass.

Setup now uses an unmodified vendored Voxrig source snapshot at
`47a05029e126cb049ee8a7536e5df867f3851c12`. `scripts/vendor_voxrig.py --check`
checks all 204 recorded files without requiring Voxrig's Git checkout. The
source repository retains its independent contribution history. A separate
checkout/build validation is the final acceptance check for this setup change.

Final outline review added world-coordinate edge cases at 100, 42,000 and
29,999,980. A native miss at a lever edge initially reproduced as a false hit;
applying the native epsilon after translating the box fixes it. The final 118
Voxrig tests, Clippy and regenerated oracle checksum check pass with this fix.
