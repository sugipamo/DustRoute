//! A typed mechanism remains an immutable child; all proof uses the combined
//! actual world. Reserved motion space is a request, never a behavior certificate.
use super::component::{attach_assembly, merge_records};
use super::door_interface::{align_aperture, door_boundaries};
use super::{BuildingDesignError, GeneratedBuilding, finish, geometry, sources};
use crate::location_behavior::LocationBehaviorBinding;
use crate::piston_construction::electrical_snapshot;
use crate::runtime_behavior::RuntimeBehaviorModel;
use dustroute_library::Provenance;
use dustroute_library::assembly::BlueprintGrouping;
use dustroute_library::blueprint::*;
use dustroute_library::building::BuildingWithDoorRequest;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_minecraft::{BlockKind, Pos, Region, RotationY};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct AttachedBuildingDoor {
    pub source_assembly_revision: AssemblyRevisionId,
    pub source_instance: InstancePath,
    pub behavior_type: TypeRevisionId,
    pub origin: Pos,
    pub rotation: RotationY,
    /// Component-owned motion space in building coordinates.
    pub reserved_space: Region,
    pub control: Pos,
    pub closed_when_powered: bool,
    pub aperture: Vec<Pos>,
}

/// Author a bounded enclosure around a selected immutable door Assembly.
/// The MCP caller additionally requires unique adoption of the source. Neither
/// that adoption nor an isolated door pass is reused as proof of composition.
pub fn generate_building_with_door(
    request: BuildingWithDoorRequest,
    source_catalog: &BlueprintCatalog,
    source_context: &RuntimeBehaviorContext,
) -> Result<GeneratedBuilding, BuildingDesignError> {
    let specification = request.building;
    let attachment = request.door;
    let (mut geometry, entrance) = geometry::expand(&specification)?;
    if entrance.width != 3 || entrance.height != 3 {
        return Err("a typed 3x3 door requires an explicit 3-wide, 3-high entrance".into());
    }
    let source = source_catalog
        .assembly(&attachment.assembly_revision_id)
        .ok_or("door Assembly Revision is missing")?;
    let view = source
        .assembly
        .inspect(source_catalog)
        .map_err(|e| e.to_string())?;
    let occurrence = view
        .occurrences
        .get(&attachment.instance)
        .ok_or("door occurrence is missing")?;
    let definition = source_catalog
        .revision(&occurrence.revision)
        .ok_or("door definition is missing")?;
    let bindings = definition
        .behavior_bindings
        .iter()
        .filter(|b| b.behavior_type() == &attachment.behavior_type)
        .collect::<Vec<_>>();
    let [binding] = bindings.as_slice() else {
        return Err("door occurrence must declare exactly one selected behavioral binding".into());
    };
    let TypeContract::PistonDoor { requirement } = &source_catalog
        .type_revision(&attachment.behavior_type)
        .ok_or("door behavioral type is missing")?
        .contract
    else {
        return Err("building attachment requires a PistonDoor behavioral type".into());
    };
    let resolved = LocationBehaviorBinding::resolve(
        source_catalog,
        &source.assembly,
        &attachment.instance,
        binding,
    )?;
    resolved.validate_door(requirement)?;
    RuntimeBehaviorModel::from_fresh_assembly(
        source_catalog,
        &source.assembly,
        &attachment.instance,
        binding,
        source_context,
    )?;
    let aperture = requirement
        .aperture
        .iter()
        .flatten()
        .map(|c| resolved.outputs[&c.air].position)
        .collect::<Vec<_>>();
    let transform = align_aperture(&aperture, attachment.rotation, i32::from(entrance.offset))?;
    bounded_region(attachment.reserved_space)?;
    if !source_context
        .known_region
        .contains(attachment.reserved_space.min)
        || !source_context
            .known_region
            .contains(attachment.reserved_space.max)
        || source.assembly.blocks.iter().any(|b| {
            b.block.kind != BlockKind::Air && !attachment.reserved_space.contains(b.position)
        })
        || resolved
            .inputs
            .values()
            .chain(resolved.outputs.values())
            .any(|p| !attachment.reserved_space.contains(p.position))
    {
        return Err(
            "reserved door space must be known and contain all mechanism cells and terminals"
                .into(),
        );
    }
    let reserved = transform.region(attachment.reserved_space)?;
    let wall_top = i32::from(specification.height)
        - 1
        - i32::from(matches!(
            specification.roof,
            dustroute_library::building::BuildingRoof::Flat
        ));
    if reserved.min.z != 0
        || reserved.max.z != 0
        || reserved.min.x <= 0
        || reserved.max.x >= i32::from(specification.width) - 1
        || reserved.max.y > wall_top
    {
        return Err("door motion space must be one block deep at z=0, between north-wall corners and below the roof".into());
    }
    let (moved, mut context) = transform.apply(&source.assembly, source_context)?;
    geometry.region = enclosing_region(geometry.region, context.known_region);
    bounded_region(geometry.region)?;
    context.known_region = geometry.region;
    geometry.reserved.push(reserved);
    for (_, cells) in &mut geometry.parts {
        cells.retain(|b| !reserved.contains(b.pos));
    }
    geometry.initial.min = geometry.region.min;
    geometry.initial.max = geometry.region.max;
    geometry
        .initial
        .blocks
        .retain(|b| !reserved.contains(b.pos));
    let (mut records, mut proposal) = sources::build(&specification, &geometry, &context)?;
    let mut catalog = source_catalog.clone();
    merge_records(&mut catalog, &records)?;
    let (boundaries, closed_when_powered) =
        door_boundaries(&attachment.instance, binding, requirement, &view)?;
    let mut component = source.assembly.clone();
    component.boundaries = boundaries;
    let (wrapper, _) = component
        .group_as_blueprint(
            source_catalog,
            BlueprintGrouping {
                id: BlueprintRevisionId::new(format!(
                    "{}.door.component.v1",
                    specification.namespace
                ))
                .map_err(str::to_owned)?,
                name: "Pinned door Assembly with explicit building terminals".into(),
                classifications: vec![],
                provenance: Provenance {
                    author: "DustRoute typed building composition".into(),
                    source_url: None,
                    license: Some("Apache-2.0".into()),
                    retrieved_on: None,
                },
            },
        )
        .map_err(|e| e.to_string())?;
    if catalog.revision(&wrapper.id).is_some() {
        return Err("building namespace collides with an existing door component".into());
    }
    let door_instance = InstanceId::new("door").map_err(str::to_owned)?;
    let ports = attach_assembly(
        &mut proposal,
        &wrapper,
        moved,
        transform,
        &door_instance,
        "",
    )?;
    proposal.candidate_state.assembly.name = "Enclosure with typed piston door".into();
    proposal.title = format!(
        "Build {} by {} by {} enclosure with a typed 3x3 door",
        specification.width, specification.height, specification.depth
    );
    proposal.description = "Preserve the selected immutable door and all its retained requirements. Carve only the declared component space; require exact fixed structure and clearance outside it throughout a fresh combined physical proof. No automatic adoption or live-world writes.".into();
    proposal.revisions.push(wrapper);
    records = BlueprintRecords {
        types: catalog.type_revisions().cloned().collect(),
        classifications: catalog.classifications().cloned().collect(),
        revisions: catalog.revisions().cloned().collect(),
        assemblies: catalog.assemblies().cloned().collect(),
    };
    catalog
        .insert_revisions(proposal.revisions.clone())
        .map_err(|e| e.to_string())?;
    geometry.initial = electrical_snapshot(
        &proposal
            .candidate_state
            .assembly
            .inspect(&catalog)
            .map_err(|e| e.to_string())?
            .proposed_world(),
        geometry.region,
    )?;
    let door_cells = source
        .assembly
        .blocks
        .iter()
        .filter(|b| b.block.kind != BlockKind::Air)
        .count();
    // Counts remain disjoint: reserved cells were removed from shell parts.
    let info = AttachedBuildingDoor {
        source_assembly_revision: attachment.assembly_revision_id,
        source_instance: attachment.instance,
        behavior_type: attachment.behavior_type,
        origin: transform.target_anchor,
        rotation: transform.rotation,
        reserved_space: reserved,
        control: ports
            .iter()
            .find(|p| p.name == "door_control")
            .ok_or("missing control alias")?
            .position,
        closed_when_powered,
        aperture: aperture
            .into_iter()
            .map(|p| transform.position(p))
            .collect::<Result<_, _>>()?,
    };
    let mut generated = finish(
        specification,
        entrance,
        geometry,
        records,
        proposal,
        context,
        Some(info),
    )?;
    generated.parts.insert("door".into(), door_cells);
    Ok(generated)
}

fn enclosing_region(a: Region, b: Region) -> Region {
    Region::new(
        Pos::new(
            a.min.x.min(b.min.x),
            a.min.y.min(b.min.y),
            a.min.z.min(b.min.z),
        ),
        Pos::new(
            a.max.x.max(b.max.x),
            a.max.y.max(b.max.y),
            a.max.z.max(b.max.z),
        ),
    )
}

fn bounded_region(region: Region) -> Result<(), String> {
    let dimensions = [
        i64::from(region.max.x) - i64::from(region.min.x) + 1,
        i64::from(region.max.y) - i64::from(region.min.y) + 1,
        i64::from(region.max.z) - i64::from(region.min.z) + 1,
    ];
    if dimensions.iter().any(|d| *d <= 0)
        || dimensions
            .into_iter()
            .try_fold(1_i64, i64::checked_mul)
            .is_none_or(|v| v > 8192)
    {
        return Err(
            "door building requires valid rectangular regions with at most 8192 known cells".into(),
        );
    }
    Ok(())
}
