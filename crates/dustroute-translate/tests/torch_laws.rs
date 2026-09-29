use std::collections::BTreeMap;

use dustroute_library::builtin_laws::{TORCH_LAW_REVISION, builtin_laws, torch_law_revision};
use dustroute_minecraft::law::{ExecutableLaw, LawState};
use dustroute_translate::{
    sim::RedstoneTickSimulator, world::Block, world::BlockKind, world::Facing, world::Pos,
    world::World,
};
use serde::Deserialize;

fn input(law: &ExecutableLaw, state: &LawState, powered: bool) -> LawState {
    law.event(
        state,
        "neighbor_update",
        &BTreeMap::from([("powered".into(), u16::from(powered))]),
    )
    .unwrap()
}

#[test]
fn repeated_notifications_do_not_reset_or_shorten_the_pending_recovery() {
    let law = torch_law_revision()
        .law
        .as_ref()
        .unwrap()
        .compile()
        .unwrap();
    let mut state = law.initial_state();
    for tick in 0..32 {
        if tick % 2 == 0 {
            state = input(&law, &state, tick % 4 == 0);
        }
        state = law.advance(&state).unwrap();
    }
    assert_eq!(state.register("lit"), Some(0));
    assert_eq!(state.register("burnout_seen"), Some(1));
    assert_eq!(state.pending("scheduled_tick"), Some(158));
    for tick in 32..190 {
        let once = input(&law, &state, false);
        assert_eq!(input(&law, &once, false), once);
        state = law.advance(&once).unwrap();
        assert_eq!(state.register("lit"), Some(u16::from(tick == 189)));
    }
    assert_eq!(state.pending("scheduled_tick"), None);
}

#[test]
fn law_blueprints_cannot_be_silently_projected_into_legacy_geometry() {
    for projection in [
        dustroute_translate::blueprint::blueprint_cell,
        dustroute_translate::blueprint::blueprint_cell_for_routing,
    ] {
        assert!(projection(builtin_laws(), &torch_law_revision().id).is_err());
    }
}

#[derive(Deserialize)]
struct Observation {
    schema: String,
    minecraft_version: String,
    evidence: String,
    server_jar_sha1: String,
    complete: bool,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    orientation: String,
    duration_game_ticks: usize,
    inputs: Vec<Input>,
    samples: Vec<Sample>,
}
#[derive(Deserialize)]
struct Input {
    game_tick: usize,
    powered: bool,
}
#[derive(Deserialize)]
struct Sample {
    game_tick: usize,
    lit: bool,
}

#[test]
fn pinned_torch_law_matches_every_observed_game_tick() {
    let observation: Observation =
        serde_json::from_str(include_str!("fixtures/torch_burnout_1_21_11.json")).unwrap();
    assert_eq!(observation.schema, "dustroute.torch-observation.v1");
    assert_eq!(observation.minecraft_version, "1.21.11");
    assert_eq!(observation.evidence, "observed_server_block_state");
    assert_eq!(
        observation.server_jar_sha1,
        "64bb6d763bed0a9f1d632ec347938594144943ed"
    );
    assert!(observation.complete);
    assert_eq!(observation.cases.len(), 18);
    assert_eq!(torch_law_revision().id.as_str(), TORCH_LAW_REVISION);
    let law = torch_law_revision()
        .law
        .as_ref()
        .unwrap()
        .compile()
        .unwrap();
    for case in observation.cases {
        assert_eq!(case.samples.len(), case.duration_game_ticks + 1);
        let mut state = law.initial_state();
        for (time, sample) in case.samples.iter().enumerate() {
            assert_eq!(sample.game_tick, time);
            assert_eq!(
                state.register("lit"),
                Some(u16::from(sample.lit)),
                "{} {} game tick {time}",
                case.name,
                case.orientation
            );
            if let Some(change) = case.inputs.iter().find(|change| change.game_tick == time) {
                state = input(&law, &state, change.powered);
            }
            state = law.advance(&state).unwrap();
        }

        // The existing electrical simulator uses two-game-tick boundaries. Only
        // compare cases whose stimuli lie on those boundaries; odd-tick evidence
        // above validates the local law without claiming a finer global scheduler.
        if case.inputs.iter().any(|change| change.game_tick % 2 != 0) {
            continue;
        }
        let mut world = World::new();
        world.set(Pos::default(), Block::new(BlockKind::Solid));
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.powered = Some(false);
        lever.support_offset = Some(Pos::new(1, 0, 0));
        let torch = if case.orientation == "standing" {
            Pos::new(0, 1, 0)
        } else {
            Pos::new(1, 0, 0)
        };
        let block = world.place(BlockKind::RedstoneTorch, torch);
        block.support_offset = Some(if case.orientation == "standing" {
            Pos::new(0, -1, 0)
        } else {
            Pos::new(-1, 0, 0)
        });
        block.facing = (case.orientation != "standing").then_some(Facing::East);
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        for time in (0..=case.duration_game_ticks).step_by(2) {
            assert_eq!(
                simulator.snapshot().torch_lit[&torch],
                case.samples[time].lit,
                "simulator {} {} game tick {time}",
                case.name,
                case.orientation
            );
            if let Some(change) = case.inputs.iter().find(|change| change.game_tick == time) {
                simulator
                    .set_lever_state(Pos::new(-1, 0, 0), change.powered)
                    .unwrap();
            }
            simulator.advance_tick().unwrap();
        }
    }
}
