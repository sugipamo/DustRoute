//! Fixed records transcribed from the independent historical fixture.
//! No recipe compiler or presentation decoder runs while loading this table.

use super::{
    AND_CLASSIFICATION_REVISION, AND_REVISION, BLOCK_POWER_TYPE_REVISION,
    BUFFER_CLASSIFICATION_REVISION, BUFFER_REVISION, EXTERNAL_XOR_REVISION,
    NAND_CLASSIFICATION_REVISION, NAND_REVISION, NOT_CLASSIFICATION_REVISION, NOT_SIDE_REVISION,
    NOT_TOP_REVISION, OR_CLASSIFICATION_REVISION, OR_REVISION, TERMINAL_REVISION,
    WIRE_TYPE_REVISION, XOR_CLASSIFICATION_REVISION, XOR_COMPACT_REVISION, XOR_REVISION,
};

use crate::blueprint::{
    BlueprintPort, BlueprintPortKind, BlueprintRecords, BlueprintRevision, BlueprintRevisionId,
    ClassificationRevision, ClassificationRevisionId, TypeContract, TypeRevision, TypeRevisionId,
};

use crate::{LogicalSpec, Port, PortDirection, Provenance};

use crate::builtin_definitions::{Cell, cell};

use dustroute_minecraft::{BlockKind, Facing, Pos, WireConnection};

