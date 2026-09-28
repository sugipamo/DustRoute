use std::collections::BTreeMap;

use dustroute_library::PortDirection;
use dustroute_library::assembly::Assembly;
use dustroute_library::behavior_type::{BehaviorInitialCondition, BooleanRow, RepeatedSettling};
use dustroute_library::blueprint::*;
use dustroute_library::builtin_laws::{builtin_laws, dust_law_revision};
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_library::runtime_behavior::{RuntimeBehaviorContext, RuntimeBehaviorProfile};
use dustroute_minecraft::{
    BlockKind, Facing, PistonState, PistonVariant, Pos, Region, RotationY, World,
};
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel};
use dustroute_translate::promotion::CheckStatus;
use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;

fn fixture(
    controls: usize,
) -> (
    BlueprintCatalog,
    Assembly,
    InstancePath,
    BehaviorBinding,
    RuntimeBehaviorContext,
) {
    let mut world = World::new();
    let mut ports = vec![];
    let mut observed_inputs = BTreeMap::new();
    let mut observed_outputs = BTreeMap::new();
    let mut inputs = vec![];
    for (i, facing) in [Facing::East, Facing::Up, Facing::Down]
        .into_iter()
        .enumerate()
    {
        let pos = Pos::new(i as i32 * 8, 4, 0);
        let piston = world.place(BlockKind::Piston, pos);
        piston.facing = Some(facing);
        piston.piston_variant = Some(PistonVariant::Sticky);
        piston.piston_state = Some(PistonState::Retracted);
        let step = facing.offset();
        world.place(BlockKind::Solid, pos.offset(step.x, step.y, step.z));
        let input = pos.offset(0, 0, -1);
        if i < controls {
            world.place(BlockKind::Solid, input.offset(0, -1, 0));
            let lever = world.place(BlockKind::Lever, input);
            lever.powered = Some(false);
            lever.support_offset = Some(Facing::Down.offset());
            inputs.push(input);
            let name = format!("input{i}");
            ports.push(BlueprintPort {
                name: name.clone(),
                position: input,
                direction: PortDirection::Input,
                kind: BlueprintPortKind::BlockState,
                facing: None,
                required_source_types: vec![],
            });
            observed_inputs.insert(
                name.clone(),
                ObservedPort::Location {
                    port: name,
                    predicate: LocationPredicate::Powered {
                        block_kind: BlockKind::Lever,
                        powered: true,
                    },
                },
            );
        } else {
            world.place(BlockKind::RedstoneBlock, input);
        }
        let name = format!("body{i}");
        ports.push(BlueprintPort {
            name: name.clone(),
            position: pos,
            direction: PortDirection::Output,
            kind: BlueprintPortKind::BlockState,
            facing: None,
            required_source_types: vec![],
        });
        observed_outputs.insert(
            name.clone(),
            ObservedPort::Location {
                port: name,
                predicate: LocationPredicate::PistonState {
                    state: PistonState::Extended,
                },
            },
        );
    }
    let region = Region::new(Pos::new(-4, -1, -4), Pos::new(20, 10, 4));
    let context = RuntimeBehaviorContext {
        profile: RuntimeBehaviorProfile::UnifiedPistonElectricalRootExplorationV17,
        initial_condition: BehaviorInitialCondition::FreshConstruction,
        known_region: region,
        input_levers: inputs,
        root_limits: Default::default(),
    };
    let definition = TypeRevision {
        id: TypeRevisionId::new("test.unified.body-relation.v1").unwrap(),
        name: "Mixed facing body relation".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: observed_inputs.keys().cloned().collect(),
                outputs: observed_outputs.keys().cloned().collect(),
                rows: (0..(1 << controls))
                    .map(|mask| BooleanRow {
                        inputs: (0..controls).map(|i| mask & (1 << i) != 0).collect(),
                        outputs: (0..3)
                            .map(|i| i >= controls || mask & (1 << i) != 0)
                            .collect(),
                    })
                    .collect(),
            },
        },
    };
    let binding = BehaviorBinding::Observed {
        behavior_type: definition.id.clone(),
        observed_inputs,
        observed_outputs,
    };
    let mut source = dust_law_revision().clone();
    source.id = BlueprintRevisionId::new("test.unified.mixed-body.v1").unwrap();
    source.name = "Independent horizontal/up/down pistons".into();
    source.law = None;
    source.blocks = world
        .iter()
        .map(|(position, block)| PositionedBlock {
            position: *position,
            block: block.clone(),
        })
        .collect();
    source.required_laws = context
        .execution_context()
        .laws
        .values()
        .map(|id| BlueprintRevisionId::new(id).unwrap())
        .collect();
    source.ports = ports;
    source.behavior_bindings = vec![binding.clone()];
    let instance = InstanceId::new("mixed").unwrap();
    let assembly = Assembly {
        name: source.name.clone(),
        blocks: source.blocks.clone(),
        known_regions: vec![region],
        instances: vec![BlueprintInclusion {
            instance: instance.clone(),
            revision: source.id.clone(),
            origin: Pos::default(),
            rotation: RotationY::R0,
        }],
        connections: vec![],
        boundaries: vec![],
    };
    let mut catalog = builtin_laws().clone();
    catalog.insert_type(definition).unwrap();
    catalog.insert_revision(source).unwrap();
    (catalog, assembly, vec![instance], binding, context)
}

