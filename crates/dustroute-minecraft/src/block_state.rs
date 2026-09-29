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

#[cfg(test)]
mod tests {
    use super::*;

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
