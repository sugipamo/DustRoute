#[path = "support/repeater_models.rs"]
mod repeater_models;

#[test]
fn observation_mutations_retain_the_existing_per_position_queue_lifecycle() {
    use dustroute_minecraft::{Block, BlockKind, Facing, Pos, World};
    use dustroute_translate::sim::RedstoneTickSimulator;
    let input = Pos::new(-1, 1, 0);
    let main = Pos::new(0, 1, 0);
    let mut world = World::new();
    world.fill(
        Pos::new(-1, 0, 0),
        Pos::new(3, 0, 0),
        Block::new(BlockKind::Solid),
    );
    let source = world.place(BlockKind::Lever, input);
    source.powered = Some(false);
    source.support_offset = Some(Pos::new(0, -1, 0));
    for pos in [main, Pos::new(3, 1, 0)] {
        let repeater = world.place(BlockKind::Repeater, pos);
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(3);
        repeater.powered = Some(false);
    }
    let original = world.get(main).unwrap().clone();
    let mut sim = RedstoneTickSimulator::new(world).unwrap();
    sim.set_powered(input, true).unwrap();
    assert!(!sim.advance_tick().unwrap().repeater_powered[&main]);
    sim.set_block_state(main, Block::new(BlockKind::Air))
        .unwrap();
    assert!(
        !sim.advance_tick()
            .unwrap()
            .repeater_powered
            .contains_key(&main)
    );
    assert!(sim.has_pending_events());
    sim.set_block_state(main, original).unwrap();
    assert!(!sim.advance_tick().unwrap().repeater_powered[&main]);
    assert!(sim.advance_tick().unwrap().repeater_powered[&main]);
}

#[test]
fn repeater_model_traces_retain_their_distinct_pre_migration_contracts() {
    let expected: Vec<serde_json::Value> = include_str!("fixtures/repeater_models_v1.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let actual = repeater_models::capture();
    assert_eq!(actual.len(), 32);
    for (actual, expected) in actual.iter().zip(&expected) {
        assert_eq!(
            actual, expected,
            "delay={}, initial={}, pattern={}",
            actual["delay"], actual["initially_powered"], actual["pattern"]
        );
    }
    assert_eq!(actual.len(), expected.len());
}
