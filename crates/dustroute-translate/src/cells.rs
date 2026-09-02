use crate::logic::GateKind;
use crate::world::{Block, BlockKind, Facing, Pos, World};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PortKind {
    #[default]
    Wire,
    BlockPower,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputPort {
    pub name: String,
    pub pos: Pos,
    pub kind: PortKind,
    pub facing: Option<Facing>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputPort {
    pub name: String,
    pub pos: Pos,
    pub kind: PortKind,
    pub facing: Option<Facing>,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum RotationY {
    #[default]
    R0 = 0,
    R90 = 1,
    R180 = 2,
    R270 = 3,
}

impl RotationY {
    #[must_use]
    pub const fn pos(self, pos: Pos) -> Pos {
        match self {
            Self::R0 => pos,
            Self::R90 => Pos::new(-pos.z, pos.y, pos.x),
            Self::R180 => Pos::new(-pos.x, pos.y, -pos.z),
            Self::R270 => Pos::new(pos.z, pos.y, -pos.x),
        }
    }

    #[must_use]
    pub fn facing(self, facing: Facing) -> Facing {
        if matches!(facing, Facing::Up | Facing::Down) {
            return facing;
        }
        let index = match facing {
            Facing::North => 0,
            Facing::East => 1,
            Facing::South => 2,
            Facing::West => 3,
            Facing::Up | Facing::Down => unreachable!(),
        };
        [Facing::North, Facing::East, Facing::South, Facing::West]
            [(index + usize::from(self as u8)) % 4]
    }

    #[must_use]
    pub fn block(self, block: &Block) -> Block {
        let mut result = block.clone();
        result.facing = result.facing.map(|facing| self.facing(facing));
        result.support_offset = result.support_offset.map(|offset| self.pos(offset));
        result.wire_connections = result.wire_connections.as_ref().map(|connections| {
            connections
                .iter()
                .map(|(facing, connection)| (self.facing(*facing), *connection))
                .collect()
        });
        result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalCell {
    pub name: String,
    pub world: World,
    pub inputs: Vec<InputPort>,
    pub outputs: Vec<OutputPort>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacedCell {
    pub cell: PhysicalCell,
    pub origin: Pos,
    pub rotation: RotationY,
}

impl PlacedCell {
    fn transform_pos(&self, pos: Pos) -> Pos {
        let rotated = self.rotation.pos(pos);
        rotated.offset(self.origin.x, self.origin.y, self.origin.z)
    }

    #[must_use]
    pub fn input_port(&self, name: &str) -> Option<InputPort> {
        self.cell
            .inputs
            .iter()
            .find(|port| port.name == name)
            .map(|port| InputPort {
                name: port.name.clone(),
                pos: self.transform_pos(port.pos),
                kind: port.kind,
                facing: port.facing.map(|facing| self.rotation.facing(facing)),
            })
    }

    #[must_use]
    pub fn output_port(&self, name: &str) -> Option<OutputPort> {
        self.cell
            .outputs
            .iter()
            .find(|port| port.name == name)
            .map(|port| OutputPort {
                name: port.name.clone(),
                pos: self.transform_pos(port.pos),
                kind: port.kind,
                facing: port.facing.map(|facing| self.rotation.facing(facing)),
            })
    }

    pub fn blocks(&self) -> impl Iterator<Item = (Pos, Block)> + '_ {
        self.cell
            .world
            .iter()
            .map(|(pos, block)| (self.transform_pos(*pos), self.rotation.block(block)))
    }
}

#[must_use]
pub fn terminal_cell(name: &str) -> PhysicalCell {
    let mut world = World::new();
    // Boundary dust must not pick up power conducted through its support from
    // an unrelated route passing beside the terminal.
    world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Transparent));
    world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    PhysicalCell {
        name: name.into(),
        world,
        inputs: vec![InputPort {
            name: "in".into(),
            pos: Pos::new(0, 1, 0),
            kind: PortKind::Wire,
            facing: None,
        }],
        outputs: vec![OutputPort {
            name: "out".into(),
            pos: Pos::new(0, 1, 0),
            kind: PortKind::Wire,
            facing: None,
        }],
    }
}

#[must_use]
pub fn not_cell() -> PhysicalCell {
    let mut world = World::new();
    world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
    torch.facing = Some(Facing::East);
    torch.support_offset = Some(Pos::new(-1, 0, 0));
    world.set(Pos::new(2, -1, 0), Block::new(BlockKind::Solid));
    world.place(BlockKind::RedstoneWire, Pos::new(2, 0, 0));
    PhysicalCell {
        name: "not_torch_block_power".into(),
        world,
        inputs: vec![InputPort {
            name: "a".into(),
            pos: Pos::new(0, 0, 0),
            kind: PortKind::BlockPower,
            facing: Some(Facing::West),
        }],
        outputs: vec![OutputPort {
            name: "out".into(),
            pos: Pos::new(2, 0, 0),
            kind: PortKind::Wire,
            facing: Some(Facing::East),
        }],
    }
}

#[must_use]
pub fn not_top_cell() -> PhysicalCell {
    let mut world = World::new();
    world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    let torch = world.place(BlockKind::RedstoneTorch, Pos::new(0, 1, 0));
    torch.facing = Some(Facing::Up);
    torch.support_offset = Some(Pos::new(0, -1, 0));
    world.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
    world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
    crate::wire::update_wire_shapes(&mut world);
    PhysicalCell {
        name: "not_torch_top".into(),
        world,
        inputs: vec![InputPort {
            name: "a".into(),
            pos: Pos::new(0, 0, 0),
            kind: PortKind::BlockPower,
            facing: Some(Facing::West),
        }],
        outputs: vec![OutputPort {
            name: "out".into(),
            pos: Pos::new(1, 1, 0),
            kind: PortKind::Wire,
            facing: Some(Facing::East),
        }],
    }
}

#[must_use]
pub fn buffered_boundary_cell(name: &str) -> PhysicalCell {
    let mut world = World::new();
    world.fill(
        Pos::new(0, 0, 0),
        Pos::new(2, 0, 0),
        Block::new(BlockKind::Solid),
    );
    world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    let repeater = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
    repeater.facing = Some(Facing::East);
    repeater.delay = Some(1);
    world.place(BlockKind::RedstoneWire, Pos::new(2, 1, 0));
    crate::wire::update_wire_shapes(&mut world);
    PhysicalCell {
        name: name.into(),
        world,
        inputs: vec![InputPort {
            name: "in".into(),
            pos: Pos::new(0, 1, 0),
            kind: PortKind::Wire,
            facing: Some(Facing::West),
        }],
        outputs: vec![OutputPort {
            name: "out".into(),
            pos: Pos::new(2, 1, 0),
            kind: PortKind::Wire,
            facing: Some(Facing::East),
        }],
    }
}

#[must_use]
pub fn or_buffered_cell() -> PhysicalCell {
    let mut world = World::new();
    world.fill(
        Pos::new(0, 0, 0),
        Pos::new(5, 0, 2),
        Block::new(BlockKind::Solid),
    );
    for pos in [
        Pos::new(0, 1, 0),
        Pos::new(1, 1, 0),
        Pos::new(2, 1, 0),
        Pos::new(2, 1, 1),
        Pos::new(0, 1, 2),
        Pos::new(1, 1, 2),
        Pos::new(2, 1, 2),
        Pos::new(3, 1, 1),
    ] {
        world.place(BlockKind::RedstoneWire, pos);
    }
    let repeater = world.place(BlockKind::Repeater, Pos::new(4, 1, 1));
    repeater.facing = Some(Facing::East);
    repeater.delay = Some(1);
    world.place(BlockKind::RedstoneWire, Pos::new(5, 1, 1));
    crate::wire::update_wire_shapes(&mut world);
    PhysicalCell {
        name: "or_dust_buffered".into(),
        world,
        inputs: vec![
            InputPort {
                name: "a".into(),
                pos: Pos::new(0, 1, 0),
                kind: PortKind::Wire,
                facing: Some(Facing::West),
            },
            InputPort {
                name: "b".into(),
                pos: Pos::new(0, 1, 2),
                kind: PortKind::Wire,
                facing: Some(Facing::West),
            },
        ],
        outputs: vec![OutputPort {
            name: "out".into(),
            pos: Pos::new(5, 1, 1),
            kind: PortKind::Wire,
            facing: Some(Facing::East),
        }],
    }
}

#[must_use]
pub fn and_cell() -> PhysicalCell {
    let mut world = World::new();
    world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    world.set(Pos::new(0, 0, 4), Block::new(BlockKind::Solid));
    for z in [0, 4] {
        let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, z));
        torch.facing = Some(Facing::East);
        torch.support_offset = Some(Pos::new(-1, 0, 0));
    }
    world.fill(
        Pos::new(2, -1, 0),
        Pos::new(3, -1, 4),
        Block::new(BlockKind::Solid),
    );
    for z in 0..5 {
        world.place(BlockKind::RedstoneWire, Pos::new(2, 0, z));
    }
    world.place(BlockKind::RedstoneWire, Pos::new(3, 0, 2));
    world.set(Pos::new(4, -1, 2), Block::new(BlockKind::Solid));
    let repeater = world.place(BlockKind::Repeater, Pos::new(4, 0, 2));
    repeater.facing = Some(Facing::East);
    repeater.delay = Some(1);
    world.set(Pos::new(5, 0, 2), Block::new(BlockKind::Solid));
    let torch = world.place(BlockKind::RedstoneTorch, Pos::new(6, 0, 2));
    torch.facing = Some(Facing::East);
    torch.support_offset = Some(Pos::new(-1, 0, 0));
    world.set(Pos::new(7, -1, 2), Block::new(BlockKind::Solid));
    world.place(BlockKind::RedstoneWire, Pos::new(7, 0, 2));
    crate::wire::update_wire_shapes(&mut world);
    PhysicalCell {
        name: "and_demorgan_repeater".into(),
        world,
        inputs: vec![
            InputPort {
                name: "a".into(),
                pos: Pos::new(0, 0, 0),
                kind: PortKind::BlockPower,
                facing: Some(Facing::West),
            },
            InputPort {
                name: "b".into(),
                pos: Pos::new(0, 0, 4),
                kind: PortKind::BlockPower,
                facing: Some(Facing::West),
            },
        ],
        outputs: vec![OutputPort {
            name: "out".into(),
            pos: Pos::new(7, 0, 2),
            kind: PortKind::Wire,
            facing: Some(Facing::East),
        }],
    }
}

#[must_use]
pub fn nand_cell() -> PhysicalCell {
    let mut world = World::new();
    world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    world.set(Pos::new(0, 0, 4), Block::new(BlockKind::Solid));
    for z in [0, 4] {
        let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, z));
        torch.facing = Some(Facing::East);
        torch.support_offset = Some(Pos::new(-1, 0, 0));
    }
    world.fill(
        Pos::new(2, -1, 0),
        Pos::new(3, -1, 4),
        Block::new(BlockKind::Solid),
    );
    for z in 0..5 {
        world.place(BlockKind::RedstoneWire, Pos::new(2, 0, z));
    }
    world.place(BlockKind::RedstoneWire, Pos::new(3, 0, 2));
    crate::wire::update_wire_shapes(&mut world);
    PhysicalCell {
        name: "nand_torch_merge".into(),
        world,
        inputs: vec![
            InputPort {
                name: "a".into(),
                pos: Pos::new(0, 0, 0),
                kind: PortKind::BlockPower,
                facing: Some(Facing::West),
            },
            InputPort {
                name: "b".into(),
                pos: Pos::new(0, 0, 4),
                kind: PortKind::BlockPower,
                facing: Some(Facing::West),
            },
        ],
        outputs: vec![OutputPort {
            name: "out".into(),
            pos: Pos::new(3, 0, 2),
            kind: PortKind::Wire,
            facing: Some(Facing::East),
        }],
    }
}

