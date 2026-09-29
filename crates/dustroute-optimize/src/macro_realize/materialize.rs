//! Build a patch and Assembly while retaining source ownership.
use super::ownership::replacement_ownership;
use super::routing::repeater_sites;
use super::structure::validate_macro_structure;
use super::{
    MacroBoundaryDirection, MacroRealizationError, MacroReplacementPlan,
    MaterializedMacroReplacement,
};
use dustroute_library::assembly::Assembly;
use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintInclusion, InstanceId, InstancePath, PositionedBlock,
};
use dustroute_physical::{
    Block, BlockKind, PhysicalBlockChange, PhysicalPatch, PhysicalPatchReason, Pos, World,
};
use std::collections::BTreeSet;

struct MacroGeometry {
    world: World,
    patch: PhysicalPatch,
    added_supports: Vec<Pos>,
    inserted_repeaters: Vec<Pos>,
}

/// Converts a structurally valid skeleton into a virtual world and an exact
/// reversible patch. The observed world itself is never mutated.
/// This compatibility entry point only knows the occupied bounding box. Use
/// `materialize_macro_replacement_in_known_regions` to retain a real scan's air
/// coverage, or the Assembly entry point for existing typed interpretations.
pub fn materialize_macro_replacement(
    plan: &MacroReplacementPlan,
    observed: &World,
    replaceable: &BTreeSet<Pos>,
    max_wire_run: usize,
) -> Result<MaterializedMacroReplacement, MacroRealizationError> {
    let known_regions = observed
        .bounds()
        .map(|(min, max)| dustroute_translate::Region::new(min, max))
        .into_iter()
        .collect::<Vec<_>>();
    materialize_macro_replacement_in_known_regions(
        plan,
        observed,
        &known_regions,
        replaceable,
        max_wire_run,
    )
}

/// Retains caller-declared observed or modeled coverage. It never enlarges the
/// region to make a candidate pass or treats absent cells beyond it as air.
pub fn materialize_macro_replacement_in_known_regions(
    plan: &MacroReplacementPlan,
    observed: &World,
    known_regions: &[dustroute_translate::Region],
    replaceable: &BTreeSet<Pos>,
    max_wire_run: usize,
) -> Result<MaterializedMacroReplacement, MacroRealizationError> {
    let revision = plan.placed.cell.source_revision.clone().ok_or_else(|| {
        MacroRealizationError::Blueprint("replacement has no source revision".into())
    })?;
    let context = Assembly {
        name: "Macro replacement candidate".into(),
        instances: vec![BlueprintInclusion {
            instance: InstanceId::new("replacement").expect("static instance ID"),
            revision,
            origin: plan.placed.origin,
            rotation: plan.placed.rotation,
        }],
        blocks: observed
            .iter()
            .map(|(position, block)| PositionedBlock {
                position: *position,
                block: block.clone(),
            })
            .collect(),
        known_regions: known_regions.to_vec(),
        connections: vec![],
        boundaries: vec![],
    };
    materialize_macro_replacement_in_assembly(
        plan,
        &plan.source_catalog,
        &context,
        &vec![InstanceId::new("replacement").expect("static instance ID")],
        replaceable,
        max_wire_run,
    )
}

