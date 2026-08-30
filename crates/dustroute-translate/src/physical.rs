use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::cells::{PlacedCell, PortKind};
use crate::logic::GateKind;
use crate::routing::{RouteResult, materialize_route};
use crate::wire::update_wire_shapes;
use crate::world::{Block, BlockKind, Facing, Pos, World};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RouteId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalNode {
    pub id: CellId,
    pub logical_kind: GateKind,
    pub placed: PlacedCell,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub cell: Option<CellId>,
    pub port: String,
    pub pos: Pos,
    pub kind: PortKind,
    pub facing: Option<Facing>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Route {
    pub id: RouteId,
    pub source: Endpoint,
    pub sink: Endpoint,
    pub path: Vec<Pos>,
    pub repeaters: Vec<Pos>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PhysicalError {
    MissingCell(CellId),
    MissingPort { cell: CellId, port: String },
    CellOverlap(Pos),
    InvalidRepeater(Pos),
}

impl Display for PhysicalError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCell(id) => write!(f, "missing cell {}", id.0),
            Self::MissingPort { cell, port } => write!(f, "missing port {port} on cell {}", cell.0),
            Self::CellOverlap(pos) => write!(f, "cell overlap at {},{},{}", pos.x, pos.y, pos.z),
            Self::InvalidRepeater(pos) => write!(
                f,
                "repeater is not on a horizontal route step at {},{},{}",
                pos.x, pos.y, pos.z
            ),
        }
    }
}

impl Error for PhysicalError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlacementCircuit {
    pub cells: BTreeMap<CellId, PhysicalNode>,
    pub routes: BTreeMap<RouteId, Route>,
    next_cell: u32,
    next_route: u32,
}

impl PlacementCircuit {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_cell(&mut self, logical_kind: GateKind, placed: PlacedCell) -> CellId {
        let id = CellId(self.next_cell);
        self.next_cell += 1;
        self.cells.insert(
            id,
            PhysicalNode {
                id,
                logical_kind,
                placed,
            },
        );
        id
    }

    #[must_use]
    pub fn boundary(name: &str, pos: Pos, kind: PortKind, facing: Option<Facing>) -> Endpoint {
        Endpoint {
            cell: None,
            port: name.into(),
            pos,
            kind,
            facing,
        }
    }

    pub fn input_endpoint(&self, cell: CellId, name: &str) -> Result<Endpoint, PhysicalError> {
        let node = self
            .cells
            .get(&cell)
            .ok_or(PhysicalError::MissingCell(cell))?;
        let port = node
            .placed
            .input_port(name)
            .ok_or_else(|| PhysicalError::MissingPort {
                cell,
                port: name.into(),
            })?;
        Ok(Endpoint {
            cell: Some(cell),
            port: name.into(),
            pos: port.pos,
            kind: port.kind,
            facing: port.facing,
        })
    }

    pub fn output_endpoint(&self, cell: CellId, name: &str) -> Result<Endpoint, PhysicalError> {
        let node = self
            .cells
            .get(&cell)
            .ok_or(PhysicalError::MissingCell(cell))?;
        let port = node
            .placed
            .output_port(name)
            .ok_or_else(|| PhysicalError::MissingPort {
                cell,
                port: name.into(),
            })?;
        Ok(Endpoint {
            cell: Some(cell),
            port: name.into(),
            pos: port.pos,
            kind: port.kind,
            facing: port.facing,
        })
    }

    pub fn add_route(
        &mut self,
        source: Endpoint,
        sink: Endpoint,
        path: Vec<Pos>,
        repeaters: Vec<Pos>,
    ) -> RouteId {
        let id = RouteId(self.next_route);
        self.next_route += 1;
        self.routes.insert(
            id,
            Route {
                id,
                source,
                sink,
                path,
                repeaters,
            },
        );
        id
    }

    pub fn incoming(&self, cell: CellId) -> impl Iterator<Item = &Route> {
        self.routes
            .values()
            .filter(move |route| route.sink.cell == Some(cell))
    }

    pub fn outgoing(&self, cell: CellId) -> impl Iterator<Item = &Route> {
        self.routes
            .values()
            .filter(move |route| route.source.cell == Some(cell))
    }

    pub fn cell_world(&self) -> Result<World, PhysicalError> {
        let mut world = World::new();
        for node in self.cells.values() {
            for (pos, block) in node.placed.blocks() {
                if world.kind_at(pos) != BlockKind::Air {
                    return Err(PhysicalError::CellOverlap(pos));
                }
                world.set(pos, block);
            }
        }
        Ok(world)
    }

    /// Materialize cells and all recorded routes into one simulation/export world.
    pub fn build_world(&self) -> Result<World, PhysicalError> {
        let mut world = self.cell_world()?;
        for route in self.routes.values() {
            if route.path.is_empty() {
                continue;
            }
            materialize_route(
                &mut world,
                &RouteResult {
                    path: route.path.clone(),
                    cost: 0,
                },
                BlockKind::Solid,
            );
            for repeater in &route.repeaters {
                let previous = route
                    .path
                    .windows(2)
                    .find_map(|pair| (pair[1] == *repeater).then_some(pair[0]))
                    .ok_or(PhysicalError::InvalidRepeater(*repeater))?;
                let dx = repeater.x - previous.x;
                let dz = repeater.z - previous.z;
                let facing = match (dx, dz) {
                    (0, -1) => Facing::North,
                    (1, 0) => Facing::East,
                    (0, 1) => Facing::South,
                    (-1, 0) => Facing::West,
                    _ => return Err(PhysicalError::InvalidRepeater(*repeater)),
                };
                let mut block = Block::new(BlockKind::Repeater);
                block.facing = Some(facing);
                block.delay = Some(1);
                world.set(*repeater, block);
            }
        }
        update_wire_shapes(&mut world);
        Ok(world)
    }
}

#[cfg(test)]
mod tests {
    use crate::cells::{RotationY, terminal_cell};

    use super::*;

    #[test]
    fn endpoints_and_overlap_are_checked() {
        let placed = |origin| PlacedCell {
            cell: terminal_cell("terminal"),
            origin,
            rotation: RotationY::R0,
        };
        let mut circuit = PlacementCircuit::new();
        let first = circuit.add_cell(GateKind::Input, placed(Pos::new(0, 0, 0)));
        assert_eq!(
            circuit.output_endpoint(first, "out").unwrap().pos,
            Pos::new(0, 1, 0)
        );
        circuit.add_cell(GateKind::Output, placed(Pos::new(0, 0, 0)));
        assert_eq!(
            circuit.cell_world().unwrap_err(),
            PhysicalError::CellOverlap(Pos::new(0, 0, 0))
        );
    }

    #[test]
    fn build_world_materializes_recorded_repeaters() {
        let mut circuit = PlacementCircuit::new();
        let source = PlacementCircuit::boundary("in", Pos::new(0, 1, 0), PortKind::Wire, None);
        let sink = PlacementCircuit::boundary("out", Pos::new(3, 1, 0), PortKind::Wire, None);
        circuit.add_route(
            source,
            sink,
            vec![
                Pos::new(0, 1, 0),
                Pos::new(1, 1, 0),
                Pos::new(2, 1, 0),
                Pos::new(3, 1, 0),
            ],
            vec![Pos::new(2, 1, 0)],
        );
        let world = circuit.build_world().unwrap();
        let repeater = world.get(Pos::new(2, 1, 0)).unwrap();
        assert_eq!(repeater.kind, BlockKind::Repeater);
        assert_eq!(repeater.facing, Some(Facing::East));
    }
}
