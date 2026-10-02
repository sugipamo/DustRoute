//! Read-only geometry boundary shared by live and hypothetical player checks.
use super::*;

pub(super) trait GeometryView {
    /// Complete static native cell, or an error. Missing never means air.
    fn block(&self, position: [i32; 3]) -> Result<crate::NativeBlockState>;
}

impl GeometryView for State {
    fn block(&self, position: [i32; 3]) -> Result<crate::NativeBlockState> {
        let (_, height) = self
            .world
            .dimension
            .as_ref()
            .context("dimension unavailable")?;
        if position[1] < height.min_y || position[1] >= height.min_y + height.height {
            return Err(invalid("geometry crosses the observed dimension bounds"));
        }
        let cell = self.reconstruction.cell(&self.world, position);
        if cell.moving.is_some() {
            return Err(invalid("moving geometry is not a static planning cell"));
        }
        cell.state.ok_or_else(|| {
            Error::new(
                ErrorKind::State,
                anyhow::anyhow!("static geometry unavailable at {position:?}"),
            )
        })
    }
}
