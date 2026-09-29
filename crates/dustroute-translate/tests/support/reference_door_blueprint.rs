//! Immutable reference geometry plus an explicit, unadopted behavior proposal.
use std::collections::BTreeMap;

use dustroute_library::assembly::{Assembly, AssemblyRevision};
use dustroute_library::behavior_type::{BooleanRow, RepeatedSettling};
use dustroute_library::blueprint::*;
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_library::{PortDirection, Provenance};
use dustroute_translate::blueprint_update::BlueprintUpdateRequest;
use dustroute_translate::snapshot::assembly_from_snapshot;
use dustroute_translate::{
    cells::RotationY, snapshot::MinecraftSnapshot, world::BlockKind, world::Pos, world::Region,
    world::World,
};

pub struct Fixture {
    pub catalog: BlueprintCatalog,
    pub base: AssemblyRevision,
    pub request: BlueprintUpdateRequest,
    pub context: RuntimeBehaviorContext,
    pub world: World,
}

fn id(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(format!("reference-door.{value}")).unwrap()
}
fn include(name: &str, revision: BlueprintRevisionId) -> BlueprintInclusion {
    BlueprintInclusion {
        instance: InstanceId::new(name).unwrap(),
        revision,
        origin: Pos::default(),
        rotation: RotationY::R0,
    }
}
fn port(name: String, position: Pos, direction: PortDirection) -> BlueprintPort {
    BlueprintPort {
        name,
        position,
        direction,
        kind: BlueprintPortKind::BlockState,
        facing: None,
        required_source_types: vec![],
    }
}

pub fn fixture() -> Fixture {
    let raw: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/reference-3x3-bobiloosky-v1.json")).unwrap();
    let snapshot: MinecraftSnapshot = serde_json::from_value(raw["initial"].clone()).unwrap();
    let region = Region::new(snapshot.min, snapshot.max);
    let literal = assembly_from_snapshot(&snapshot, "Reference 3x3 door", vec![region]).unwrap();
    let world = literal
        .inspect(&BlueprintCatalog::default())
        .unwrap()
        .proposed_world();
    let context = RuntimeBehaviorContext::fresh_pistons(region, vec![Pos::new(0, 11, 0)]);
    let source = BlueprintRevision {
        id: id("mechanism.v1"),
        parents: vec![],
        name: "Literal Bobiloosky reference door; no adoption claim".into(),
        classifications: vec![],
        blocks: literal.blocks.clone(),
        law: None,
        initial_layout: None,
        inclusions: vec![],
        connections: vec![],
        port_bindings: vec![],
        static_type_bindings: vec![],
        required_laws: vec![],
        ports: vec![],
        behavior_bindings: vec![],
        provenance: Provenance {
            author: "Reference-world import".into(),
            source_url: None,
            license: None,
            retrieved_on: None,
        },
    };
    let mut parent = source.clone();
    parent.id = id("parent.v1");
    parent.name = "Reference door wrapper".into();
    parent.blocks.clear();
    parent.inclusions = vec![include("mechanism", source.id.clone())];
    let base = AssemblyRevision {
        id: AssemblyRevisionId::new("reference-door.state.v1").unwrap(),
        parents: vec![],
        assembly: Assembly {
            instances: vec![include("root", parent.id.clone())],
            ..literal
        },
    };
    let mut next = source.clone();
    next.id = id("mechanism.v2");
    next.parents = vec![source.id.clone()];
    next.name = "Reference door with explicit repeated-settling aperture requirement".into();
    next.required_laws = context
        .execution_context()
        .laws
        .values()
        .map(|law| BlueprintRevisionId::new(law).unwrap())
        .collect();
    next.ports = vec![port(
        "control".into(),
        Pos::new(0, 11, 0),
        PortDirection::Input,
    )];
    let mut outputs = Vec::new();
    let mut observed_outputs = BTreeMap::new();
    for y in 6..=8 {
        for z in -1..=1 {
            let name = format!("aperture_{}_{}", y - 6, z + 1);
            next.ports
                .push(port(name.clone(), Pos::new(0, y, z), PortDirection::Output));
            for (suffix, block_kind) in [("air", BlockKind::Air), ("solid", BlockKind::Solid)] {
                let output = format!("{name}_{suffix}");
                outputs.push(output.clone());
                observed_outputs.insert(
                    output,
                    ObservedPort::Location {
                        port: name.clone(),
                        predicate: LocationPredicate::BlockKind { block_kind },
                    },
                );
            }
        }
    }
    // This is the existing unrestricted repeated-settling contract: input may
    // change between complete roots, including during motion. It is stronger
    // than replaying the four captured, widely spaced input changes. The Solid
    // predicate is functional; exact quartz identity remains in the geometry.
    let relation = TypeRevision {
        id: TypeRevisionId::new("reference-door.aperture-repeated-settling.v1").unwrap(),
        name: "Nine air cells when open; nine solid cells when closed".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["closed_command".into()],
                outputs,
                rows: [false, true]
                    .into_iter()
                    .map(|closed| BooleanRow {
                        inputs: vec![closed],
                        outputs: (0..9).flat_map(|_| [!closed, closed]).collect(),
                    })
                    .collect(),
            },
        },
    };
    next.behavior_bindings = vec![BehaviorBinding::Observed {
        behavior_type: relation.id.clone(),
        observed_inputs: BTreeMap::from([(
            "closed_command".into(),
            ObservedPort::Location {
                port: "control".into(),
                predicate: LocationPredicate::Powered {
                    block_kind: BlockKind::Lever,
                    powered: true,
                },
            },
        )]),
        observed_outputs,
    }];
    let mut next_parent = parent.clone();
    next_parent.id = id("parent.v2");
    next_parent.parents = vec![parent.id.clone()];
    next_parent.inclusions[0].revision = next.id.clone();
    let mut candidate = base.clone();
    candidate.id = AssemblyRevisionId::new("reference-door.state.v2").unwrap();
    candidate.parents = vec![base.id.clone()];
    candidate.assembly.instances[0].revision = next_parent.id.clone();
    let request = BlueprintUpdateRequest {
        id: BlueprintUpdateId::new("reference-door.adoption-audit.v1").unwrap(),
        title: "Review the reference door under the current native runtime".into(),
        description: "Preserve all 43 blocks and introduce an explicit aperture contract. No historical replay or strict-placement record grants adoption.".into(),
        base_state: base.id.clone(),
        base_parent: parent.id.clone(),
        candidate_parent: next_parent.id.clone(),
        parent_instance: vec![InstanceId::new("root").unwrap()],
        child_before: vec![InstanceId::new("mechanism").unwrap()],
        previous_child: source.id.clone(),
        child_after: vec![InstanceId::new("mechanism").unwrap()],
        next_child: next.id.clone(),
        revisions: vec![next_parent, next],
        candidate_state: candidate,
        behavior_context: Some(context.clone().into()),
    };
    let mut catalog = dustroute_library::builtin_laws::builtin_laws().clone();
    catalog.insert_type(relation).unwrap();
    catalog.insert_revisions(vec![parent, source]).unwrap();
    catalog.insert_assembly(base.clone()).unwrap();
    Fixture {
        catalog,
        base,
        request,
        context,
        world,
    }
}

