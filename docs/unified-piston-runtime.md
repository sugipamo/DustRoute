# Unified electrical piston runtime

The current v8 execution/exploration profiles implement source-ordered movement
writes and native stationary/moving observer and lamp callbacks in one world.
The reference 3×3 door matches retained Java 1.21.11 tick ends, ordered palette
writes and callback-visible worlds. Saved v1–v7 contexts require explicit fresh
verification. See [movement staging and evidence](staged-piston-motion.md) and
[native devices](native-device-callbacks.md) for the tested scope.

`time::piston_runtime::new_piston_runtime(world, region, limits)` creates the
single supported callback piston runtime, `ElectricalPistonRuntime`.
Horizontal, upward and downward bodies share one world, queue and physical
law selection. `schedule_electrical_input` or `input_now` operates actual levers.
Construction and removal simulations also use this same runtime. The v6
increment models command preprocessing, flags-258 callback order and explicit
observer initialization; see [live command evidence](reference-door-live-construction.md).

`schedule_electrical_input` places an input before a modeled tick.
`schedule_electrical_input_after_tick` represents a measured server-thread
interaction after that world tick returns. Keeping the latter's world time is
necessary for the same-tick piston retract decision; simply moving the input
before the next tick can select a different block event. See
[transient comparison evidence](piston-transient-conformance.md).

The world profile is `dustroute.piston-electrical-callbacks.java-1-21-11.v8`;
behavior review uses `dustroute.piston-electrical-root-exploration.v8`.
Lamp, observer and stone-button callbacks now use [Rust constant device programs](typed-device-runtime.md).
`use_now` and `schedule_device_use_after_tick` expose stone-button interaction in
the library runtime. Behavioral review still uses declared lever inputs; no new
public MCP button-operation protocol or live button certification is implied.

The supported electrical scope includes dust, repeaters, conductor power and
quasi-connectivity, lamp transitions and stationary/moving observer pulses. Fresh construction validates stable explicit evidence and
queues initial notifications. Every proposed write is checked before commitment.

The runtime preserves source-ordered nested callbacks, independent motion
carriers, tick sections and pending work. Exact checkpoints and behavior-state
restoration both check runtime/adapter identity. A visible-world snapshot cannot
recover hidden pending work. The behavior explorer uses this runtime directly;
there is no direction/profile dispatch or intermediate runtime enum.

Java 1.21.11 behavior and recorded observations are the reference. Historical
horizontal, vertical and direct-only callback runtimes have been removed. Their
profile IDs are rejected during deserialization; they are not aliases for the
electrical profile. Old checkpoints cannot resume here. Fresh validation needs
explicit initial conditions and current law requirements, not a renamed saved
pass. See [retirement and saved data](stabilization-legacy-paths.md).

The electrical v2 profile introduced movable payloads for retracted ordinary/sticky
bodies in all directions and mixed linear chains. Electrical v1 and v2 contexts
are rejected rather than silently upgraded; existing law requirements need an
explicit proposal and fresh review. See [body movement](piston-payload-conformance.md).

The current boundaries, verification and live evidence are documented in:

- [Movable piston bodies and 3×3 prerequisites](piston-payload-conformance.md)
- [Electrical source audit](piston-electrical-source-audit.md)
- [Electrical live comparison](piston-electrical-live-evidence.md)
- [Custom Assembly construction](custom-piston-assembly-placement.md)
- [Persistent placed Assembly management](placed-assembly-management.md)

`piston_runtime` and `unified_piston_runtime` test interruption, all facings,
shared queues and restoration on the electrical runtime. `electrical_piston_runtime`
covers electrical callbacks; `electrical_piston_observations` replays recorded
server-applied inputs and observed settled states. Historical captures remain
unaltered evidence, not golden outputs imposed on current execution.
