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
outline/auxiliary shapes for 22,899 states and retains 25,370 independent native
raycast cases plus 2,304 native rotation cases. The complete native state identity
mapping is verified. All 118 Voxrig tests, its doctest, Clippy and reproducible
data generation checks pass. DustRoute chooses `block_outline`; the explicit
Voxrig collision API remains available.

The [isolated target trial](evidence/voxrig/native-outline-a-20260929-manifest.json)
selects dust, lever, repeater and comparator from received player gaze, respects
stone occlusion and rejects unsupported light-block geometry. The final 648 cells
match an independent server predicate. This is static client-world selection,
not a graphical camera-frame or server receipt.