#[test]
fn mixed_world_exploration_drives_all_physical_inputs_and_reloads_the_context() {
    let (catalog, assembly, instance, binding, context) = fixture(3);
    let context: RuntimeBehaviorContext =
        serde_json::from_str(&serde_json::to_string(&context).unwrap()).unwrap();
    let catalog = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog, &assembly, &instance, &binding, &context,
    )
    .unwrap();
    for mask in 0..8 {
        let inputs: Vec<_> = (0..3).map(|i| mask & (1 << i) != 0).collect();
        let mut state = model
            .with_inputs(&model.initial_state().unwrap(), &inputs)
            .unwrap();
        for _ in 0..80 {
            state = model.step(&state).unwrap();
        }
        assert_eq!(model.outputs(&state).unwrap(), inputs);
        let opposite: Vec<_> = inputs.iter().map(|b| !b).collect();
        state = model.with_inputs(&state, &opposite).unwrap();
        for _ in 0..80 {
            state = model.step(&state).unwrap();
        }
        assert_eq!(model.outputs(&state).unwrap(), opposite);
    }
    let other = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog, &assembly, &instance, &binding, &context,
    )
    .unwrap();
    assert!(other.outputs(&model.initial_state().unwrap()).is_err());
}

#[test]
fn a_mixed_world_with_one_variable_input_can_close_its_behavior_graph() {
    let (catalog, assembly, instance, binding, context) = fixture(1);
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog, &assembly, &instance, &binding, &context,
    )
    .unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert!(report.graph_closed);
    let incomplete = model.verify(BehaviorBudget {
        max_states: 1,
        ..Default::default()
    });
    assert_eq!(incomplete.status, CheckStatus::Undetermined);
}

#[test]
fn moving_an_upward_piston_body_can_close_its_behavior_graph() {
    let (catalog, mut assembly, instance, binding, context) = fixture(1);
    let payload = assembly
        .blocks
        .iter_mut()
        .find(|b| b.position == Pos::new(1, 4, 0))
        .unwrap();
    payload.block = dustroute_minecraft::Block::new(BlockKind::Piston);
    payload.block.facing = Some(Facing::Up);
    // Source geometry is only an interpretation; the actual Assembly drives
    // execution. This change is deliberately not an adopted source revision.
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog, &assembly, &instance, &binding, &context,
    )
    .unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert!(report.graph_closed);
}

#[test]
fn attachment_loss_can_close_the_piston_behavior_graph() {
    let (catalog, mut assembly, instance, binding, context) = fixture(1);
    let mut attached = dustroute_minecraft::Block::new(BlockKind::Lever);
    attached.powered = Some(false);
    attached.support_offset = Some(Facing::Down.offset());
    assembly.blocks.push(PositionedBlock {
        position: Pos::new(1, 5, 0),
        block: attached,
    });
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog, &assembly, &instance, &binding, &context,
    )
    .unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert!(report.graph_closed);
}

#[test]
fn electrical_profile_explores_mixed_assemblies_after_catalog_reload() {
    let (catalog, assembly, instance, binding, context) = fixture(1);
    let catalog = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    let context = serde_json::from_str(&serde_json::to_string(&context).unwrap()).unwrap();
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog, &assembly, &instance, &binding, &context,
    )
    .unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert!(report.graph_closed);
    let incomplete = model.verify(BehaviorBudget {
        max_states: 1,
        ..Default::default()
    });
    assert_eq!(incomplete.status, CheckStatus::Undetermined);
}

#[test]
fn retired_law_requirements_are_rejected_by_the_current_catalog() {
    let (mut catalog, assembly, _, _, _) = fixture(1);
    let mut source = catalog
        .revision(&assembly.instances[0].revision)
        .unwrap()
        .clone();
    source.id = BlueprintRevisionId::new("test.retired-law-requirement.v1").unwrap();
    let retired =
        BlueprintRevisionId::new("dustroute.law.piston.direct-input.java-1-21-11.v1").unwrap();
    source.required_laws.push(retired.clone());
    assert!(catalog.revision(&retired).is_none());
    assert!(matches!(
        catalog.insert_revision(source),
        Err(BlueprintError::UnknownRevision(id)) if id == retired
    ));
}
