//! Pre-migration model capture. This is not a live Minecraft observation.
use dustroute_minecraft::time::{PhysicsEngine, PhysicsEventKind};
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, World};
use dustroute_translate::sim::{RedstoneTickSimulator, SimulationEventKind};
use serde_json::{Value, json};

const INPUT: Pos = Pos::new(-2, 1, 0);
const REPEATER: Pos = Pos::new(0, 1, 0);
const OUTPUT: Pos = Pos::new(1, 1, 0);
const LOCK_INPUT: Pos = Pos::new(0, 1, -2);
const LOCK: Pos = Pos::new(0, 1, -1);

fn world(delay: u8, powered: bool) -> World {
    let mut world = World::new();
    world.fill(
        Pos::new(-2, 0, -2),
        Pos::new(2, 0, 0),
        Block::new(BlockKind::Solid),
    );
    let input = world.place(BlockKind::Lever, INPUT);
    input.powered = Some(powered);
    input.support_offset = Some(Pos::new(0, -1, 0));
    world.place(BlockKind::RedstoneWire, Pos::new(-1, 1, 0));
    let repeater = world.place(BlockKind::Repeater, REPEATER);
    repeater.facing = Some(Facing::East);
    repeater.delay = Some(delay);
    repeater.powered = Some(powered);
    world.place(BlockKind::RedstoneWire, OUTPUT);
    world.place(BlockKind::RedstoneLamp, Pos::new(2, 1, 0));
    dustroute_translate::wire::update_wire_shapes(&mut world);
    world
}

pub fn capture() -> Vec<Value> {
    let mut rows = Vec::new();
    for delay in 1..=4 {
        for initially_powered in [false, true] {
            for pattern in ["held", "pulse", "reverse_twice"] {
                let world = world(delay, initially_powered);
                let mut sim = RedstoneTickSimulator::new(world.clone()).unwrap();
                let mut engine =
                    PhysicsEngine::new_diagnostic(world, 2048).with_piston_planning_region(
                        Region::new(Pos::new(-3, 0, -3), Pos::new(3, 2, 1)),
                    );
                let mut compat = Vec::new();
                for boundary in 0..10 {
                    let change = match (boundary, pattern) {
                        (0, _) | (2, "reverse_twice") => Some(!initially_powered),
                        (1, "pulse" | "reverse_twice") => Some(initially_powered),
                        _ => None,
                    };
                    if let Some(powered) = change {
                        sim.set_powered(INPUT, powered).unwrap();
                        engine.schedule_redstone_input(boundary * 2, INPUT, powered);
                    }
                    let mut events = Vec::new();
                    loop {
                        events.push(sim.step_event().unwrap());
                        if sim.pending_scheduler_events() == 0 {
                            break;
                        }
                    }
                    let update = events
                        .iter()
                        .find(|e| e.event_kind == SimulationEventKind::RepeaterUpdate)
                        .unwrap();
                    let end = sim.snapshot();
                    compat.push(json!([
                        update.time,
                        update.from.repeater_powered[&REPEATER],
                        update.to.repeater_powered[&REPEATER],
                        format!("{:?}", update.kind),
                        end.strength(OUTPUT),
                        sim.has_pending_events()
                    ]));
                }
                engine.run_redstone_propagation().unwrap();
                let bounded: Vec<_> = engine
                    .event_trace()
                    .records
                    .iter()
                    .filter(|r| matches!(r.event.kind, PhysicsEventKind::RepeaterTick { .. }))
                    .map(|r| json!([r.event.time, r.event.kind, r.status]))
                    .collect();
                let changes: Vec<_> = engine
                    .transition_trace()
                    .records
                    .iter()
                    .flat_map(|r| {
                        r.changes
                            .iter()
                            .filter(|c| c.position == REPEATER)
                            .map(|c| json!([r.time, c.before.powered, c.after.powered]))
                    })
                    .collect();
                rows.push(json!({
                    "delay": delay, "initially_powered": initially_powered, "pattern": pattern,
                    "compatibility": compat, "bounded_events": bounded, "bounded_changes": changes,
                    "bounded_final": engine.world().get(REPEATER).unwrap().powered,
                    "bounded_complete": engine.trace_status().is_complete()
                }));
            }
            // Lock a queued edge, then release it while the rear input reverses.
            // The bounded runner explicitly excludes locking; do not run it here.
            let mut world = world(delay, initially_powered);
            let lock_input = world.place(BlockKind::Lever, LOCK_INPUT);
            lock_input.powered = Some(false);
            lock_input.support_offset = Some(Pos::new(0, -1, 0));
            let lock = world.place(BlockKind::Repeater, LOCK);
            lock.facing = Some(Facing::South);
            lock.delay = Some(1);
            lock.powered = Some(false);
            let mut sim = RedstoneTickSimulator::new(world).unwrap();
            let mut states = Vec::new();
            for boundary in 0..12 {
                match boundary {
                    0 => {
                        sim.set_powered(INPUT, !initially_powered).unwrap();
                        sim.set_powered(LOCK_INPUT, true).unwrap();
                    }
                    2 => {
                        sim.set_powered(INPUT, initially_powered).unwrap();
                    }
                    4 => {
                        sim.set_powered(LOCK_INPUT, false).unwrap();
                    }
                    _ => {}
                }
                let state = sim.advance_tick().unwrap();
                states.push(json!([
                    state.tick,
                    state.repeater_powered[&REPEATER],
                    state.repeater_powered[&LOCK],
                    sim.has_pending_events()
                ]));
            }
            rows.push(json!({"delay": delay, "initially_powered": initially_powered, "pattern": "lock_then_release", "compatibility": states}));
        }
    }
    rows
}
