#[path = "support/archive_fixture.rs"]
mod archive_fixture;
use archive_fixture::FixtureJson;
use std::collections::BTreeMap;

use dustroute_library::assembly::{Assembly, AssemblyRevision};
use dustroute_library::behavior_type::{BooleanRow, PhysicalBehaviorProfile, RepeatedSettling};
use dustroute_library::blueprint::*;
use dustroute_library::builtin_laws::{
    DUST_LAW_REVISION, TORCH_LAW_REVISION, builtin_laws, dust_law_revision, torch_law_revision,
};
use dustroute_minecraft::law::{Expr, Instruction};
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, World};
use dustroute_translate::abstract_behavior::AbstractBehaviorModel;
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel, WitnessAction};
use dustroute_translate::dust_law::DustLaw;
use dustroute_translate::physical_behavior::*;
use dustroute_translate::promotion::CheckStatus;

fn catalog(world: &World, relation: RepeatedSettling) -> BlueprintCatalog {
    let mut catalog = builtin_laws().clone();
    catalog
        .insert_type(TypeRevision {
            id: TypeRevisionId::new("test.relation.v1").unwrap(),
            name: "No gate classification".into(),
            contract: TypeContract::RepeatedSettling { relation },
        })
        .unwrap();
    catalog
        .insert_assembly(AssemblyRevision {
            id: AssemblyRevisionId::new("test.actual.v1").unwrap(),
            parents: vec![],
            assembly: Assembly {
                name: "Actual physical candidate".into(),
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
    catalog
}

fn relation(invert: bool) -> RepeatedSettling {
    RepeatedSettling {
        inputs: vec!["a".into()],
        outputs: vec!["out".into()],
        rows: [false, true]
            .into_iter()
            .map(|b| BooleanRow {
                inputs: vec![b],
                outputs: vec![b ^ invert],
            })
            .collect(),
    }
}

fn selection(input: Pos, output: Pos) -> PhysicalBehaviorSelection {
    PhysicalBehaviorSelection {
        assembly: AssemblyRevisionId::new("test.actual.v1").unwrap(),
        behavior_type: TypeRevisionId::new("test.relation.v1").unwrap(),
        dust_law: BlueprintRevisionId::new(DUST_LAW_REVISION).unwrap(),
        torch_law: BlueprintRevisionId::new(TORCH_LAW_REVISION).unwrap(),
        inputs: BTreeMap::from([("a".into(), input)]),
        outputs: BTreeMap::from([("out".into(), PhysicalOutput::Signal { position: output })]),
        max_electrical_iterations: 128,
    }
}

#[test]
fn historical_v1_execution_remains_reproducible_but_does_not_authorize_current_placement() {
    let mut world = World::new();
    let input = Pos::new(-1, 1, 0);
    let lower = Pos::new(0, 1, 0);
    let upper = Pos::new(1, 2, 0);
    for pos in [Pos::new(-1, 0, 0), Pos::default(), Pos::new(1, 1, 0)] {
        world.set(pos, Block::new(BlockKind::Solid));
    }
    world.place(BlockKind::Lever, input).powered = Some(false);
    world.place(BlockKind::RedstoneWire, lower);
    world.place(BlockKind::RedstoneWire, upper);
    dustroute_translate::wire::update_wire_shapes(&mut world);
    world.set(lower.offset(0, 1, 0), Block::new(BlockKind::Solid));
    assert!(dustroute_minecraft::ValidatedWorld::try_from(world.clone()).is_err());
    let catalog = catalog(&world, relation(false));
    let selected = selection(input, upper);
    for profile in [
        PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
        PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
    ] {
        let model = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
            &catalog,
            selected.clone(),
            profile,
        )
        .unwrap();
        let report = model.verify(BehaviorBudget::default());
        assert_eq!(
            report.placement_validation_profile,
            dustroute_minecraft::HistoricalPlacementV1::PROFILE
        );
        assert_eq!(report.behavior.status, CheckStatus::Passed);
        let state = model
            .with_inputs(&model.initial_state().unwrap(), &[true])
            .unwrap();
        assert_eq!(model.outputs(&state).unwrap(), vec![true]);
    }
    let assembly = &catalog.assembly(&selected.assembly).unwrap().assembly;
    assert_eq!(
        dustroute_translate::promotion::review_assembly(&catalog, assembly)
            .unwrap()
            .status(),
        CheckStatus::Failed
    );
    assert!(dustroute_translate::assembly::validate_assembly(&catalog, assembly).is_err());
}

fn wire_line(world: &mut World, z: i32, length: i32) {
    for x in 0..=length {
        world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
        let block = world.place(
            if x == 0 {
                BlockKind::Lever
            } else {
                BlockKind::RedstoneWire
            },
            Pos::new(x, 1, z),
        );
        if x == 0 {
            block.powered = Some(false);
        }
    }
}

fn torch_world(standing: bool) -> (World, Pos, Pos) {
    let mut world = World::new();
    world.set(Pos::default(), Block::new(BlockKind::Solid));
    let input = Pos::new(-1, 0, 0);
    let block = world.place(BlockKind::Lever, input);
    block.powered = Some(false);
    block.support_offset = Some(Pos::new(1, 0, 0));
    let output = if standing {
        Pos::new(0, 1, 0)
    } else {
        Pos::new(1, 0, 0)
    };
    let block = world.place(BlockKind::RedstoneTorch, output);
    block.facing = Some(if standing { Facing::Up } else { Facing::East });
    block.support_offset = Some(if standing {
        Pos::new(0, -1, 0)
    } else {
        Pos::new(-1, 0, 0)
    });
    (world, input, output)
}

#[test]
fn dust_domain_matches_the_previous_model_and_rejects_stateful_rules() {
    let law = DustLaw::from_revision(dust_law_revision()).unwrap();
    for direct in 0u8..=15 {
        for neighbor in 0u8..=15 {
            // Differential oracle for the formula replaced in electrical.rs.
            assert_eq!(
                law.strength(direct, neighbor),
                direct.max(neighbor.saturating_sub(1))
            );
        }
    }
    let mut invalid = dust_law_revision().clone();
    invalid
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::ScheduleIfAbsent {
            event: "evaluate".into(),
            after: 1,
        });
    assert!(DustLaw::from_revision(&invalid).is_err());
}

#[test]
fn physical_wires_attenuate_and_multiple_ports_preserve_the_type_order() {
    let mut world = World::new();
    wire_line(&mut world, 0, 17);
    wire_line(&mut world, 4, 3);
    let relation = RepeatedSettling {
        inputs: vec!["right".into(), "left".into()],
        outputs: vec!["second".into(), "first".into()],
        rows: (0..4)
            .map(|bits| {
                let inputs = vec![bits & 1 != 0, bits & 2 != 0];
                BooleanRow {
                    outputs: vec![inputs[1], inputs[0]],
                    inputs,
                }
            })
            .collect(),
    };
    let catalog = catalog(&world, relation);
    let mut selected = selection(Pos::new(0, 1, 0), Pos::new(3, 1, 0));
    selected.inputs = BTreeMap::from([
        ("left".into(), Pos::new(0, 1, 0)),
        ("right".into(), Pos::new(0, 1, 4)),
    ]);
    selected.outputs = BTreeMap::from([
        (
            "second".into(),
            PhysicalOutput::Signal {
                position: Pos::new(3, 1, 0),
            },
        ),
        (
            "first".into(),
            PhysicalOutput::Signal {
                position: Pos::new(3, 1, 4),
            },
        ),
    ]);
    let model = PhysicalBehaviorModel::from_fresh_assembly(&catalog, selected.clone()).unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.behavior.status, CheckStatus::Passed, "{report:?}");
    assert_eq!(report.behavior.reachable_states, 4);
    let abstract_model = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
        &catalog,
        selected,
        PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
    )
    .unwrap();
    let abstract_report = abstract_model.verify_history_abstraction(BehaviorBudget::default());
    assert_eq!(
        abstract_report.behavior.status,
        CheckStatus::Passed,
        "{abstract_report:?}"
    );
    assert_eq!(abstract_report.behavior.abstract_states, 4);
    let initial = model.initial_state().unwrap();
    let on = model.with_inputs(&initial, &[false, true]).unwrap();
    assert_eq!(model.outputs(&on).unwrap(), vec![true, false]);
    let electrical = model.electrical_state(&on).unwrap();
    for x in 1..=17 {
        assert_eq!(electrical.signal(Pos::new(x, 1, 0)), (16 - x).max(0) as u8);
    }
    let off = model.with_inputs(&on, &[false, false]).unwrap();
    assert_eq!(off, initial);
    assert_eq!(model.outputs(&off).unwrap(), vec![false, false]);
}