/// Explicit new candidate from the literal v1 source. The historical
/// unrestricted v2 proposal and its type remain unchanged and unadopted.
#[allow(dead_code)]
pub fn ordinary_fixture() -> Fixture {
    let mut f = fixture();
    let definition = TypeRevision {
        id: TypeRevisionId::new("dustroute.type.piston-door-3x3.v1").unwrap(),
        name: "Ordinary 3x3 piston door; commands after completed operations".into(),
        contract: TypeContract::PistonDoor {
            requirement: Box::new(dustroute_library::behavior_type::PistonDoor::three_by_three()),
        },
    };
    let child = id("mechanism.v3");
    let parent = id("parent.v3");
    for revision in &mut f.request.revisions {
        if revision.id == f.request.next_child {
            revision.id = child.clone();
            revision.name = "Reference door with ordinary completed-operation contract".into();
            let BehaviorBinding::Observed { behavior_type, .. } =
                &mut revision.behavior_bindings[0]
            else {
                unreachable!()
            };
            *behavior_type = definition.id.clone();
        } else {
            revision.id = parent.clone();
            revision.inclusions[0].revision = child.clone();
        }
    }
    f.catalog.insert_type(definition).unwrap();
    f.request.id = BlueprintUpdateId::new("reference-door.ordinary-adoption.v1").unwrap();
    f.request.title = "Adopt the ordinary 3x3 door under completed-operation inputs".into();
    f.request.description = "Explicit new candidate from literal v1, preserving all geometry and Laws. The prior unrestricted type and proposal are not rewritten.".into();
    f.request.next_child = child;
    f.request.candidate_parent = parent.clone();
    f.request.candidate_state.id = AssemblyRevisionId::new("reference-door.state.v3").unwrap();
    f.request.candidate_state.assembly.instances[0].revision = parent;
    f
}
