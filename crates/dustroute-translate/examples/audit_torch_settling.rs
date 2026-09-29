//! Read-only proof-premise investigation, not a repeated-use certificate.
//! Retains exact execution state; no summaries are accepted by adoption gates.
use std::collections::BTreeMap;

use dustroute_library::assembly::{Assembly, AssemblyRevision};
use dustroute_library::behavior_type::{BooleanRow, PhysicalBehaviorProfile, RepeatedSettling};
use dustroute_library::blueprint::*;
use dustroute_library::builtin_laws::{DUST_LAW_REVISION, TORCH_LAW_REVISION, builtin_laws};
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, ValidatedWorld, World};
use dustroute_translate::behavior_type::BehaviorModel;
use dustroute_translate::electrical::{
    DeviceOutputState, solve_instantaneous, torch_support_is_powered,
};
use dustroute_translate::physical_behavior::{
    PhysicalBehaviorModel, PhysicalBehaviorSelection, PhysicalBehaviorState, PhysicalOutput,
};
use serde::Deserialize;
use serde_json::{Value, json};

fn isolated(standing: bool) -> (World, Pos, Pos) {
    let mut world = World::new();
    world.set(Pos::default(), Block::new(BlockKind::Solid));
    let input = Pos::new(-1, 0, 0);
    let lever = world.place(BlockKind::Lever, input);
    lever.powered = Some(false);
    lever.support_offset = Some(Pos::new(1, 0, 0));
    let output = if standing {
        Pos::new(0, 1, 0)
    } else {
        Pos::new(1, 0, 0)
    };
    let torch = world.place(BlockKind::RedstoneTorch, output);
    torch.facing = Some(if standing { Facing::Up } else { Facing::East });
    torch.support_offset = Some(if standing {
        Pos::new(0, -1, 0)
    } else {
        Pos::new(-1, 0, 0)
    });
    (world, input, output)
}

fn model(world: &World, input: Pos, output: Pos) -> PhysicalBehaviorModel {
    let mut catalog = builtin_laws().clone();
    let type_id = TypeRevisionId::new("audit.not.v1").unwrap();
    let assembly_id = AssemblyRevisionId::new("audit.actual.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: type_id.clone(),
            name: "Repeated inversion under arbitrary input changes".into(),
            contract: TypeContract::RepeatedSettling {
                relation: RepeatedSettling {
                    inputs: vec!["input".into()],
                    outputs: vec!["output".into()],
                    rows: [false, true]
                        .into_iter()
                        .map(|value| BooleanRow {
                            inputs: vec![value],
                            outputs: vec![!value],
                        })
                        .collect(),
                },
            },
        })
        .unwrap();
    catalog
        .insert_assembly(AssemblyRevision {
            id: assembly_id.clone(),
            parents: vec![],
            assembly: Assembly {
                name: "Exact diagnostic realization".into(),
                instances: vec![],
                blocks: world
                    .iter()
                    .map(|(p, b)| PositionedBlock {
                        position: *p,
                        block: b.clone(),
                    })
                    .collect(),
                known_regions: world.positions().map(|p| Region::around(p, 2)).collect(),
                connections: vec![],
                boundaries: vec![],
            },
        })
        .unwrap();
    PhysicalBehaviorModel::from_fresh_assembly_with_profile(
        &catalog,
        PhysicalBehaviorSelection {
            assembly: assembly_id,
            behavior_type: type_id,
            dust_law: BlueprintRevisionId::new(DUST_LAW_REVISION).unwrap(),
            torch_law: BlueprintRevisionId::new(TORCH_LAW_REVISION).unwrap(),
            inputs: BTreeMap::from([("input".into(), input)]),
            outputs: BTreeMap::from([(
                "output".into(),
                PhysicalOutput::Signal { position: output },
            )]),
            max_electrical_iterations: 128,
        },
        PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
    )
    .unwrap()
}

/// Exhausts this static electrical domain, independently of history/reachability.
fn electrical_rows(world: &World, input: Option<Pos>, torch: Pos, output: Pos) -> Vec<Value> {
    ValidatedWorld::try_from(world.clone()).unwrap();
    let mut rows = vec![];
    for level in if input.is_some() {
        vec![false, true]
    } else {
        vec![false]
    } {
        for lit in [false, true] {
            let mut driven = world.clone();
            if let Some(input) = input {
                let lever = driven.get_mut(input).unwrap();
                lever.powered = Some(level);
                lever.power_level = Some(if level { 15 } else { 0 });
            }
            let devices = DeviceOutputState {
                torch_lit: BTreeMap::from([(torch, lit)]),
                ..Default::default()
            };
            let solved = solve_instantaneous(&driven, &devices, 128).unwrap();
            rows.push(json!({"input":input.map(|_|level), "torch_lit":lit,
                "support_powered":torch_support_is_powered(&driven, torch, &solved),
                "output":solved.signal(output)>0}));
        }
    }
    rows
}

