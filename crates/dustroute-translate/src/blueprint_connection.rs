//! Contextual connection checks. No logical classification is consulted here.

use std::collections::BTreeMap;

use dustroute_library::PortDirection;
use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintError, BlueprintPort, BlueprintPortKind, BlueprintPortRef,
    BlueprintRevisionId, TypeContract,
};

use crate::connectivity::{PhysicalStepKind, physical_step};
use crate::{cells::RotationY, world::Block, world::BlockKind, world::Pos, world::ValidatedWorld};

/// A selected port of one placed occurrence. Other ports and classifications
/// of the producer do not participate in connection typing.
#[derive(Clone, Copy, Debug)]
pub struct BlueprintEndpoint<'a> {
    pub revision: &'a BlueprintRevisionId,
    pub port: &'a str,
    pub origin: Pos,
    pub rotation: RotationY,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlueprintConnectionError {
    Blueprint(BlueprintError),
    MissingPort(String),
    UnsupportedBlockStateRoute,
    InvalidTerminal(Pos),
    InvalidRouteEndpoints,
    WrongApproach { position: Pos, expected: Pos },
    DisconnectedStep { source: Pos, sink: Pos },
}

impl std::fmt::Display for BlueprintConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BlueprintConnectionError {}

impl From<BlueprintError> for BlueprintConnectionError {
    fn from(error: BlueprintError) -> Self {
        Self::Blueprint(error)
    }
}

/// Checks connection types and a supplied, directed redstone route against one
/// validated placement snapshot. Paths run from physical port to physical port,
/// including the final conductor when the sink is BlockPower. No path is found
/// implicitly through a gate's internals. This proves local connectivity only:
/// signal strength, timing, whole-circuit behavior and live placement still use
/// their existing validation stages.
pub fn check_blueprint_connection(
    catalog: &BlueprintCatalog,
    world: &ValidatedWorld,
    source: BlueprintEndpoint<'_>,
    sink: BlueprintEndpoint<'_>,
    path: &[Pos],
) -> Result<(), BlueprintConnectionError> {
    let (source, source_rotation) = resolve(catalog, source)?;
    let (sink, _) = resolve(catalog, sink)?;
    check_port_connection(
        catalog,
        world,
        &source,
        &sink,
        path,
        source_rotation,
        |position| world.get(position).cloned(),
    )
}

