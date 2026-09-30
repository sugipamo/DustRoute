//! Explicit finite-flight proposal, using the existing native runtime fixture.
#[allow(dead_code)]
#[path = "runtime_blueprint.rs"]
pub mod base;

use dustroute_library::PortDirection;
use dustroute_library::assembly::Assembly;
use dustroute_library::behavior_type::SingleOperation;
use dustroute_library::blueprint::*;
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_translate::snapshot::{MinecraftSnapshot, assembly_from_snapshot};
use dustroute_translate::{world::BlockKind, world::PistonState, world::Pos, world::Region};
use std::collections::BTreeMap;

pub fn fixture() -> base::Fixture {
    let f = base::fixture(false, false);
    let snapshot: MinecraftSnapshot =
        serde_json::from_str(include_str!("../fixtures/flying-machine-initial.json")).unwrap();
    let region = Region::new(snapshot.min, snapshot.max);
    let assembly = assembly_from_snapshot(&snapshot, "finite flight", vec![region]).unwrap();
    let world = assembly.inspect(&f.catalog).unwrap().proposed_world();
    let control = Pos::new(0, 2, 0);
    let mut observations = Vec::new();
    // Declare every cell swept by the engine: six target blocks and empty
    // departure/intermediate cells. This is a requirement, not model output.
    for x in 0..=11 {
        for y in 0..=1 {
            for z in 0..=1 {
                let position = Pos::new(x, y, z);
                let target = world
                    .get(Pos::new(x - 10, y, z))
                    .map_or(BlockKind::Air, |b| b.kind);
                let before = world.get(position).map_or(BlockKind::Air, |b| b.kind) == target;
                observations.push((
                    format!("cell_{x}_{y}_{z}"),
                    position,
                    LocationPredicate::BlockKind { block_kind: target },
                    before,
                ));
            }
        }
    }
    for (name, position) in [
        ("push_retracted", Pos::new(10, 0, 1)),
        ("pull_retracted", Pos::new(11, 0, 0)),
    ] {
        observations.push((
            name.into(),
            position,
            LocationPredicate::PistonState {
                state: PistonState::Retracted,
            },
            false,
        ));
    }
    bind_single_operation(
        f,
        assembly,
        control,
        observations,
        "flight.single-course.v1",
        "One launch and ten-block arrival",
    )
}

/// The observed six-block engine and two-block bar, with a declared four-block
/// course and four mature cane columns. Soil, water, trigger and stopper are
/// proposed initial conditions, not a fresh live capture or a preset recipe.
pub fn observed_cane_fixture() -> base::Fixture {
    let f = base::fixture(false, false);
    let snapshot = observed_cane_initial();
    let expected: MinecraftSnapshot = serde_json::from_str(include_str!(
        "../fixtures/observed-machine-cane-flight.expected.json"
    ))
    .unwrap();
    let region = Region::new(snapshot.min, snapshot.max);
    let assembly =
        assembly_from_snapshot(&snapshot, "observed engine cane trial", vec![region]).unwrap();
    let world = assembly.inspect(&f.catalog).unwrap().proposed_world();
    let target = assembly_from_snapshot(&expected, "declared arrival", vec![region])
        .unwrap()
        .inspect(&f.catalog)
        .unwrap()
        .proposed_world();
    let mut observations = Vec::new();
    // Include roots, upper crops, the body and the entire swept corridor. Kind
    // predicates are deliberately accompanied by an exact native snapshot
    // regression; they do not distinguish cane, glass and slime identities.
    for x in 0..=5 {
        for y in -1..=1 {
            for z in 0..=3 {
                let position = Pos::new(x, y, z);
                let kind = target.get(position).map_or(BlockKind::Air, |b| b.kind);
                observations.push((
                    format!("cell_{x}_{y}_{z}"),
                    position,
                    LocationPredicate::BlockKind { block_kind: kind },
                    world.get(position).map_or(BlockKind::Air, |b| b.kind) == kind,
                ));
            }
        }
    }
    for (name, position) in [
        ("east_retracted", Pos::new(4, 0, 0)),
        ("west_retracted", Pos::new(5, 0, 1)),
    ] {
        observations.push((
            name.into(),
            position,
            LocationPredicate::PistonState {
                state: PistonState::Retracted,
            },
            false,
        ));
    }
    let mut f = bind_single_operation(
        f,
        assembly,
        Pos::new(0, 1, 2),
        observations,
        "flight.observed-cane.single-course.v1",
        "One launch, four-block arrival and four rooted cane columns",
    );
    let field_type = TypeRevision {
        id: TypeRevisionId::new("flight.observed-cane.fixed-field.v1").unwrap(),
        name: "Fixed native soil, enclosed water, barriers and cane roots".into(),
        contract: TypeContract::BlockPattern {
            blocks: world
                .iter()
                .filter(|(p, _)| p.y < 0)
                .map(|(p, b)| PositionedBlock {
                    position: *p,
                    block: b.clone(),
                })
                .collect(),
        },
    };
    let field_type_id = field_type.id.clone();
    f.catalog.insert_type(field_type).unwrap();
    let child = f
        .request
        .revisions
        .iter_mut()
        .find(|r| r.id == f.request.next_child)
        .unwrap();
    child.ports.push(BlueprintPort {
        name: "fixed_field".into(),
        position: Pos::default(),
        direction: PortDirection::Output,
        kind: BlueprintPortKind::BlockState,
        facing: None,
        required_source_types: vec![],
    });
    child.static_type_bindings.push(StaticTypeBinding {
        port: "fixed_field".into(),
        type_revision: field_type_id,
    });
    f
}

