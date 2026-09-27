//! Read-only model comparison for the spatial-law migration. This is a
//! diagnostic, not a claim that either existing model matches live Minecraft.
use std::collections::BTreeMap;

use dustroute_minecraft::time::PhysicsEngine;
use dustroute_minecraft::{
    Block, BlockKind, Facing, HistoricalPlacementV1, Pos, ValidatedWorld, WireConnection, World,
};
use dustroute_translate::electrical::{DeviceOutputState, solve_instantaneous};

fn main() {
    let input = Pos::new(-1, 1, 0);
    let lower = Pos::new(0, 1, 0);
    let upper = Pos::new(1, 2, 0);
    for blocked in [false, true] {
        let mut world = World::new();
        for pos in [Pos::new(-1, 0, 0), Pos::new(0, 0, 0), Pos::new(1, 1, 0)] {
            world.set(pos, Block::new(BlockKind::Solid));
        }
        let lever = world.place(BlockKind::Lever, input);
        lever.support_offset = Some(Pos::new(0, -1, 0));
        lever.powered = Some(false);
        for (pos, east, west) in [
            (lower, WireConnection::Up, WireConnection::Side),
            (upper, WireConnection::None, WireConnection::Side),
        ] {
            let wire = world.place(BlockKind::RedstoneWire, pos);
            wire.support_offset = Some(Pos::new(0, -1, 0));
            wire.power_level = Some(0);
            wire.wire_connections = Some(BTreeMap::from([
                (Facing::North, WireConnection::None),
                (Facing::East, east),
                (Facing::South, WireConnection::None),
                (Facing::West, west),
            ]));
        }
        if blocked {
            // Preserve the explicit arm while adding an obstruction. This is
            // accepted only by the historical placement gate when obstructed.
            world.set(lower.offset(0, 1, 0), Block::new(BlockKind::Solid));
        }
        HistoricalPlacementV1::try_from(world.clone()).expect("historical placement accepted");
        let current_passes = ValidatedWorld::try_from(world.clone()).is_ok();
        // Diagnostic replay never grants a current placement proof.
        let mut engine = PhysicsEngine::new_diagnostic(world.clone(), 4096);
        engine.schedule_redstone_input(0, input, true);
        engine
            .run_redstone_propagation()
            .expect("event run completes");
        world.get_mut(input).unwrap().powered = Some(true);
        let solved = solve_instantaneous(&world, &DeviceOutputState::initially_lit(&world), 128)
            .expect("electrical solve completes");
        println!(
            "blocked={blocked}; historical_placement=passed; current_placement_passes={current_passes}; solver(lower,upper)=({},{}); event_runner(lower,upper)=({},{})",
            solved.signal(lower),
            solved.signal(upper),
            engine.world().get(lower).unwrap().power_level.unwrap(),
            engine.world().get(upper).unwrap().power_level.unwrap(),
        );
    }
}
