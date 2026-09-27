use dustroute_library::builtin_laws::dust_law_revision;
use dustroute_minecraft::time::{PhysicsEngine, PhysicsEventKind};
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, World};

const SOURCE: Pos = Pos::new(0, 1, 0);
const FIRST: Pos = Pos::new(1, 1, 0);
const SECOND: Pos = Pos::new(2, 1, 0);

fn world(level: u8) -> World {
    let mut world = World::new();
    world.fill(
        Pos::new(-1, 0, -1),
        Pos::new(3, 0, 1),
        Block::new(BlockKind::Solid),
    );
    let b = world.place(BlockKind::Comparator, SOURCE);
    b.facing = Some(Facing::East);
    b.power_level = Some(level);
    world.place(BlockKind::RedstoneWire, FIRST);
    world.place(BlockKind::RedstoneWire, SECOND);
    world
}

fn engine(world: World) -> PhysicsEngine {
    PhysicsEngine::new_diagnostic(world, 512)
        .with_piston_planning_region(Region::new(Pos::new(-3, -1, -3), Pos::new(5, 3, 3)))
}

#[test]
fn pinned_revision_and_all_legal_source_levels_keep_their_existing_results() {
    assert_eq!(
        dust_law_revision().law.as_ref().unwrap(),
        dustroute_minecraft::dust_law::builtin_program()
    );
    for level in 0u8..=15 {
        let mut engine = engine(world(level));
        engine.schedule_external(
            0,
            FIRST,
            PhysicsEventKind::NeighborUpdate { source: SOURCE },
        );
        engine.run_redstone_propagation().unwrap();
        assert_eq!(
            engine.world().get(FIRST).unwrap().power_level.unwrap_or(0),
            level
        );
        assert_eq!(
            engine.world().get(SECOND).unwrap().power_level.unwrap_or(0),
            level.saturating_sub(1)
        );
        assert_eq!(engine.pending_event_count(), 0);
        assert!(engine.trace_status().is_complete());
    }
}

#[test]
fn invalid_typed_or_observed_levels_reject_before_consuming_the_event_or_changing_raw_state() {
    for level in [16, 255] {
        for (position, raw) in [
            (SOURCE, false),
            (FIRST, false),
            (FIRST, true),
            (SECOND, false),
            (SECOND, true),
        ] {
            let mut world = world(15);
            let b = world.get_mut(position).unwrap();
            if raw {
                b.observed_name = Some("minecraft:redstone_wire".into());
                b.observed_properties
                    .insert("power".into(), level.to_string());
                b.wire_connections = Some(
                    [Facing::East, Facing::West]
                        .into_iter()
                        .map(|f| (f, dustroute_minecraft::WireConnection::Side))
                        .collect(),
                );
            } else {
                b.power_level = Some(level);
            }
            let mut engine = engine(world.clone());
            let id = engine.schedule_external(
                0,
                FIRST,
                PhysicsEventKind::NeighborUpdate { source: SOURCE },
            );
            let error = engine.run_redstone_propagation().unwrap_err();
            assert!(error.to_string().contains("outside 0..=15"), "{error}");
            assert_eq!(engine.world(), &world);
            assert_eq!(engine.pending_event_count(), 1);
            assert_eq!(engine.checkpoint().pending_events().next().unwrap().id, id);
            assert!(engine.transition_trace().records.is_empty());
            assert!(engine.event_trace().records.is_empty());
            assert!(engine.trace_status().is_failed());
        }
    }
}

#[test]
fn invalid_neighbor_rejects_an_external_input_before_it_commits() {
    let mut world = world(0);
    let lever = Pos::new(1, 1, -1);
    world.place(BlockKind::Lever, lever).powered = Some(false);
    world.get_mut(SECOND).unwrap().power_level = Some(16);
    let mut engine = engine(world.clone());
    engine.schedule_redstone_input(0, lever, true);
    assert!(engine.run_redstone_propagation().is_err());
    assert_eq!(engine.world(), &world);
    assert_eq!(engine.pending_event_count(), 1);
    assert!(engine.transition_trace().records.is_empty());
}

#[test]
fn invalid_sources_reject_at_receiver_evaluation_without_mutating_the_receiver() {
    for level in [16, 255] {
        for kind in [BlockKind::Repeater, BlockKind::RedstoneLamp] {
            let mut world = world(level);
            let receiver = world.place(kind, FIRST);
            receiver.facing = Some(Facing::East);
            receiver.delay = Some(1);
            receiver.powered = Some(false);
            let mut engine = engine(world.clone());
            engine.schedule_external(
                0,
                FIRST,
                PhysicsEventKind::NeighborUpdate { source: SOURCE },
            );
            let error = engine.run_redstone_propagation().unwrap_err();
            assert!(error.to_string().contains("outside 0..=15"), "{error}");
            assert_eq!(engine.world(), &world);
            assert_eq!(engine.pending_event_count(), 1);
            assert!(engine.event_trace().records.is_empty());
        }
    }
}
