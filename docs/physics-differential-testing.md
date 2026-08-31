# Minecraft differential physics testing

Minecraft Java 1.21.11 is the source of truth for low-level redstone behavior.
DustRoute compares normalized observations rather than treating its simulator as
an oracle.

The Mineflayer E2E runner supports `block_trace` steps. It samples every two
game ticks (one redstone tick) and writes successful opted-in traces under
`.local/e2e-artifacts/`. These files are intentionally outside Git.

Each observation records the relative tick, cell-relative position, physical
block class, dust strength, powered state, and torch lit state. Weak and strong
conductor power are `null` for Minecraft observations because the client
protocol does not expose them. The comparator only compares properties that
both sources can observe.

The external XOR probe captures all four stable input combinations. Compare a
captured case with DustRoute using:

```console
cargo run -p dustroute-translate --example compare_external_xor_trace -- \
  .local/e2e-artifacts/external_library_xor_compatibility_probe-trace01.trace.json \
  0 1
```

The comparison reports the first mismatch in redstone-tick, position, property
order. This is the boundary for adding one physical rule and rerunning both the
semantic probe suite and Rust regressions.

The XOR reduction identified two missing distinctions:

- A lit torch strongly powers a solid block directly above it. Dust on that
  block can therefore read strength 15.
- Strong power does not chain from one solid conductor into another solid
  conductor. It may activate an adjacent receiver, but the second conductor is
  not a new powered conductor.
- Dust reads an adjacent strongly powered block even when its rendered wire
  shape has no arm in that direction. Wire-to-wire connectivity and receiving
  block power are separate rules.

After adding those rules, all four XOR input states compare without a mismatch
for 33 observations per state. Minecraft and DustRoute now agree that the
imported layout's output remains low for every input combination. The cell is
still rejected as XOR; matching the broken behavior is simulator validation,
not component-library promotion.
