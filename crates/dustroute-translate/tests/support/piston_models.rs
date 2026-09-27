//! Captured after the approved input validation repair, before law migration.
use dustroute_minecraft::time::{PhysicsEngine, PhysicsEventKind};
use dustroute_minecraft::{
    BlockKind, Facing, PistonAction, PistonState, PistonVariant, Pos, Region, World,
    plan_piston_in_region,
};
use serde_json::{Value, json};

pub fn capture() -> Vec<Value> {
    let mut rows = Vec::new();
    for facing in [Facing::North, Facing::East, Facing::South, Facing::West] {
        for variant in [PistonVariant::Normal, PistonVariant::Sticky] {
            for payload in ["empty", "stone", "glass", "twelve", "piston"] {
                let body = Pos::new(0, 1, 0);
                let d = facing.offset();
                let source = body.offset(-d.x, -d.y, -d.z);
                let mut world = World::new();
                let block = world.place(BlockKind::Piston, body);
                block.facing = Some(facing);
                block.piston_variant = Some(variant);
                block.piston_state = Some(PistonState::Retracted);
                block.observed_name = Some(
                    if variant == PistonVariant::Sticky {
                        "minecraft:sticky_piston"
                    } else {
                        "minecraft:piston"
                    }
                    .into(),
                );
                block
                    .observed_properties
                    .insert("extended".into(), "false".into());
                world.place(BlockKind::Lever, source).powered = Some(false);
                for n in 1..=match payload {
                    "empty" => 0,
                    "twelve" => 12,
                    _ => 1,
                } {
                    let pos = body.offset(d.x * n, d.y * n, d.z * n);
                    let block = world.place(
                        match payload {
                            "glass" => BlockKind::Transparent,
                            "piston" => BlockKind::Piston,
                            _ => BlockKind::Solid,
                        },
                        pos,
                    );
                    if payload == "piston" {
                        block.facing = Some(Facing::North);
                        block.piston_variant = Some(PistonVariant::Normal);
                        block.piston_state = Some(PistonState::Retracted);
                        block.powered = Some(false);
                    }
                }
                let known = Region::new(Pos::new(-15, 0, -15), Pos::new(15, 2, 15));
                let plan =
                    plan_piston_in_region(&world, known, body, PistonAction::Extend).unwrap();
                let direct = json!([
                    plan.state_before,
                    plan.state_after,
                    plan.moved,
                    plan.world_delta().changes
                ]);
                let mut engine =
                    PhysicsEngine::new_diagnostic(world, 128).with_piston_planning_region(known);
                let mut stable = Vec::new();
                for powered in [true, false] {
                    engine.schedule_redstone_input(engine.time().game_tick + 1, source, powered);
                    engine.run_redstone_piston_events().unwrap();
                    stable.push(json!([
                        engine.world().iter().collect::<Vec<_>>(),
                        engine.pending_event_count(),
                        engine.trace_status()
                    ]));
                }
                let events: Vec<_> = engine
                    .event_trace()
                    .records
                    .iter()
                    .map(|record| {
                        let kind = match record.event.kind {
                            PhysicsEventKind::RedstoneInput { .. } => "input",
                            PhysicsEventKind::NeighborUpdate { .. } => "neighbor",
                            PhysicsEventKind::BlockEvent { .. } => "block_event",
                            PhysicsEventKind::PistonComplete { .. } => "complete",
                            _ => "other",
                        };
                        json!([record.event.time, record.event.target, kind, record.status])
                    })
                    .collect();
                let transitions: Vec<_> = engine
                    .transition_trace()
                    .records
                    .iter()
                    .map(|r| json!([r.time, r.changes, r.moves, r.cause]))
                    .collect();
                rows.push(json!({"facing": facing, "variant": variant, "payload": payload, "direct_plan": direct, "events": events, "transitions": transitions, "stable": stable}));
            }
        }
    }
    rows
}
