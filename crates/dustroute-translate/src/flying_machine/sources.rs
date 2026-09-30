//! Turn declared geometry into immutable source revisions and a fresh proposal.
use super::recipe::Recipe;
use crate::blueprint_update::BlueprintUpdateRequest;
use crate::snapshot::assembly_from_snapshot;
use crate::{cells::RotationY, world::BlockKind, world::PistonState, world::Pos};
use dustroute_library::assembly::AssemblyRevision;
use dustroute_library::behavior_type::SingleOperation;
use dustroute_library::blueprint::*;
use dustroute_library::flying_machine::FlyingMachineRequest;
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_library::{PortDirection, Provenance};
use std::collections::BTreeMap;

pub(super) fn build(
    spec: &FlyingMachineRequest,
    recipe: &Recipe,
) -> Result<(BlueprintRecords, BlueprintUpdateRequest), String> {
    let id = |suffix: &str| {
        BlueprintRevisionId::new(format!("{}.{suffix}", spec.namespace)).map_err(str::to_owned)
    };
    let state_id = |suffix: &str| {
        AssemblyRevisionId::new(format!("{}.{suffix}", spec.namespace)).map_err(str::to_owned)
    };
    let include =
        |name: &str, revision: BlueprintRevisionId| -> Result<BlueprintInclusion, String> {
            Ok(BlueprintInclusion {
                instance: InstanceId::new(name).map_err(str::to_owned)?,
                revision,
                origin: Pos::default(),
                rotation: RotationY::R0,
            })
        };
    let mut catalog = dustroute_library::builtin_laws::builtin_laws().clone();
    let mut assembly = assembly_from_snapshot(
        &recipe.initial,
        "Generated finite flight",
        vec![recipe.context.known_region],
    )
    .map_err(|e| e.to_string())?;
    let world = assembly
        .inspect(&catalog)
        .map_err(|e| e.to_string())?
        .proposed_world();
    let arrived = assembly_from_snapshot(
        &recipe.arrival,
        "Declared arrival",
        vec![recipe.context.known_region],
    )
    .map_err(|e| e.to_string())?
    .inspect(&catalog)
    .map_err(|e| e.to_string())?
    .proposed_world();
    let control = recipe.context.input_levers[0];
    let mut ports = vec![BlueprintPort {
        name: "input".into(),
        position: control,
        direction: PortDirection::Input,
        kind: BlueprintPortKind::BlockState,
        facing: None,
        required_source_types: vec![],
    }];
    let mut observations = BTreeMap::new();
    let mut names = Vec::new();
    let mut initial = Vec::new();
    let mut observe = |position: Pos, predicate: LocationPredicate, before: bool| {
        let name = format!("observation_{}", names.len());
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
        observations.insert(
            name.clone(),
            ObservedPort::Location {
                port: name,
                predicate,
            },
        );
    };
    for position in &recipe.sweep {
        let expected = arrived.get(*position).map_or(BlockKind::Air, |b| b.kind);
        observe(
            *position,
            LocationPredicate::BlockKind {
                block_kind: expected,
            },
            world.get(*position).map_or(BlockKind::Air, |b| b.kind) == expected,
        );
    }
    for (position, block) in arrived.iter().filter(|(_, b)| b.kind == BlockKind::Piston) {
        let state = block.piston_state.ok_or("declared piston state missing")?;
        observe(
            *position,
            LocationPredicate::PistonState { state },
            world
                .get(*position)
                .is_some_and(|b| b.piston_state == Some(PistonState::Retracted)),
        );
    }
    let definition = TypeRevision {
        id: TypeRevisionId::new(format!("{}.arrival.v1", spec.namespace)).map_err(str::to_owned)?,
        name: "One launch with declared arrival occupancy".into(),
        contract: TypeContract::SingleOperation {
            requirement: SingleOperation {
                input: "launch".into(),
                completed: vec![true; names.len()],
                outputs: names,
                initial,
            },
        },
    };
    catalog
        .insert_type(definition.clone())
        .map_err(|e| e.to_string())?;
    let binding = BehaviorBinding::Observed {
        behavior_type: definition.id,
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
        observed_outputs: observations,
    };
    let original = BlueprintRevision {
        id: id("machine.v1")?,
        parents: vec![],
        name: "Unverified generated initial layout".into(),
        classifications: vec![],
        blocks: assembly.blocks.clone(),
        law: None,
        initial_layout: None,
        inclusions: vec![],
        connections: vec![],
        port_bindings: vec![],
        static_type_bindings: vec![],
        required_laws: recipe
            .context
            .execution_context()
            .laws
            .values()
            .map(|law| BlueprintRevisionId::new(law).map_err(str::to_owned))
            .collect::<Result<_, _>>()?,
        ports: vec![],
        behavior_bindings: vec![],
        provenance: Provenance {
            author: "DustRoute typed flying-machine recipes".into(),
            source_url: None,
            license: Some("Apache-2.0".into()),
            retrieved_on: None,
        },
    };
    let mut candidate = original.clone();
    candidate.id = id("machine.v2")?;
    candidate.parents = vec![original.id.clone()];
    candidate.name = "Generated single-launch flight".into();
    candidate.ports = ports;
    candidate.behavior_bindings = vec![binding];
    let mut parent = original.clone();
    parent.id = id("parent.v1")?;
    parent.name = "Flight course".into();
    parent.blocks.clear();
    parent.required_laws.clear();
    parent.inclusions = vec![include("machine", original.id.clone())?];
    let mut next_parent = parent.clone();
    next_parent.id = id("parent.v2")?;
    next_parent.parents = vec![parent.id.clone()];
    next_parent.inclusions[0].revision = candidate.id.clone();
    assembly.instances = vec![include("root", parent.id.clone())?];
    let base = AssemblyRevision {
        id: state_id("state.v1")?,
        parents: vec![],
        assembly,
    };
    let mut state = base.clone();
    state.id = state_id("state.v2")?;
    state.parents = vec![base.id.clone()];
    state.assembly.instances[0].revision = next_parent.id.clone();
    let request=BlueprintUpdateRequest {id:BlueprintUpdateId::new(format!("{}.proposal.v1",spec.namespace)).map_err(str::to_owned)?,
        title:format!("Generate {:?}/{:?}, {}-block finite flight",spec.engine,spec.body,spec.distance),
        description:"Explicit generated layout and arrival requirement; no automatic import, adoption or world placement.".into(),
        base_state:base.id.clone(),base_parent:parent.id.clone(),candidate_parent:next_parent.id.clone(),
        parent_instance:vec![InstanceId::new("root").map_err(str::to_owned)?],
        child_before:vec![InstanceId::new("machine").map_err(str::to_owned)?],previous_child:original.id.clone(),
        child_after:vec![InstanceId::new("machine").map_err(str::to_owned)?],next_child:candidate.id.clone(),
        revisions:vec![next_parent,candidate],candidate_state:state,behavior_context:Some(recipe.context.clone().into())};
    // Include pinned Law definitions so exported records also work in a clean catalog.
    catalog
        .insert_revisions(vec![parent, original])
        .map_err(|e| e.to_string())?;
    catalog
        .insert_assembly(base.clone())
        .map_err(|e| e.to_string())?;
    let records = BlueprintRecords {
        types: catalog.type_revisions().cloned().collect(),
        classifications: catalog.classifications().cloned().collect(),
        revisions: catalog.revisions().cloned().collect(),
        assemblies: vec![base],
    };
    Ok((records, request))
}
