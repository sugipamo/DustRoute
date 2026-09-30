//! Common immutable Assembly wrapping, transform and terminal propagation.
use super::design::{AttachedDesignComponent, BuildingDesignError};
use super::design_geometry::{contains_region, interior, intersects, volume};
use super::geometry::Geometry;
use crate::assembly_transform::AssemblyTransform;
use crate::blueprint_update::BlueprintUpdateRequest;
use crate::piston_construction::electrical_snapshot;
use dustroute_library::Provenance;
use dustroute_library::assembly::{Assembly, AssemblyPortRef, BlueprintGrouping};
use dustroute_library::blueprint::*;
use dustroute_library::building::{BuildingDesignComponent, BuildingDesignRequest};
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_minecraft::{BlockKind, Pos, Region};

pub(super) struct PreparedComponent {
    source_catalog: BlueprintCatalog,
    original: Assembly,
    moved: Assembly,
    pub context: RuntimeBehaviorContext,
    pub reserved: Region,
    transform: AssemblyTransform,
    specification: BuildingDesignComponent,
}

pub(super) fn prepare(
    component: &BuildingDesignComponent,
    source: Option<(&BlueprintCatalog, &RuntimeBehaviorContext)>,
    request: &BuildingDesignRequest,
    geometry: &Geometry,
) -> Result<PreparedComponent, BuildingDesignError> {
    let error =
        |detail| BuildingDesignError::new("invalid_component", detail).at(&component.name, None);
    let (catalog, context) =
        source.ok_or_else(|| error("source catalog and runtime context are required".into()))?;
    let source = catalog
        .assembly(&component.assembly_revision_id)
        .ok_or_else(|| error("source Assembly Revision is missing".into()))?;
    volume(context.known_region)?;
    volume(component.reserved_space).map_err(|e| e.at(&component.name, None))?;
    if !contains_region(context.known_region, component.reserved_space)
        || source.assembly.blocks.iter().any(|b| {
            b.block.kind != BlockKind::Air && !component.reserved_space.contains(b.position)
        })
        || context
            .input_levers
            .iter()
            .any(|p| !component.reserved_space.contains(*p))
    {
        return Err(error(
            "reserved_space must be known and contain every mechanism cell and input".into(),
        ));
    }
    let mut original = source.assembly.clone();
    if !component.exports.is_empty() {
        original.boundaries = component.exports.clone();
    }
    if original.boundaries.len() > 64 {
        return Err(error("at most 64 component exports are admitted".into()));
    }
    let view = original
        .inspect(catalog)
        .map_err(|e| error(e.to_string()))?;
    for boundary in &original.boundaries {
        let (port, _) = view
            .resolved_port(&boundary.port)
            .map_err(|e| error(e.to_string()))?;
        if !component.reserved_space.contains(port.position) {
            return Err(error(format!(
                "export {} is outside reserved_space",
                boundary.name
            )));
        }
    }
    let transform = AssemblyTransform {
        source_anchor: component.source_anchor,
        target_anchor: component.target_anchor,
        rotation: component.rotation,
    };
    let reserved = transform.region(component.reserved_space).map_err(error)?;
    let (moved, mut context) = transform.apply(&original, context).map_err(error)?;
    if !contains_region(request.known_region, context.known_region)
        || !interior(request.known_region, reserved.min)
        || !interior(request.known_region, reserved.max)
    {
        return Err(error("transformed source context must fit known_region, and reserved_space must retain the outer air guard".into()));
    }
    if let Some(b) = geometry
        .initial
        .blocks
        .iter()
        .find(|b| reserved.contains(b.pos))
    {
        return Err(BuildingDesignError::new(
            "component_collision",
            "a building part occupies component motion space; add an explicit part cutout",
        )
        .at(&component.name, Some(b.pos)));
    }
    if let Some(s) = request
        .spaces
        .iter()
        .find(|s| intersects(s.region, reserved))
    {
        return Err(error(format!(
            "permanent air space {} overlaps dynamic reserved_space",
            s.name
        )));
    }
    let count = geometry.initial.blocks.len()
        + moved
            .blocks
            .iter()
            .filter(|b| b.block.kind != BlockKind::Air)
            .count();
    if count > 256 {
        return Err(BuildingDesignError::new(
            "block_budget",
            "combined design exceeds 256 non-air cells",
        ));
    }
    context.known_region = request.known_region;
    Ok(PreparedComponent {
        source_catalog: catalog.clone(),
        original,
        moved,
        context,
        reserved,
        transform,
        specification: component.clone(),
    })
}

