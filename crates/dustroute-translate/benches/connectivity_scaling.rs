use std::time::Instant;

use dustroute_translate::{Block, BlockKind, Pos, World, extract_connectivity, update_wire_shapes};

fn main() {
    for component_count in [100, 500, 1_000, 2_000, 4_000, 8_000] {
        let mut world = World::new();
        for x in 0..component_count {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
            world.place(BlockKind::RedstoneWire, Pos::new(x, 1, 0));
        }
        update_wire_shapes(&mut world);
        let started = Instant::now();
        let graph = extract_connectivity(&world);
        let elapsed = started.elapsed();
        println!(
            "components={component_count} world_blocks={} edges={} extract_ms={:.3}",
            world.iter().count(),
            graph.edges.len(),
            elapsed.as_secs_f64() * 1_000.0,
        );
        std::hint::black_box(graph);
    }
}