/// XOR cell derived from Redstone-Compiler's MIT-licensed
/// `test/xor-generated.nbt` at commit
/// `cc997732b82d957a8b5cc80d14c07b375562dd9d`.
///
/// The upstream structure's two levers are external input drivers, so this
/// reusable form exposes their support blocks as block-power input ports.
#[must_use]
pub fn external_xor_cell() -> PhysicalCell {
    use dustroute_library::REDSTONE_COMPILER_XOR_ID;

    let mut world = World::new();
    let normalize = |x, y, z| Pos::new(x - 5, y, z);
    for (x, y, z) in [
        (6, 0, 1),
        (6, 0, 2),
        (5, 1, 1),
        (6, 1, 3),
        (7, 1, 4),
        (5, 2, 2),
        (7, 2, 2),
        (7, 2, 3),
        (6, 3, 1),
        (6, 3, 2),
        (7, 3, 3),
        (7, 4, 4),
    ] {
        world.set(normalize(x, y, z), Block::new(BlockKind::Solid));
    }
    for (x, y, z, facing) in [
        (5, 0, 1, Facing::West),
        (5, 3, 1, Facing::West),
        (7, 1, 3, Facing::East),
        (6, 2, 2, Facing::East),
        (7, 3, 1, Facing::East),
        (7, 3, 4, Facing::South),
    ] {
        let torch = world.place(BlockKind::RedstoneTorch, normalize(x, y, z));
        torch.facing = Some(facing);
        let outward = facing
            .horizontal_offset()
            .expect("wall torch facing is horizontal");
        torch.support_offset = Some(Pos::new(-outward.x, -outward.y, -outward.z));
    }
    let top_torch = world.place(BlockKind::RedstoneTorch, normalize(6, 1, 1));
    top_torch.facing = Some(Facing::Up);
    top_torch.support_offset = Some(Pos::new(0, -1, 0));
    for (x, y, z) in [(6, 1, 2), (7, 3, 2), (5, 2, 1), (7, 2, 4)] {
        world.place(BlockKind::RedstoneWire, normalize(x, y, z));
    }
    crate::wire::update_wire_shapes(&mut world);
    PhysicalCell {
        name: REDSTONE_COMPILER_XOR_ID.into(),
        world,
        inputs: vec![
            InputPort {
                name: "a".into(),
                pos: normalize(6, 0, 1),
                kind: PortKind::BlockPower,
                facing: Some(Facing::North),
            },
            InputPort {
                name: "b".into(),
                pos: normalize(6, 3, 1),
                kind: PortKind::BlockPower,
                facing: Some(Facing::North),
            },
        ],
        outputs: vec![OutputPort {
            name: "out".into(),
            pos: normalize(7, 2, 4),
            kind: PortKind::Wire,
            facing: Some(Facing::South),
        }],
    }
}

