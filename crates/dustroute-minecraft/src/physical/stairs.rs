//! Dry stair states and the target's pure, neighbor-dependent shape selection.
//! Names are admitted by the passive registry; a suffix alone grants nothing.
use crate::{Block, Facing, RotationY};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum StairShape {
    Straight,
    InnerLeft,
    InnerRight,
    OuterLeft,
    OuterRight,
}

/// Horizontal full faces relative to the stair's high side (native facing).
/// Outer corners have no full vertical face. The base half supplies Up/Down.
const SHAPES: [(&str, u8); 5] = [
    ("straight", 1),
    ("inner_left", 1 | 2),
    ("inner_right", 1 | 4),
    ("outer_left", 0),
    ("outer_right", 0),
];

impl StairShape {
    pub const ALL: [Self; 5] = [
        Self::Straight,
        Self::InnerLeft,
        Self::InnerRight,
        Self::OuterLeft,
        Self::OuterRight,
    ];
    pub const fn name(self) -> &'static str {
        SHAPES[self as usize].0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StairState {
    pub facing: Facing,
    pub top: bool,
    pub shape: StairShape,
}

impl StairState {
    pub(super) fn parse(block: &Block) -> Option<Self> {
        let p = &block.observed_properties;
        if p.len() != 4 || p.get("waterlogged")?.as_str() != "false" {
            return None;
        }
        let facing = match p.get("facing")?.as_str() {
            "north" => Facing::North,
            "east" => Facing::East,
            "south" => Facing::South,
            "west" => Facing::West,
            _ => return None,
        };
        if block.facing.is_some_and(|f| f != facing) {
            return None;
        }
        let top = match p.get("half")?.as_str() {
            "top" => true,
            "bottom" => false,
            _ => return None,
        };
        let shape = StairShape::ALL
            .into_iter()
            .find(|s| s.name() == p.get("shape").map(String::as_str).unwrap_or(""))?;
        Some(Self { facing, top, shape })
    }

    pub fn full_face(self, face: Facing) -> bool {
        if face == if self.top { Facing::Up } else { Facing::Down } {
            return true;
        }
        let bits = SHAPES[self.shape as usize].1;
        (bits & 1 != 0 && face == self.facing)
            || (bits & 2 != 0 && face == RotationY::R270.facing(self.facing))
            || (bits & 4 != 0 && face == RotationY::R90.facing(self.facing))
    }

    /// StairsBlock.getStairShape: front outer corner takes precedence over a
    /// rear inner corner. A parallel side stair can suppress either corner.
    /// Preserve native short-circuit reads; unknown consumed neighbors fail.
    pub fn neighbor_shape<E>(
        self,
        mut neighbor: impl FnMut(Facing) -> Result<Option<Self>, E>,
    ) -> Result<StairShape, E> {
        let left = RotationY::R270.facing(self.facing);
        let perpendicular = |other: Self| {
            other.top == self.top
                && other.facing != self.facing
                && other.facing != self.facing.opposite()
        };
        let same_orientation = |other: Self| other.top == self.top && other.facing == self.facing;
        if let Some(front) = neighbor(self.facing)?.filter(|s| perpendicular(*s))
            && !neighbor(front.facing.opposite())?.is_some_and(same_orientation)
        {
            return Ok(if front.facing == left {
                StairShape::OuterLeft
            } else {
                StairShape::OuterRight
            });
        }
        if let Some(back) = neighbor(self.facing.opposite())?.filter(|s| perpendicular(*s))
            && !neighbor(back.facing)?.is_some_and(same_orientation)
        {
            return Ok(if back.facing == left {
                StairShape::InnerLeft
            } else {
                StairShape::InnerRight
            });
        }
        Ok(StairShape::Straight)
    }
}

pub fn state(block: &Block) -> Option<StairState> {
    (super::of_block(block)?.spec().shape == super::Shape::Stairs)
        .then(|| StairState::parse(block))
        .flatten()
}
