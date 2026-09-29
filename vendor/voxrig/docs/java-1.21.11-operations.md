# Java 1.21.11 construction operations

`Client::java_1_21_11_operations()` returns a version-specific operations handle.
The existing Java 1.16.1 Bot API remains available. No protocol IDs or item IDs
are shared implicitly across versions.

Implemented controls: short creative flight steps, rotation, flight requests,
creative default-item hotbar writes, hotbar selection, precise use-on-block,
creative digging, and unsigned commands on the offline connection. Player game
mode, flight permission, inventory, interaction acknowledgements and periodic
server-time packets are received explicitly. Packet submission is not acceptance.
A block interaction acknowledgement is not proof that the requested block was
placed or removed: callers must observe the resulting world state.

Movement updates the local position with `position_from_server=false` until a
server position packet replaces it. There is no collision/pathfinding or survival
movement/mining implementation in this increment. Flight requires a received
permission and explicit request; creative inventory/digging requires a received
creative game mode. Position, hit, slot, stack and sequence bounds are checked.

The generated item registry is pinned beside the block registry. Creative writes
use default items without added/removed components. Received complex components
make the affected inventory baseline unavailable, never empty. Other container
windows invalidate player inventory knowledge. Submitted creative writes also
invalidate the affected slot and appear in `pending_creative` until an inventory
packet establishes it. No optimistic creative stack is labelled received.

System messages retain native text component values, including compact strings
and translation keys/arguments. Projection is bounded and can be unavailable;
the last 128 messages are retained, and a cursor behind dropped history fails.
`literal_text` does not flatten translated/concatenated components into a command
marker. Periodic server time is labelled as a received sample, not a current-tick
fence or a substitute for independent server readback.

## Evidence

`docs/evidence/client-operations-20260929.manifest.json` retains two isolated
vanilla 1.21.11 runs. Native placement and creative removal of stone, interaction
acknowledgements and command results were observed. In the corrected probe, a
console read independently matched the submitted flight position
`[100.5,182.0,102.5]`, flight permission/state, and the final stone support/air
region. The first probe incorrectly assumed an object-only text component and
failed its assertion on a correctly decoded compact string; that failure is
retained. Native message decoding did not require an optimistic response.

The owned fixture and force-load were removed, the temporary OP grant revoked,
and the server saved all dimensions and stopped normally. These controls are
the core operation increment; DustRoute operation dispatch, complete live
workflows and the final default-path migration remain.

104 unit tests, all examples, 1 doctest, all-target Clippy with warnings denied,
format, package list and pinned generator checks pass. Logs:
`.local/voxrig-operations-{all-targets,doc,clippy-final,package}.log`.

## Remote players

`Operations::visible_players` joins received profiles to spawned player entities
and returns connection/dimension/sequence-bound positions and head rotations.
Relative movement uses the native `TrackedPosition` rounding, including negative
half ties and unchanged coordinates. This is packet state, without render
interpolation, collision or entity physics. Destroy/remove packets and dimension
changes invalidate world entities; a rejoining UUID gets a new entity identity.

Standing, crouching, swimming, fall-flying and spin-attack eye heights include the
received scale attribute. Sleeping/other unsupported poses and unknown metadata
produce no eye position. An ordinary health update cannot silently recover an
unknown pose. Packet counts, stored profiles/entities and modifiers are bounded;
malformed supported packets do not partially commit changes.

The pinned protocol JSON's attribute mapper is stale: it calls ID 22 scale, while
native 1.21.11 uses 22 for movement speed and 25 for scale. The separate attributes
registry matches native `EntityAttributes` registration and the recorded packets,
and now supplies the generated constant. Both the failed and corrected two-client
trials are retained in `evidence/client-players-20260929.manifest.json`. The second
trial covers movement/rotation, scale, removal and reconnect; final server console
position, rotation and scale match. Deterministic packet replay checks all five
reported observation boundaries. These observations do not promise that a
rendered crosshair or a later server tick will have the same target.

Player increment validation: the 109-test full suite/all examples passed before
the retained replay was added; all six player tests (including that replay) then
passed, along with all-target Clippy with warnings denied and generator/format
checks. Logs are `.local/voxrig-players-all-targets.log`,
`.local/voxrig-player-replay-tests.log` and `.local/voxrig-players-clippy.log`.

## Static collision targeting

`Operations::observe_player_target` reads the player pose and reconstructed world
under one session lock. Its explicit `block_collision` geometry uses the pinned
1.21.11 collision table, including partial shapes and boxes protruding into
neighboring cells. Candidate traversal is bounded to 64 blocks. Unavailable
chunks/poses/reconstruction and moving carriers cause an error; unknown cells
are never air. Unloaded space beyond a nearer known collision is irrelevant.

This is useful for selecting solid construction surfaces; it is not the graphical
client's outline raycast. Fluids and decorations with empty collision shapes are
not targeted, and entity-dependent collision behavior is not modeled. Edge ties
use deterministic axis/cell order. The response retains the chosen geometry and
connection, receive sequence and client frame; none implies server confirmation.

`default_item` provides a pure native-registry preflight for creative batches;
the creative inventory writer uses the same validation.

## Received block update recordings

`start_block_recording` / `stop_block_recording` capture a bounded stream of
single-block and section-block update packets from a fully loaded baseline.
Every event retains its previous received state, packet state, connection-local
packet sequence, in-packet cell order and local 20 Hz frame. Piston actions and
reconstructed frames remain separate evidence; this stream is not a complete
server event history. An intersecting chunk reload/unload, world change or lost
connection explicitly invalidates the recording. Overflow reports truncation
and a total seen count; a wrong stop ID does not consume an active recording.

The downstream DustRoute native bridge trial `bridge-b-20260929` retained 12
ordered state updates for command writes, a lever toggle and ordinary creative
stone placement/removal. Its final 648 cells independently matched a vanilla
server assertion. No command-based readback is used by the recording API.

Current increment validation: 114 unit tests and all examples pass, as do
all-target Clippy with warnings denied, formatting and pinned generator checks.
Logs: `.local/voxrig-target-recording-tests.log` and
`.local/voxrig-final-clippy.log`. The target tests cover partial slabs, protruding
fences and unavailable cells before/beyond a known collision. Recording tests
cover native order, previous states, overflow and chunk invalidation.
