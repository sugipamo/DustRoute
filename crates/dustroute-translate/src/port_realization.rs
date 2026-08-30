use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::cells::PortKind;
use crate::physical::Endpoint;
use crate::world::{Facing, Pos};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortRealization {
    pub terminal: Pos,
    pub approach: Pos,
    pub leaf_required: bool,
    pub approach_facing: Option<Facing>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortRealizationError {
    BlockPowerRequiresHorizontalFacing,
}

impl Display for PortRealizationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("BLOCK_POWER port requires horizontal facing")
    }
}

impl Error for PortRealizationError {}

pub fn terminal_for_endpoint(endpoint: &Endpoint) -> Result<Pos, PortRealizationError> {
    match endpoint.kind {
        PortKind::Wire => Ok(endpoint.pos),
        PortKind::BlockPower => {
            let delta = endpoint
                .facing
                .and_then(Facing::horizontal_offset)
                .ok_or(PortRealizationError::BlockPowerRequiresHorizontalFacing)?;
            Ok(endpoint.pos.offset(delta.x, delta.y, delta.z))
        }
    }
}

pub fn realize_sink_endpoint(endpoint: &Endpoint) -> Result<PortRealization, PortRealizationError> {
    let terminal = terminal_for_endpoint(endpoint)?;
    let Some(delta) = endpoint.facing.and_then(Facing::horizontal_offset) else {
        return Ok(PortRealization {
            terminal,
            approach: terminal,
            leaf_required: true,
            approach_facing: None,
        });
    };
    Ok(PortRealization {
        terminal,
        approach: terminal.offset(delta.x, delta.y, delta.z),
        leaf_required: true,
        approach_facing: endpoint.facing,
    })
}

pub fn realize_source_endpoint(
    endpoint: &Endpoint,
) -> Result<PortRealization, PortRealizationError> {
    let terminal = terminal_for_endpoint(endpoint)?;
    Ok(PortRealization {
        terminal,
        approach: terminal,
        leaf_required: false,
        approach_facing: endpoint.facing,
    })
}

#[cfg(test)]
mod tests {
    use crate::physical::PlacementCircuit;

    use super::*;

    #[test]
    fn block_power_targets_external_conductor() {
        let endpoint = PlacementCircuit::boundary(
            "a",
            Pos::new(10, 2, 4),
            PortKind::BlockPower,
            Some(Facing::West),
        );
        let realized = realize_sink_endpoint(&endpoint).unwrap();
        assert_eq!(realized.terminal, Pos::new(9, 2, 4));
        assert_eq!(realized.approach, Pos::new(8, 2, 4));
        assert!(realized.leaf_required);
    }
}
