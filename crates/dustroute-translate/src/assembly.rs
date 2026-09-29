//! Capture composed physical state and revalidate saved assemblies.

use std::collections::BTreeSet;

use dustroute_library::PortDirection;
use dustroute_library::assembly::{
    Assembly, AssemblyBoundary, AssemblyConnection, AssemblyPortRef,
};
use dustroute_library::behavior_context::BehaviorReviewContext;
use dustroute_library::behavior_type::PhysicalBehaviorContext;
use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintError, BlueprintInclusion, InstanceId, PositionedBlock, TypeContract,
};

use crate::blueprint_connection::{BlueprintConnectionError, check_port_connection};
use crate::compiler::BaselineCompileResult;
use crate::multinet::MultiNetRouting;
use crate::multinet::{root_path, rooted_parent};
use crate::physical::PlacementCircuit;
use crate::port_realization::terminal_for_endpoint;
use crate::{world::Region, world::ValidatedWorld, world::World, world::WorldValidationError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssemblyValidationError {
    Blueprint(BlueprintError),
    World(WorldValidationError),
    Connection {
        index: usize,
        error: BlueprintConnectionError,
    },
    UnboundRequirement(AssemblyPortRef),
    UnboundSourceConnection {
        source: AssemblyPortRef,
        sink: AssemblyPortRef,
    },
    Review(Box<crate::promotion::PromotionReport>),
}

impl std::fmt::Display for AssemblyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AssemblyValidationError {}

/// Rechecks actual placement and every declared connection. Returning a world
/// proves initial placement only; no logical/temporal/live evidence is stored in
/// or restored from the archive. Source block-state differences are not errors
/// by themselves: explicit type requirements decide what must still hold.
pub fn validate_assembly(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
) -> Result<ValidatedWorld, AssemblyValidationError> {
    let world = validate_assembly_routes(catalog, assembly, false)?;
    let view = assembly
        .inspect(catalog)
        .map_err(AssemblyValidationError::Blueprint)?;
    if view.occurrences.values().any(|occurrence| {
        let source = catalog
            .revision(&occurrence.revision)
            .expect("indexed source");
        !source.behavior_bindings.is_empty()
            || !source.static_type_bindings.is_empty()
            || !source.required_laws.is_empty()
    }) {
        let report = crate::promotion::review_assembly(catalog, assembly)
            .map_err(AssemblyValidationError::Blueprint)?;
        if report.status() != crate::promotion::CheckStatus::Passed {
            return Err(AssemblyValidationError::Review(Box::new(report)));
        }
    }
    Ok(world)
}

/// No saved proof is accepted. Geometry and every occurrence's requirements are
/// rechecked, then declared autonomous behavioral obligations run in the selected model.
pub fn validate_assembly_in_context(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
    context: Option<&PhysicalBehaviorContext>,
    budget: crate::behavior_type::BehaviorBudget,
) -> Result<ValidatedWorld, AssemblyValidationError> {
    let world = validate_assembly_routes(catalog, assembly, context.is_some())?;
    let report = crate::promotion::review_assembly_in_context(catalog, assembly, context, budget)
        .map_err(AssemblyValidationError::Blueprint)?;
    if report.status() != crate::promotion::CheckStatus::Passed {
        return Err(AssemblyValidationError::Review(Box::new(report)));
    }
    Ok(world)
}

/// A common fresh adoption gate, deliberately not a conversion to the old
/// fixed-placement proof type. The runtime branch must independently satisfy
/// its native initial gate and every retained obligation over execution.
pub(crate) fn validate_assembly_for_adoption(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
    context: Option<&BehaviorReviewContext>,
    budget: crate::behavior_type::BehaviorBudget,
) -> Result<(), AssemblyValidationError> {
    match context {
        None => validate_assembly_in_context(catalog, assembly, None, budget).map(|_| ()),
        Some(BehaviorReviewContext::FixedGeometry(context)) => {
            validate_assembly_in_context(catalog, assembly, Some(context), budget).map(|_| ())
        }
        Some(BehaviorReviewContext::Runtime(_)) => {
            let report =
                crate::promotion::review_assembly_with_context(catalog, assembly, context, budget)
                    .map_err(AssemblyValidationError::Blueprint)?;
            if report.status() != crate::promotion::CheckStatus::Passed {
                return Err(AssemblyValidationError::Review(Box::new(report)));
            }
            Ok(())
        }
    }
}

fn validate_assembly_routes(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
    defer_behavior_to_review: bool,
) -> Result<ValidatedWorld, AssemblyValidationError> {
    let view = assembly
        .inspect(catalog)
        .map_err(AssemblyValidationError::Blueprint)?;
    let raw_world = view.proposed_world();
    let issues = raw_world.placement_issues_with_lookup(|pos| view.block_at(pos));
    if !issues.is_empty() {
        return Err(AssemblyValidationError::World(WorldValidationError {
            issues,
        }));
    }
    let world = ValidatedWorld::try_from(raw_world).map_err(AssemblyValidationError::World)?;
    let mut connected = BTreeSet::new();
    let mut connected_pairs = BTreeSet::new();
    for (index, connection) in assembly.connections.iter().enumerate() {
        let (source, source_rotation) = view
            .resolved_port(&connection.source)
            .map_err(AssemblyValidationError::Blueprint)?;
        let (mut sink, _) = view
            .resolved_port(&connection.sink)
            .map_err(AssemblyValidationError::Blueprint)?;
        if defer_behavior_to_review {
            sink.required_source_types.retain(|id| {
                !catalog.type_revision(id).is_some_and(|definition| {
                    matches!(
                        definition.contract,
                        TypeContract::RepeatedSettling { .. }
                            | TypeContract::Periodic { .. }
                            | TypeContract::FiniteBurst { .. }
                            | TypeContract::SingleOperation { .. }
                    )
                })
            });
        }
        check_port_connection(
            catalog,
            &world,
            &source,
            &sink,
            &connection.path,
            source_rotation,
            |position| view.block_at(position),
        )
        .map_err(|error| AssemblyValidationError::Connection { index, error })?;
        let key = view
            .connection_key(connection)
            .map_err(AssemblyValidationError::Blueprint)?;
        connected.insert(key.1.clone());
        connected_pairs.insert(key);
    }
    for edge in view.source_connections() {
        let key = view
            .connection_key(edge)
            .map_err(AssemblyValidationError::Blueprint)?;
        if !connected_pairs.contains(&key) {
            return Err(AssemblyValidationError::UnboundSourceConnection {
                source: key.0,
                sink: key.1,
            });
        }
    }
    // Unconnected consumers remain unresolved even if they are exposed as an
    // external boundary. No upstream requirement is silently certified.
    for (path, occurrence) in &view.occurrences {
        let source = catalog
            .revision(&occurrence.revision)
            .expect("indexed source exists");
        for port in &source.ports {
            if port.direction == PortDirection::Input && !port.required_source_types.is_empty() {
                let reference = AssemblyPortRef {
                    instance: path.clone(),
                    port: port.name.clone(),
                };
                let canonical = view
                    .canonical_port_ref(&reference)
                    .map_err(AssemblyValidationError::Blueprint)?;
                if !connected.contains(canonical) {
                    return Err(AssemblyValidationError::UnboundRequirement(reference));
                }
            }
        }
    }
    Ok(world)
}

/// Full initial-state gate for compiler/optimizer proposals. Ordinary route
/// validation is followed by independent parent/descendant/shared reviews.
/// No logical, temporal or live-world evidence is inferred here.
pub fn validate_assembly_occurrences(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
) -> Result<ValidatedWorld, AssemblyValidationError> {
    let world = validate_assembly(catalog, assembly)?;
    let report = crate::promotion::review_assembly(catalog, assembly)
        .map_err(AssemblyValidationError::Blueprint)?;
    if report.status() != crate::promotion::CheckStatus::Passed {
        return Err(AssemblyValidationError::Review(Box::new(report)));
    }
    Ok(world)
}

impl BaselineCompileResult {
    /// Captures the composed state using source references carried by cells.
    pub fn capture_assembly(&self, name: impl Into<String>) -> Result<Assembly, BlueprintError> {
        self.capture_assembly_in_catalog(
            dustroute_library::builtin_blueprints::builtin_blueprints(),
            name,
        )
    }

    pub fn capture_assembly_in_catalog(
        &self,
        catalog: &BlueprintCatalog,
        name: impl Into<String>,
    ) -> Result<Assembly, BlueprintError> {
        capture_placement_assembly(&self.physical, &self.routing, &self.world, name)?
            .with_source_connections(catalog)
    }
}

/// Captures a proposed placed/routed circuit, including moved, rotated or
/// replaced cells. Callers must validate the result against its source catalog.
/// Source identity is never guessed from a name or matching geometry.
pub fn capture_placement_assembly(
    circuit: &PlacementCircuit,
    routing: &MultiNetRouting,
    world: &World,
    name: impl Into<String>,
) -> Result<Assembly, BlueprintError> {
    let instance_id = |cell: crate::physical::CellId| {
        InstanceId::new(format!("cell-{}", cell.0)).expect("valid generated instance ID")
    };
    let reference =
        |endpoint: &crate::physical::Endpoint| -> Result<AssemblyPortRef, BlueprintError> {
            let cell = endpoint.cell.ok_or_else(|| {
                BlueprintError::Invalid("endpoint has no source occurrence".into())
            })?;
            Ok(AssemblyPortRef {
                instance: vec![instance_id(cell)],
                port: endpoint.port.clone(),
            })
        };
    let instances = circuit
        .cells
        .iter()
        .map(|(cell, node)| {
            let revision = node.placed.cell.source_revision.as_ref().ok_or_else(|| {
                BlueprintError::Invalid("missing source revision for placed cell".into())
            })?;
            Ok(BlueprintInclusion {
                instance: instance_id(*cell),
                revision: revision.clone(),
                origin: node.placed.origin,
                rotation: node.placed.rotation,
            })
        })
        .collect::<Result<_, _>>()?;
    let mut connections = Vec::new();
    for net in routing.nets.values() {
        let terminal = |endpoint| {
            terminal_for_endpoint(endpoint)
                .map_err(|error| BlueprintError::Invalid(error.to_string()))
        };
        let parents = rooted_parent(terminal(&net.source)?, &net.branches);
        for sink in &net.sinks {
            let mut path = root_path(&parents, terminal(sink)?)
                .ok_or_else(|| BlueprintError::Invalid("routing tree has no sink path".into()))?;
            if path.first() != Some(&net.source.pos) {
                path.insert(0, net.source.pos);
            }
            if path.last() != Some(&sink.pos) {
                path.push(sink.pos);
            }
            connections.push(AssemblyConnection {
                source: reference(&net.source)?,
                sink: reference(sink)?,
                path,
            });
        }
    }
    Ok(Assembly {
        name: name.into(),
        instances,
        blocks: world
            .iter()
            .map(|(position, block)| PositionedBlock {
                position: *position,
                block: block.clone(),
            })
            .collect(),
        known_regions: world
            .bounds()
            .map(|(min, max)| Region::new(min, max))
            .into_iter()
            .collect(),
        connections,
        boundaries: circuit
            .terminals
            .iter()
            .map(|(name, terminal)| {
                Ok(AssemblyBoundary {
                    name: name.clone(),
                    port: reference(&terminal.endpoint)?,
                })
            })
            .collect::<Result<_, BlueprintError>>()?,
    })
}
