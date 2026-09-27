# Movable piston bodies and the path to a 3×3 door

Work started 2026-09-27 against the existing isolated Java 1.21.11 server,
Yarn 1.21.11+build.6, with redstone experiments disabled.

## Scope

The electrical callback runtime admits retracted ordinary and sticky piston
bodies facing any of the six directions as moving payloads. Mixed linear
chains of admitted materials and piston bodies retain the twelve-block limit.
Extended bodies and active movement carriers remain physical obstructions.
Slime/honey attachment, entities, component destruction and additional
redstone component adapters are outside this change.

The target includes a two-stage sticky extender that extends a solid payload,
retracts its upper body, pulls that body back, then uses it again to recover the
solid payload. This supplies a mechanical building block for a 3×3 door; it
does not establish an entire door's wiring, arbitrary input behavior or placement.

## Target implementation audit

The local named target artifact is the one pinned by the
[electrical audit](piston-electrical-source-audit.md). Its classes were inspected
with `javap -p -c`:

- `PistonBlock.isMovable` accepts either piston variant when `EXTENDED` is false.
  This predicate does not reject a body by facing or sampled power. A powered
  but still retracted body can move before its queued event is delivered.
- `PistonHandler.tryMove` uses the common moved-block count limit of twelve.
  It does not limit a linear chain to one piston body.
- Existing event identity guards discard work at a vacated body position.
  Existing carrier completion notifications reevaluate a body at its new
  coordinate; its input is read from the actual destination environment.

SHA-256 of audited class bytes:

| Class | SHA-256 |
| --- | --- |
| `net.minecraft.block.PistonBlock` | `00cadea51e97b910da939da68b97c591843675ced0619145a4bb7c412c31cd02` |
| `net.minecraft.block.piston.PistonHandler` | `d0647b9d63b2d81c4f7cc0508e9327a8fd1419e5d6ff09ac55aa4d6ad0fe5ee3` |

The payload and geometry laws have new immutable v2 revision IDs. The single
standard execution/behavior profiles also advance to v2; there is no direction
dispatch or alternate legacy runtime. Saved v1 execution/behavior contexts are
rejected rather than silently assigned these wider assumptions. Existing
Blueprint requirements are not rewritten; new proposals require fresh review.
Historical program data and captured observations retain their original meaning.

## Verification

`piston_payload_runtime` covers all 72 movement-direction/body-facing/variant
combinations, mixed chains at and beyond the limit, powered-body event races,
destination power, extended-body obstruction, and double-extender recovery with
exact checkpoint and behavior-state restoration. Live transient comparison uses
the [existing bounded projection](piston-transient-conformance.md), retains actual
applied input times, and independently checks region cleanup.

All six isolated captures matched, totaling 502 observed event/carrier/ordinary
notification boundaries. Each region was inspected empty before construction,
emptied after observation, and released from force loading. Every server shut
down normally. No host operation or public shared-world deployment was involved.

| Capture | Mechanism | Actual input offsets | Matching boundaries |
| --- | --- | --- | ---: |
| `piston-payload-20260927-a` | Two-stage upward sticky extender and full payload recovery | 0, 8, 16, 24, 32, 40 | 160 |
| `piston-payload-20260927-b` | Twelve-block mixed chain, including eight piston bodies | 0 | 140 |
| `piston-payload-20260927-c` | Downward driver pushes and pulls an east-facing sticky body | 0, 8 | 50 |
| `piston-payload-20260927-d` | Arriving upward sticky body activates from destination power | 0 | 52 |
| `piston-payload-20260927-e` | Same-tick inputs: move a powered retracted body before its event | 0, 0 | 29 |
| `piston-payload-20260927-f` | Short-pulse interruption and reversal with a downward sticky payload | 0, 1, 2, 14 | 71 |

In capture a the solid payload reaches four blocks above the lower body, and
the final complete region equals the initial region. The other captures test
individual interactions, not an automatically synthesized door controller.

Capture f initially exposed a comparison-projection defect: the model's direct
onBlockAdded check following forced completion was counted as an ordinary
neighbor callback. The existing normal-completion exclusion now also covers
forced completion, only for that direct first self check. The real trailing
neighbor callback is retained. No runtime callback was removed. The original
72-versus-71 diagnostic and its raw artifacts are preserved; corrected comparison
has 71 matching boundaries.

`piston-payload-observed-*-v2.json` retains server-derived expectations, a complete
raw comparison interval, original artifact hashes and verified moving-state
restoration. `python3 tools/test_piston_transients.py` replays these six plus the
eleven earlier transient captures through the current compiled example, deriving
expectations again from server records. No old saved pass supplies the result.

Final validation passed: 214 Minecraft tests, 51 selected library/translate
integration tests, all 80 MCP library tests, and seven offline transient tests
covering all 17 retained captures. All-target workspace Clippy with warnings
denied and formatting checks passed. The full workspace test suite was not
rerun. Test commands, counts and source hashes are recorded in the
[verification manifest](evidence/piston-payload-final-checks-20260927.json).

## Subsequent redstone integration

Dust, repeaters, conductor power and quasi-connectivity already share this
runtime. The subsequent v5 increment adds native lamp and stationary/moving
observer callbacks, including source-ordered movement notifications and the
reference 3×3 door comparison. See [the completed increment](staged-piston-motion.md).

Torch burnout and comparator source/delivery semantics remain outside this
callback adapter. Their calculations in other execution contexts do not certify
coupling to this moving world. They were not needed by the supplied door.
