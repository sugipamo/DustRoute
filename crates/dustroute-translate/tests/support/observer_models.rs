//! Frozen compatibility-model capture; not a live Minecraft observation.
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, World};
use dustroute_translate::sim::{RedstoneTickSimulator, SimulationEventKind};
use serde_json::{Value, json};

pub fn world(facing: Facing, powered: bool) -> (World, Pos, Pos) {
    let observer = Pos::new(0, 2, 0);
    let d = facing.offset();
    let target = observer.offset(-d.x, -d.y, -d.z);
    let mut world = World::new();
    let block = world.place(BlockKind::Observer, observer);
    block.facing = Some(facing);
    block.powered = Some(powered);
    world.set(target, Block::new(BlockKind::Solid));
    (world, observer, target)
}

pub fn changed(value: &str) -> Block {
    let mut block = Block::new(BlockKind::Solid);
    block.observed_name = Some("minecraft:stone".into());
    // Preserve exact Block-record equality as the model's native input fact.
    block
        .observed_properties
        .insert("capture_marker".into(), value.into());
    block
}

pub fn drain(sim: &mut RedstoneTickSimulator, observer: Pos) -> Vec<Value> {
    let mut events = Vec::new();
    loop {
        let step = sim.step_event().unwrap();
        if matches!(
            step.event_kind,
            SimulationEventKind::CompatibilityBoundary
                | SimulationEventKind::ObserverPulseEnd
                | SimulationEventKind::ObserverPulseStart
        ) {
            events.push(json!([
                step.time,
                format!("{:?}", step.event_kind),
                format!("{:?}", step.kind),
                step.from.observer_powered.get(&observer),
                step.to.observer_powered.get(&observer),
                sim.has_pending_events()
            ]));
        }
        if sim.pending_scheduler_events() == 0 {
            break;
        }
    }
    events
}

pub fn capture() -> Vec<Value> {
    let mut rows = Vec::new();
    for facing in [
        Facing::North,
        Facing::East,
        Facing::South,
        Facing::West,
        Facing::Up,
        Facing::Down,
    ] {
        for powered in [false, true] {
            for pattern in ["idle", "single", "repeat", "coalesced"] {
                let (world, observer, target) = world(facing, powered);
                let mut sim = RedstoneTickSimulator::new(world).unwrap();
                let initial = json!([
                    sim.snapshot().observer_powered[&observer],
                    sim.has_pending_events()
                ]);
                let mut samples = Vec::new();
                for tick in 0..6 {
                    match (tick, pattern) {
                        (0, "single" | "repeat" | "coalesced") => {
                            sim.set_block_state(target, changed("a")).unwrap();
                        }
                        (1 | 2, "repeat") => {
                            sim.set_block_state(target, changed(&tick.to_string()))
                                .unwrap();
                        }
                        _ => {}
                    }
                    if tick == 0 && pattern == "coalesced" {
                        sim.set_block_state(target, changed("b")).unwrap();
                        sim.set_block_state(target, changed("a")).unwrap();
                    }
                    let pending = sim.has_pending_events();
                    let events = drain(&mut sim, observer);
                    samples.push(json!([
                        pending,
                        events,
                        sim.snapshot().observer_powered[&observer],
                        sim.has_pending_events()
                    ]));
                }
                rows.push(json!({"facing": facing, "initially_powered": powered, "pattern": pattern, "initial": initial, "samples": samples}));
            }
        }
    }
    rows
}