#[test]
fn changing_a_law_revision_changes_execution_without_rewriting_existing_pins() {
    let mut world = World::new();
    wire_line(&mut world, 0, 3);
    let mut catalog = catalog(&world, relation(false));
    let selected = selection(Pos::new(0, 1, 0), Pos::new(3, 1, 0));
    let old = PhysicalBehaviorModel::from_fresh_assembly(&catalog, selected.clone()).unwrap();
    let original = dust_law_revision().clone();
    let mut changed = original.clone();
    changed.id = BlueprintRevisionId::new("test.dust-blocked.v1").unwrap();
    changed.parents = vec![original.id.clone()];
    changed.law.as_mut().unwrap().handlers.insert(
        "evaluate".into(),
        vec![Instruction::Set {
            register: "strength".into(),
            value: Expr::Constant { value: 0 },
        }],
    );
    catalog.insert_revision(changed.clone()).unwrap();
    let mut candidate = selected;
    candidate.dust_law = changed.id.clone();
    let loaded = archive_fixture::catalog(&catalog.fixture_json().unwrap()).unwrap();
    let new = PhysicalBehaviorModel::from_fresh_assembly(&loaded, candidate).unwrap();
    assert_eq!(
        old.verify(BehaviorBudget::default()).behavior.status,
        CheckStatus::Passed
    );
    let report = new.verify(BehaviorBudget::default());
    assert_eq!(report.dust_revision, changed);
    assert_eq!(report.torch_revision, *torch_law_revision());
    assert_eq!(report.behavior.status, CheckStatus::Failed);
    let witness = report.behavior.counterexample.unwrap();
    let mut state = new.initial_state().unwrap();
    for action in witness.prefix {
        state = match action {
            WitnessAction::SetInputs { inputs } => new.with_inputs(&state, &inputs).unwrap(),
            WitnessAction::Advance => new.step(&state).unwrap(),
        };
    }
    state = new.with_inputs(&state, &witness.held_inputs).unwrap();
    state = new.step(&state).unwrap();
    assert_ne!(new.outputs(&state).unwrap(), witness.expected_outputs);
    assert_eq!(catalog.revision(&original.id), Some(&original));
    assert!(new.step(&old.initial_state().unwrap()).is_err());
}

