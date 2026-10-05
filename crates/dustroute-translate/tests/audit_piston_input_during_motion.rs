//! Read-only preflight for location-state behavioral verification.
//! Existing deterministic model results, not live Minecraft conformance or a
//! definition of allowed input timing. No production handler is changed here.
#[allow(dead_code)]
#[path = "../../../tests/support/diagnostic_fixture.rs"]
mod diagnostic_fixture;

use dustroute_minecraft::time::{PhysicsEngine, PhysicsEventKind};
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonState, PistonVariant, Pos, Region, World,
};
use dustroute_translate::{snapshot::MinecraftSnapshot, snapshot::world_from_snapshot};
use serde_json::{Value, json};

fn simple_world() -> (World, Region, Pos, Vec<Pos>) {
    let mut world = World::new();
    world.fill(
        Pos::new(-1, 0, -1),
        Pos::new(3, 0, 1),
        Block::new(BlockKind::Solid),
    );
    let piston = world.place(BlockKind::Piston, Pos::new(0, 1, 0));
    piston.facing = Some(Facing::East);
    piston.piston_variant = Some(PistonVariant::Sticky);
    piston.piston_state = Some(PistonState::Retracted);
    world.place(BlockKind::Solid, Pos::new(1, 1, 0));
    let source = Pos::new(-1, 1, 0);
    let lever = world.place(BlockKind::Lever, source);
    lever.powered = Some(false);
    lever.support_offset = Some(Pos::new(0, -1, 0));
    (
        world,
        Region::new(Pos::new(-3, -1, -3), Pos::new(5, 3, 3)),
        source,
        vec![Pos::new(2, 1, 0)],
    )
}

fn two_row_world() -> (World, Region, Pos, Vec<Pos>) {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer/07-single-input-two-row.json"
    ))
    .unwrap();
    let initial: MinecraftSnapshot = serde_json::from_value(fixture["initial"].clone()).unwrap();
    (
        world_from_snapshot(&initial).unwrap(),
        Region::new(initial.min, initial.max),
        serde_json::from_value(fixture["input"].clone()).unwrap(),
        vec![Pos::new(2, 0, 0), Pos::new(2, 1, 0)],
    )
}

#[test]
#[ignore = "explicit offline diagnostic fixture; requires absolute DUSTROUTE_DIAGNOSTIC_OUTPUT"]
fn retain_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = diagnostic_fixture::output()?;
    for (layout, direct) in [
        ("single_sticky_direct", true),
        ("single_sticky_propagation", false),
        ("existing_two_row_propagation", false),
    ] {
        for off_tick in [3, 8] {
            let (world, known, input, outputs) = if layout == "existing_two_row_propagation" {
                two_row_world()
            } else {
                simple_world()
            };
            let mut engine =
                PhysicsEngine::new_diagnostic(world, 4096).with_piston_planning_region(known);
            let initial_outputs: Vec<_> =
                outputs.iter().map(|p| engine.world().kind_at(*p)).collect();
            engine.schedule_redstone_input(1, input, true);
            engine.schedule_redstone_input(off_tick, input, false);
            let result = if direct {
                engine.run_redstone_piston_events()
            } else {
                engine.run_redstone_propagation()
            };
            let events: Vec<_> = engine.event_trace().records.iter().filter_map(|r| {
                let kind = match &r.event.kind {
                    PhysicsEventKind::RedstoneInput { powered } => if *powered { "input_on" } else { "input_off" },
                    PhysicsEventKind::BlockEvent { .. } => "block_event",
                    PhysicsEventKind::PistonComplete { .. } => "complete",
                    PhysicsEventKind::NeighborUpdate { .. } if engine.world().kind_at(r.event.target) == BlockKind::Piston => "piston_neighbor_update",
                    _ => return None,
                };
                Some(json!({"time":r.event.time, "target":r.event.target, "kind":kind, "status":r.status}))
            }).collect();
            let samples: Vec<_> = engine.transition_trace().records.iter().flat_map(|r| {
                r.changes.iter().filter(|c| outputs.contains(&c.position)).map(|c| json!({"time":r.time, "position":c.position, "before":c.before.kind, "after":c.after.kind}))
            }).collect();
            let final_outputs: Vec<_> =
                outputs.iter().map(|p| engine.world().kind_at(*p)).collect();
            diagnostic_fixture::row(
                &mut output,
                &json!({
                    "layout":layout, "on_tick":1, "off_tick":off_tick, "input_position":input, "output_positions":outputs,
                    "execution_context":engine.execution_context(),
                    "initial_outputs":initial_outputs, "result":result.as_ref().map(|_| "ok").map_err(ToString::to_string),
                    "final_input":engine.world().get(input).unwrap().powered,
                    "final_outputs":final_outputs, "expected_if_off_settles_to_absence":vec![BlockKind::Air; outputs.len()],
                    "pending_events":engine.pending_event_count(), "trace_status":engine.trace_status(),
                    "events":events, "location_changes":samples,
                }),
            )?;
        }
    }
    Ok(())
}
