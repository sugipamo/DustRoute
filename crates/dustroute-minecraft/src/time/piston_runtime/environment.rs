//! A fixed water boundary is validated against every intermediate world.
//! Opening its container needs fluid execution and stops this adapter.
use super::geometry::along;
use super::{RuntimeView, unsupported};
use crate::Facing;
use crate::physical::{self, Shape, environment};
use crate::time::runtime::RuntimeError;

pub(super) fn validate(view: RuntimeView<'_>) -> Result<(), RuntimeError> {
    for (pos, _) in view
        .world()
        .iter()
        .filter(|(_, b)| environment::is_water(b))
    {
        for side in [
            Facing::Down,
            Facing::North,
            Facing::South,
            Facing::East,
            Facing::West,
        ] {
            let neighbor = view.block(along(*pos, side, 1)?)?;
            if !environment::is_water(&neighbor)
                && !physical::of_block(&neighbor).is_some_and(|p| {
                    // A supporting face alone does not establish a fluid
                    // barrier: partial/waterloggable geometry is outside the
                    // fixed-environment scope.
                    p.spec().shape == Shape::FullCube && p.full_face(&neighbor, side.opposite())
                })
            {
                return Err(unsupported(format!(
                    "fixed water at {pos:?} requires a closed known container; fluid flow is not modeled"
                )));
            }
        }
    }
    Ok(())
}
