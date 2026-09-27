//! Pre-migration model captures, not live Minecraft observations.
use dustroute_minecraft::time::PhysicsEngine;
use dustroute_minecraft::{Block, BlockKind, Pos, Region, World};
use dustroute_translate::sim::{RedstoneTickSimulator, SimulationEventKind};
use serde_json::{Value, json};

pub const INPUT: Pos = Pos::new(0, 1, 0);
pub const LAMP: Pos = Pos::new(2, 1, 0);

pub fn world(powered: bool, lit: Option<bool>) -> World {
    let mut world = World::new();
    world.fill(
        Pos::new(0, 0, 0),
        Pos::new(2, 0, 0),
        Block::new(BlockKind::Solid),
    );
    let input = world.place(BlockKind::Lever, INPUT);
    input.powered = Some(powered);
    input.support_offset = Some(Pos::new(0, -1, 0));
    let wire = world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
    wire.power_level = Some(if powered { 15 } else { 0 });
    world.place(BlockKind::RedstoneLamp, LAMP).powered = lit;
    dustroute_translate::wire::update_wire_shapes(&mut world);
    world
}

pub fn drain(sim: &mut RedstoneTickSimulator) -> Vec<Value> {
    let mut rows = Vec::new();
    loop {
        let event = sim.step_event().unwrap();
        if event.event_kind == SimulationEventKind::LampUpdate {
            rows.push(json!([
                event.time,
                format!("{:?}", event.kind),
                event.from.lamp_lit.get(&LAMP),
                event.to.lamp_lit.get(&LAMP),
                sim.has_pending_events()
            ]));
        }
        if sim.pending_scheduler_events() == 0 {
            break;
        }
    }
    rows
}

pub fn capture() -> Vec<Value> {
    let mut rows = Vec::new();
    for powered in [false, true] {
        for lit in [None, Some(false), Some(true)] {
            for pattern in ["idle", "held", "pulse", "reverse_twice"] {
                let world = world(powered, lit);
                let mut sim = RedstoneTickSimulator::new(world.clone()).unwrap();
                let mut engine =
                    PhysicsEngine::new_diagnostic(world, 1024).with_piston_planning_region(
                        Region::new(Pos::new(-1, 0, -1), Pos::new(3, 2, 1)),
                    );
                let initial = json!([
                    sim.snapshot().lamp_lit.get(&LAMP),
                    sim.has_pending_events(),
                    engine.world().get(LAMP).unwrap().powered
                ]);
                let mut compatibility = Vec::new();
                for boundary in 0..7 {
                    let change = match (boundary, pattern) {
                        (0, "held" | "pulse" | "reverse_twice") | (2, "reverse_twice") => {
                            Some(!powered)
                        }
                        (1, "pulse" | "reverse_twice") => Some(powered),
                        _ => None,
                    };
                    if let Some(value) = change {
                        sim.set_powered(INPUT, value).unwrap();
                        engine.schedule_redstone_input(boundary * 2, INPUT, value);
                    }
                    compatibility.extend(drain(&mut sim));
                }
                engine.run_redstone_propagation().unwrap();
                let bounded_changes: Vec<_> = engine
                    .transition_trace()
                    .records
                    .iter()
                    .flat_map(|record| {
                        record
                            .changes
                            .iter()
                            .filter(|c| c.position == LAMP)
                            .map(|c| json!([record.time, c.before.powered, c.after.powered]))
                    })
                    .collect();
                rows.push(json!({"powered": powered, "lit": lit, "pattern": pattern, "initial": initial, "compatibility": compatibility, "bounded_changes": bounded_changes, "bounded_final": engine.world().get(LAMP).unwrap().powered, "bounded_complete": engine.trace_status().is_complete()}));
            }
        }
    }
    rows
}