pub(super) fn attach(
    namespace: &str,
    prepared: PreparedComponent,
    records: &mut BlueprintRecords,
    proposal: &mut BlueprintUpdateRequest,
    geometry: &mut Geometry,
) -> Result<AttachedDesignComponent, BuildingDesignError> {
    let c = &prepared.specification;
    let error = |detail| BuildingDesignError::new("invalid_component", detail).at(&c.name, None);
    let mut catalog = prepared.source_catalog.clone();
    merge_records(&mut catalog, records).map_err(error)?;
    let (wrapper, _) = prepared
        .original
        .group_as_blueprint(
            &prepared.source_catalog,
            BlueprintGrouping {
                id: BlueprintRevisionId::new(format!("{namespace}.component.{}.v1", c.name))
                    .map_err(|e| error(e.into()))?,
                name: format!("Pinned equipment: {}", c.name),
                classifications: vec![],
                provenance: Provenance {
                    author: "DustRoute typed building composition".into(),
                    source_url: None,
                    license: Some("Apache-2.0".into()),
                    retrieved_on: None,
                },
            },
        )
        .map_err(|e| error(e.to_string()))?;
    if catalog.revision(&wrapper.id).is_some() {
        return Err(error("component namespace already exists".into()));
    }
    let instance = InstanceId::new(format!("component.{}", c.name)).map_err(|e| error(e.into()))?;
    let ports = attach_assembly(
        proposal,
        &wrapper,
        prepared.moved,
        prepared.transform,
        &instance,
        &format!("{}.", c.name),
    )
    .map_err(error)?;
    proposal.revisions.push(wrapper);
    *records = BlueprintRecords {
        types: catalog.type_revisions().cloned().collect(),
        classifications: catalog.classifications().cloned().collect(),
        revisions: catalog.revisions().cloned().collect(),
        assemblies: catalog.assemblies().cloned().collect(),
    };
    catalog
        .insert_revisions(proposal.revisions.clone())
        .map_err(|e| error(e.to_string()))?;
    geometry.initial = electrical_snapshot(
        &proposal
            .candidate_state
            .assembly
            .inspect(&catalog)
            .map_err(|e| error(e.to_string()))?
            .proposed_world(),
        geometry.region,
    )
    .map_err(error)?;
    Ok(AttachedDesignComponent {
        name: c.name.clone(),
        source_assembly_revision: c.assembly_revision_id.clone(),
        origin: prepared.transform.position(Pos::default()).map_err(error)?,
        reserved_space: prepared.reserved,
        terminals: ports.into_iter().map(|p| (p.name, p.position)).collect(),
    })
}

/// Preserve nested requirements/routes and expose transformed aliases. The
/// wrapper stays in source coordinates; its inclusion carries the transform.
pub(super) fn attach_assembly(
    proposal: &mut BlueprintUpdateRequest,
    wrapper: &BlueprintRevision,
    moved: Assembly,
    transform: AssemblyTransform,
    instance: &InstanceId,
    prefix: &str,
) -> Result<Vec<BlueprintPort>, String> {
    let building = InstanceId::new("building").map_err(str::to_owned)?;
    let root = InstanceId::new("root").map_err(str::to_owned)?;
    let mut ports = wrapper.ports.clone();
    let mut bindings = Vec::new();
    for port in &mut ports {
        let source_name = port.name.clone();
        port.name = format!("{prefix}{}", port.name);
        port.position = transform.position(port.position)?;
        port.facing = port.facing.map(|f| transform.rotation.facing(f));
        bindings.push(BlueprintPortBinding {
            name: port.name.clone(),
            port: AssemblyPortRef {
                instance: vec![instance.clone()],
                port: source_name,
            },
        });
    }
    let body = proposal
        .revisions
        .iter_mut()
        .find(|r| r.id == proposal.next_child)
        .ok_or("building body is missing")?;
    body.inclusions.push(BlueprintInclusion {
        instance: instance.clone(),
        revision: wrapper.id.clone(),
        origin: transform.position(Pos::default())?,
        rotation: transform.rotation,
    });
    body.ports.extend(ports.clone());
    body.port_bindings.extend(bindings);
    let parent = proposal
        .revisions
        .iter_mut()
        .find(|r| r.id == proposal.candidate_parent)
        .ok_or("building parent is missing")?;
    parent.ports.extend(ports.clone());
    parent.port_bindings.extend(aliases(&ports, &building));
    let assembly = &mut proposal.candidate_state.assembly;
    assembly.blocks.extend(moved.blocks);
    assembly.boundaries.extend(aliases(&ports, &root));
    for mut connection in moved.connections {
        for endpoint in [&mut connection.source, &mut connection.sink] {
            endpoint
                .instance
                .splice(0..0, [root.clone(), building.clone(), instance.clone()]);
        }
        assembly.connections.push(connection);
    }
    Ok(ports)
}

pub(super) fn aliases(ports: &[BlueprintPort], instance: &InstanceId) -> Vec<BlueprintPortBinding> {
    ports
        .iter()
        .map(|p| BlueprintPortBinding {
            name: p.name.clone(),
            port: BlueprintPortRef {
                instance: vec![instance.clone()],
                port: p.name.clone(),
            },
        })
        .collect()
}

pub(super) fn merge_records(
    catalog: &mut BlueprintCatalog,
    records: &BlueprintRecords,
) -> Result<(), String> {
    for t in &records.types {
        match catalog.type_revision(&t.id) {
            Some(existing) if existing == t => {}
            Some(_) => return Err("building namespace collides with an existing type".into()),
            None => catalog.insert_type(t.clone()).map_err(|e| e.to_string())?,
        }
    }
    for r in &records.revisions {
        if catalog
            .revision(&r.id)
            .is_some_and(|existing| existing != r)
        {
            return Err("building namespace collides with an existing definition".into());
        }
    }
    catalog
        .insert_revisions(
            records
                .revisions
                .iter()
                .filter(|r| catalog.revision(&r.id).is_none())
                .cloned()
                .collect(),
        )
        .map_err(|e| e.to_string())?;
    for a in &records.assemblies {
        if catalog.assembly(&a.id).is_some() {
            return Err("building namespace collides with an existing Assembly".into());
        }
        catalog
            .insert_assembly(a.clone())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