const AND_CELLS: &[Cell] = &[
    cell(Pos::new(0, 0, 0), BlockKind::Solid),
    cell(Pos::new(0, 0, 4), BlockKind::Solid),
    cell(Pos::new(1, 0, 0), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(1, 0, 4), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(2, -1, 0), BlockKind::Solid),
    cell(Pos::new(2, -1, 1), BlockKind::Solid),
    cell(Pos::new(2, -1, 2), BlockKind::Solid),
    cell(Pos::new(2, -1, 3), BlockKind::Solid),
    cell(Pos::new(2, -1, 4), BlockKind::Solid),
    cell(Pos::new(2, 0, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(2, 0, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 0, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 0, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 0, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(3, -1, 0), BlockKind::Solid),
    cell(Pos::new(3, -1, 1), BlockKind::Solid),
    cell(Pos::new(3, -1, 2), BlockKind::Solid),
    cell(Pos::new(3, -1, 3), BlockKind::Solid),
    cell(Pos::new(3, -1, 4), BlockKind::Solid),
    cell(Pos::new(3, 0, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(4, -1, 2), BlockKind::Solid),
    cell(Pos::new(4, 0, 2), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(5, 0, 2), BlockKind::Solid),
    cell(Pos::new(6, 0, 2), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(7, -1, 2), BlockKind::Solid),
    cell(Pos::new(7, 0, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
];

const BUFFER_CELLS: &[Cell] = &[
    cell(Pos::new(0, 0, 0), BlockKind::Solid),
    cell(Pos::new(0, 1, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(1, 0, 0), BlockKind::Solid),
    cell(Pos::new(1, 1, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(2, 0, 0), BlockKind::Solid),
    cell(Pos::new(2, 1, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
];

const NAND_CELLS: &[Cell] = &[
    cell(Pos::new(0, 0, 0), BlockKind::Solid),
    cell(Pos::new(0, 0, 4), BlockKind::Solid),
    cell(Pos::new(1, 0, 0), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(1, 0, 4), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(2, -1, 0), BlockKind::Solid),
    cell(Pos::new(2, -1, 1), BlockKind::Solid),
    cell(Pos::new(2, -1, 2), BlockKind::Solid),
    cell(Pos::new(2, -1, 3), BlockKind::Solid),
    cell(Pos::new(2, -1, 4), BlockKind::Solid),
    cell(Pos::new(2, 0, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(2, 0, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 0, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 0, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 0, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(3, -1, 0), BlockKind::Solid),
    cell(Pos::new(3, -1, 1), BlockKind::Solid),
    cell(Pos::new(3, -1, 2), BlockKind::Solid),
    cell(Pos::new(3, -1, 3), BlockKind::Solid),
    cell(Pos::new(3, -1, 4), BlockKind::Solid),
    cell(Pos::new(3, 0, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
];

const NOT_SIDE_CELLS: &[Cell] = &[
    cell(Pos::new(0, 0, 0), BlockKind::Solid),
    cell(Pos::new(1, 0, 0), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(2, -1, 0), BlockKind::Solid),
    cell(Pos::new(2, 0, 0), BlockKind::RedstoneWire).support(Pos::new(0, -1, 0)),
];

const NOT_TOP_CELLS: &[Cell] = &[
    cell(Pos::new(0, 0, 0), BlockKind::Solid),
    cell(Pos::new(0, 1, 0), BlockKind::RedstoneTorch)
        .facing(Facing::Up)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(1, 0, 0), BlockKind::Solid),
    cell(Pos::new(1, 1, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
];

const OR_CELLS: &[Cell] = &[
    cell(Pos::new(0, 0, 0), BlockKind::Solid),
    cell(Pos::new(0, 0, 1), BlockKind::Solid),
    cell(Pos::new(0, 0, 2), BlockKind::Solid),
    cell(Pos::new(0, 1, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(0, 1, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(1, 0, 0), BlockKind::Solid),
    cell(Pos::new(1, 0, 1), BlockKind::Solid),
    cell(Pos::new(1, 0, 2), BlockKind::Solid),
    cell(Pos::new(1, 1, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(1, 1, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(2, 0, 0), BlockKind::Solid),
    cell(Pos::new(2, 0, 1), BlockKind::Solid),
    cell(Pos::new(2, 0, 2), BlockKind::Solid),
    cell(Pos::new(2, 1, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(2, 1, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 1, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(3, 0, 0), BlockKind::Solid),
    cell(Pos::new(3, 0, 1), BlockKind::Solid),
    cell(Pos::new(3, 0, 2), BlockKind::Solid),
    cell(Pos::new(3, 1, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(4, 0, 0), BlockKind::Solid),
    cell(Pos::new(4, 0, 1), BlockKind::Solid),
    cell(Pos::new(4, 0, 2), BlockKind::Solid),
    cell(Pos::new(4, 1, 1), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(5, 0, 0), BlockKind::Solid),
    cell(Pos::new(5, 0, 1), BlockKind::Solid),
    cell(Pos::new(5, 0, 2), BlockKind::Solid),
    cell(Pos::new(5, 1, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
];

const TERMINAL_CELLS: &[Cell] = &[
    cell(Pos::new(0, 0, 0), BlockKind::Transparent),
    cell(Pos::new(0, 1, 0), BlockKind::RedstoneWire).support(Pos::new(0, -1, 0)),
];

const XOR_COMPACT_CELLS: &[Cell] = &[
    cell(Pos::new(0, 2, 0), BlockKind::Solid),
    cell(Pos::new(0, 2, 6), BlockKind::Solid),
    cell(Pos::new(0, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(0, 3, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(1, 2, 0), BlockKind::Solid),
    cell(Pos::new(1, 2, 6), BlockKind::Solid),
    cell(Pos::new(1, 3, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(1, 3, 6), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(2, 1, 2), BlockKind::Transparent),
    cell(Pos::new(2, 1, 3), BlockKind::Transparent),
    cell(Pos::new(2, 1, 4), BlockKind::Transparent),
    cell(Pos::new(2, 1, 5), BlockKind::Solid),
    cell(Pos::new(2, 2, 0), BlockKind::Solid),
    cell(Pos::new(2, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 2, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 2, 5), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Up),
        ]),
    cell(Pos::new(2, 2, 6), BlockKind::Solid),
    cell(Pos::new(2, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Up),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(2, 3, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Up),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(3, 1, 1), BlockKind::Transparent),
    cell(Pos::new(3, 1, 2), BlockKind::Transparent),
    cell(Pos::new(3, 2, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(3, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(3, 3, 0), BlockKind::Solid),
    cell(Pos::new(3, 3, 6), BlockKind::Solid),
    cell(Pos::new(3, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(3, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(4, 1, 0), BlockKind::Transparent),
    cell(Pos::new(4, 1, 1), BlockKind::Transparent),
    cell(Pos::new(4, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(4, 2, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(4, 3, 0), BlockKind::Transparent),
    cell(Pos::new(4, 3, 6), BlockKind::Transparent),
    cell(Pos::new(4, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(4, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(5, 1, 0), BlockKind::Transparent),
    cell(Pos::new(5, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(5, 3, 0), BlockKind::Transparent),
    cell(Pos::new(5, 3, 6), BlockKind::Transparent),
    cell(Pos::new(5, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(5, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(6, 1, 0), BlockKind::Transparent),
    cell(Pos::new(6, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(6, 3, 0), BlockKind::Transparent),
    cell(Pos::new(6, 3, 6), BlockKind::Transparent),
    cell(Pos::new(6, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(6, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(7, 1, 0), BlockKind::Transparent),
    cell(Pos::new(7, 1, 3), BlockKind::Solid),
    cell(Pos::new(7, 1, 4), BlockKind::Transparent),
    cell(Pos::new(7, 1, 5), BlockKind::Transparent),
    cell(Pos::new(7, 1, 6), BlockKind::Transparent),
    cell(Pos::new(7, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(7, 2, 2), BlockKind::Solid),
    cell(Pos::new(7, 2, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(7, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(7, 2, 5), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(7, 2, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
        ]),
    cell(Pos::new(7, 3, 0), BlockKind::Solid),
    cell(Pos::new(7, 3, 1), BlockKind::Solid),
    cell(Pos::new(7, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(7, 3, 6), BlockKind::Solid),
    cell(Pos::new(7, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Up),
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(7, 4, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(7, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Up),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(8, 1, 0), BlockKind::Solid),
    cell(Pos::new(8, 1, 6), BlockKind::Solid),
    cell(Pos::new(8, 2, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(8, 2, 6), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(8, 4, 0), BlockKind::Solid),
    cell(Pos::new(8, 4, 6), BlockKind::Solid),
    cell(Pos::new(8, 5, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(8, 5, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(9, 2, 0), BlockKind::Solid),
    cell(Pos::new(9, 2, 6), BlockKind::Solid),
    cell(Pos::new(9, 3, 0), BlockKind::RedstoneTorch)
        .facing(Facing::Up)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(9, 3, 6), BlockKind::RedstoneTorch)
        .facing(Facing::Up)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(9, 4, 0), BlockKind::Transparent),
    cell(Pos::new(9, 4, 6), BlockKind::Transparent),
    cell(Pos::new(9, 5, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(9, 5, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(10, 1, 1), BlockKind::Solid),
    cell(Pos::new(10, 1, 2), BlockKind::Transparent),
    cell(Pos::new(10, 1, 3), BlockKind::Transparent),
    cell(Pos::new(10, 1, 4), BlockKind::Transparent),
    cell(Pos::new(10, 2, 0), BlockKind::Solid),
    cell(Pos::new(10, 2, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(10, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(10, 2, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(10, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
        ]),
    cell(Pos::new(10, 2, 6), BlockKind::Solid),
    cell(Pos::new(10, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(10, 3, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(10, 4, 0), BlockKind::Transparent),
    cell(Pos::new(10, 4, 6), BlockKind::Transparent),
    cell(Pos::new(10, 5, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(10, 5, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(11, 1, 4), BlockKind::Transparent),
    cell(Pos::new(11, 1, 6), BlockKind::Solid),
    cell(Pos::new(11, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(11, 2, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(11, 4, 0), BlockKind::Solid),
    cell(Pos::new(11, 4, 6), BlockKind::Solid),
    cell(Pos::new(11, 5, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(11, 5, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(12, 1, 4), BlockKind::Transparent),
    cell(Pos::new(12, 1, 6), BlockKind::Transparent),
    cell(Pos::new(12, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(12, 2, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(12, 3, 0), BlockKind::Solid),
    cell(Pos::new(12, 3, 6), BlockKind::Solid),
    cell(Pos::new(12, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(12, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(13, 1, 4), BlockKind::Transparent),
    cell(Pos::new(13, 1, 6), BlockKind::Transparent),
    cell(Pos::new(13, 2, 0), BlockKind::Solid),
    cell(Pos::new(13, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(13, 2, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(13, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(13, 3, 6), BlockKind::Transparent),
    cell(Pos::new(13, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(14, 1, 0), BlockKind::Solid),
    cell(Pos::new(14, 1, 4), BlockKind::Transparent),
    cell(Pos::new(14, 1, 6), BlockKind::Transparent),
    cell(Pos::new(14, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(14, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(14, 2, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(14, 3, 6), BlockKind::Transparent),
    cell(Pos::new(14, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(15, 1, 0), BlockKind::Solid),
    cell(Pos::new(15, 1, 4), BlockKind::Transparent),
    cell(Pos::new(15, 1, 6), BlockKind::Transparent),
    cell(Pos::new(15, 2, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(15, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(15, 2, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(15, 3, 6), BlockKind::Solid),
    cell(Pos::new(15, 4, 6), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(16, 1, 0), BlockKind::Transparent),
    cell(Pos::new(16, 1, 4), BlockKind::Transparent),
    cell(Pos::new(16, 1, 6), BlockKind::Transparent),
    cell(Pos::new(16, 1, 9), BlockKind::Solid),
    cell(Pos::new(16, 1, 10), BlockKind::Transparent),
    cell(Pos::new(16, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(16, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(16, 2, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(16, 2, 8), BlockKind::Solid),
    cell(Pos::new(16, 2, 9), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(16, 2, 10), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
        ]),
    cell(Pos::new(16, 3, 6), BlockKind::Transparent),
    cell(Pos::new(16, 3, 7), BlockKind::Solid),
    cell(Pos::new(16, 3, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(16, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(16, 4, 7), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(17, 1, 0), BlockKind::Solid),
    cell(Pos::new(17, 1, 4), BlockKind::Solid),
    cell(Pos::new(17, 1, 6), BlockKind::Solid),
    cell(Pos::new(17, 1, 10), BlockKind::Solid),
    cell(Pos::new(17, 2, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(17, 2, 4), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(17, 2, 6), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(17, 2, 10), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(18, 2, 0), BlockKind::Solid),
    cell(Pos::new(18, 2, 4), BlockKind::Solid),
    cell(Pos::new(18, 2, 6), BlockKind::Solid),
    cell(Pos::new(18, 2, 10), BlockKind::Solid),
    cell(Pos::new(19, 2, 0), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(19, 2, 4), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(19, 2, 6), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(19, 2, 10), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(20, 1, 0), BlockKind::Solid),
    cell(Pos::new(20, 1, 1), BlockKind::Solid),
    cell(Pos::new(20, 1, 2), BlockKind::Solid),
    cell(Pos::new(20, 1, 3), BlockKind::Solid),
    cell(Pos::new(20, 1, 4), BlockKind::Solid),
    cell(Pos::new(20, 1, 6), BlockKind::Solid),
    cell(Pos::new(20, 1, 7), BlockKind::Solid),
    cell(Pos::new(20, 1, 8), BlockKind::Solid),
    cell(Pos::new(20, 1, 9), BlockKind::Solid),
    cell(Pos::new(20, 1, 10), BlockKind::Solid),
    cell(Pos::new(20, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 7), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 9), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 10), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(21, 1, 0), BlockKind::Solid),
    cell(Pos::new(21, 1, 1), BlockKind::Solid),
    cell(Pos::new(21, 1, 2), BlockKind::Solid),
    cell(Pos::new(21, 1, 3), BlockKind::Solid),
    cell(Pos::new(21, 1, 4), BlockKind::Solid),
    cell(Pos::new(21, 1, 6), BlockKind::Solid),
    cell(Pos::new(21, 1, 7), BlockKind::Solid),
    cell(Pos::new(21, 1, 8), BlockKind::Solid),
    cell(Pos::new(21, 1, 9), BlockKind::Solid),
    cell(Pos::new(21, 1, 10), BlockKind::Solid),
    cell(Pos::new(21, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(21, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(22, 1, 2), BlockKind::Solid),
    cell(Pos::new(22, 1, 8), BlockKind::Solid),
    cell(Pos::new(22, 2, 2), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(22, 2, 8), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(23, 2, 2), BlockKind::Solid),
    cell(Pos::new(23, 2, 8), BlockKind::Solid),
    cell(Pos::new(24, 2, 2), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(24, 2, 8), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(25, 1, 1), BlockKind::Solid),
    cell(Pos::new(25, 1, 2), BlockKind::Solid),
    cell(Pos::new(25, 1, 8), BlockKind::Solid),
    cell(Pos::new(25, 2, 0), BlockKind::Solid),
    cell(Pos::new(25, 2, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(25, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(25, 2, 7), BlockKind::Solid),
    cell(Pos::new(25, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(25, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(25, 3, 2), BlockKind::Solid),
    cell(Pos::new(25, 3, 3), BlockKind::Transparent),
    cell(Pos::new(25, 3, 4), BlockKind::Transparent),
    cell(Pos::new(25, 3, 5), BlockKind::Transparent),
    cell(Pos::new(25, 3, 6), BlockKind::Solid),
    cell(Pos::new(25, 3, 7), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(25, 4, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(25, 4, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(25, 4, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(25, 4, 5), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(25, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 0), BlockKind::Transparent),
    cell(Pos::new(26, 2, 2), BlockKind::Solid),
    cell(Pos::new(26, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(26, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(27, 2, 0), BlockKind::Solid),
    cell(Pos::new(27, 2, 1), BlockKind::Solid),
    cell(Pos::new(27, 2, 2), BlockKind::Solid),
    cell(Pos::new(27, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(27, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(28, 2, 0), BlockKind::Solid),
    cell(Pos::new(28, 2, 1), BlockKind::Solid),
    cell(Pos::new(28, 2, 2), BlockKind::Solid),
    cell(Pos::new(28, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(28, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(29, 2, 0), BlockKind::Solid),
    cell(Pos::new(29, 2, 1), BlockKind::Solid),
    cell(Pos::new(29, 2, 2), BlockKind::Solid),
    cell(Pos::new(29, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(29, 3, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(29, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(30, 2, 0), BlockKind::Solid),
    cell(Pos::new(30, 2, 1), BlockKind::Solid),
    cell(Pos::new(30, 2, 2), BlockKind::Solid),
    cell(Pos::new(30, 3, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(31, 2, 0), BlockKind::Solid),
    cell(Pos::new(31, 2, 1), BlockKind::Solid),
    cell(Pos::new(31, 2, 2), BlockKind::Solid),
    cell(Pos::new(31, 3, 1), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(32, 2, 0), BlockKind::Solid),
    cell(Pos::new(32, 2, 1), BlockKind::Solid),
    cell(Pos::new(32, 2, 2), BlockKind::Solid),
    cell(Pos::new(32, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(32, 3, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(33, 2, 0), BlockKind::Transparent),
    cell(Pos::new(33, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(34, 2, 0), BlockKind::Transparent),
    cell(Pos::new(34, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(35, 2, 0), BlockKind::Transparent),
    cell(Pos::new(35, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(36, 2, 0), BlockKind::Solid),
    cell(Pos::new(36, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(37, 2, 0), BlockKind::Solid),
    cell(Pos::new(37, 3, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(38, 2, 0), BlockKind::Solid),
    cell(Pos::new(38, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
];

const XOR_CELLS: &[Cell] = &[
    cell(Pos::new(0, 2, 0), BlockKind::Solid),
    cell(Pos::new(0, 2, 8), BlockKind::Solid),
    cell(Pos::new(0, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(0, 3, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(1, 2, 0), BlockKind::Solid),
    cell(Pos::new(1, 2, 8), BlockKind::Solid),
    cell(Pos::new(1, 3, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(1, 3, 8), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(2, 2, 0), BlockKind::Solid),
    cell(Pos::new(2, 2, 8), BlockKind::Solid),
    cell(Pos::new(2, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Up),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(2, 3, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Up),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(3, 3, 0), BlockKind::Solid),
    cell(Pos::new(3, 3, 8), BlockKind::Solid),
    cell(Pos::new(3, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(3, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(4, 3, 0), BlockKind::Transparent),
    cell(Pos::new(4, 3, 8), BlockKind::Transparent),
    cell(Pos::new(4, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(4, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(5, 3, 0), BlockKind::Transparent),
    cell(Pos::new(5, 3, 8), BlockKind::Transparent),
    cell(Pos::new(5, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(5, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(6, 3, 0), BlockKind::Transparent),
    cell(Pos::new(6, 3, 8), BlockKind::Transparent),
    cell(Pos::new(6, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(6, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(7, 3, 0), BlockKind::Transparent),
    cell(Pos::new(7, 3, 8), BlockKind::Transparent),
    cell(Pos::new(7, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(7, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(8, 1, 0), BlockKind::Transparent),
    cell(Pos::new(8, 1, 1), BlockKind::Transparent),
    cell(Pos::new(8, 1, 2), BlockKind::Solid),
    cell(Pos::new(8, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(8, 2, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(8, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Up),
        ]),
    cell(Pos::new(8, 2, 3), BlockKind::Solid),
    cell(Pos::new(8, 3, 0), BlockKind::Transparent),
    cell(Pos::new(8, 3, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Up),
        ]),
    cell(Pos::new(8, 3, 8), BlockKind::Transparent),
    cell(Pos::new(8, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(8, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(9, 1, 0), BlockKind::Transparent),
    cell(Pos::new(9, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(9, 3, 0), BlockKind::Transparent),
    cell(Pos::new(9, 3, 3), BlockKind::Solid),
    cell(Pos::new(9, 3, 4), BlockKind::Solid),
    cell(Pos::new(9, 3, 5), BlockKind::Transparent),
    cell(Pos::new(9, 3, 6), BlockKind::Transparent),
    cell(Pos::new(9, 3, 7), BlockKind::Transparent),
    cell(Pos::new(9, 3, 8), BlockKind::Transparent),
    cell(Pos::new(9, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(9, 4, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(9, 4, 4), BlockKind::Repeater)
        .facing(Facing::North)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(9, 4, 5), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(9, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(9, 4, 7), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(9, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(10, 1, 0), BlockKind::Transparent),
    cell(Pos::new(10, 1, 3), BlockKind::Solid),
    cell(Pos::new(10, 1, 4), BlockKind::Transparent),
    cell(Pos::new(10, 1, 5), BlockKind::Solid),
    cell(Pos::new(10, 1, 6), BlockKind::Transparent),
    cell(Pos::new(10, 1, 7), BlockKind::Transparent),
    cell(Pos::new(10, 1, 8), BlockKind::Transparent),
    cell(Pos::new(10, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(10, 2, 2), BlockKind::Solid),
    cell(Pos::new(10, 2, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(10, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(10, 2, 5), BlockKind::Repeater)
        .facing(Facing::South)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(10, 2, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(10, 2, 7), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(10, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
        ]),
    cell(Pos::new(10, 3, 0), BlockKind::Solid),
    cell(Pos::new(10, 3, 1), BlockKind::Solid),
    cell(Pos::new(10, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(10, 3, 8), BlockKind::Solid),
    cell(Pos::new(10, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Up),
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(10, 4, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(10, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Up),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(11, 1, 0), BlockKind::Solid),
    cell(Pos::new(11, 1, 8), BlockKind::Solid),
    cell(Pos::new(11, 2, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(11, 2, 8), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(11, 4, 0), BlockKind::Solid),
    cell(Pos::new(11, 4, 8), BlockKind::Solid),
    cell(Pos::new(11, 5, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(11, 5, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(12, 2, 0), BlockKind::Solid),
    cell(Pos::new(12, 2, 8), BlockKind::Solid),
    cell(Pos::new(12, 3, 0), BlockKind::RedstoneTorch)
        .facing(Facing::Up)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(12, 3, 8), BlockKind::RedstoneTorch)
        .facing(Facing::Up)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(12, 4, 0), BlockKind::Transparent),
    cell(Pos::new(12, 4, 8), BlockKind::Transparent),
    cell(Pos::new(12, 5, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(12, 5, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(13, 2, 0), BlockKind::Solid),
    cell(Pos::new(13, 2, 1), BlockKind::Solid),
    cell(Pos::new(13, 2, 8), BlockKind::Solid),
    cell(Pos::new(13, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(13, 3, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Up),
        ]),
    cell(Pos::new(13, 3, 2), BlockKind::Solid),
    cell(Pos::new(13, 3, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(13, 4, 0), BlockKind::Solid),
    cell(Pos::new(13, 4, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
        ]),
    cell(Pos::new(13, 4, 8), BlockKind::Solid),
    cell(Pos::new(13, 5, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(13, 5, 8), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(14, 1, 8), BlockKind::Solid),
    cell(Pos::new(14, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(14, 3, 2), BlockKind::Transparent),
    cell(Pos::new(14, 4, 0), BlockKind::Solid),
    cell(Pos::new(14, 4, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(14, 4, 8), BlockKind::Solid),
    cell(Pos::new(14, 5, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(14, 5, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(15, 1, 8), BlockKind::Transparent),
    cell(Pos::new(15, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(15, 3, 0), BlockKind::Solid),
    cell(Pos::new(15, 3, 2), BlockKind::Transparent),
    cell(Pos::new(15, 3, 8), BlockKind::Solid),
    cell(Pos::new(15, 4, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(15, 4, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(15, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(16, 1, 8), BlockKind::Transparent),
    cell(Pos::new(16, 2, 0), BlockKind::Solid),
    cell(Pos::new(16, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(16, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(16, 3, 2), BlockKind::Transparent),
    cell(Pos::new(16, 3, 8), BlockKind::Transparent),
    cell(Pos::new(16, 4, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(16, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(17, 1, 0), BlockKind::Solid),
    cell(Pos::new(17, 1, 8), BlockKind::Transparent),
    cell(Pos::new(17, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(17, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(17, 3, 1), BlockKind::Transparent),
    cell(Pos::new(17, 3, 2), BlockKind::Transparent),
    cell(Pos::new(17, 3, 8), BlockKind::Transparent),
    cell(Pos::new(17, 4, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(17, 4, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(17, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(18, 1, 0), BlockKind::Transparent),
    cell(Pos::new(18, 1, 8), BlockKind::Transparent),
    cell(Pos::new(18, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(18, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(18, 3, 1), BlockKind::Transparent),
    cell(Pos::new(18, 3, 8), BlockKind::Transparent),
    cell(Pos::new(18, 4, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(18, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(19, 1, 0), BlockKind::Transparent),
    cell(Pos::new(19, 1, 8), BlockKind::Transparent),
    cell(Pos::new(19, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(19, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(19, 3, 1), BlockKind::Transparent),
    cell(Pos::new(19, 3, 8), BlockKind::Transparent),
    cell(Pos::new(19, 4, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(19, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(20, 1, 0), BlockKind::Transparent),
    cell(Pos::new(20, 1, 8), BlockKind::Transparent),
    cell(Pos::new(20, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(20, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(20, 3, 1), BlockKind::Transparent),
    cell(Pos::new(20, 3, 8), BlockKind::Transparent),
    cell(Pos::new(20, 4, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(20, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(21, 1, 0), BlockKind::Transparent),
    cell(Pos::new(21, 1, 8), BlockKind::Transparent),
    cell(Pos::new(21, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(21, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(21, 3, 1), BlockKind::Solid),
    cell(Pos::new(21, 3, 8), BlockKind::Solid),
    cell(Pos::new(21, 4, 1), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(21, 4, 8), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(22, 1, 0), BlockKind::Transparent),
    cell(Pos::new(22, 1, 3), BlockKind::Solid),
    cell(Pos::new(22, 1, 4), BlockKind::Transparent),
    cell(Pos::new(22, 1, 8), BlockKind::Transparent),
    cell(Pos::new(22, 1, 11), BlockKind::Solid),
    cell(Pos::new(22, 1, 12), BlockKind::Transparent),
    cell(Pos::new(22, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(22, 2, 2), BlockKind::Solid),
    cell(Pos::new(22, 2, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(22, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
        ]),
    cell(Pos::new(22, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(22, 2, 10), BlockKind::Solid),
    cell(Pos::new(22, 2, 11), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(22, 2, 12), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
        ]),
    cell(Pos::new(22, 3, 1), BlockKind::Solid),
    cell(Pos::new(22, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(22, 3, 8), BlockKind::Transparent),
    cell(Pos::new(22, 3, 9), BlockKind::Solid),
    cell(Pos::new(22, 3, 10), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(22, 4, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(22, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(22, 4, 9), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(23, 1, 0), BlockKind::Solid),
    cell(Pos::new(23, 1, 4), BlockKind::Solid),
    cell(Pos::new(23, 1, 8), BlockKind::Solid),
    cell(Pos::new(23, 1, 12), BlockKind::Solid),
    cell(Pos::new(23, 2, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(23, 2, 4), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(23, 2, 8), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(23, 2, 12), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(24, 2, 0), BlockKind::Solid),
    cell(Pos::new(24, 2, 4), BlockKind::Solid),
    cell(Pos::new(24, 2, 8), BlockKind::Solid),
    cell(Pos::new(24, 2, 12), BlockKind::Solid),
    cell(Pos::new(25, 2, 0), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(25, 2, 4), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(25, 2, 8), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(25, 2, 12), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(26, 1, 0), BlockKind::Solid),
    cell(Pos::new(26, 1, 1), BlockKind::Solid),
    cell(Pos::new(26, 1, 2), BlockKind::Solid),
    cell(Pos::new(26, 1, 3), BlockKind::Solid),
    cell(Pos::new(26, 1, 4), BlockKind::Solid),
    cell(Pos::new(26, 1, 8), BlockKind::Solid),
    cell(Pos::new(26, 1, 9), BlockKind::Solid),
    cell(Pos::new(26, 1, 10), BlockKind::Solid),
    cell(Pos::new(26, 1, 11), BlockKind::Solid),
    cell(Pos::new(26, 1, 12), BlockKind::Solid),
    cell(Pos::new(26, 2, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 9), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 10), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 11), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(26, 2, 12), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(27, 1, 0), BlockKind::Solid),
    cell(Pos::new(27, 1, 1), BlockKind::Solid),
    cell(Pos::new(27, 1, 2), BlockKind::Solid),
    cell(Pos::new(27, 1, 3), BlockKind::Solid),
    cell(Pos::new(27, 1, 4), BlockKind::Solid),
    cell(Pos::new(27, 1, 8), BlockKind::Solid),
    cell(Pos::new(27, 1, 9), BlockKind::Solid),
    cell(Pos::new(27, 1, 10), BlockKind::Solid),
    cell(Pos::new(27, 1, 11), BlockKind::Solid),
    cell(Pos::new(27, 1, 12), BlockKind::Solid),
    cell(Pos::new(27, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(27, 2, 10), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(28, 1, 2), BlockKind::Solid),
    cell(Pos::new(28, 1, 10), BlockKind::Solid),
    cell(Pos::new(28, 2, 2), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(28, 2, 10), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(29, 2, 2), BlockKind::Solid),
    cell(Pos::new(29, 2, 10), BlockKind::Solid),
    cell(Pos::new(30, 2, 2), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(30, 2, 10), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(31, 1, 2), BlockKind::Solid),
    cell(Pos::new(31, 1, 10), BlockKind::Solid),
    cell(Pos::new(31, 2, 0), BlockKind::Transparent),
    cell(Pos::new(31, 2, 1), BlockKind::Solid),
    cell(Pos::new(31, 2, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(31, 2, 9), BlockKind::Solid),
    cell(Pos::new(31, 2, 10), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(31, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(31, 3, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(31, 3, 3), BlockKind::Transparent),
    cell(Pos::new(31, 3, 4), BlockKind::Transparent),
    cell(Pos::new(31, 3, 5), BlockKind::Transparent),
    cell(Pos::new(31, 3, 6), BlockKind::Transparent),
    cell(Pos::new(31, 3, 7), BlockKind::Transparent),
    cell(Pos::new(31, 3, 8), BlockKind::Solid),
    cell(Pos::new(31, 3, 9), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Up),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(31, 4, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(31, 4, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(31, 4, 5), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(31, 4, 6), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(31, 4, 7), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(31, 4, 8), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(32, 2, 0), BlockKind::Transparent),
    cell(Pos::new(32, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(32, 3, 2), BlockKind::Solid),
    cell(Pos::new(32, 3, 3), BlockKind::Transparent),
    cell(Pos::new(32, 4, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(32, 4, 3), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(33, 2, 0), BlockKind::Transparent),
    cell(Pos::new(33, 2, 2), BlockKind::Solid),
    cell(Pos::new(33, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(33, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Up),
        ]),
    cell(Pos::new(34, 2, 0), BlockKind::Transparent),
    cell(Pos::new(34, 2, 2), BlockKind::Solid),
    cell(Pos::new(34, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(34, 3, 2), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(35, 2, 0), BlockKind::Transparent),
    cell(Pos::new(35, 2, 2), BlockKind::Transparent),
    cell(Pos::new(35, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(35, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(36, 2, 0), BlockKind::Solid),
    cell(Pos::new(36, 2, 1), BlockKind::Solid),
    cell(Pos::new(36, 2, 2), BlockKind::Solid),
    cell(Pos::new(36, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(36, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(37, 2, 0), BlockKind::Solid),
    cell(Pos::new(37, 2, 1), BlockKind::Solid),
    cell(Pos::new(37, 2, 2), BlockKind::Solid),
    cell(Pos::new(37, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(37, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(38, 2, 0), BlockKind::Solid),
    cell(Pos::new(38, 2, 1), BlockKind::Solid),
    cell(Pos::new(38, 2, 2), BlockKind::Solid),
    cell(Pos::new(38, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::South, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(38, 3, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(38, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(39, 2, 0), BlockKind::Solid),
    cell(Pos::new(39, 2, 1), BlockKind::Solid),
    cell(Pos::new(39, 2, 2), BlockKind::Solid),
    cell(Pos::new(39, 3, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(40, 2, 0), BlockKind::Solid),
    cell(Pos::new(40, 2, 1), BlockKind::Solid),
    cell(Pos::new(40, 2, 2), BlockKind::Solid),
    cell(Pos::new(40, 3, 1), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(41, 2, 0), BlockKind::Solid),
    cell(Pos::new(41, 2, 1), BlockKind::Solid),
    cell(Pos::new(41, 2, 2), BlockKind::Solid),
    cell(Pos::new(41, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(41, 3, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(42, 2, 0), BlockKind::Transparent),
    cell(Pos::new(42, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(43, 2, 0), BlockKind::Transparent),
    cell(Pos::new(43, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(44, 2, 0), BlockKind::Transparent),
    cell(Pos::new(44, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(45, 2, 0), BlockKind::Transparent),
    cell(Pos::new(45, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(46, 2, 0), BlockKind::Transparent),
    cell(Pos::new(46, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(47, 2, 0), BlockKind::Transparent),
    cell(Pos::new(47, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(48, 2, 0), BlockKind::Solid),
    cell(Pos::new(48, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
    cell(Pos::new(49, 2, 0), BlockKind::Solid),
    cell(Pos::new(49, 3, 0), BlockKind::Repeater)
        .facing(Facing::East)
        .delay(1)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(50, 2, 0), BlockKind::Solid),
    cell(Pos::new(50, 3, 0), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::East, WireConnection::Side),
            (Facing::West, WireConnection::Side),
        ]),
];

const EXTERNAL_XOR_CELLS: &[Cell] = &[
    cell(Pos::new(0, 0, 1), BlockKind::RedstoneTorch)
        .facing(Facing::West)
        .support(Pos::new(1, 0, 0)),
    cell(Pos::new(0, 1, 1), BlockKind::Solid),
    cell(Pos::new(0, 2, 1), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[]),
    cell(Pos::new(0, 2, 2), BlockKind::Solid),
    cell(Pos::new(0, 3, 1), BlockKind::RedstoneTorch)
        .facing(Facing::West)
        .support(Pos::new(1, 0, 0)),
    cell(Pos::new(1, 0, 1), BlockKind::Solid),
    cell(Pos::new(1, 0, 2), BlockKind::Solid),
    cell(Pos::new(1, 1, 1), BlockKind::RedstoneTorch)
        .facing(Facing::Up)
        .support(Pos::new(0, -1, 0)),
    cell(Pos::new(1, 1, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(1, 1, 3), BlockKind::Solid),
    cell(Pos::new(1, 2, 2), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(1, 3, 1), BlockKind::Solid),
    cell(Pos::new(1, 3, 2), BlockKind::Solid),
    cell(Pos::new(2, 1, 3), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(2, 1, 4), BlockKind::Solid),
    cell(Pos::new(2, 2, 2), BlockKind::Solid),
    cell(Pos::new(2, 2, 3), BlockKind::Solid),
    cell(Pos::new(2, 2, 4), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[]),
    cell(Pos::new(2, 3, 1), BlockKind::RedstoneTorch)
        .facing(Facing::East)
        .support(Pos::new(-1, 0, 0)),
    cell(Pos::new(2, 3, 2), BlockKind::RedstoneWire)
        .support(Pos::new(0, -1, 0))
        .wires(&[
            (Facing::North, WireConnection::Side),
            (Facing::South, WireConnection::Side),
        ]),
    cell(Pos::new(2, 3, 3), BlockKind::Solid),
    cell(Pos::new(2, 3, 4), BlockKind::RedstoneTorch)
        .facing(Facing::South)
        .support(Pos::new(0, 0, -1)),
    cell(Pos::new(2, 4, 4), BlockKind::Solid),
];

pub(super) fn records() -> BlueprintRecords {
    BlueprintRecords { types: vec![TypeRevision { id: TypeRevisionId::new(BLOCK_POWER_TYPE_REVISION).expect("fixed built-in identifier"), name: "Powered block output".into(), contract: TypeContract::Signal { port_kind: BlueprintPortKind::BlockPower } }, TypeRevision { id: TypeRevisionId::new(WIRE_TYPE_REVISION).expect("fixed built-in identifier"), name: "Redstone wire output".into(), contract: TypeContract::Signal { port_kind: BlueprintPortKind::Wire } }], classifications: vec![ClassificationRevision { id: ClassificationRevisionId::new(AND_CLASSIFICATION_REVISION).expect("fixed built-in identifier"), name: "AND".into(), logical_claim: Some(LogicalSpec { ports: vec![Port { name: "a".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "b".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "out".into(), direction: PortDirection::Output, bit_width: 1 }], truth_table: vec![vec![false, false, false], vec![true, false, false], vec![false, true, false], vec![true, true, true]], stateful: false }) }, ClassificationRevision { id: ClassificationRevisionId::new(BUFFER_CLASSIFICATION_REVISION).expect("fixed built-in identifier"), name: "Buffer".into(), logical_claim: Some(LogicalSpec { ports: vec![Port { name: "in".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "out".into(), direction: PortDirection::Output, bit_width: 1 }], truth_table: vec![vec![false, false], vec![true, true]], stateful: false }) }, ClassificationRevision { id: ClassificationRevisionId::new(NAND_CLASSIFICATION_REVISION).expect("fixed built-in identifier"), name: "NAND".into(), logical_claim: Some(LogicalSpec { ports: vec![Port { name: "a".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "b".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "out".into(), direction: PortDirection::Output, bit_width: 1 }], truth_table: vec![vec![false, false, true], vec![true, false, true], vec![false, true, true], vec![true, true, false]], stateful: false }) }, ClassificationRevision { id: ClassificationRevisionId::new(NOT_CLASSIFICATION_REVISION).expect("fixed built-in identifier"), name: "NOT".into(), logical_claim: Some(LogicalSpec { ports: vec![Port { name: "a".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "out".into(), direction: PortDirection::Output, bit_width: 1 }], truth_table: vec![vec![false, true], vec![true, false]], stateful: false }) }, ClassificationRevision { id: ClassificationRevisionId::new(OR_CLASSIFICATION_REVISION).expect("fixed built-in identifier"), name: "OR".into(), logical_claim: Some(LogicalSpec { ports: vec![Port { name: "a".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "b".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "out".into(), direction: PortDirection::Output, bit_width: 1 }], truth_table: vec![vec![false, false, false], vec![true, false, true], vec![false, true, true], vec![true, true, true]], stateful: false }) }, ClassificationRevision { id: ClassificationRevisionId::new(XOR_CLASSIFICATION_REVISION).expect("fixed built-in identifier"), name: "XOR".into(), logical_claim: Some(LogicalSpec { ports: vec![Port { name: "a".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "b".into(), direction: PortDirection::Input, bit_width: 1 }, Port { name: "out".into(), direction: PortDirection::Output, bit_width: 1 }], truth_table: vec![vec![false, false, false], vec![true, false, true], vec![false, true, true], vec![true, true, false]], stateful: false }) }], revisions: vec![BlueprintRevision { id: BlueprintRevisionId::new(AND_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "and_demorgan_repeater".into(), classifications: vec![ClassificationRevisionId::new(AND_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: AND_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "a".into(), direction: PortDirection::Input, position: Pos::new(0, 0, 0), kind: BlueprintPortKind::BlockPower, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "b".into(), direction: PortDirection::Input, position: Pos::new(0, 0, 4), kind: BlueprintPortKind::BlockPower, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(7, 0, 2), kind: BlueprintPortKind::Wire, facing: Some(Facing::East), required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "DustRoute".into(), source_url: None, license: Some("Apache-2.0".into()), retrieved_on: None }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }, BlueprintRevision { id: BlueprintRevisionId::new(BUFFER_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "buffer".into(), classifications: vec![ClassificationRevisionId::new(BUFFER_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: BUFFER_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "in".into(), direction: PortDirection::Input, position: Pos::new(0, 1, 0), kind: BlueprintPortKind::Wire, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(2, 1, 0), kind: BlueprintPortKind::Wire, facing: Some(Facing::East), required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "DustRoute".into(), source_url: None, license: Some("Apache-2.0".into()), retrieved_on: None }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }, BlueprintRevision { id: BlueprintRevisionId::new(NAND_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "nand_torch_merge".into(), classifications: vec![ClassificationRevisionId::new(NAND_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: NAND_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "a".into(), direction: PortDirection::Input, position: Pos::new(0, 0, 0), kind: BlueprintPortKind::BlockPower, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "b".into(), direction: PortDirection::Input, position: Pos::new(0, 0, 4), kind: BlueprintPortKind::BlockPower, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(3, 0, 2), kind: BlueprintPortKind::Wire, facing: Some(Facing::East), required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "DustRoute".into(), source_url: None, license: Some("Apache-2.0".into()), retrieved_on: None }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }, BlueprintRevision { id: BlueprintRevisionId::new(NOT_SIDE_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "not_torch_block_power".into(), classifications: vec![ClassificationRevisionId::new(NOT_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: NOT_SIDE_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "a".into(), direction: PortDirection::Input, position: Pos::new(0, 0, 0), kind: BlueprintPortKind::BlockPower, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(2, 0, 0), kind: BlueprintPortKind::Wire, facing: Some(Facing::East), required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "DustRoute".into(), source_url: None, license: Some("Apache-2.0".into()), retrieved_on: None }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }, BlueprintRevision { id: BlueprintRevisionId::new(NOT_TOP_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "not_torch_top".into(), classifications: vec![ClassificationRevisionId::new(NOT_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: NOT_TOP_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "a".into(), direction: PortDirection::Input, position: Pos::new(0, 0, 0), kind: BlueprintPortKind::BlockPower, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(1, 1, 0), kind: BlueprintPortKind::Wire, facing: Some(Facing::East), required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "DustRoute".into(), source_url: None, license: Some("Apache-2.0".into()), retrieved_on: None }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }, BlueprintRevision { id: BlueprintRevisionId::new(OR_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "or_dust_buffered".into(), classifications: vec![ClassificationRevisionId::new(OR_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: OR_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "a".into(), direction: PortDirection::Input, position: Pos::new(0, 1, 0), kind: BlueprintPortKind::Wire, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "b".into(), direction: PortDirection::Input, position: Pos::new(0, 1, 2), kind: BlueprintPortKind::Wire, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(5, 1, 1), kind: BlueprintPortKind::Wire, facing: Some(Facing::East), required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "DustRoute".into(), source_url: None, license: Some("Apache-2.0".into()), retrieved_on: None }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }, BlueprintRevision { id: BlueprintRevisionId::new(TERMINAL_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "terminal".into(), classifications: vec![ClassificationRevisionId::new(BUFFER_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: TERMINAL_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "in".into(), direction: PortDirection::Input, position: Pos::new(0, 1, 0), kind: BlueprintPortKind::Wire, facing: None, required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(0, 1, 0), kind: BlueprintPortKind::Wire, facing: None, required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "DustRoute".into(), source_url: None, license: Some("Apache-2.0".into()), retrieved_on: None }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }, BlueprintRevision { id: BlueprintRevisionId::new(XOR_COMPACT_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "dustroute.xor.compact_compiled.1_21_11".into(), classifications: vec![ClassificationRevisionId::new(XOR_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: XOR_COMPACT_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "a".into(), direction: PortDirection::Input, position: Pos::new(0, 3, 0), kind: BlueprintPortKind::Wire, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "b".into(), direction: PortDirection::Input, position: Pos::new(0, 3, 6), kind: BlueprintPortKind::Wire, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(38, 3, 0), kind: BlueprintPortKind::Wire, facing: Some(Facing::East), required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "DustRoute".into(), source_url: None, license: Some("Apache-2.0".into()), retrieved_on: None }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }, BlueprintRevision { id: BlueprintRevisionId::new(XOR_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "dustroute.xor.compiled_baseline.1_21_11".into(), classifications: vec![ClassificationRevisionId::new(XOR_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: XOR_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "a".into(), direction: PortDirection::Input, position: Pos::new(0, 3, 0), kind: BlueprintPortKind::Wire, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "b".into(), direction: PortDirection::Input, position: Pos::new(0, 3, 8), kind: BlueprintPortKind::Wire, facing: Some(Facing::West), required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(50, 3, 0), kind: BlueprintPortKind::Wire, facing: Some(Facing::East), required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "DustRoute".into(), source_url: None, license: Some("Apache-2.0".into()), retrieved_on: None }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }, BlueprintRevision { id: BlueprintRevisionId::new(EXTERNAL_XOR_REVISION).expect("fixed built-in identifier"), parents: vec![], name: "redstone-compiler.xor-generated".into(), classifications: vec![ClassificationRevisionId::new(XOR_CLASSIFICATION_REVISION).expect("fixed built-in identifier")], blocks: EXTERNAL_XOR_CELLS.iter().copied().map(Cell::positioned).collect(), ports: vec![BlueprintPort { name: "a".into(), direction: PortDirection::Input, position: Pos::new(1, 0, 1), kind: BlueprintPortKind::BlockPower, facing: Some(Facing::North), required_source_types: vec![] }, BlueprintPort { name: "b".into(), direction: PortDirection::Input, position: Pos::new(1, 3, 1), kind: BlueprintPortKind::BlockPower, facing: Some(Facing::North), required_source_types: vec![] }, BlueprintPort { name: "out".into(), direction: PortDirection::Output, position: Pos::new(2, 2, 4), kind: BlueprintPortKind::Wire, facing: Some(Facing::South), required_source_types: vec![] }], static_type_bindings: vec![], provenance: Provenance { author: "Redstone-Compiler contributors".into(), source_url: Some("https://github.com/Redstone-Compiler/redstone-compiler/blob/cc997732b82d957a8b5cc80d14c07b375562dd9d/test/xor-generated.nbt".into()), license: Some("MIT".into()), retrieved_on: Some("2026-08-31".into()) }, inclusions: vec![], connections: vec![], port_bindings: vec![], behavior_bindings: vec![], required_laws: vec![], initial_layout: None, law: None }], assemblies: vec![] }
}