#[test]
fn physical_torch_execution_uses_the_selected_program_and_reports_a_wrong_rule() {
    let (world, input, output) = torch_world(false);
    let mut catalog = catalog(&world, relation(true));
    let mut changed = torch_law_revision().clone();
    changed.id = BlueprintRevisionId::new("test.torch-stuck-on.v1").unwrap();
    changed.parents = vec![torch_law_revision().id.clone()];
    changed.law.as_mut().unwrap().handlers.insert(
        "scheduled_tick".into(),
        vec![Instruction::Set {
            register: "lit".into(),
            value: Expr::Constant { value: 1 },
        }],
    );
    catalog.insert_revision(changed.clone()).unwrap();
    let mut selected = selection(input, output);
    selected.torch_law = changed.id;
    let model = PhysicalBehaviorModel::from_fresh_assembly(&catalog, selected).unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.behavior.status, CheckStatus::Failed, "{report:?}");
    assert!(report.behavior.counterexample.is_some());
}

#[test]
fn composed_not_with_dust_retains_burnout_state_and_matches_the_compatibility_simulator() {
    for cell in [
        dustroute_translate::cells::not_top_cell(),
        dustroute_translate::cells::not_cell(),
    ] {
        let mut world = cell.world;
        let input = Pos::new(-1, 0, 0);
        let block = world.place(BlockKind::Lever, input);
        block.powered = Some(false);
        block.support_offset = Some(Pos::new(1, 0, 0));
        let output = cell.outputs[0].pos;
        let torch = world
            .iter()
            .find(|(_, b)| b.kind == BlockKind::RedstoneTorch)
            .map(|(p, _)| *p)
            .unwrap();
        let model = PhysicalBehaviorModel::from_fresh_assembly(
            &catalog(&world, relation(true)),
            selection(input, output),
        )
        .unwrap();
        let mut state = model.initial_state().unwrap();
        let mut old = dustroute_translate::sim::RedstoneTickSimulator::new(world).unwrap();
        for tick in 0..=220 {
            if tick % 2 == 0 {
                assert_eq!(
                    model.electrical_state(&state).unwrap().signal(output),
                    old.snapshot().strength(output),
                    "{} game tick {tick}",
                    cell.name
                );
            }
            if tick == 32 {
                assert_eq!(
                    model
                        .torch_state(&state, torch)
                        .unwrap()
                        .pending("scheduled_tick"),
                    Some(158)
                );
                assert_eq!(model.outputs(&state).unwrap(), vec![false]);
            }
            if tick == 190 {
                assert_eq!(model.outputs(&state).unwrap(), vec![true]);
            }
            let change = if tick < 32 && tick % 2 == 0 {
                Some(tick % 4 == 0)
            } else {
                match tick {
                    70 => Some(true),
                    72 => Some(false),
                    _ => None,
                }
            };
            if let Some(value) = change {
                state = model.with_inputs(&state, &[value]).unwrap();
                old.set_lever_state(input, value).unwrap();
            }
            state = model.step(&state).unwrap();
            if tick % 2 == 1 {
                old.advance_tick().unwrap();
            }
        }
    }
}

