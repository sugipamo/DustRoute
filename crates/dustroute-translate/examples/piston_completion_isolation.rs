//! Offline isolation only. Does not change production completion validation.
use dustroute_minecraft::time::PhysicsEngine;
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonAction, PistonVariant, Pos, World, plan_piston,
};
use serde_json::{Value, json};

fn world() -> World {
    let mut world = World::new();
    for z in [0, 10] {
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(Facing::East);
        piston.piston_variant = Some(PistonVariant::Sticky);
        world.set(Pos::new(0, 1, z), piston);
        world.set(Pos::new(1, 1, z), Block::new(BlockKind::Solid));
    }
    let mut lever = Block::new(BlockKind::Lever);
    lever.powered = Some(false);
    world.set(Pos::new(20, 1, 20), lever);
    world
}

fn scheduled(action: PistonAction, gap: u64, reverse: bool) -> Value {
    let mut world = world();
    if action == PistonAction::Retract {
        for z in [0, 10] {
            plan_piston(&world, Pos::new(0, 1, z), PistonAction::Extend)
                .unwrap()
                .apply(&mut world)
                .unwrap();
        }
    }
    let mut engine = PhysicsEngine::new_diagnostic(world, 64);
    let order = if reverse { [10, 0] } else { [0, 10] };
    engine.schedule_piston_action(0, Pos::new(0, 1, order[0]), action);
    engine.schedule_piston_action(gap, Pos::new(0, 1, order[1]), action);
    let result = engine.run_piston_events();
    json!({"action": action, "gap_ticks": gap, "reverse_order": reverse,
        "error": result.err().map(|e|e.to_string()), "pending":engine.pending_event_count(),
        "transitions":engine.transition_trace(), "trace_status":engine.trace_status()})
}

fn mutation(name: &str) -> Value {
    let mut world = world();
    let plan = plan_piston(&world, Pos::new(0, 1, 0), PistonAction::Extend).unwrap();
    plan.start_delta().apply(&mut world).unwrap();
    let completion = plan.completion_plan(&world).unwrap();
    let original_shape = world.shape_id();
    match name {
        "none" => {}
        "distant_geometry" => {
            world.set(Pos::new(30, 1, 30), Block::new(BlockKind::Solid));
        }
        "distant_signal" => {
            let pos = Pos::new(20, 1, 20);
            let mut lever = world.get(pos).unwrap().clone();
            lever.powered = Some(true);
            world.set(pos, lever);
        }
        "other_piston_start" => {
            plan_piston(&world, Pos::new(0, 1, 10), PistonAction::Extend)
                .unwrap()
                .start_delta()
                .apply(&mut world)
                .unwrap();
        }
        "destination_replaced" => {
            world.set(Pos::new(2, 1, 0), Block::new(BlockKind::Solid));
        }
        "moving_source_removed" => {
            world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Air));
        }
        _ => unreachable!(),
    }
    let before = world.state_id();
    let mut applied = world.clone();
    let error = completion
        .world_delta()
        .apply(&mut applied)
        .err()
        .map(|e| e.to_string());
    // Diagnostic copy only: identify which additional per-cell check would fire.
    // Never used by the engine; this is not a proposed production fix.
    let mut diagnostic = completion.world_delta().clone();
    diagnostic.parent_shape = world.shape_id();
    let local_error = diagnostic.validate(&world).err().map(|e| e.to_string());
    json!({"mutation":name,"shape_changed":world.shape_id()!=original_shape,
        "error":error,"unchanged_on_rejection":error.is_none() || applied.state_id()==before,
        "diagnostic_rebased_validation_error":local_error})
}

fn main() {
    let mut schedules = Vec::new();
    for action in [PistonAction::Extend, PistonAction::Retract] {
        for gap in [0, 1, 2, 3] {
            schedules.push(scheduled(action, gap, false));
        }
        schedules.push(scheduled(action, 0, true));
    }
    let mutations: Vec<_> = [
        "none",
        "distant_geometry",
        "distant_signal",
        "other_piston_start",
        "destination_replaced",
        "moving_source_removed",
    ]
    .into_iter()
    .map(mutation)
    .collect();
    println!(
        "{}",
        json!({"schema_version":"dustroute.piston-completion-isolation.v1",
        "execution_mode":"offline_diagnostic","schedules":schedules,"mutations":mutations})
    );
}
