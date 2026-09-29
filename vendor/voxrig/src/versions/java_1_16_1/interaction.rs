//! Block positions, faces, digging, placement, and block raycast results.

use crate::versions::java_1_16_1::protocol::get_varint;
use anyhow::{Context, Result};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::Cursor;

use crate::versions::java_1_16_1::physics::Vec3;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// State and protocol data represented by `BlockPos`.
pub struct BlockPos {
    /// The `x` value.
    pub x: i32,
    /// The `y` value.
    pub y: i32,
    /// The `z` value.
    pub z: i32,
}

impl BlockPos {
    /// Performs the `packed` operation.
    pub fn packed(self) -> u64 {
        ((self.x as u64 & 0x3ff_ffff) << 38)
            | ((self.z as u64 & 0x3ff_ffff) << 12)
            | (self.y as u64 & 0xfff)
    }

    /// Performs the `unpack` operation.
    pub fn unpack(value: u64) -> Self {
        let x = ((value as i64) >> 38) as i32;
        let y = ((value & 0xfff) as i32) << 20 >> 20;
        let z = (((value >> 12) & 0x3ff_ffff) as i32) << 6 >> 6;
        Self { x, y, z }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
/// Possible values represented by `Hand`.
pub enum Hand {
    /// Documentation for this public variant.
    Main = 0,
    /// Documentation for this public variant.
    Off = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i8)]
/// Possible values represented by `BlockFace`.
pub enum BlockFace {
    /// Documentation for this public variant.
    Down = 0,
    /// Documentation for this public variant.
    Up = 1,
    /// Documentation for this public variant.
    North = 2,
    /// Documentation for this public variant.
    South = 3,
    /// Documentation for this public variant.
    West = 4,
    /// Documentation for this public variant.
    East = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
/// Possible values represented by `DiggingStatus`.
pub enum DiggingStatus {
    /// Documentation for this public variant.
    Started = 0,
    /// Documentation for this public variant.
    Cancelled = 1,
    /// Documentation for this public variant.
    Finished = 2,
    /// Documentation for this public variant.
    DropStack = 3,
    /// Documentation for this public variant.
    DropItem = 4,
    /// Documentation for this public variant.
    ReleaseUseItem = 5,
    /// Documentation for this public variant.
    SwapHands = 6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// State and protocol data represented by `DiggingAcknowledgement`.
pub struct DiggingAcknowledgement {
    /// The `position` value.
    pub position: BlockPos,
    /// The `block_state_id` value.
    pub block_state_id: i32,
    /// The `status` value.
    pub status: i32,
    /// The `successful` value.
    pub successful: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// State and protocol data represented by `BlockBreakProgress`.
pub struct BlockBreakProgress {
    /// The `entity_id` value.
    pub entity_id: i32,
    /// The `position` value.
    pub position: BlockPos,
    /// The `stage` value.
    pub stage: i8,
}

/// The first collision shape intersected by a world ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockRaycastHit {
    /// The `position` value.
    pub position: BlockPos,
    /// The `state_id` value.
    pub state_id: i32,
    /// The `face` value.
    pub face: BlockFace,
    /// The `point` value.
    pub point: Vec3,
    /// The `distance` value.
    pub distance: f64,
}

/// Protocol-independent facts used to decide whether to start digging.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiggingInfo {
    /// The `state_id` value.
    pub state_id: Option<i32>,
    /// The `loaded` value.
    pub loaded: bool,
    /// The `reachable` value.
    pub reachable: bool,
    /// The `visible` value.
    pub visible: bool,
    /// The `diggable` value.
    pub diggable: bool,
    /// The `harvestable` value.
    pub harvestable: bool,
    /// The `effective_tool` value.
    pub effective_tool: bool,
    /// The `predicted_ticks` value.
    pub predicted_ticks: Option<u64>,
}

/// Client-known facts about a block-use/placement request.
///
/// Server permissions and plugin rules cannot be predicted by the client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacementInfo {
    /// The `target_loaded` value.
    pub target_loaded: bool,
    /// The `reachable` value.
    pub reachable: bool,
    /// The `visible` value.
    pub visible: bool,
    /// Whether the player's body intersects the unit cell where a block would be placed.
    /// The final placed block shape remains server-authoritative.
    pub player_intersects_target_cell: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// State and protocol data represented by `PlacementRequest`.
pub struct PlacementRequest {
    /// The `hand` value.
    pub hand: Hand,
    /// The `clicked` value.
    pub clicked: BlockPos,
    /// The `face` value.
    pub face: BlockFace,
    /// The `cursor` value.
    pub cursor: [f32; 3],
    /// The `inside_block` value.
    pub inside_block: bool,
}

pub(crate) fn parse_digging_ack(payload: &[u8]) -> Result<DiggingAcknowledgement> {
    let mut cursor = Cursor::new(payload);
    let position = BlockPos::unpack(cursor.read_u64::<BigEndian>()?);
    let mut rest = &payload[cursor.position() as usize..];
    let block_state_id = get_varint(&mut rest)?;
    let status = get_varint(&mut rest)?;
    let successful = *rest.first().context("missing digging result")? != 0;
    Ok(DiggingAcknowledgement {
        position,
        block_state_id,
        status,
        successful,
    })
}

pub(crate) fn parse_break_progress(payload: &[u8]) -> Result<BlockBreakProgress> {
    let mut rest = payload;
    let entity_id = get_varint(&mut rest)?;
    let mut cursor = Cursor::new(rest);
    Ok(BlockBreakProgress {
        entity_id,
        position: BlockPos::unpack(cursor.read_u64::<BigEndian>()?),
        stage: cursor.read_i8()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_block_positions_preserve_signed_coordinates() {
        let position = BlockPos {
            x: -12345,
            y: -10,
            z: 54321,
        };
        assert_eq!(BlockPos::unpack(position.packed()), position);
    }
}
