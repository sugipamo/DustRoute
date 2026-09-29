//! Kind-specific mutation views keep canonical and observed fields together.
//! Raw observations stay representable; obtaining a view does not admit a block
//! into an executor. Callers still perform their existing evidence validation.
use crate::{Block, BlockKind, Facing, PistonState, WireConnection};
use std::collections::BTreeMap;

pub(crate) struct PistonStateMut<'a>(&'a mut Block);
pub(crate) struct WireStateMut<'a>(&'a mut Block);

impl Block {
    pub(crate) fn piston_state_mut(&mut self) -> Option<PistonStateMut<'_>> {
        (self.kind == BlockKind::Piston).then_some(PistonStateMut(self))
    }
    pub(crate) fn wire_state_mut(&mut self) -> Option<WireStateMut<'_>> {
        (self.kind == BlockKind::RedstoneWire).then_some(WireStateMut(self))
    }
}

impl PistonStateMut<'_> {
    pub fn set_extension(&mut self, state: PistonState) {
        self.0.piston_state = Some(state);
        if self.0.observed_name.is_some() {
            self.0
                .observed_properties
                .insert("extended".into(), state.is_extended().to_string());
        }
    }
}

impl WireStateMut<'_> {
    pub fn set_power(&mut self, power: u8) {
        self.0.power_level = Some(power);
        if self.0.observed_name.is_some() {
            self.0
                .observed_properties
                .insert("power".into(), power.to_string());
        }
    }
    pub fn set_shape(&mut self, shape: BTreeMap<Facing, WireConnection>) {
        if self.0.observed_name.is_some() {
            for (direction, connection) in &shape {
                let name = match direction {
                    Facing::North => "north",
                    Facing::East => "east",
                    Facing::South => "south",
                    Facing::West => "west",
                    Facing::Up => "up",
                    Facing::Down => "down",
                };
                let value = match connection {
                    WireConnection::None => "none",
                    WireConnection::Side => "side",
                    WireConnection::Up => "up",
                };
                self.0.observed_properties.insert(name.into(), value.into());
            }
        }
        self.0.wire_connections = Some(shape);
    }
}

/// Kind-specific state produced after the electrical evidence gate. Raw `Block`
/// remains the observation/archive record, including unknown identities. This
/// projection grants neither world support nor scheduler-context admission.
/// No Deserialize or mutable raw-block access is available through this boundary.
pub struct ElectricalBlock<'a> {
    state: ElectricalState<'a>,
}
pub enum ElectricalState<'a> {
    Air,
    Passive,
    ConstantSource,
    Lever {
        powered: bool,
        support: Facing,
    },
    Wire {
        power: u8,
        connections: &'a BTreeMap<Facing, WireConnection>,
    },
    Device(crate::device_program::DeviceState<'a>),
    Piston {
        facing: Facing,
        variant: crate::PistonVariant,
        extension: PistonState,
    },
    Head(&'a crate::PistonHeadState),
    Moving(&'a crate::PistonBlockEntityState),
}
impl<'a> ElectricalBlock<'a> {
    pub fn try_from_block(block: &'a Block) -> Result<Self, crate::time::runtime::RuntimeError> {
        use crate::time::runtime::RuntimeError;
        crate::piston_electrical::validate_evidence(block)?;
        let state = if let Some(program) = crate::device_program::program(block) {
            ElectricalState::Device(
                program
                    .definition()
                    .state_view(block)
                    .map_err(RuntimeError::Invalid)?,
            )
        } else {
            match block.kind {
                BlockKind::Air => ElectricalState::Air,
                BlockKind::Solid | BlockKind::Transparent => ElectricalState::Passive,
                BlockKind::RedstoneBlock => ElectricalState::ConstantSource,
                BlockKind::Lever => ElectricalState::Lever {
                    powered: block.powered.expect("validated lever state"),
                    support: crate::piston_electrical::SIDES
                        .into_iter()
                        .find(|side| Some(side.offset()) == block.support_offset)
                        .expect("validated support"),
                },
                BlockKind::RedstoneWire => ElectricalState::Wire {
                    power: block.power_level.expect("validated power"),
                    connections: block.wire_connections.as_ref().expect("validated shape"),
                },
                BlockKind::Piston => ElectricalState::Piston {
                    facing: block.facing.expect("validated facing"),
                    variant: crate::piston_variant(block),
                    extension: block.piston_state.expect("validated piston state"),
                },
                BlockKind::PistonHead => {
                    ElectricalState::Head(block.piston_head.as_ref().expect("validated head"))
                }
                BlockKind::MovingPiston => ElectricalState::Moving(
                    block.piston_entity.as_deref().expect("validated carrier"),
                ),
                BlockKind::RedstoneTorch
                | BlockKind::Repeater
                | BlockKind::Comparator
                | BlockKind::Button
                | BlockKind::PressurePlate
                | BlockKind::RedstoneLamp
                | BlockKind::Observer
                | BlockKind::CopperBulb => {
                    return Err(RuntimeError::Invalid(
                        "device kind has no admitted program".into(),
                    ));
                }
            }
        };
        Ok(Self { state })
    }
    pub fn state(&self) -> &ElectricalState<'a> {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_observation_is_not_an_electrical_state_and_wire_projection_is_complete() {
        let mut wire = Block::new(BlockKind::RedstoneWire);
        wire.power_level = Some(11);
        wire.support_offset = Some(Facing::Down.offset());
        wire.wire_connections = Some(
            crate::piston_electrical::HORIZONTAL
                .into_iter()
                .map(|f| (f, WireConnection::Side))
                .collect(),
        );
        let state = ElectricalBlock::try_from_block(&wire).unwrap();
        assert!(
            matches!(state.state(), ElectricalState::Wire { power: 11, connections } if connections.len() == 4)
        );
        wire.observed_name = Some("minecraft:redstone_wire".into());
        // Adding a native identity without its state evidence must not inherit
        // the previous synthetic admission.
        assert!(ElectricalBlock::try_from_block(&wire).is_err());
        wire.observed_name = Some("minecraft:chest".into());
        assert!(ElectricalBlock::try_from_block(&wire).is_err());
        assert_eq!(wire.observed_name.as_deref(), Some("minecraft:chest"));
    }

    #[test]
    fn mutation_views_preserve_raw_identity_and_do_not_invent_observed_properties() {
        let mut synthetic = Block::new(BlockKind::RedstoneWire);
        assert!(synthetic.piston_state_mut().is_none());
        synthetic.wire_state_mut().unwrap().set_power(7);
        assert!(synthetic.observed_properties.is_empty());
        let mut observed = synthetic.clone();
        observed.observed_name = Some("minecraft:redstone_wire".into());
        observed
            .observed_properties
            .insert("future_property".into(), "retained".into());
        let shape = [(Facing::North, WireConnection::Up)].into();
        observed.wire_state_mut().unwrap().set_shape(shape);
        observed.wire_state_mut().unwrap().set_power(3);
        assert_eq!(observed.observed_properties["power"], "3");
        assert_eq!(observed.power_level, Some(3));
        assert_eq!(observed.observed_properties["north"], "up");
        assert_eq!(observed.observed_properties["future_property"], "retained");
        assert_eq!(
            observed.observed_name.as_deref(),
            Some("minecraft:redstone_wire")
        );
    }
}
