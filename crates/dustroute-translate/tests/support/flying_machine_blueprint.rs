//! Explicit finite-flight proposal, using the existing native runtime fixture.
#[allow(dead_code)]
#[path = "runtime_blueprint.rs"]
pub mod base;

use dustroute_library::PortDirection;
use dustroute_library::behavior_type::SingleOperation;
use dustroute_library::blueprint::*;
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_translate::snapshot::{MinecraftSnapshot, assembly_from_snapshot};
use dustroute_translate::{BlockKind, PistonState, Pos, Region};
use std::collections::BTreeMap;

pub fn fixture() -> base::Fixture {
    let mut f = base::fixture(false, false);
    let snapshot: MinecraftSnapshot =
        serde_json::from_str(include_str!("../fixtures/flying-machine-initial.json")).unwrap();
    let region = Region::new(snapshot.min, snapshot.max);
    let assembly = assembly_from_snapshot(&snapshot, "finite flight", vec![region]).unwrap();
    let world = assembly.inspect(&f.catalog).unwrap().proposed_world();
    let control = Pos::new(0, 2, 0);
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
    let mut observe = |name: String, position: Pos, predicate: LocationPredicate, before: bool| {
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
    };
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
                observe(
                    format!("cell_{x}_{y}_{z}"),
                    position,
                    LocationPredicate::BlockKind { block_kind: target },
                    before,
                );
            }
        }
    }
    for (name, position) in [
        ("push_retracted", Pos::new(10, 0, 1)),
        ("pull_retracted", Pos::new(11, 0, 0)),
    ] {
        observe(
            name.into(),
            position,
            LocationPredicate::PistonState {
                state: PistonState::Retracted,
            },
            false,
        );
    }
    let definition = TypeRevision {
        id: TypeRevisionId::new("flight.single-course.v1").unwrap(),
        name: "One launch and ten-block arrival".into(),
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
    f.request.title = "One-shot flight in a fixed empty corridor".into();
    f.request.description = "Declare one launch, intact endpoint occupancy and an empty swept path; retain old revisions and use the common native runtime.".into();
    f
}