#[test]
fn saved_wire_shapes_are_not_repaired_into_a_passing_circuit() {
    let mut world = World::new();
    wire_line(&mut world, 0, 3);
    world.get_mut(Pos::new(2, 1, 0)).unwrap().wire_connections = Some(
        [Facing::East, Facing::West, Facing::North, Facing::South]
            .into_iter()
            .map(|d| (d, dustroute_minecraft::WireConnection::None))
            .collect(),
    );
    let model = PhysicalBehaviorModel::from_fresh_assembly(
        &catalog(&world, relation(false)),
        selection(Pos::new(0, 1, 0), Pos::new(3, 1, 0)),
    )
    .unwrap();
    assert_eq!(
        model.verify(BehaviorBudget::default()).behavior.status,
        CheckStatus::Failed
    );
}

#[test]
fn unknown_neighborhood_unsupported_devices_and_invalid_placement_cannot_pass() {
    let mut world = World::new();
    wire_line(&mut world, 0, 3);
    let selected = selection(Pos::new(0, 1, 0), Pos::new(3, 1, 0));
    let mut unknown = catalog(&world, relation(false));
    let mut partial = unknown.assembly(&selected.assembly).unwrap().clone();
    partial.id = AssemblyRevisionId::new("test.unknown.v1").unwrap();
    partial.assembly.known_regions.clear();
    unknown.insert_assembly(partial.clone()).unwrap();
    let mut choice = selected.clone();
    choice.assembly = partial.id;
    assert!(
        PhysicalBehaviorModel::from_fresh_assembly(&unknown, choice)
            .unwrap_err()
            .contains("unknown physical neighborhood")
    );
    world.place(BlockKind::Observer, Pos::new(3, 2, 0));
    assert!(
        PhysicalBehaviorModel::from_fresh_assembly(
            &catalog(&world, relation(false)),
            selected.clone()
        )
        .unwrap_err()
        .contains("unsupported physical behavior")
    );
    world.remove(Pos::new(3, 2, 0));
    world.remove(Pos::new(1, 0, 0));
    assert!(
        PhysicalBehaviorModel::from_fresh_assembly(&catalog(&world, relation(false)), selected)
            .unwrap_err()
            .contains("placement validation")
    );
}

