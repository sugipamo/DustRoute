//! Read-only model diagnostic for the pre-migration input-boundary contract.
//! This is not a live Minecraft observation or an authorization to place blocks.
#[allow(dead_code)]
#[path = "../../../tests/support/diagnostic_fixture.rs"]
mod diagnostic_fixture;

use dustroute_minecraft::time::PhysicsEngine;
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonState, PistonVariant, Pos, Region, World, piston_state,
};
use serde_json::json;

#[test]
#[ignore = "explicit offline diagnostic fixture; requires absolute DUSTROUTE_DIAGNOSTIC_OUTPUT"]
fn retain_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = diagnostic_fixture::output()?;
    let piston = Pos::new(0, 1, 0);
    for (name, source, region) in [
        (
            "north_on_south_unknown",
            Pos::new(0, 1, -1),
            Region::new(Pos::new(-1, 0, -1), Pos::new(3, 2, 0)),
        ),
        (
            "south_on_north_unknown",
            Pos::new(0, 1, 1),
            Region::new(Pos::new(-1, 0, 0), Pos::new(3, 2, 1)),
        ),
        (
            "north_on_complete",
            Pos::new(0, 1, -1),
            Region::new(Pos::new(-1, 0, -1), Pos::new(3, 2, 1)),
        ),
    ] {
        let mut world = World::new();
        world.fill(
            Pos::new(-1, 0, -1),
            Pos::new(3, 0, 1),
            Block::new(BlockKind::Solid),
        );
        let body = world.place(BlockKind::Piston, piston);
        body.facing = Some(Facing::East);
        body.piston_variant = Some(PistonVariant::Normal);
        body.piston_state = Some(PistonState::Retracted);
        world.place(BlockKind::Solid, Pos::new(1, 1, 0));
        let input = world.place(BlockKind::Lever, source);
        input.powered = Some(false);
        input.support_offset = Some(Pos::new(0, -1, 0));
        let mut engine =
            PhysicsEngine::new_diagnostic(world, 64).with_piston_planning_region(region);
        for powered in [true, false] {
            let before = engine.world().clone();
            engine.schedule_redstone_input(engine.time().game_tick + 1, source, powered);
            let result = engine.run_redstone_piston_events();
            diagnostic_fixture::row(
                &mut output,
                &json!({
                    "case": name, "requested_powered": powered,
                    "result": result.as_ref().map(|_| "ok").map_err(ToString::to_string),
                    "world_unchanged": engine.world() == &before,
                    "stored_input": engine.world().get(source).unwrap().powered,
                    "piston_state": piston_state(engine.world().get(piston).unwrap()),
                    "payload_at_two": engine.world().kind_at(Pos::new(2, 1, 0)) == BlockKind::Solid,
                    "pending_events": engine.pending_event_count(),
                    "trace_complete": engine.trace_status().is_complete(),
                }),
            )?;
            if result.is_err() {
                break;
            }
        }
    }
    Ok(())
}
