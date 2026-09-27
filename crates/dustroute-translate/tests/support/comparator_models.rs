//! Captures of the pre-migration compatibility model, not live observations.
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, RotationY, World};
use dustroute_translate::sim::{RedstoneTickSimulator, SimulationEventKind};
use serde_json::{Value, json};

const MAIN: Pos = Pos::new(0, 1, 0);
const FOLLOWER: Pos = Pos::new(2, 1, 0);
const INPUTS: [Pos; 3] = [Pos::new(-1, 1, 0), Pos::new(0, 1, -1), Pos::new(0, 1, 1)];
const WAVE: [[u8; 3]; 10] = [
    [0, 0, 0],
    [10, 9, 2],
    [10, 10, 0],
    [10, 11, 1],
    [15, 0, 14],
    [2, 0, 5],
    [0, 0, 0],
    [7, 3, 6],
    [0, 0, 0],
    [0, 0, 0],
];

pub fn world(mode: Option<&str>, initial: u8, rotation: RotationY) -> World {
    let mut world = World::new();
    world.fill(
        Pos::new(-1, 0, -1),
        Pos::new(3, 0, 1),
        Block::new(BlockKind::Solid),
    );
    for pos in INPUTS {
        let input = world.place(BlockKind::PressurePlate, pos);
        input.powered = Some(false);
        input.power_level = Some(0);
        input.support_offset = Some(Pos::new(0, -1, 0));
    }
    for (pos, level) in [(MAIN, initial), (FOLLOWER, 0)] {
        let comparator = world.place(BlockKind::Comparator, pos);
        comparator.facing = Some(Facing::East);
        comparator.powered = Some(level != 0);
        comparator.power_level = Some(level);
    }
    if let Some(mode) = mode {
        world
            .get_mut(MAIN)
            .unwrap()
            .observed_properties
            .insert("mode".into(), mode.into());
    }
    for pos in [Pos::new(1, 1, 0), Pos::new(3, 1, 0)] {
        world.place(BlockKind::RedstoneWire, pos);
    }
    dustroute_translate::wire::update_wire_shapes(&mut world);
    let mut rotated = World::new();
    for (pos, block) in world.iter() {
        rotated.set(rotation.pos(*pos), rotation.block(block));
    }
    rotated
}

pub fn capture() -> Vec<Value> {
    let mut rows = Vec::new();
    for mode in [Some("compare"), Some("subtract"), None, Some("unknown")] {
        for initial in [0, 7, 15] {
            for rotation in [
                RotationY::R0,
                RotationY::R90,
                RotationY::R180,
                RotationY::R270,
            ] {
                let mut sim = RedstoneTickSimulator::new(world(mode, initial, rotation)).unwrap();
                let main = rotation.pos(MAIN);
                let follower = rotation.pos(FOLLOWER);
                let initial_snapshot = json!([
                    sim.snapshot().comparator_output[&main],
                    sim.snapshot().comparator_output[&follower],
                    sim.snapshot().strength(rotation.pos(Pos::new(1, 1, 0))),
                    sim.has_pending_events()
                ]);
                let mut samples = Vec::new();
                for levels in WAVE {
                    for (pos, level) in INPUTS.into_iter().zip(levels) {
                        sim.set_pressure_plate_level(rotation.pos(pos), level)
                            .unwrap();
                    }
                    let mut events = Vec::new();
                    loop {
                        let step = sim.step_event().unwrap();
                        if matches!(
                            step.event_kind,
                            SimulationEventKind::CompatibilityBoundary
                                | SimulationEventKind::ComparatorUpdate
                        ) {
                            events.push(json!([
                                step.time,
                                format!("{:?}", step.event_kind),
                                format!("{:?}", step.kind),
                                step.from.comparator_output[&main],
                                step.to.comparator_output[&main],
                                step.from.comparator_output[&follower],
                                step.to.comparator_output[&follower],
                                sim.has_pending_events()
                            ]));
                        }
                        if sim.pending_scheduler_events() == 0 {
                            break;
                        }
                    }
                    let end = sim.snapshot();
                    samples.push(json!({
                        "inputs": levels, "events": events,
                        "settled": [end.strength(rotation.pos(Pos::new(1, 1, 0))), end.strength(rotation.pos(Pos::new(3, 1, 0))), sim.has_pending_events()]
                    }));
                }
                rows.push(json!({"mode": mode, "initial": initial, "rotation": rotation, "initial_snapshot": initial_snapshot, "samples": samples}));
            }
        }
    }
    rows
}
