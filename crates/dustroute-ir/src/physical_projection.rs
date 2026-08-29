use dustroute_physical::{ComponentId, ConnectionKind, PhysicalCircuit};
use serde::{Deserialize, Serialize};

use crate::LogicDag;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AbstractionLevel {
    Physical,
    Signal,
    Logic,
    Behavior,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignalEdge {
    pub source: ComponentId,
    pub sink: ComponentId,
    pub physical_kind: ConnectionKind,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignalIr {
    pub components: Vec<ComponentId>,
    pub edges: Vec<SignalEdge>,
}

#[derive(Clone, Debug)]
pub enum IrProjection {
    Physical(PhysicalCircuit),
    Signal(SignalIr),
    Logic(LogicDag),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionError {
    Unsupported {
        from: AbstractionLevel,
        to: AbstractionLevel,
    },
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { from, to } => {
                write!(
                    formatter,
                    "projection from {from:?} to {to:?} is not implemented"
                )
            }
        }
    }
}

impl std::error::Error for ProjectionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalProjection {
    pub signal: SignalIr,
}

impl PhysicalProjection {
    #[must_use]
    pub fn from_physical(physical: &PhysicalCircuit) -> Self {
        Self {
            signal: SignalIr {
                components: physical
                    .components
                    .iter()
                    .map(|component| component.id)
                    .collect(),
                edges: physical
                    .connections
                    .iter()
                    .map(|connection| SignalEdge {
                        source: connection.source,
                        sink: connection.sink,
                        physical_kind: connection.kind,
                    })
                    .collect(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use dustroute_physical::{
        Block, BlockKind, ComponentId, ConnectionKind, PhysicalComponent, PhysicalConnection, Pos,
    };

    use super::*;

    #[test]
    fn projects_verified_physical_connections_to_signal_ir() {
        let physical = PhysicalCircuit::from_parts(
            vec![
                PhysicalComponent {
                    id: ComponentId(0),
                    pos: Pos::new(0, 64, 0),
                    block: Block::new(BlockKind::Lever),
                },
                PhysicalComponent {
                    id: ComponentId(1),
                    pos: Pos::new(1, 64, 0),
                    block: Block::new(BlockKind::RedstoneWire),
                },
            ],
            [PhysicalConnection {
                source: ComponentId(0),
                sink: ComponentId(1),
                kind: ConnectionKind::DirectSource,
            }],
        );
        let projection = PhysicalProjection::from_physical(&physical);
        assert_eq!(projection.signal.components.len(), 2);
        assert_eq!(projection.signal.edges.len(), 1);
    }
}