pub fn observed_cane_initial() -> MinecraftSnapshot {
    serde_json::from_str(include_str!(
        "../fixtures/observed-machine-cane-flight.initial.json"
    ))
    .unwrap()
}

fn bind_single_operation(
    mut f: base::Fixture,
    assembly: Assembly,
    control: Pos,
    observations: Vec<(String, Pos, LocationPredicate, bool)>,
    type_id: &str,
    title: &str,
) -> base::Fixture {
    let region = assembly.known_regions[0];
    f.context = RuntimeBehaviorContext::fresh_pistons(region, vec![control]);
    let mut outputs = BTreeMap::new();
    let mut names = Vec::new();
    let mut initial = Vec::new();
    let mut ports = vec![BlueprintPort {
        name: "input".into(),
        position: control,
        direction: PortDirection::Input,
        kind: BlueprintPortKind::BlockState,
        facing: None,
        required_source_types: vec![],
    }];
    for (name, position, predicate, before) in observations {
        names.push(name.clone());
        initial.push(before);
        ports.push(BlueprintPort {
            name: name.clone(),
            position,
            direction: PortDirection::Output,
            kind: BlueprintPortKind::BlockState,
            facing: None,
            required_source_types: vec![],
        });
        outputs.insert(
            name.clone(),
            ObservedPort::Location {
                port: name,
                predicate,
            },
        );
    }
    let definition = TypeRevision {
        id: TypeRevisionId::new(type_id).unwrap(),
        name: title.into(),
        contract: TypeContract::SingleOperation {
            requirement: SingleOperation {
                input: "launch".into(),
                completed: vec![true; names.len()],
                outputs: names,
                initial,
            },
        },
    };
    let binding = BehaviorBinding::Observed {
        behavior_type: definition.id.clone(),
        observed_inputs: BTreeMap::from([(
            "launch".into(),
            ObservedPort::Location {
                port: "input".into(),
                predicate: LocationPredicate::Powered {
                    block_kind: BlockKind::Lever,
                    powered: true,
                },
            },
        )]),
        observed_outputs: outputs,
    };
    f.catalog.insert_type(definition).unwrap();
    let child = f
        .request
        .revisions
        .iter_mut()
        .find(|r| r.id == f.request.next_child)
        .unwrap();
    child.blocks = assembly.blocks.clone();
    child.ports = ports;
    child.behavior_bindings = vec![binding];
    child.required_laws = f
        .context
        .execution_context()
        .laws
        .values()
        .map(|id| BlueprintRevisionId::new(id).unwrap())
        .collect();
    f.request.candidate_state.assembly.blocks = assembly.blocks;
    f.request.candidate_state.assembly.known_regions = vec![region];
    f.request.behavior_context = Some(f.context.clone().into());
    f.request.title = title.into();
    f.request.description = "Declare one launch, intact endpoint occupancy and cleared swept cells; retain old revisions and use the common native runtime. Fixed environment and exact native identities are separate obligations.".into();
    f
}
