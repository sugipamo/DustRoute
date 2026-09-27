use std::collections::BTreeMap;

use dustroute_minecraft::spatial::{
    DustElevation, DustTransferFacts, WireShapeFacts, builtin_spatial_laws,
};

use crate::world::{BlockKind, Facing, Pos, WireConnection, World};

pub const HORIZONTAL: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DustTransfer {
    Horizontal,
    Rise,
    FallThroughConductor,
}

fn horizontal(pos: Pos, facing: Facing) -> Pos {
    let delta = facing.horizontal_offset().expect("horizontal facing");
    pos.offset(delta.x, 0, delta.z)
}

#[must_use]
pub fn infer_wire_connection(world: &World, pos: Pos, facing: Facing) -> WireConnection {
    let side = horizontal(pos, facing);
    let laws = builtin_spatial_laws();
    let mut facts = WireShapeFacts {
        side_connects: world
            .get(side)
            .is_some_and(|b| laws.component_connects(b, facing)),
        support_shape: world
            .get(side)
            .and_then(|b| b.redstone_traits().wire_rise_connection),
        upper_wire: false,
        blocked: false,
        side_air: world.kind_at(side) == BlockKind::Air,
        lower_wire: false,
    };
    // Preserve this profile's local lookup scope. A direct side connection
    // never needed above/below coordinates, including at coordinate limits.
    if facts.side_connects {
        return laws.infer_shape(facts);
    }
    if facts.support_shape.is_some() {
        facts.upper_wire = world.kind_at(side.offset(0, 1, 0)) == BlockKind::RedstoneWire;
        if facts.upper_wire {
            facts.blocked = world
                .get(pos.offset(0, 1, 0))
                .is_some_and(|b| b.redstone_traits().blocks_wire_rise_when_above);
        }
    }
    let shape = laws.infer_shape(facts);
    if shape != WireConnection::None {
        return shape;
    }
    if facts.side_air {
        facts.lower_wire = world.kind_at(side.offset(0, -1, 0)) == BlockKind::RedstoneWire;
    }
    laws.infer_shape(facts)
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
    dust_transmits(world, a, b) || dust_transmits(world, b, a)
}

/// Returns the vanilla-Java physical rule by which power can move from one
/// dust position to another. Vertical connections over non-conductors are
/// intentionally asymmetric.
#[must_use]
pub fn dust_transfer(world: &World, source: Pos, sink: Pos) -> Option<DustTransfer> {
    if world.kind_at(source) != BlockKind::RedstoneWire
        || world.kind_at(sink) != BlockKind::RedstoneWire
    {
        return None;
    }
    let laws = builtin_spatial_laws();
    for facing in HORIZONTAL {
        let side = horizontal(source, facing);
        let (kind, facts) = if sink == side {
            (
                DustTransfer::Horizontal,
                DustTransferFacts {
                    elevation: DustElevation::Horizontal,
                    source_arm: resolved_wire_connection(world, source, facing),
                    sink_arm: resolved_wire_connection(world, sink, facing.opposite()),
                    support_shape: None,
                    conducts: false,
                    clear: true,
                    enforce_clearance: false,
                },
            )
        } else if sink == side.offset(0, 1, 0) {
            (
                DustTransfer::Rise,
                DustTransferFacts {
                    elevation: DustElevation::Rise,
                    source_arm: resolved_wire_connection(world, source, facing),
                    sink_arm: WireConnection::None,
                    support_shape: world
                        .get(side)
                        .and_then(|b| b.redstone_traits().wire_rise_connection),
                    conducts: false,
                    clear: true,
                    enforce_clearance: false,
                },
            )
        } else if sink == side.offset(0, -1, 0) {
            let support = world
                .get(source.offset(0, -1, 0))
                .map(|b| b.redstone_traits());
            (
                DustTransfer::FallThroughConductor,
                DustTransferFacts {
                    elevation: DustElevation::Fall,
                    source_arm: WireConnection::None,
                    sink_arm: resolved_wire_connection(world, sink, facing.opposite()),
                    support_shape: support.and_then(|t| t.wire_rise_connection),
                    conducts: support.is_some_and(|t| t.strong_power_drives_dust),
                    clear: true,
                    enforce_clearance: false,
                },
            )
        } else {
            continue;
        };
        if laws.dust_transfers(facts) {
            return Some(kind);
        }
    }
    None
}

#[must_use]
pub fn dust_transmits(world: &World, source: Pos, sink: Pos) -> bool {
    dust_transfer(world, source, sink).is_some()
}

pub fn update_wire_shapes(world: &mut World) {
    let updates: Vec<_> = world
        .iter()
        .filter(|(_, block)| block.kind == BlockKind::RedstoneWire)
        .map(|(pos, block)| {
            let mut connections: BTreeMap<_, _> = HORIZONTAL
                .into_iter()
                .filter_map(|facing| {
                    let state = infer_wire_connection(world, *pos, facing);
                    (state != WireConnection::None).then_some((facing, state))
                })
                .collect();
            if connections.len() == 1 {
                let only = *connections.keys().next().expect("one connection");
                connections.insert(only.opposite(), WireConnection::Side);
            }
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

    fn glass() -> Block {
        let mut block = Block::new(BlockKind::Transparent);
        block.observed_name = Some("minecraft:glass".to_owned());
        block
    }

    fn top_slab() -> Block {
        let mut block = Block::new(BlockKind::Transparent);
        block.observed_name = Some("minecraft:stone_slab".to_owned());
        block
            .observed_properties
            .insert("type".to_owned(), "top".to_owned());
        block
    }

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

    #[test]
    fn non_conducting_vertical_support_is_an_upward_only_wire() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(1, 1, 0), glass());
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
        update_wire_shapes(&mut world);

        assert_eq!(
            dust_transfer(&world, Pos::new(0, 1, 0), Pos::new(1, 2, 0)),
            Some(DustTransfer::Rise)
        );
        assert_eq!(
            dust_transfer(&world, Pos::new(1, 2, 0), Pos::new(0, 1, 0)),
            None
        );
        assert!(dust_connected(&world, Pos::new(0, 1, 0), Pos::new(1, 2, 0)));
    }

    #[test]
    fn top_slab_rise_uses_side_shape_and_full_block_above_blocks_it() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(1, 1, 0), top_slab());
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
        update_wire_shapes(&mut world);
        assert_eq!(
            resolved_wire_connection(&world, Pos::new(0, 1, 0), Facing::East),
            WireConnection::Side
        );
        assert!(dust_transmits(&world, Pos::new(0, 1, 0), Pos::new(1, 2, 0)));

        world.set(Pos::new(0, 2, 0), Block::new(BlockKind::Solid));
        update_wire_shapes(&mut world);
        assert!(!dust_connected(
            &world,
            Pos::new(0, 1, 0),
            Pos::new(1, 2, 0)
        ));
    }
}
