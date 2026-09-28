# Waxed copper bulb integration

The unified callback adapter admits `waxed_copper_bulb`,
`waxed_exposed_copper_bulb`, `waxed_weathered_copper_bulb` and
`waxed_oxidized_copper_bulb`. A synthetic `CopperBulb` selects the first identity.
Unwaxed bulbs remain unsupported because oxidation/random ticks are outside the
declared block-only contract.

## Target behavior and implementation

The cached Yarn-mapped Minecraft Java 1.21.11 build.6 `BulbBlock` class was read
with `javap -p -c`. `onBlockAdded` and `neighborUpdate` both call `update`.
That method samples world power, returns when POWERED is unchanged, toggles LIT
only on a rising edge, and writes both properties together with flags 3.
`getComparatorOutput` reads LIT and returns 15 or 0. It is not normal signal
emission. Sound, oxidation and entity interactions are not simulated.

`device_program/builtin_laws.rs` expresses that transition as an eight-row input
domain in a checked Rust `StaticLaw`. `builtins.rs` binds both callbacks to the
same queries and atomic write operation. No bulb-specific runtime event,
continuation or timer was introduced. Added callbacks may now sample installed
world power; removed and pre-write shape callbacks retain their restrictions.

The definition's primary Boolean is LIT, mirrored by `Block.powered`, as for a
lamp. POWERED is a separate required Boolean in the block properties, including
on synthetic blocks. `comparator_output` is independent of weak/strong signal
emission. The readout query is ready for the comparator milestone; comparator
execution itself is not admitted by this milestone.

## Physical, placement and persistence boundaries

The bulb has its own `BlockKind`, appended without changing existing enum tags
used by stored world identifiers. Physical traits declare a conducting full cube
with full support and no dust arm connection. New kinds project these facts into
ordinary placement traits; historical finite spatial laws keep their old ABI.
Current placement validates declared bulb state. Historical placement, the old
compatibility/bounded executors and piston payload laws reject the new kind.
Moving a bulb with a piston is outside this goal's payload contract.

Current command snapshots export device state from the definition's property
list and physical orientation. This also preserves existing directional and
attached device state without per-device export overrides. A powered bulb
installation may settle to a different LIT/POWERED pair than the requested
command; the construction plan exposes the request and expected settled result.
Teardown is simulated through the same callbacks. Missing properties and extra
unexported observed properties cannot pass the lossless snapshot check.

Both bits remain in the world/checkpoint and behavioral comparison state.
Execution/root exploration is v11, device programs v5, physical admission v2,
and ordinary placement v3. Old v10 execution approvals require fresh review.
There is no checkpoint conversion and no new public MCP operation.

## Evidence

Tests cover six input directions, held/repeated input, all eight initial input
combinations, four concrete identities, atomic writes during powered insertion,
restoration at every microstep and root comparison restoration, property
roundtrip, construction/teardown, and historical/payload admission rejection.
These are source-derived and model regression checks. No new live Minecraft
measurement has been made for the copper bulb.
