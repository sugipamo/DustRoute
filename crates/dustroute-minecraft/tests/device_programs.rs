use dustroute_minecraft::device_program::*;
use dustroute_minecraft::piston_electrical::{ElectricalWorld, SIDES};
use dustroute_minecraft::time::piston_runtime::*;
use dustroute_minecraft::{Block, BlockKind, Facing, PistonState, Pos, Region, World};

const BUTTON: Pos = Pos::new(0, 4, 0);
fn region() -> Region {
    Region::new(Pos::new(-8, -4, -8), Pos::new(8, 14, 8))
}
fn along(p: Pos, d: Facing) -> Pos {
    let d = d.offset();
    p.offset(d.x, d.y, d.z)
}
fn button(support: Facing) -> Block {
    let mut b = Block::new(BlockKind::Button);
    b.powered = Some(false);
    b.support_offset = Some(support.offset());
    b.facing = Some(if support.horizontal_offset().is_some() {
        support.opposite()
    } else {
        Facing::North
    });
    b.observed_name = Some("minecraft:stone_button".into());
    b.observed_properties = [
        ("powered", "false"),
        (
            "face",
            match support {
                Facing::Down => "floor",
                Facing::Up => "ceiling",
                _ => "wall",
            },
        ),
        (
            "facing",
            match b.facing.unwrap() {
                Facing::North => "north",
                Facing::South => "south",
                Facing::East => "east",
                Facing::West => "west",
                _ => unreachable!(),
            },
        ),
    ]
    .map(|(k, v)| (k.into(), v.into()))
    .into();
    b
}
fn scene(support: Facing) -> World {
    let mut world = World::new();
    world.place(BlockKind::Solid, along(BUTTON, support));
    world.set(BUTTON, button(support));
    world
}
fn transitions(rt: &ElectricalPistonRuntime, position: Pos) -> Vec<(u64, bool)> {
    rt.trace()
        .iter()
        .flat_map(|r| {
            r.delta
                .iter()
                .flat_map(|d| &d.changes)
                .filter(move |c| c.position == position && c.before.powered != c.after.powered)
                .map(move |c| (r.invocation.time.game_tick, c.after.powered.unwrap()))
        })
        .collect()
}

#[test]
fn definitions_reject_unbound_outputs_invalid_domains_and_prewrite_mutations() {
    let law = &dustroute_minecraft::device_callback_law::builtin_programs()[1];
    for mode in 0..5 {
        let mut definition = definitions()[1].clone();
        match mode {
            0 => definition
                .handlers
                .get_mut(&Callback::Tick)
                .unwrap()
                .effects
                .push(Effect::Schedule {
                    delay: Value::Output {
                        name: "missing".into(),
                    },
                    priority: Value::Constant { number: 3 },
                }),
            1 => {
                definition.handlers.get_mut(&Callback::Tick).unwrap().inputs[0].sample =
                    Query::Constant { value: 100 }
            }
            2 => definition
                .handlers
                .get_mut(&Callback::Shape)
                .unwrap()
                .effects
                .push(Effect::WriteState {
                    when: Value::Constant { number: 1 },
                    values: vec![(
                        Property::Bool(BoolProperty::Powered),
                        Value::Constant { number: 1 },
                    )],
                    notifications: WriteNotifications::Shapes,
                }),
            3 => {
                definition.handlers.remove(&Callback::Tick);
            }
            _ => definition.orientation = Orientation::None,
        }
        assert!(
            DeviceProgram::compile(definition, law).is_err(),
            "mode {mode}"
        );
    }
}

#[test]
fn button_uses_the_same_effect_program_and_does_not_extend_a_held_press() {
    let mut world = scene(Facing::Down);
    let lamp = BUTTON.offset(1, 0, 0);
    world.place(BlockKind::RedstoneLamp, lamp).powered = Some(false);
    let mut rt = new_piston_runtime(world, region(), Default::default()).unwrap();
    for tick in [1, 10, 22] {
        schedule_device_use_after_tick(&mut rt, tick, BUTTON).unwrap();
    }
    rt.run_until_idle().unwrap();
    assert_eq!(
        transitions(&rt, BUTTON),
        [(1, true), (21, false), (22, true), (42, false)]
    );
    // The queued first lamp-off tick sees the new press, so it cannot extinguish it.
    assert_eq!(transitions(&rt, lamp), [(1, true), (46, false)]);
    assert_eq!(
        rt.view().block(BUTTON).unwrap().observed_properties["powered"],
        "false"
    );
    assert_eq!(rt.pending_count(), 0);
}

