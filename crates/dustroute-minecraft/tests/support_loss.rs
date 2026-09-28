use dustroute_minecraft::piston_electrical::{HORIZONTAL, SIDES};
use dustroute_minecraft::time::piston_runtime::{
    ElectricalPistonRuntime, new_piston_runtime, schedule_device_use_after_tick,
    schedule_electrical_input_after_tick,
};
use dustroute_minecraft::time::runtime::DeliveryResult;
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonState, PistonVariant, Pos, Region, WireConnection, World,
};

const BASE: Pos = Pos::new(0, 4, 0);
fn region() -> Region {
    Region::new(Pos::new(-8, -4, -8), Pos::new(12, 12, 8))
}
fn component(kind: BlockKind, support: Facing) -> Block {
    let mut block = Block::new(kind);
    block.support_offset = Some(support.offset());
    block.powered = Some(kind == BlockKind::RedstoneTorch);
    match kind {
        BlockKind::Repeater | BlockKind::Comparator => {
            block.facing = Some(Facing::East);
            block.delay = Some(1);
            if kind == BlockKind::Comparator {
                block
                    .observed_properties
                    .insert("mode".into(), "compare".into());
            }
        }
        BlockKind::RedstoneWire => {
            block.power_level = Some(0);
            block.wire_connections = Some(HORIZONTAL.map(|d| (d, WireConnection::None)).into());
        }
        _ => {}
    }
    block
}
fn scene(kind: BlockKind, support: Facing) -> (ElectricalPistonRuntime, Pos) {
    let d = support.offset();
    let pos = BASE.offset(-d.x, -d.y, -d.z);
    let mut world = World::new();
    world.place(BlockKind::Solid, BASE);
    world.set(pos, component(kind, support));
    let mut run = new_piston_runtime(world, region(), Default::default()).unwrap();
    run.run_until_idle().unwrap();
    (run, pos)
}

#[test]
fn support_removal_breaks_all_admitted_attachment_mounts_and_resumes_mid_callback() {
    for kind in [
        BlockKind::RedstoneWire,
        BlockKind::RedstoneTorch,
        BlockKind::Lever,
        BlockKind::Button,
        BlockKind::Repeater,
        BlockKind::Comparator,
    ] {
        for support in SIDES {
            if matches!(
                kind,
                BlockKind::RedstoneWire | BlockKind::Repeater | BlockKind::Comparator
            ) && support != Facing::Down
            {
                continue;
            }
            if kind == BlockKind::RedstoneTorch && support == Facing::Up {
                continue;
            }
            let (mut run, pos) = scene(kind, support);
            run.remove_now(BASE).unwrap();
            let mut checkpoint = None;
            while run.microstep().unwrap().is_some() {
                if checkpoint.is_none()
                    && run.view().block(BASE).unwrap().kind == BlockKind::Air
                    && run.view().block(pos).unwrap().kind == kind
                {
                    checkpoint = Some(run.checkpoint());
                }
            }
            assert_eq!(
                run.view().block(pos).unwrap().kind,
                BlockKind::Air,
                "{kind:?} {support:?}"
            );
            let mut restored = ElectricalPistonRuntime::from_checkpoint(
                &checkpoint.expect("pending support callback"),
            )
            .unwrap();
            restored.run_until_idle().unwrap();
            assert_eq!(restored.state_key(), run.state_key());
            let root = run.behavior_state().unwrap();
            assert_eq!(
                ElectricalPistonRuntime::from_behavior_state(&root)
                    .unwrap()
                    .behavior_state()
                    .unwrap(),
                root
            );
        }
    }
}

#[test]
fn a_missing_support_is_still_rejected_as_a_fresh_initial_world() {
    let mut world = World::new();
    world.set(BASE, component(BlockKind::Button, Facing::Down));
    assert!(new_piston_runtime(world, region(), Default::default()).is_err());
}