#[test]
fn electrical_or_reachability_budget_exhaustion_is_undetermined() {
    let mut world = World::new();
    wire_line(&mut world, 0, 3);
    let catalog = catalog(&world, relation(false));
    let mut selected = selection(Pos::new(0, 1, 0), Pos::new(3, 1, 0));
    selected.max_electrical_iterations = 1;
    let model = PhysicalBehaviorModel::from_fresh_assembly(&catalog, selected).unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.behavior.status, CheckStatus::Undetermined);
    assert!(report.behavior.detail.contains("did not converge"));
    let (world, input, output) = torch_world(true);
    let model = PhysicalBehaviorModel::from_fresh_assembly(
        &self::catalog(&world, relation(true)),
        selection(input, output),
    )
    .unwrap();
    let report = model.verify(BehaviorBudget {
        max_states: 256,
        ..BehaviorBudget::default()
    });
    assert_eq!(report.behavior.status, CheckStatus::Undetermined);
    assert_eq!(report.behavior.reachable_states, 256);
    assert!(report.behavior.counterexample.is_none());
}

#[test]
fn physical_torch_binding_matches_every_observed_tick_including_input_changes_during_recovery() {
    #[derive(serde::Deserialize)]
    struct Observation {
        cases: Vec<Case>,
    }
    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        orientation: String,
        inputs: Vec<Input>,
        samples: Vec<Sample>,
    }
    #[derive(serde::Deserialize)]
    struct Input {
        game_tick: usize,
        powered: bool,
    }
    #[derive(serde::Deserialize)]
    struct Sample {
        game_tick: usize,
        lit: bool,
    }
    let observation: Observation =
        serde_json::from_str(include_str!("fixtures/torch_burnout_1_21_11.json")).unwrap();
    for case in observation.cases {
        let (world, input, output) = torch_world(case.orientation == "standing");
        let catalog = catalog(&world, relation(true));
        for profile in [
            PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
            PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        ] {
            let model = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
                &catalog,
                selection(input, output),
                profile,
            )
            .unwrap();
            let mut state = model.initial_state().unwrap();
            for sample in &case.samples {
                assert_eq!(
                    model.outputs(&state).unwrap(),
                    vec![sample.lit],
                    "{} {} at {} under {profile:?}",
                    case.name,
                    case.orientation,
                    sample.game_tick
                );
                if profile == PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1 {
                    let abstract_model = model.history_abstraction().unwrap();
                    let projected = abstract_model.project_state(&state).unwrap();
                    assert_eq!(
                        abstract_model.outputs(&projected).unwrap(),
                        model.outputs(&state).unwrap()
                    );
                    for held in [false, true] {
                        let driven = model.with_inputs(&state, &[held]).unwrap();
                        let abstract_driven =
                            abstract_model.with_inputs(&projected, &[held]).unwrap();
                        assert_eq!(
                            abstract_driven,
                            abstract_model.project_state(&driven).unwrap()
                        );
                        let next = model.step(&driven).unwrap();
                        assert!(
                            abstract_model
                                .successors(&abstract_driven, 8)
                                .unwrap()
                                .contains(&abstract_model.project_state(&next).unwrap())
                        );
                    }
                }
                if let Some(change) = case.inputs.iter().find(|i| i.game_tick == sample.game_tick) {
                    state = model.with_inputs(&state, &[change.powered]).unwrap();
                    assert_eq!(state, model.with_inputs(&state, &[change.powered]).unwrap());
                }
                state = model.step(&state).unwrap();
            }
        }
    }
}

#[test]
#[ignore = "manual default-budget scalability measurement; does not certify NOT"]
fn measure_full_physical_not_reachability() {
    let (world, input, output) = torch_world(true);
    let model = PhysicalBehaviorModel::from_fresh_assembly(
        &catalog(&world, relation(true)),
        selection(input, output),
    )
    .unwrap();
    let start = std::time::Instant::now();
    let report = model.verify(BehaviorBudget::default());
    eprintln!("{:?}; elapsed {:?}", report.behavior, start.elapsed());
    assert_ne!(report.behavior.status, CheckStatus::Failed, "{report:?}");
}

