//! Shared fresh-world fixture for native review and the public MCP workflow.
use dustroute_library::assembly::{Assembly, AssemblyRevision};
use dustroute_library::behavior_type::{BehaviorInitialCondition, BooleanRow, RepeatedSettling};
use dustroute_library::blueprint::*;
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_library::runtime_behavior::{RuntimeBehaviorContext, RuntimeBehaviorProfile};
use dustroute_library::{PortDirection, Provenance};
use dustroute_translate::blueprint_update::BlueprintUpdateRequest;
use dustroute_translate::{
    BlockKind, Facing, PistonState, PistonVariant, Pos, Region, RotationY, World,
};
use std::collections::BTreeMap;

pub struct Fixture {
    pub catalog: BlueprintCatalog,
    pub base: AssemblyRevision,
    pub request: BlueprintUpdateRequest,
    pub context: RuntimeBehaviorContext,
}

/// Explicit migration proposal: keep the recorded horizontal source revisions
/// intact and replace only the candidate with a mixed electrical arrangement.
#[allow(dead_code)] // This shared module also serves historical-profile tests.
pub fn electrical_fixture(child_violation: bool) -> Fixture {
    let mut f = fixture(child_violation, false);
    let mut world = World::new();
    for x in -3..=-1 {
        world.place(BlockKind::Solid, Pos::new(x, 0, 0));
    }
    let input = Pos::new(-3, 1, 0);
    let lever = world.place(BlockKind::Lever, input);
    lever.powered = Some(false);
    lever.support_offset = Some(Facing::Down.offset());
    let dust = world.place(BlockKind::RedstoneWire, Pos::new(-2, 1, 0));
    dust.support_offset = Some(Facing::Down.offset());
    dust.power_level = Some(0);
    dust.wire_connections = Some(
        [Facing::North, Facing::East, Facing::South, Facing::West]
            .into_iter()
            .map(|d| {
                (
                    d,
                    if matches!(d, Facing::East | Facing::West) {
                        dustroute_translate::WireConnection::Side
                    } else {
                        dustroute_translate::WireConnection::None
                    },
                )
            })
            .collect(),
    );
    let repeater = world.place(BlockKind::Repeater, Pos::new(-1, 1, 0));
    repeater.support_offset = Some(Facing::Down.offset());
    repeater.facing = Some(Facing::East);
    repeater.delay = Some(1);
    repeater.powered = Some(false);
    for (pos, direction) in [
        (
            Pos::new(0, 1, 0),
            if child_violation {
                Facing::East
            } else {
                Facing::Up
            },
        ),
        (
            Pos::new(8, 4, 0),
            if child_violation {
                Facing::Up
            } else {
                Facing::East
            },
        ),
        (Pos::new(16, 4, 0), Facing::Down),
    ] {
        let body = world.place(BlockKind::Piston, pos);
        body.facing = Some(direction);
        body.piston_variant = Some(PistonVariant::Sticky);
        body.piston_state = Some(PistonState::Retracted);
        let step = direction.offset();
        world.place(BlockKind::Solid, pos.offset(step.x, step.y, step.z));
        if pos.x != 0 {
            world.place(BlockKind::RedstoneBlock, pos.offset(0, 0, -1));
        }
    }
    f.context.profile = RuntimeBehaviorProfile::UnifiedPistonElectricalRootExplorationV6;
    f.context.known_region = Region::new(Pos::new(-6, -2, -5), Pos::new(22, 12, 5));
    f.context.input_levers = vec![input];
    let next = f
        .request
        .revisions
        .iter_mut()
        .find(|r| r.id == f.request.next_child)
        .unwrap();
    next.blocks = world
        .iter()
        .map(|(position, block)| PositionedBlock {
            position: *position,
            block: block.clone(),
        })
        .collect();
    next.ports
        .iter_mut()
        .find(|p| p.name == "input")
        .unwrap()
        .position = input;
    next.required_laws = f
        .context
        .execution_context()
        .laws
        .values()
        .map(|law| id(law))
        .collect();
    f.request.candidate_state.assembly.blocks = next.blocks.clone();
    f.request.candidate_state.assembly.known_regions = vec![f.context.known_region];
    f.request.behavior_context = Some(f.context.clone().into());
    f
}
fn id(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(value).unwrap()
}
fn path(name: &str) -> InstancePath {
    vec![InstanceId::new(name).unwrap()]
}
fn include(name: &str, revision: BlueprintRevisionId) -> BlueprintInclusion {
    BlueprintInclusion {
        instance: InstanceId::new(name).unwrap(),
        revision,
        origin: Pos::default(),
        rotation: RotationY::R0,
    }
}
pub fn fixture(child_violation: bool, relocate: bool) -> Fixture {
    let mut world = World::new();
    world.place(BlockKind::Solid, Pos::new(-1, 0, 0));
    let lever = world.place(BlockKind::Lever, Pos::new(-1, 1, 0));
    lever.powered = Some(false);
    lever.support_offset = Some(Pos::new(0, -1, 0));
    let body = world.place(BlockKind::Piston, Pos::new(0, 1, 0));
    body.facing = Some(Facing::East);
    body.piston_variant = Some(PistonVariant::Sticky);
    body.piston_state = Some(PistonState::Retracted);
    world.place(BlockKind::Solid, Pos::new(1, 1, 0));
    let relation = TypeRevision {
        id: TypeRevisionId::new("runtime-test.body-relation.v1").unwrap(),
        name: "Body state follows the declared input".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["control".into()],
                outputs: vec!["extended".into()],
                rows: [false, true]
                    .into_iter()
                    .map(|b| BooleanRow {
                        inputs: vec![b],
                        outputs: vec![b],
                    })
                    .collect(),
            },
        },
    };
    let mut mechanism = BlueprintRevision {
        id: id("runtime-test.mechanism.v1"),
        parents: vec![],
        name: "Horizontal piston body relation".into(),
        classifications: vec![],
        blocks: world
            .iter()
            .map(|(p, b)| PositionedBlock {
                position: *p,
                block: b.clone(),
            })
            .collect(),
        law: None,
        initial_layout: None,
        inclusions: vec![],
        connections: vec![],
        port_bindings: vec![],
        static_type_bindings: vec![],
        required_laws: vec![id("dustroute.law.piston.control.java-1-21-11.v1")],
        ports: [
            ("input", Pos::new(-1, 1, 0), PortDirection::Input),
            ("body", Pos::new(0, 1, 0), PortDirection::Output),
        ]
        .into_iter()
        .map(|(name, position, direction)| BlueprintPort {
            name: name.into(),
            position,
            direction,
            kind: BlueprintPortKind::BlockState,
            facing: None,
            required_source_types: vec![],
        })
        .collect(),
        behavior_bindings: vec![BehaviorBinding::Observed {
            behavior_type: relation.id.clone(),
            observed_inputs: BTreeMap::from([(
                "control".into(),
                ObservedPort::Location {
                    port: "input".into(),
                    predicate: LocationPredicate::Powered {
                        block_kind: BlockKind::Lever,
                        powered: true,
                    },
                },
            )]),
            observed_outputs: BTreeMap::from([(
                "extended".into(),
                ObservedPort::Location {
                    port: "body".into(),
                    predicate: LocationPredicate::PistonState {
                        state: PistonState::Extended,
                    },
                },
            )]),
        }],
        provenance: Provenance {
            author: "test".into(),
            source_url: None,
            license: None,
            retrieved_on: None,
        },
    };
    let mut catalog = dustroute_library::builtin_laws::builtin_laws().clone();
    catalog.insert_type(relation).unwrap();
    if child_violation {
        let identity = TypeRevision {
            id: TypeRevisionId::new("runtime-test.body-identity.v1").unwrap(),
            name: "Actual piston block".into(),
            contract: TypeContract::BlockKind {
                block_kind: BlockKind::Piston,
            },
        };
        let mut child = mechanism.clone();
        child.id = id("runtime-test.retained-body.v1");
        child.blocks.retain(|r| r.block.kind == BlockKind::Piston);
        child.ports.retain(|p| p.name == "body");
        child.behavior_bindings.clear();
        child.static_type_bindings = vec![StaticTypeBinding {
            port: "body".into(),
            type_revision: identity.id.clone(),
        }];
        mechanism.inclusions.push(include("body", child.id.clone()));
        catalog.insert_type(identity).unwrap();
        catalog.insert_revision(child).unwrap();
    }
    let mut parent = mechanism.clone();
    parent.id = id("runtime-test.parent.v1");
    parent.blocks.clear();
    parent.ports.clear();
    parent.behavior_bindings.clear();
    parent.required_laws.clear();
    parent.inclusions = vec![include("mechanism", mechanism.id.clone())];
    let region = Region::new(Pos::new(-4, -1, -4), Pos::new(12, 5, 4));
    let base = AssemblyRevision {
        id: AssemblyRevisionId::new("runtime-test.state.v1").unwrap(),
        parents: vec![],
        assembly: Assembly {
            name: "Concrete piston scene".into(),
            instances: vec![include("root", parent.id.clone())],
            blocks: mechanism.blocks.clone(),
            known_regions: vec![region],
            connections: vec![],
            boundaries: vec![],
        },
    };
    let mut next = mechanism.clone();
    next.id = id("runtime-test.mechanism.v2");
    next.parents = vec![mechanism.id.clone()];
    let mut next_parent = parent.clone();
    next_parent.id = id("runtime-test.parent.v2");
    next_parent.parents = vec![parent.id.clone()];
    next_parent.inclusions[0].revision = next.id.clone();
    let origin = if relocate {
        Pos::new(6, 0, 0)
    } else {
        Pos::default()
    };
    let rotation = if relocate {
        RotationY::R90
    } else {
        RotationY::R0
    };
    next_parent.inclusions[0].origin = origin;
    next_parent.inclusions[0].rotation = rotation;
    let transform = |p: Pos| {
        let p = rotation.pos(p);
        p.offset(origin.x, origin.y, origin.z)
    };
    let mut candidate = base.clone();
    candidate.id = AssemblyRevisionId::new("runtime-test.state.v2").unwrap();
    candidate.parents = vec![base.id.clone()];
    candidate.assembly.instances[0].revision = next_parent.id.clone();
    for record in &mut candidate.assembly.blocks {
        record.position = transform(record.position);
        record.block = rotation.block(&record.block);
    }
    let context = RuntimeBehaviorContext {
        profile: RuntimeBehaviorProfile::UnifiedPistonElectricalRootExplorationV6,
        initial_condition: BehaviorInitialCondition::FreshConstruction,
        known_region: region,
        input_levers: vec![transform(Pos::new(-1, 1, 0))],
        root_limits: Default::default(),
    };
    let request = BlueprintUpdateRequest {
        id: BlueprintUpdateId::new("runtime-test.update.v1").unwrap(),
        title: "Explicit physical revision update".into(),
        description: "Retain the whole native world and all child obligations".into(),
        base_state: base.id.clone(),
        base_parent: parent.id.clone(),
        candidate_parent: next_parent.id.clone(),
        parent_instance: path("root"),
        child_before: path("mechanism"),
        previous_child: mechanism.id.clone(),
        child_after: path("mechanism"),
        next_child: next.id.clone(),
        revisions: vec![next_parent, next],
        candidate_state: candidate,
        behavior_context: Some(context.clone().into()),
    };
    catalog.insert_revisions(vec![parent, mechanism]).unwrap();
    catalog.insert_assembly(base.clone()).unwrap();
    Fixture {
        catalog,
        base,
        request,
        context,
    }
}
