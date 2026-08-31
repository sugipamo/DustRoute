use std::time::Instant;

use dustroute_translate::{
    Block, BlockKind, Pos, RegionBounds, World, analyze_signal_liveness, analyze_world_region,
    update_wire_shapes,
};

fn main() {
    for component_count in [100, 500, 1_000, 2_000, 4_000] {
        let mut world = World::new();
        for x in 0..component_count {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
            world.place(BlockKind::RedstoneWire, Pos::new(x, 1, 0));
        }
        update_wire_shapes(&mut world);

        let analysis_started = Instant::now();
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(0, 0, 0), Pos::new(component_count - 1, 1, 0)),
        );
        let analysis_elapsed = analysis_started.elapsed();

        let liveness_started = Instant::now();
        let liveness = analyze_signal_liveness(&analysis.scene);
        let liveness_elapsed = liveness_started.elapsed();
        println!(
            "components={component_count} edges={} analyze_ms={:.3} liveness_ms={:.3}",
            analysis.graph.edges.len(),
            analysis_elapsed.as_secs_f64() * 1_000.0,
            liveness_elapsed.as_secs_f64() * 1_000.0,
        );
        std::hint::black_box((analysis, liveness));
    }
}
