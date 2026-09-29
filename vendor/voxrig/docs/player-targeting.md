# Player block targeting

`observe_player_target` retains the existing `block_collision` query.
`observe_player_outline_target` returns `block_outline` and includes thin circuit
parts. Both read player pose and the reconstructed world under one connection
lock, returning connection ID, packet sequence and local client frame.
Connection IDs are process-local counters. Archived observations must also be
scoped to their owning process/capture; restarting a program does not make old
observations current merely because its new connection counter is the same.

The outline query selects **static blocks**, skips fluids, and does not select
entities. It is based on received pose/rotation, not a graphical client's
interpolated camera frame or a server readback. A moving carrier, missing chunk,
unknown pose, incomplete reconstruction or unsupported shape makes the result
unavailable; no collision/full-cube/air fallback is used.

The audited Java 1.21.11 table covers 22,899 of 29,671 native states. It includes
full cubes, air/fluids, redstone wire, switches, pressure plates, torches, gates,
pistons/heads, stairs/slabs, fences/panes/walls, doors/trapdoors, tripwire,
daylight detectors, rods and hoppers. This is **selection geometry coverage**, not
a claim that every block's physics is simulated. `outline_coverage.json` lists
every block and native method owner; unreviewed owners remain unsupported.
Examples requiring additional context include light blocks (held item),
scaffolding, shulker boxes and position-offset plants.

Native DDA cell order, crossing ties, the inside-shape probe, box epsilon rules,
and the hopper's auxiliary face shape are separate from collision targeting.
Native float angles and sine-table indexing determine the outline ray direction.

## Independent native oracle

The original Java helper invokes the local game's shape and raycast methods;
expected hits are not calculated by the Rust implementation. The retained 25,394
cases cover every distinct supported outline/auxiliary shape pair, axis faces,
edges, thin layers, inside starts, randomized rays, zero length, negative
coordinates and multi-cell crossing ties. Another 2,304 cases call native
`Entity.getRotationVector`. Every native state ID/name/property is checked against
the existing registry using a stable identity checksum.

World-coordinate edge regressions include positions 100, 42,000 and 29,999,980.
The original local-coordinate epsilon check incorrectly selected a lever for
native miss case 25376 at `(100.3124999, 180.5, 98)`. Translating each box to world
coordinates before intersection preserves native rounding and passes the case.

Regeneration requires locally obtained Java 1.21.11 merged development input,
Yarn `1.21.11+build.6` mappings (`official`, `intermediary`, `named` namespaces),
Java 21+, TinyRemapper 0.14.0, mapping-io 0.8.0, ASM 9.10.1 and game libraries.
Provide an absolute colon-separated classpath in a text file:

```sh
python3 scripts/export_outline_shapes.py \
  --minecraft-jar /path/to/minecraft-merged.jar \
  --mappings /path/to/mappings.tiny \
  --classpath-file /path/to/classpath.txt --check
```

Omit `--check` to regenerate. This is a development-only operation; consumers
need neither Java nor these libraries. The temporary JAR first gets package
access repairs from TinyRemapper, then public access flags for non-private
members because remapping splits original packages. No native method bodies are
changed. No Minecraft JAR, mapping contents or decompiled source is distributed.
`outline_source.json` records input, generator and output checksums.
