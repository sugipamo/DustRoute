use std::collections::BTreeMap;

use crate::world::{BlockKind, Facing, Pos, WireConnection, World};

pub const HORIZONTAL: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

fn horizontal(pos: Pos, facing: Facing) -> Pos {
    let delta = facing.horizontal_offset().expect("horizontal facing");
    pos.offset(delta.x, 0, delta.z)
}

fn component_connects(world: &World, pos: Pos, direction: Facing) -> bool {
    let block = world.get(pos);
    match block.map(|block| block.kind) {
        Some(BlockKind::Lever | BlockKind::RedstoneTorch | BlockKind::RedstoneBlock) => true,
        Some(BlockKind::Repeater | BlockKind::Comparator) => block
            .and_then(|block| block.facing)
            .is_some_and(|facing| facing == direction || facing == direction.opposite()),
        _ => false,
    }
}

#[must_use]
pub fn infer_wire_connection(world: &World, pos: Pos, facing: Facing) -> WireConnection {
    let side = horizontal(pos, facing);
    let side_kind = world.kind_at(side);
    if side_kind == BlockKind::RedstoneWire || component_connects(world, side, facing) {
        return WireConnection::Side;
    }
    if side_kind.properties().supports_components
        && world.kind_at(side.offset(0, 1, 0)) == BlockKind::RedstoneWire
        && world.kind_at(pos.offset(0, 1, 0)) == BlockKind::Air
    {
        return WireConnection::Up;
    }
    if side_kind == BlockKind::Air
        && world.kind_at(side.offset(0, -1, 0)) == BlockKind::RedstoneWire
    {
        return WireConnection::Side;
    }
    WireConnection::None
}

#[must_use]
pub fn resolved_wire_connection(world: &World, pos: Pos, facing: Facing) -> WireConnection {
    let Some(block) = world.get(pos) else {
        return WireConnection::None;
    };
    if block.kind != BlockKind::RedstoneWire {
        return WireConnection::None;
    }
    block
        .wire_connections
        .as_ref()
        .and_then(|connections| connections.get(&facing).copied())
        .unwrap_or_else(|| infer_wire_connection(world, pos, facing))
}

#[must_use]
pub fn wire_has_arm(world: &World, pos: Pos, facing: Facing) -> bool {
    resolved_wire_connection(world, pos, facing) != WireConnection::None
}

#[must_use]
pub fn dust_connected(world: &World, a: Pos, b: Pos) -> bool {
    if world.kind_at(a) != BlockKind::RedstoneWire || world.kind_at(b) != BlockKind::RedstoneWire {
        return false;
    }
    for facing in HORIZONTAL {
        let side = horizontal(a, facing);
        if b == side {
            return wire_has_arm(world, a, facing) && wire_has_arm(world, b, facing.opposite());
        }
        if b == side.offset(0, 1, 0) {
            return resolved_wire_connection(world, a, facing) == WireConnection::Up;
        }
        let reverse_side = horizontal(b, facing);
        if a == reverse_side.offset(0, 1, 0) {
            return resolved_wire_connection(world, b, facing) == WireConnection::Up;
        }
    }
    false
}

pub fn update_wire_shapes(world: &mut World) {
    let updates: Vec<_> = world
        .iter()
        .filter(|(_, block)| block.kind == BlockKind::RedstoneWire)
        .map(|(pos, block)| {
            let connections: BTreeMap<_, _> = HORIZONTAL
                .into_iter()
                .filter_map(|facing| {
                    let state = infer_wire_connection(world, *pos, facing);
                    (state != WireConnection::None).then_some((facing, state))
                })
                .collect();
            let mut block = block.clone();
            block.wire_connections = Some(connections);
            (*pos, block)
        })
        .collect();
    for (pos, block) in updates {
        world.set(pos, block);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::Block;

    use super::*;

    #[test]
    fn resolves_corner_and_stair_connections() {
        let mut world = World::new();
        for pos in [Pos::new(0, 0, 0), Pos::new(1, 0, 0), Pos::new(1, 0, 1)] {
            world.set(pos, Block::new(BlockKind::Solid));
        }
        for pos in [Pos::new(0, 1, 0), Pos::new(1, 1, 0), Pos::new(1, 1, 1)] {
            world.place(BlockKind::RedstoneWire, pos);
        }
        update_wire_shapes(&mut world);
        assert!(dust_connected(&world, Pos::new(0, 1, 0), Pos::new(1, 1, 0)));
        assert!(wire_has_arm(&world, Pos::new(1, 1, 0), Facing::South));
    }
}
