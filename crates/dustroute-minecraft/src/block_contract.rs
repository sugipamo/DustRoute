//! Compile-time declarations for analysis and port projection. These contracts
//! do not grant admission to a physics executor or certify construction.
use crate::{BlockCapabilities, BlockKind, CapabilityLevel};
use CapabilityLevel::{Full, NotApplicable as NA, Partial, Unsupported as No};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortLayout {
    None,
    Wire,
    Diode,
    Torch,
    Source,
    Piston,
    Observer,
    PassiveConductor,
}
#[derive(Clone, Copy, Debug)]
pub struct BlockContract {
    pub analysis: BlockCapabilities,
    pub ports: PortLayout,
}
const fn contract(levels: [CapabilityLevel; 7], ports: PortLayout) -> BlockContract {
    let [
        observation,
        physical_classification,
        connectivity,
        steady_state,
        temporal,
        repair,
        placement,
    ] = levels;
    BlockContract {
        analysis: BlockCapabilities {
            observation,
            physical_classification,
            connectivity,
            steady_state,
            temporal,
            repair,
            placement,
        },
        ports,
    }
}
impl BlockKind {
    /// Exhaustive match deliberately has no fallback: adding a block requires
    /// an explicit contract, independent of callback-program registration.
    pub const fn contract(self) -> BlockContract {
        match self {
            Self::Air => contract([Full, Full, NA, NA, NA, NA, Full], PortLayout::None),
            Self::Solid | Self::Transparent => contract(
                [Full, Full, Full, NA, NA, NA, Full],
                PortLayout::PassiveConductor,
            ),
            Self::RedstoneWire => contract(
                [Full, Full, Full, Full, Partial, Full, Full],
                PortLayout::Wire,
            ),
            Self::RedstoneTorch => contract(
                [Full, Full, Full, Full, Partial, Partial, Full],
                PortLayout::Torch,
            ),
            Self::Repeater | Self::Comparator => contract(
                [Full, Full, Full, Full, Partial, Partial, Full],
                PortLayout::Diode,
            ),
            Self::Lever | Self::RedstoneBlock => contract(
                [Full, Full, Full, Full, Full, Partial, Full],
                PortLayout::Source,
            ),
            // Entity occupancy is observable, but not reproduced here.
            Self::Button | Self::PressurePlate => contract(
                [Full, Full, Full, Full, Partial, Partial, Full],
                PortLayout::Source,
            ),
            Self::RedstoneLamp => contract(
                [Full, Full, Full, Full, Partial, Partial, Full],
                PortLayout::PassiveConductor,
            ),
            Self::CopperBulb => contract(
                [Full, Full, Full, No, Partial, No, Full],
                PortLayout::PassiveConductor,
            ),
            Self::Observer => contract(
                [Full, Full, Full, NA, Full, Partial, Full],
                PortLayout::Observer,
            ),
            // The compatibility analysis model does not inherit the expanded
            // moving-world executor's mechanical or placement guarantees.
            Self::Piston => contract([Full, Partial, Partial, No, No, No, No], PortLayout::Piston),
            Self::PistonHead => contract(
                [Full, Partial, Partial, Partial, Partial, No, No],
                PortLayout::None,
            ),
            Self::MovingPiston => {
                contract([Full, Partial, No, No, Partial, No, No], PortLayout::None)
            }
        }
    }
}
