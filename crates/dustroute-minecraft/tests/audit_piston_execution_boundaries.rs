//! Read-only diagnostics for the motion-time source audit. These capture the
//! retained v1 engine, not desired behavior or Vanilla conformance fixtures.
#[allow(dead_code)]
#[path = "../../../tests/support/diagnostic_fixture.rs"]
mod diagnostic_fixture;

use dustroute_minecraft::time::{
    EventOutcome, PhysicsEngine, PhysicsEventKind, PhysicsEventPhase, QueuedEvent,
};
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonState, PistonVariant, Pos, Region, World,
};
use serde_json::{Value, json};

fn callback_order(child_phase: PhysicsEventPhase) -> Value {
    let a = Pos::new(0, 1, 0);
    let b = Pos::new(4, 1, 0);
    let mut engine = PhysicsEngine::new_diagnostic(World::new(), 16);
    for (position, action) in [(a, "complete_a"), (b, "complete_b")] {
        engine.schedule_external_in_phase(
            4,
            position,
            PhysicsEventPhase::BlockEntity,
            PhysicsEventKind::UserAction {
                action: action.into(),
            },
        );
    }
    let result = engine.run_until_idle_checked(|event, _| {
        let queued = matches!(
            &event.kind,
            PhysicsEventKind::UserAction { action } if action == "complete_a"
        )
        .then_some(QueuedEvent {
            delay_ticks: 0,
            target: a,
            phase: child_phase,
            kind: PhysicsEventKind::NeighborUpdate { source: a },
        })
        .into_iter()
        .collect();
        Ok(EventOutcome {
            queued,
            ..EventOutcome::default()
        })
    });
    let accepted: Vec<_> = engine
        .event_trace()
        .records
        .iter()
        .map(|r| json!({"time": r.event.time, "target": r.event.target, "kind": r.event.kind}))
        .collect();
    json!({
        "case": "completion_callback_order", "child_phase": child_phase,
        "diagnostic_kind": "synthetic_callback_delivery_only",
        "execution_context": engine.execution_context(),
        "result": result.map_err(|e| e.to_string()), "accepted": accepted,
        "pending_events": engine.pending_event_count(), "trace_status": engine.trace_status(),
        "required_if_callback_is_synchronous": ["complete_a", "neighbor_a", "complete_b"]
    })
}

fn piston_edges(off_tick: u64) -> Value {
    let mut world = World::new();
    world.fill(
        Pos::new(-1, 0, -1),
        Pos::new(3, 0, 1),
        Block::new(BlockKind::Solid),
    );
    let body = Pos::new(0, 1, 0);
    let piston = world.place(BlockKind::Piston, body);
    piston.facing = Some(Facing::East);
    piston.piston_variant = Some(PistonVariant::Sticky);
    piston.piston_state = Some(PistonState::Retracted);
    let head = Pos::new(1, 1, 0);
    let output = Pos::new(2, 1, 0);
    world.place(BlockKind::Solid, head);
    let source = Pos::new(-1, 1, 0);
    let lever = world.place(BlockKind::Lever, source);
    lever.powered = Some(false);
    lever.support_offset = Some(Pos::new(0, -1, 0));
    let mut engine = PhysicsEngine::new_diagnostic(world, 256)
        .with_piston_planning_region(Region::new(Pos::new(-3, -1, -3), Pos::new(5, 3, 3)));
    engine.schedule_redstone_input(1, source, true);
    engine.schedule_redstone_input(off_tick, source, false);
    let result = engine.run_redstone_piston_events();
    let events: Vec<_> = engine
        .event_trace()
        .records
        .iter()
        .filter_map(|r| {
            let label = match r.event.kind {
                PhysicsEventKind::RedstoneInput { powered: true } => "input_on",
                PhysicsEventKind::RedstoneInput { powered: false } => "input_off",
                PhysicsEventKind::BlockEvent { .. } => "block_event",
                PhysicsEventKind::PistonComplete { .. } => "completion",
                _ => return None,
            };
            Some(json!({"time": r.event.time, "label": label, "status": r.status}))
        })
        .collect();
    let changes: Vec<_> = engine
        .trace()
        .iter()
        .filter(|t| [body, head, output].contains(&t.position))
        .map(|t| json!({"time": t.time, "position": t.position, "after": t.after}))
        .collect();
    json!({
        "case": if off_tick == 2 { "power_removed_before_queued_extension" } else { "settled_retraction_carriers" },
        "execution_context": engine.execution_context(), "on_tick": 1, "off_tick": off_tick,
        "body": body, "head": head, "output": output,
        "result": result.map_err(|e| e.to_string()), "events": events, "changes": changes,
        "final_input": engine.world().get(source).unwrap().powered,
        "final_body": engine.world().get(body), "final_output": engine.world().kind_at(output),
        "pending_events": engine.pending_event_count(), "trace_status": engine.trace_status()
    })
}

#[test]
#[ignore = "explicit offline diagnostic fixture; requires absolute DUSTROUTE_DIAGNOSTIC_OUTPUT"]
fn retain_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = diagnostic_fixture::output()?;
    for row in [
        callback_order(PhysicsEventPhase::NeighborUpdate),
        callback_order(PhysicsEventPhase::BlockEntity),
        piston_edges(2),
        piston_edges(8),
    ] {
        diagnostic_fixture::row(&mut output, &row)?;
    }
    Ok(())
}