/// Both the route and requirements use the same caller-supplied actual state.
/// Assembly validation can also supply explicitly known air from its coverage.
pub(crate) fn check_port_connection(
    catalog: &BlueprintCatalog,
    world: &ValidatedWorld,
    source: &BlueprintPort,
    sink: &BlueprintPort,
    path: &[Pos],
    source_rotation: RotationY,
    block_at: impl Fn(Pos) -> Option<Block>,
) -> Result<(), BlueprintConnectionError> {
    // Collect actual states first, with checked coordinate arithmetic. Missing
    // cells remain unknown: sparse World cannot certify an explicit air claim.
    let mut source_blocks = BTreeMap::new();
    let mut insert_state = |offset| -> Result<(), BlueprintError> {
        let rotated = source_rotation
            .checked_pos(offset)
            .ok_or(BlueprintError::CoordinateOverflow)?;
        let position = translated(source.position, rotated)?;
        if let Some(block) = block_at(position) {
            if !source_rotation.is_identity() && block.piston_entity.is_some() {
                return Err(BlueprintError::Invalid(
                    "rotating block entities is outside the blueprint scope".into(),
                ));
            }
            let local = source_rotation
                .inverse()
                .checked_block(&block)
                .ok_or(BlueprintError::CoordinateOverflow)?;
            source_blocks.insert(offset, local);
        }
        Ok(())
    };
    insert_state(Pos::default())?;
    for id in &sink.required_source_types {
        let definition = catalog
            .type_revision(id)
            .ok_or_else(|| BlueprintError::UnknownType(id.clone()))?;
        if let TypeContract::BlockPattern { blocks } = &definition.contract {
            for record in blocks {
                insert_state(record.position)?;
            }
        }
    }
    catalog
        .check_source_requirements(source, sink, |offset| source_blocks.get(&offset).cloned())?;
    if source.kind == BlueprintPortKind::BlockState {
        return Err(BlueprintConnectionError::UnsupportedBlockStateRoute);
    }
    if !terminal_present(world, sink) {
        return Err(BlueprintConnectionError::InvalidTerminal(sink.position));
    }
    if path.first() != Some(&source.position) || path.last() != Some(&sink.position) {
        return Err(BlueprintConnectionError::InvalidRouteEndpoints);
    }
    if path.len() > 1 {
        if let Some(facing) = sink.facing {
            let expected = translated(sink.position, facing.offset())?;
            if path[path.len() - 2] != expected {
                return Err(BlueprintConnectionError::WrongApproach {
                    position: sink.position,
                    expected,
                });
            }
        }
    }
    for position in path {
        // Existing neighbor/shape helpers look at adjacent cells. Reject a
        // coordinate that cannot represent that neighborhood before calling them.
        translated(*position, Pos::new(-2, -2, -2))?;
        translated(*position, Pos::new(2, 2, 2))?;
    }
    for pair in path.windows(2) {
        // physical_step uses local offsets; reject non-neighbors before its
        // coordinate subtraction, which also avoids distant i32 overflow.
        let adjacent = pair[0].x.abs_diff(pair[1].x) <= 1
            && pair[0].y.abs_diff(pair[1].y) <= 1
            && pair[0].z.abs_diff(pair[1].z) <= 1;
        let direct_device = world
            .get(pair[0])
            .is_some_and(|block| BlueprintPortKind::DeviceOutput.matches_signal_block(block))
            && world
                .get(pair[1])
                .is_some_and(|block| block.redstone_traits().conducts_strong_power)
            && crate::electrical::strong_output_targets(world, pair[0])
                .is_some_and(|targets| targets.contains(&pair[1]));
        let connected = adjacent
            && (direct_device
                || physical_step(world, pair[0], pair[1])
                    .is_some_and(|step| !matches!(step.kind, PhysicalStepKind::ObserverInput)));
        if !connected {
            return Err(BlueprintConnectionError::DisconnectedStep {
                source: pair[0],
                sink: pair[1],
            });
        }
    }
    Ok(())
}

fn resolve(
    catalog: &BlueprintCatalog,
    endpoint: BlueprintEndpoint<'_>,
) -> Result<(BlueprintPort, RotationY), BlueprintConnectionError> {
    let revision = catalog
        .revision(endpoint.revision)
        .ok_or_else(|| BlueprintError::UnknownRevision(endpoint.revision.clone()))?;
    revision
        .ports
        .iter()
        .find(|port| port.name == endpoint.port)
        .ok_or_else(|| BlueprintConnectionError::MissingPort(endpoint.port.into()))?;
    let (mut port, source_rotation) = catalog.resolve_port(
        endpoint.revision,
        &BlueprintPortRef {
            instance: vec![],
            port: endpoint.port.into(),
        },
    )?;
    let rotated = endpoint
        .rotation
        .checked_pos(port.position)
        .ok_or(BlueprintError::CoordinateOverflow)?;
    port.position = translated(rotated, endpoint.origin)?;
    port.facing = port.facing.map(|facing| endpoint.rotation.facing(facing));
    Ok((port, endpoint.rotation.then(source_rotation)))
}

fn terminal_present(world: &ValidatedWorld, port: &BlueprintPort) -> bool {
    if port.direction != PortDirection::Input {
        return false;
    }
    world
        .get(port.position)
        .is_some_and(|block| match port.kind {
            BlueprintPortKind::Wire => block.kind == BlockKind::RedstoneWire,
            BlueprintPortKind::BlockPower => {
                let traits = block.redstone_traits();
                traits.conducts_weak_power || traits.conducts_strong_power
            }
            BlueprintPortKind::BlockState | BlueprintPortKind::DeviceOutput => false,
        })
}

fn translated(position: Pos, origin: Pos) -> Result<Pos, BlueprintError> {
    Ok(Pos::new(
        position
            .x
            .checked_add(origin.x)
            .ok_or(BlueprintError::CoordinateOverflow)?,
        position
            .y
            .checked_add(origin.y)
            .ok_or(BlueprintError::CoordinateOverflow)?,
        position
            .z
            .checked_add(origin.z)
            .ok_or(BlueprintError::CoordinateOverflow)?,
    ))
}
