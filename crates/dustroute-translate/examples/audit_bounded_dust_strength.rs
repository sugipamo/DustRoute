//! Read-only diagnostic of the current bounded dust-strength boundary.
//! The pre-migration results are frozen separately; reruns reflect current policy.
//! Diagnostic u8 inputs above 15 are not valid placement or live-world evidence.
use dustroute_library::builtin_laws::dust_law_revision;
use dustroute_minecraft::time::{PhysicsEngine, PhysicsEventKind};
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, World};
use dustroute_translate::dust_law::DustLaw;
use serde_json::json;

fn main() {
    let law = DustLaw::from_revision(dust_law_revision()).unwrap();
    let source = Pos::new(0, 1, 0);
    let first = Pos::new(1, 1, 0);
    let second = Pos::new(2, 1, 0);
    for level in [0, 1, 14, 15, 16, 255] {
        let mut world = World::new();
        world.fill(
            Pos::new(-1, 0, -1),
            Pos::new(3, 0, 1),
            Block::new(BlockKind::Solid),
        );
        // The bounded runner reads this as an already supplied directional
        // source. It does not execute comparator calculation or timing.
        let b = world.place(BlockKind::Comparator, source);
        b.facing = Some(Facing::East);
        b.power_level = Some(level);
        world.place(BlockKind::RedstoneWire, first);
        world.place(BlockKind::RedstoneWire, second);
        let rejects_level = world.placement_issues().iter().any(|issue| {
            matches!(issue, dustroute_minecraft::WorldValidationIssue::InvalidState {
                position, reason, ..
            } if *position == source && reason.contains("power level"))
        });
        let mut engine = PhysicsEngine::new_diagnostic(world, 4096)
            .with_piston_planning_region(Region::new(Pos::new(-3, -1, -3), Pos::new(5, 3, 3)));
        engine.schedule_external(0, first, PhysicsEventKind::NeighborUpdate { source });
        let result = engine.run_redstone_propagation();
        println!(
            "{}",
            json!({
                "source_level": level,
                "placement_rejects_source_level": rejects_level,
                "execution_mode": "diagnostic",
                "result": result.as_ref().map(|_| "ok").map_err(ToString::to_string),
                "bounded_first_wire": engine.world().get(first).unwrap().power_level.unwrap_or(0),
                "bounded_second_wire": engine.world().get(second).unwrap().power_level.unwrap_or(0),
                "dust_law_direct": law.strength(level, 0),
                "dust_law_neighbor": law.strength(0, level),
                "pending_events": engine.pending_event_count(),
                "trace_complete": engine.trace_status().is_complete(),
            })
        );
    }
}