/// Earliest step after which this exact held-input trajectory stays correct.
/// None denotes a concrete bad recurring cycle, not an abstract counterexample.
fn hold(
    model: &PhysicalBehaviorModel,
    initial: &PhysicalBehaviorState,
    input: bool,
    cache: &mut BTreeMap<PhysicalBehaviorState, Option<usize>>,
) -> Option<usize> {
    let start = model.with_inputs(initial, &[input]).unwrap();
    let mut state = start.clone();
    let mut path = vec![];
    let mut seen = BTreeMap::new();
    let mut successor;
    loop {
        if let Some(cached) = cache.get(&state) {
            successor = *cached;
            break;
        }
        if let Some(&begin) = seen.get(&state) {
            let cycle: &[(PhysicalBehaviorState, bool)] = &path[begin..];
            successor = cycle.iter().all(|(_, correct)| *correct).then_some(0);
            for (state, _) in cycle {
                cache.insert(state.clone(), successor);
            }
            path.truncate(begin);
            break;
        }
        assert!(
            path.len() < 512,
            "diagnostic continuation budget exhausted; no conclusion"
        );
        seen.insert(state.clone(), path.len());
        let correct = model.outputs(&state).unwrap() == [!input];
        path.push((state.clone(), correct));
        state = model.step(&state).unwrap();
    }
    for (state, correct) in path.into_iter().rev() {
        successor = successor.map(|delay| if correct && delay == 0 { 0 } else { delay + 1 });
        cache.insert(state, successor);
    }
    cache[&start]
}

#[derive(Deserialize)]
struct Capture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    orientation: String,
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

fn main() {
    let mut electrical = vec![];
    for standing in [false, true] {
        let (world, input, torch) = isolated(standing);
        let rows = electrical_rows(&world, Some(input), torch, torch);
        assert!(
            rows.iter()
                .all(|r| r["support_powered"] == r["input"] && r["output"] == r["torch_lit"])
        );
        electrical.push(
            json!({"layout":if standing {"isolated_standing"} else {"isolated_wall"}, "rows":rows}),
        );
    }
    for cell in [
        dustroute_translate::cells::not_top_cell(),
        dustroute_translate::cells::not_cell(),
    ] {
        let mut world = cell.world;
        let input = Pos::new(-1, 0, 0);
        let lever = world.place(BlockKind::Lever, input);
        lever.powered = Some(false);
        lever.support_offset = Some(Pos::new(1, 0, 0));
        let torch = *world
            .iter()
            .find(|(_, b)| b.kind == BlockKind::RedstoneTorch)
            .unwrap()
            .0;
        let rows = electrical_rows(&world, Some(input), torch, cell.outputs[0].pos);
        assert!(
            rows.iter()
                .all(|r| r["support_powered"] == r["input"] && r["output"] == r["torch_lit"])
        );
        electrical.push(json!({"layout":cell.name,"rows":rows}));
    }
    let (mut feedback, input, torch) = isolated(false);
    feedback.remove(input);
    feedback.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
    feedback.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    let rows = electrical_rows(&feedback, None, torch, Pos::new(0, 1, 0));
    assert!(rows.iter().all(|r| r["support_powered"] == r["torch_lit"]));
    electrical.push(json!({"layout":"four_block_feedback", "rows":rows}));

    let capture: Capture =
        serde_json::from_str(include_str!("../tests/fixtures/torch_burnout_1_21_11.json")).unwrap();
    let mut continuations = vec![];
    for case in capture.cases {
        let (world, input, output) = isolated(case.orientation == "standing");
        let model = model(&world, input, output);
        let mut state = model.initial_state().unwrap();
        let mut caches = [BTreeMap::new(), BTreeMap::new()];
        let mut maxima = [0, 0];
        let mut checked = 0;
        for (tick, sample) in case.samples.iter().enumerate() {
            assert_eq!(sample.game_tick, tick);
            assert_eq!(model.outputs(&state).unwrap(), [sample.lit]);
            // Fork every retained prefix without resetting its history/timers.
            for held in [false, true] {
                let delay = hold(&model, &state, held, &mut caches[usize::from(held)])
                    .expect("a sampled prefix has a concrete nonsettling continuation");
                maxima[usize::from(held)] = maxima[usize::from(held)].max(delay);
                checked += 1;
            }
            if let Some(change) = case.inputs.iter().find(|i| i.game_tick == tick) {
                state = model.with_inputs(&state, &[change.powered]).unwrap();
            }
            state = model.step(&state).unwrap();
        }
        continuations.push(json!({"case":case.name,"orientation":case.orientation,
            "observed_prefixes":case.samples.len(),"held_input_continuations":checked,
            "max_settling_steps_for_false_true":maxima,
            "exact_tail_states":caches.iter().map(|cache|cache.len()).sum::<usize>()}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope":"electrical_premises_and_exact_tails_from_retained_prefixes_only",
            "profile":PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
            "repeated_settling_verified":false,"adoption_authorized":false,"writes_minecraft":false,
            "dust_law":dustroute_library::builtin_laws::dust_law_revision(),
            "torch_law":dustroute_library::builtin_laws::torch_law_revision(),
            "electrical_rows":electrical,"continuations":continuations
        }))
        .unwrap()
    );
}