#[test]
fn six_attachment_directions_have_weak_power_and_strong_power_into_support() {
    for support in SIDES {
        let mut rt = new_piston_runtime(scene(support), region(), Default::default()).unwrap();
        rt.run_until_idle().unwrap();
        rt.use_now(BUTTON).unwrap();
        rt.step().unwrap();
        assert_eq!(rt.view().block(BUTTON).unwrap().powered, Some(true));
        let q = ElectricalWorld::new(rt.view().world(), region()).unwrap();
        for query in SIDES {
            let emission = q.emission(BUTTON, query, true).unwrap();
            assert_eq!(emission.weak, 15);
            assert_eq!(
                emission.strong,
                if query == support.opposite() { 15 } else { 0 }
            );
        }
        rt.run_until_idle().unwrap();
        assert_eq!(transitions(&rt, BUTTON), [(0, true), (20, false)]);
    }
}

#[test]
fn button_can_drive_a_piston_through_its_conductor_support() {
    let mut world = scene(Facing::Down);
    let piston = BUTTON.offset(1, -1, 0);
    let b = world.place(BlockKind::Piston, piston);
    b.facing = Some(Facing::East);
    b.piston_state = Some(PistonState::Retracted);
    world.place(BlockKind::Solid, piston.offset(1, 0, 0));
    let mut rt = new_piston_runtime(world, region(), Default::default()).unwrap();
    schedule_device_use_after_tick(&mut rt, 1, BUTTON).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.view().block(piston).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    assert_eq!(
        rt.view().block(piston.offset(2, 0, 0)).unwrap().kind,
        BlockKind::Solid
    );
    assert!(rt.trace().iter().any(|r| r.delta.as_ref().is_some_and(|d| {
        d.changes
            .iter()
            .any(|c| c.position == piston && c.after.piston_state == Some(PistonState::Extended))
    })));
}

#[test]
fn every_button_program_boundary_and_pending_release_can_resume() {
    let mut rt = new_piston_runtime(scene(Facing::Down), region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    rt.use_now(BUTTON).unwrap();
    let mut checkpoints = Vec::new();
    let mut pending = None;
    while rt.microstep().unwrap().is_some() {
        checkpoints.push(rt.checkpoint());
        if rt.at_input_boundary() && rt.view().block(BUTTON).unwrap().powered == Some(true) {
            pending = Some(rt.behavior_state().unwrap());
        }
    }
    for checkpoint in checkpoints {
        let mut resumed = ElectricalPistonRuntime::from_checkpoint(&checkpoint).unwrap();
        resumed.run_until_idle().unwrap();
        assert_eq!(resumed.state_key(), rt.state_key());
    }
    let mut resumed = ElectricalPistonRuntime::from_behavior_state(&pending.unwrap()).unwrap();
    resumed.run_until_idle().unwrap();
    assert_eq!(
        resumed.behavior_state().unwrap(),
        rt.behavior_state().unwrap()
    );
}

#[test]
fn unsupported_material_incomplete_metadata_and_fresh_pressed_state_are_rejected() {
    for mode in 0..4 {
        let mut world = scene(Facing::Down);
        let block = world.get_mut(BUTTON).unwrap();
        match mode {
            0 => block.observed_name = Some("minecraft:oak_button".into()),
            1 => {
                block.powered = Some(true);
                block
                    .observed_properties
                    .insert("powered".into(), "true".into());
            }
            2 => {
                block.observed_properties.remove("face");
            }
            _ => block.support_offset = None,
        }
        assert!(new_piston_runtime(world, region(), Default::default()).is_err());
    }
    let mut rt = new_piston_runtime(scene(Facing::Down), region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    rt.use_now(BUTTON.offset(1, 0, 0)).unwrap();
    assert!(rt.step().is_err());
}

#[test]
fn command_installation_and_removal_use_device_metadata() {
    let mut world = scene(Facing::Down);
    world.set(BUTTON, Block::new(BlockKind::Air));
    let mut rt = new_piston_runtime(world, region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    rt.install_now(BUTTON, button(Facing::Down)).unwrap();
    rt.run_until_idle().unwrap();
    rt.use_now(BUTTON).unwrap();
    rt.run_until_idle().unwrap();
    rt.remove_now(BUTTON).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(rt.view().block(BUTTON).unwrap().kind, BlockKind::Air);
}