/// Builds a conservative XOR cell from DustRoute's verified primitive cells.
/// This is intentionally a correctness baseline rather than a compact layout.
pub fn compiled_xor_cell() -> Result<PhysicalCell, String> {
    static CELL: std::sync::OnceLock<Result<PhysicalCell, String>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        let mut cell = compiled_xor_cell_with_config(crate::BaselineCompileConfig::default())?;
        cell.name = "dustroute.xor.compiled_baseline.1_21_11".into();
        Ok(cell)
    })
    .clone()
}

pub fn compact_compiled_xor_cell() -> Result<PhysicalCell, String> {
    static CELL: std::sync::OnceLock<Result<PhysicalCell, String>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        let mut cell = compiled_xor_cell_with_config(crate::BaselineCompileConfig {
            spacing_x: 9,
            lane_gap: 6,
            ..crate::BaselineCompileConfig::default()
        })?;
        cell.name = "dustroute.xor.compact_compiled.1_21_11".into();
        Ok(cell)
    })
    .clone()
}

pub fn compiled_xor_cell_with_config(
    config: crate::BaselineCompileConfig,
) -> Result<PhysicalCell, String> {
    let mut builder = crate::logic::DagBuilder::new();
    let a = builder.input("a");
    let b = builder.input("b");
    let out = builder.gate(GateKind::Xor, &[a, b], Some("xor"));
    let dag = builder
        .finish([("out".into(), out)])
        .map_err(|error| error.to_string())?;
    let compiled = crate::BaselineCompiler::new(config)
        .compile(&dag)
        .map_err(|error| error.to_string())?;
    let input = |name: &str| {
        compiled
            .input_positions
            .get(name)
            .copied()
            .ok_or_else(|| format!("compiled XOR is missing input {name}"))
            .map(|pos| InputPort {
                name: name.into(),
                pos,
                kind: PortKind::Wire,
                facing: Some(Facing::West),
            })
    };
    let output = compiled
        .output_positions
        .get("out")
        .copied()
        .ok_or_else(|| "compiled XOR is missing output out".to_owned())?;
    Ok(PhysicalCell {
        name: format!(
            "dustroute.xor.compiled_baseline.1_21_11.s{}.l{}",
            config.spacing_x, config.lane_gap
        ),
        world: compiled.world,
        inputs: vec![input("a")?, input("b")?],
        outputs: vec![OutputPort {
            name: "out".into(),
            pos: output,
            kind: PortKind::Wire,
            facing: Some(Facing::East),
        }],
    })
}

#[must_use]
pub fn baseline_cell_for(kind: GateKind) -> Option<PhysicalCell> {
    match kind {
        GateKind::Input => Some(buffered_boundary_cell("input_buffer")),
        GateKind::Output => Some(buffered_boundary_cell("output")),
        GateKind::Not => Some(not_top_cell()),
        GateKind::And => Some(and_cell()),
        GateKind::Or => Some(or_buffered_cell()),
        GateKind::Nand => Some(nand_cell()),
        GateKind::Xor => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_ports_blocks_and_support_offsets() {
        let placed = PlacedCell {
            cell: not_cell(),
            origin: Pos::new(10, 3, 20),
            rotation: RotationY::R90,
        };
        let input = placed.input_port("a").unwrap();
        assert_eq!(input.pos, Pos::new(10, 3, 20));
        assert_eq!(input.facing, Some(Facing::North));
        let torch = placed
            .blocks()
            .find(|(_, block)| block.kind == BlockKind::RedstoneTorch)
            .unwrap();
        assert_eq!(torch.0, Pos::new(10, 3, 21));
        assert_eq!(torch.1.support_offset, Some(Pos::new(0, 0, -1)));
    }
}