#[test]
fn history_abstraction_closes_arbitrary_input_not_graphs_without_modifying_execution() {
    let mut layouts = vec![torch_world(false), torch_world(true)];
    for cell in [
        dustroute_translate::cells::not_top_cell(),
        dustroute_translate::cells::not_cell(),
    ] {
        let mut world = cell.world;
        let input = Pos::new(-1, 0, 0);
        let lever = world.place(BlockKind::Lever, input);
        lever.powered = Some(false);
        lever.support_offset = Some(Pos::new(1, 0, 0));
        layouts.push((world, input, cell.outputs[0].pos));
    }
    for (world, input, output) in layouts {
        let catalog = catalog(&world, relation(true));
        let unchanged = catalog.fixture_json().unwrap();
        let model = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
            &catalog,
            selection(input, output),
            PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        )
        .unwrap();
        let initial = model.initial_state().unwrap();
        let report = model.verify_history_abstraction(BehaviorBudget::default());
        eprintln!("{:#?}", report.behavior);
        assert_eq!(report.behavior.status, CheckStatus::Passed, "{report:?}");
        assert_eq!(model.initial_state().unwrap(), initial);
        assert_eq!(catalog.fixture_json().unwrap(), unchanged);
        let incomplete = model.verify_history_abstraction(BehaviorBudget {
            max_states: 1,
            ..BehaviorBudget::default()
        });
        assert_eq!(incomplete.behavior.status, CheckStatus::Undetermined);
        let legacy =
            PhysicalBehaviorModel::from_fresh_assembly(&catalog, selection(input, output)).unwrap();
        assert!(legacy.history_abstraction().is_err());
        assert_eq!(
            legacy
                .verify_history_abstraction(BehaviorBudget::default())
                .behavior
                .status,
            CheckStatus::Undetermined
        );
        assert!(
            model
                .history_abstraction()
                .unwrap()
                .project_state(&legacy.initial_state().unwrap())
                .is_err()
        );
    }
}

#[test]
fn abstract_proof_executes_changed_laws_and_cannot_hide_feedback_or_possible_errors() {
    let (world, input, output) = torch_world(false);
    let original = catalog(&world, relation(true));
    for (name, expected) in [
        ("short_history", CheckStatus::Passed),
        ("overflow", CheckStatus::Undetermined),
        ("reads_diagnostic", CheckStatus::Undetermined),
    ] {
        let mut catalog = original.clone();
        let mut law = torch_law_revision().clone();
        law.id = BlueprintRevisionId::new(format!("test.{name}.v1")).unwrap();
        law.parents = vec![torch_law_revision().id.clone()];
        let program = law.law.as_mut().unwrap();
        match name {
            "short_history" => program.histories.get_mut("off_edges").unwrap().window = 0,
            "overflow" => program.histories.get_mut("off_edges").unwrap().capacity = 1,
            "reads_diagnostic" => {
                program
                    .handlers
                    .get_mut("scheduled_tick")
                    .unwrap()
                    .push(Instruction::If {
                        condition: Expr::Register {
                            name: "burnout_seen".into(),
                        },
                        then: vec![Instruction::Set {
                            register: "lit".into(),
                            value: Expr::Constant { value: 0 },
                        }],
                        otherwise: vec![],
                    })
            }
            _ => unreachable!(),
        }
        catalog.insert_revision(law.clone()).unwrap();
        let mut selected = selection(input, output);
        selected.torch_law = law.id.clone();
        let model = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
            &catalog,
            selected,
            PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        )
        .unwrap();
        let report = model.verify_history_abstraction(BehaviorBudget::default());
        assert_eq!(report.behavior.status, expected, "{name}: {report:?}");
        assert_eq!(report.torch_revision, law);
        if name == "overflow" {
            assert!(report.behavior.detail.contains("capacity"));
        }
        assert_eq!(
            catalog.revision(&torch_law_revision().id),
            Some(torch_law_revision())
        );
    }
    let mut feedback = world;
    feedback.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
    feedback.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    let model = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
        &catalog(&feedback, relation(true)),
        selection(input, Pos::new(0, 1, 0)),
        PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
    )
    .unwrap();
    let report = model.verify_history_abstraction(BehaviorBudget::default());
    assert_eq!(report.behavior.status, CheckStatus::Undetermined);
    assert!(
        report
            .behavior
            .detail
            .contains("no concrete counterexample"),
        "{report:?}"
    );
}