/// Realizes against an explicitly supplied candidate structure. It must already
/// pin the selected replacement and retain affected parents/shared occurrences;
/// this function never rewrites source definitions or invents parent revisions.
/// External typed inputs need explicit upstream occurrences and actual routes.
pub fn materialize_macro_replacement_in_assembly(
    plan: &MacroReplacementPlan,
    catalog: &BlueprintCatalog,
    context: &Assembly,
    occurrence: &InstancePath,
    replaceable: &BTreeSet<Pos>,
    max_wire_run: usize,
) -> Result<MaterializedMacroReplacement, MacroRealizationError> {
    for source in plan.source_catalog.revisions() {
        if catalog.revision(&source.id) != Some(source) {
            return Err(MacroRealizationError::Blueprint(
                "source catalog differs from the planned revision".into(),
            ));
        }
    }
    for definition in plan.source_catalog.type_revisions() {
        if catalog.type_revision(&definition.id) != Some(definition) {
            return Err(MacroRealizationError::Blueprint(
                "source type differs from the planned definition".into(),
            ));
        }
    }
    for definition in plan.source_catalog.classifications() {
        if catalog.classification(&definition.id) != Some(definition) {
            return Err(MacroRealizationError::Blueprint(
                "classification differs from the planned definition".into(),
            ));
        }
    }
    let view = context
        .inspect(catalog)
        .map_err(|error| MacroRealizationError::Blueprint(error.to_string()))?;
    let selected = view
        .occurrences
        .get(occurrence)
        .ok_or_else(|| MacroRealizationError::Blueprint("missing replacement occurrence".into()))?;
    if Some(&selected.revision) != plan.placed.cell.source_revision.as_ref()
        || selected.origin != plan.placed.origin
        || selected.rotation != plan.placed.rotation
    {
        return Err(MacroRealizationError::Blueprint(
            "replacement occurrence does not match the plan".into(),
        ));
    }
    let observed = view.proposed_world();
    let materialized = materialize_macro_geometry(plan, &observed, replaceable, max_wire_run)?;
    let mut assembly = context.clone();
    assembly.blocks = materialized
        .world
        .iter()
        .map(|(position, block)| PositionedBlock {
            position: *position,
            block: block.clone(),
        })
        .collect();
    // Preserve explicit known air outside the cuboid, and record cleared
    // positions. Absence elsewhere remains unknown.
    let explicit_air = context
        .blocks
        .iter()
        .filter(|record| record.block.kind == BlockKind::Air)
        .map(|record| record.position)
        .chain(replaceable.iter().copied())
        .collect::<BTreeSet<_>>();
    for position in explicit_air {
        if materialized.world.kind_at(position) == BlockKind::Air {
            assembly.blocks.push(PositionedBlock {
                position,
                block: Block::new(BlockKind::Air),
            });
        }
    }
    let assembly = assembly
        .with_source_connections(catalog)
        .map_err(|error| MacroRealizationError::Blueprint(error.to_string()))?;
    dustroute_translate::assembly::validate_assembly_occurrences(catalog, &assembly)
        .map_err(|error| MacroRealizationError::Blueprint(error.to_string()))?;
    Ok(MaterializedMacroReplacement {
        world: materialized.world,
        patch: materialized.patch,
        added_supports: materialized.added_supports,
        inserted_repeaters: materialized.inserted_repeaters,
        assembly,
    })
}

fn materialize_macro_geometry(
    plan: &MacroReplacementPlan,
    observed: &World,
    replaceable: &BTreeSet<Pos>,
    max_wire_run: usize,
) -> Result<MacroGeometry, MacroRealizationError> {
    let replaceable = replacement_ownership(plan, observed, replaceable);
    let structural = validate_macro_structure(plan, observed, &replaceable);
    if !structural.valid() {
        return Err(MacroRealizationError::StructurallyInvalid(Box::new(
            structural,
        )));
    }
    let mut world = observed.clone();
    for pos in &replaceable {
        world.remove(*pos);
    }
    for (pos, block) in plan.placed.blocks() {
        let fixed_boundary = plan
            .routes
            .iter()
            .any(|route| route.boundary.position == pos)
            && observed
                .get(pos)
                .is_some_and(|actual| actual.kind == block.kind);
        if !fixed_boundary {
            world.set(pos, block);
        }
    }
    for support in &structural.required_route_supports {
        if world.kind_at(*support) == BlockKind::Air {
            world.set(*support, Block::new(BlockKind::Solid));
        }
    }

    let mut inserted_repeaters = Vec::new();
    for (route_index, route) in plan.routes.iter().enumerate() {
        let mut repeaters = repeater_sites(&route.path, max_wire_run).ok_or(
            MacroRealizationError::NoRepeaterSite {
                route: route_index,
                after_steps: max_wire_run,
            },
        )?;
        if route.boundary.direction == MacroBoundaryDirection::Input {
            for facing in repeaters.values_mut() {
                *facing = facing.opposite();
            }
        }
        for (index, pos) in route
            .path
            .iter()
            .copied()
            .enumerate()
            .skip(1)
            .take(route.path.len().saturating_sub(2))
        {
            if let Some(facing) = repeaters.get(&index) {
                let repeater = world.place(BlockKind::Repeater, pos);
                repeater.facing = Some(*facing);
                repeater.delay = Some(1);
                inserted_repeaters.push(pos);
            } else {
                world.place(BlockKind::RedstoneWire, pos);
            }
        }
    }
    dustroute_translate::update_wire_shapes(&mut world);
    let positions = observed
        .positions()
        .chain(world.positions())
        .collect::<BTreeSet<_>>();
    let changes = positions
        .into_iter()
        .filter_map(|pos| {
            let before = observed
                .get(pos)
                .cloned()
                .unwrap_or_else(|| Block::new(BlockKind::Air));
            let after = world
                .get(pos)
                .cloned()
                .unwrap_or_else(|| Block::new(BlockKind::Air));
            (before != after).then_some(PhysicalBlockChange { pos, before, after })
        })
        .collect();
    Ok(MacroGeometry {
        world,
        patch: PhysicalPatch {
            reason: PhysicalPatchReason::OptimizePlacement,
            affected_fragments: Vec::new(),
            confidence_percent: 100,
            explanation: format!("replace focused implementation with {}", plan.component_id),
            changes,
        },
        added_supports: structural.required_route_supports,
        inserted_repeaters,
    })
}
