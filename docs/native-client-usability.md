# Native client usability

Approved order (2026-09-29): improve player targeting, validate recovery through
the public MCP process, then make setup and dependency selection reproducible.
The purpose is collaborative circuit prototyping; entity physics and automated
survival construction remain deferred. A large prerequisite outside this scope
stops the work for a report.

Before implementation, DustRoute's previous work was fast-forwarded into develop
at `89e0d1d` and pushed. The old feature branch was deleted after its commits were
retained in develop. Voxrig's local develop retains `b98785e`, with upstream main
unchanged. At that stage both repositories used `codex/native-client-usability`.

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

The 2026-09-29 setup validation used an unmodified vendored Voxrig source snapshot at
`47a05029e126cb049ee8a7536e5df867f3851c12`. `scripts/vendor_voxrig.py --check`
checks all 204 recorded files without requiring Voxrig's Git checkout. The
source repository retains its independent contribution history. A separate
Git archive checkout passed the offline locked native executable build and the
default workspace/all-target check on Linux x86_64 with Rust/Cargo 1.98.0. All
local Cargo package paths were inside the exported checkout; only Cargo's
package/build caches were shared. The verifier also rejects modified or
unmanaged build inputs in an isolated negative-check fixture.

Final outline review added world-coordinate edge cases at 100, 42,000 and
29,999,980. A native miss at a lever edge initially reproduced as a false hit;
applying the native epsilon after translating the box fixes it. The final 118
Voxrig tests, Clippy and regenerated oracle checksum check pass with this fix.

The [standalone flight trial](evidence/voxrig/native-process-flight-final-a-20260929-manifest.json)
runs the exported build through three distinct MCP processes. The public
generator's mirrored `honey_nose`, rotated 270 degrees, is adopted and placed,
survives process restart, moves three blocks north and is conditionally removed.
All 1,134 arrival cells match the generator's prediction. Placed, arrived and
removed states also match independent server assertions in two disjoint 567-cell
parts each. A third process observes the removed record afresh. Temporary OP
grants were revoked and the isolated server stopped normally.

All three usability steps are complete for these declared cases.
[Validation records](evidence/voxrig/native-usability-validation-20260929.json)
retain build/test logs, scope and checksums. At the end of those trials, no
upstream Voxrig PR had been submitted; its source commit ancestry was preserved.

The current source pin is now `784c12dba126f5829cd7a8db8542360cc48434c9`, published
on Voxrig's `codex/dustroute-integration` branch. On 2026-10-02 (Asia/Tokyo), the
manifest was reconciled with all 205 included files after the shared observation
implementation was exported to that source repository. No vendored source files
needed changing. The [pin alignment verification](evidence/voxrig/source-pin-alignment-20261002.json)
records the new standalone checks; the original live evidence above remains tied
to the revisions actually used in those trials.
