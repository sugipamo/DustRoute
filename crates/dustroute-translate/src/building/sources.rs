//! Immutable building parts and explicit structural obligations.
use super::geometry::Geometry;
use crate::blueprint_update::BlueprintUpdateRequest;
use crate::snapshot::assembly_from_snapshot;
use crate::world::{Block, BlockKind, Pos, RotationY};
use dustroute_library::assembly::{Assembly, AssemblyRevision};
use dustroute_library::blueprint::*;
use dustroute_library::building::BuildingRequest;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_library::{PortDirection, Provenance};

pub(super) fn build(
    spec: &BuildingRequest,
    geometry: &Geometry,
    context: &RuntimeBehaviorContext,
) -> Result<(BlueprintRecords, BlueprintUpdateRequest), String> {
    let id = |suffix: &str| {
        BlueprintRevisionId::new(format!("{}.{suffix}", spec.namespace)).map_err(str::to_owned)
    };
    let state_id = |suffix: &str| {
        AssemblyRevisionId::new(format!("{}.{suffix}", spec.namespace)).map_err(str::to_owned)
    };
    let include = |name: &str, revision| {
        Ok::<_, String>(BlueprintInclusion {
            instance: InstanceId::new(name).map_err(str::to_owned)?,
            revision,
            origin: Pos::default(),
            rotation: RotationY::R0,
        })
    };
    let mut catalog = dustroute_library::builtin_laws::builtin_laws().clone();
    let mut assembly = assembly_from_snapshot(
        &geometry.initial,
        "Generated enclosure",
        vec![geometry.region],
    )
    .map_err(|e| e.to_string())?;
    let template = BlueprintRevision {
        id: id("empty.v1")?,
        parents: vec![],
        name: "Unadopted building component".into(),
        classifications: vec![],
        behavior_bindings: vec![],
        static_type_bindings: vec![],
        required_laws: vec![],
        blocks: vec![],
        initial_layout: None,
        law: None,
        inclusions: vec![],
        ports: vec![],
        connections: vec![],
        port_bindings: vec![],
        provenance: Provenance {
            author: "DustRoute typed building authoring".into(),
            source_url: None,
            license: Some("Apache-2.0".into()),
            retrieved_on: None,
        },
    };
    let mut original = template.clone();
    original.id = id("building.v1")?;
    let mut parent = template.clone();
    parent.id = id("parent.v1")?;
    parent.name = "Building site".into();
    parent.inclusions = vec![include("building", original.id.clone())?];
    let base = AssemblyRevision {
        id: state_id("state.v1")?,
        parents: vec![],
        assembly: Assembly {
            name: "Unbuilt enclosure site".into(),
            instances: vec![include("root", parent.id.clone())?],
            blocks: vec![],
            known_regions: vec![geometry.region],
            connections: vec![],
            boundaries: vec![],
        },
    };
    let mut revisions = Vec::new();
    let mut body = template.clone();
    body.id = id("building.v2")?;
    body.parents = vec![original.id.clone()];
    body.name = "Exact enclosure and clearance".into();
    body.required_laws = context
        .execution_context()
        .laws
        .values()
        .map(|name| BlueprintRevisionId::new(name).map_err(str::to_owned))
        .collect::<Result<_, _>>()?;
    for (name, blocks) in &geometry.parts {
        let part_snapshot = crate::snapshot::MinecraftSnapshot {
            min: geometry.region.min,
            max: geometry.region.max,
            blocks: blocks.clone(),
        };
        let part = assembly_from_snapshot(&part_snapshot, *name, vec![geometry.region])
            .map_err(|e| e.to_string())?;
        let mut definition = template.clone();
        definition.id = id(&format!("{name}.v1"))?;
        definition.name = (*name).into();
        definition.blocks = part.blocks;
        let anchor = definition.blocks[0].position;
        let pattern = definition
            .blocks
            .iter()
            .map(|b| PositionedBlock {
                position: Pos::new(
                    b.position.x - anchor.x,
                    b.position.y - anchor.y,
                    b.position.z - anchor.z,
                ),
                block: b.block.clone(),
            })
            .collect();
        require_pattern(
            &mut catalog,
            &mut definition,
            &spec.namespace,
            &format!("{name}.pattern.v1"),
            anchor,
            pattern,
        )?;
        body.inclusions.push(include(name, definition.id.clone())?);
        revisions.push(definition);
    }
    let world = assembly
        .inspect(&catalog)
        .map_err(|e| e.to_string())?
        .proposed_world();
    let mut pattern = Vec::new();
    let r = geometry.region;
    for x in r.min.x..=r.max.x {
        for y in r.min.y..=r.max.y {
            for z in r.min.z..=r.max.z {
                let position = Pos::new(x, y, z);
                pattern.push(PositionedBlock {
                    position,
                    block: world
                        .get(position)
                        .cloned()
                        .unwrap_or_else(|| Block::new(BlockKind::Air)),
                });
            }
        }
    }
    require_pattern(
        &mut catalog,
        &mut body,
        &spec.namespace,
        "clearance.pattern.v1",
        Pos::default(),
        pattern,
    )?;
    let mut next_parent = parent.clone();
    next_parent.id = id("parent.v2")?;
    next_parent.parents = vec![parent.id.clone()];
    next_parent.inclusions[0].revision = body.id.clone();
    assembly.instances = vec![include("root", next_parent.id.clone())?];
    let state = AssemblyRevision {
        id: state_id("state.v2")?,
        parents: vec![base.id.clone()],
        assembly,
    };
    let next_child = body.id.clone();
    revisions.extend([body, next_parent.clone()]);
    let request = BlueprintUpdateRequest {
        id: BlueprintUpdateId::new(format!("{}.proposal.v1", spec.namespace))
            .map_err(str::to_owned)?,
        title: format!(
            "Build {} by {} by {} enclosure",
            spec.width, spec.height, spec.depth
        ),
        description: concat!(
            "Exact building geometry, passage and one-block air perimeter; ",
            "only a completely observed empty target site is admitted. ",
            "No automatic publication or world writes."
        )
        .into(),
        base_state: base.id.clone(),
        base_parent: parent.id.clone(),
        candidate_parent: next_parent.id,
        parent_instance: vec![InstanceId::new("root").map_err(str::to_owned)?],
        child_before: vec![InstanceId::new("building").map_err(str::to_owned)?],
        previous_child: original.id.clone(),
        child_after: vec![InstanceId::new("building").map_err(str::to_owned)?],
        next_child,
        revisions,
        candidate_state: state,
        behavior_context: Some(context.clone().into()),
    };
    catalog
        .insert_revisions(vec![original, parent])
        .map_err(|e| e.to_string())?;
    catalog
        .insert_assembly(base.clone())
        .map_err(|e| e.to_string())?;
    Ok((
        BlueprintRecords {
            types: catalog.type_revisions().cloned().collect(),
            classifications: vec![],
            revisions: catalog.revisions().cloned().collect(),
            assemblies: vec![base],
        },
        request,
    ))
}

fn require_pattern(
    catalog: &mut BlueprintCatalog,
    revision: &mut BlueprintRevision,
    namespace: &str,
    suffix: &str,
    anchor: Pos,
    blocks: Vec<PositionedBlock>,
) -> Result<(), String> {
    let pattern = TypeRevision {
        id: TypeRevisionId::new(format!("{namespace}.{suffix}")).map_err(str::to_owned)?,
        name: revision.name.clone(),
        contract: TypeContract::BlockPattern { blocks },
    };
    revision.ports = vec![BlueprintPort {
        name: "layout".into(),
        position: anchor,
        direction: PortDirection::Output,
        kind: BlueprintPortKind::BlockState,
        facing: None,
        required_source_types: vec![],
    }];
    revision.static_type_bindings = vec![StaticTypeBinding {
        type_revision: pattern.id.clone(),
        port: "layout".into(),
    }];
    catalog.insert_type(pattern).map_err(|e| e.to_string())
}