#[test]
fn moving_a_support_breaks_a_pressed_button_and_its_queued_release_cannot_restore_it() {
    let mut world = World::new();
    world.place(BlockKind::Piston, BASE).facing = Some(Facing::East);
    for x in 1..=3 {
        world.place(BlockKind::Solid, BASE.offset(x, 0, 0));
    }
    let button = BASE.offset(3, 0, 1);
    world.set(button, component(BlockKind::Button, Facing::North));
    let input = BASE.offset(-1, 0, 0);
    world.place(BlockKind::Solid, BASE.offset(-2, 0, 0));
    world.set(input, component(BlockKind::Lever, Facing::West));
    let mut run = new_piston_runtime(world, region(), Default::default()).unwrap();
    schedule_device_use_after_tick(&mut run, 1, button).unwrap();
    schedule_electrical_input_after_tick(&mut run, 3, input, true).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(run.view().block(button).unwrap().kind, BlockKind::Air);
    assert_eq!(
        run.view().block(BASE).unwrap().piston_state,
        Some(PistonState::Extended)
    );
    assert_eq!(
        run.view().block(BASE.offset(4, 0, 0)).unwrap().kind,
        BlockKind::Solid
    );
    assert!(
        run.trace()
            .iter()
            .any(|r| r.invocation.time.game_tick == 21 && r.result == DeliveryResult::BlockReplaced)
    );
}

#[test]
fn automatic_comparator_removal_clears_the_hidden_output_register() {
    let pos = BASE.offset(0, 1, 0);
    let mut world = World::new();
    world.place(BlockKind::Solid, BASE);
    world.set(pos, component(BlockKind::Comparator, Facing::Down));
    world.place(BlockKind::RedstoneBlock, pos.offset(-1, 0, 0));
    let mut run = new_piston_runtime(world, region(), Default::default()).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(run.view().stored_output(pos).unwrap(), 15);
    run.remove_now(BASE).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(run.view().block(pos).unwrap().kind, BlockKind::Air);
    assert_eq!(run.view().stored_output(pos).unwrap(), 0);
}

#[test]
fn sticky_retraction_removes_dust_when_its_support_is_pulled_away() {
    let mut world = World::new();
    let body = world.place(BlockKind::Piston, BASE);
    body.facing = Some(Facing::East);
    body.piston_variant = Some(PistonVariant::Sticky);
    body.piston_state = Some(PistonState::Extended);
    world.set(
        BASE.offset(1, 0, 0),
        Block::piston_head(Facing::East, PistonVariant::Sticky, false),
    );
    world.place(BlockKind::Solid, BASE.offset(2, 0, 0));
    let dust = BASE.offset(2, 1, 0);
    world.set(dust, component(BlockKind::RedstoneWire, Facing::Down));
    world.place(BlockKind::Solid, BASE.offset(-2, 0, 0));
    let input = BASE.offset(-1, 0, 0);
    let mut lever = component(BlockKind::Lever, Facing::West);
    lever.powered = Some(true);
    world.set(input, lever);
    let mut run = new_piston_runtime(world, region(), Default::default()).unwrap();
    schedule_electrical_input_after_tick(&mut run, 1, input, false).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(run.view().block(dust).unwrap().kind, BlockKind::Air);
    assert_eq!(
        run.view().block(BASE.offset(2, 0, 0)).unwrap().kind,
        BlockKind::Air
    );
    assert_eq!(
        run.view().block(BASE.offset(1, 0, 0)).unwrap().kind,
        BlockKind::Solid
    );
}

#[test]
fn center_support_distinguishes_standing_torches_from_full_face_attachments() {
    let mut body = Block::new(BlockKind::Piston);
    body.facing = Some(Facing::East);
    body.piston_state = Some(PistonState::Extended);
    let torch = dustroute_minecraft::physical::of_kind(BlockKind::RedstoneTorch);
    let dust = dustroute_minecraft::physical::of_kind(BlockKind::RedstoneWire);
    assert!(torch.supports_attachment(&body, Facing::Down));
    assert!(!dust.supports_attachment(&body, Facing::Down));
    assert!(!torch.supports_attachment(&body, Facing::West));
    assert!(torch.supports_attachment(&body, Facing::East));
}
