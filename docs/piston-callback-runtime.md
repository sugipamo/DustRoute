# Piston callback runtime

The directional callback runtimes described by the original migration have
been retired. Use the [unified electrical runtime](unified-piston-runtime.md)
for all six body directions. Old horizontal/vertical constructor APIs, law
aliases and profile selection no longer exist.

The shared motion-time laws still implement request/delivery power checks,
normal/sticky extension and retraction, independently ticking carriers,
interruption, forced completion and source-ordered notifications. Correctness
is assessed against Java 1.21.11 and retained observed trials, not old model
output. See the [motion source audit](piston-motion-source-audit.md) and
[live interruption evidence](piston-live-interruption-conformance.md).

For current supported placement and its limits, see
[custom piston Assembly placement](custom-piston-assembly-placement.md).
For deleted paths and saved-profile rejection, see
[stabilization](stabilization-legacy-paths.md).
