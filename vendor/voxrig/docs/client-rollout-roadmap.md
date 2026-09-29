# Client rollout after the first piston trials

Authorized on 2026-09-29: implement the remaining work in order. Existing local
branch `codex/dustroute-client`; keep Java 1.16.1 and 1.21.11 coexisting. Prepare
Voxrig for a later PR. Do not publish upstream without a submission request.

| Order | Work | Acceptance |
| --- | --- | --- |
| 1 | Moving-piston chunk NBT, observation recovery | Typed native carriers/provenance, missing data remains unavailable, fresh dependency chunks permit recovery, tests and native evidence |
| 2 | Slime/honey attachment graph | Correct branching/order, push limit, separation of the two materials, reversal and competing pistons |
| 3 | Client block callbacks used by reference circuits | Explicit Rust rule table and typed behavior, declared unsupported boundaries |
| 4 | Wider native comparison | Vertical/mixed layouts, moving states, repetition, chunk/reconnect boundaries, retained successes and failures |
| 5 | DustRoute observation then interaction adapter | Observation origin and completeness preserved, required bridge functions inventoried, no simulated server-confirmation evidence |
| 6 | Remaining 1.21.11 operations | Movement/inventory/placement/removal needed by the existing bridge, independently tested |
| 7 | Replace verified Mineflayer/command paths | Reference door, flying machine, repair/cancel workflows; remove verification only where evidence supports the replacement |

Entities remain deferred. Unknown large prerequisites outside these declared
items stop the whole task for a report before implementation. In-scope additions
already authorized do not require repeated permission.

Step 1 implementation and native mid-motion chunk capture are complete; wider
live reload testing remains part of step 4. Step 2 now has the ordered adhesion
planner, 6-direction/unit coverage and two independent 770-cell native matches;
wider interaction cases remain for step 4. Step 3 now includes wire geometry,
old/new prepare callbacks, supported gate/button removal, wool materials,
98 unit tests and an independent 770-cell native callback comparison. Step 4 now has the mixed 3x3 door: two close/open cycles, native checks
of open/closed/reopened regions, reconnect and live chunk unload/reload. Its
first failure exposed retracting-body support and was corrected with a
six-direction regression. Step 5, the DustRoute observation adapter, is next. Source inspection confirms that Java 1.21.11
`PistonBlockEntity.toInitialChunkDataNbt` sends its componentless NBT, including
`blockState`, `facing`, `progress` (the serialized previous progress), `extending`
and `source`. The old receiver discarded this data. The new decoder must not
invent a block-action sequence for a carrier received in a chunk.

Step 1 recovery is conservative: all dependency chunks must arrive freshly.
Actions arriving while recovery is pending extend the required baseline. No
packet exists here to force an arbitrary server chunk resend. The client keeps
unavailable observations explicit until data arrives or a new connection resets
the world. Unsupported tick control cannot be cleared by refreshing a chunk.

The observation portion of step 5 is committed in DustRoute as `68ce938`: an
explicit optional native adapter and typed client evidence, with 11 bridge tests
and Clippy passing. Existing server-confirmed capabilities are not fabricated.
The core APIs needed for the interaction portion are now implemented in Voxrig
and independently exercised in an isolated server; see
[java-1.21.11-operations.md](java-1.21.11-operations.md). Remote-player observation,
DustRoute operation dispatch and the final workflow/default migration remain.
