//! Relocation changes physical coordinates, never immutable source records or
//! their requirements. The returned data must be reviewed again at its target.
use dustroute_library::assembly::Assembly;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_minecraft::{Pos, Region, RotationY};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssemblyTransform {
    pub source_anchor: Pos,
    pub target_anchor: Pos,
    pub rotation: RotationY,
}

impl AssemblyTransform {
    pub fn position(self, p: Pos) -> Result<Pos, String> {
        let local = Pos::new(
            p.x.checked_sub(self.source_anchor.x)
                .ok_or("coordinate overflow")?,
            p.y.checked_sub(self.source_anchor.y)
                .ok_or("coordinate overflow")?,
            p.z.checked_sub(self.source_anchor.z)
                .ok_or("coordinate overflow")?,
        );
        let rotated = self
            .rotation
            .checked_pos(local)
            .ok_or("coordinate overflow")?;
        Ok(Pos::new(
            rotated
                .x
                .checked_add(self.target_anchor.x)
                .ok_or("coordinate overflow")?,
            rotated
                .y
                .checked_add(self.target_anchor.y)
                .ok_or("coordinate overflow")?,
            rotated
                .z
                .checked_add(self.target_anchor.z)
                .ok_or("coordinate overflow")?,
        ))
    }

    pub fn region(self, region: Region) -> Result<Region, String> {
        let a = self.position(region.min)?;
        let b = self.position(region.max)?;
        Ok(Region::new(
            Pos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            Pos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        ))
    }

    /// Transform actual cells, occurrence frames, routes, known air and all
    /// physical input coordinates together. Port aliases retain their paths
    /// and resolve through the transformed occurrence frames.
    pub fn apply(
        self,
        assembly: &Assembly,
        context: &RuntimeBehaviorContext,
    ) -> Result<(Assembly, RuntimeBehaviorContext), String> {
        let mut moved = assembly.clone();
        for block in &mut moved.blocks {
            block.position = self.position(block.position)?;
            block.block = self
                .rotation
                .checked_block(&block.block)
                .ok_or("block rotation overflow")?;
        }
        for occurrence in &mut moved.instances {
            occurrence.origin = self.position(occurrence.origin)?;
            occurrence.rotation = self.rotation.then(occurrence.rotation);
        }
        for region in &mut moved.known_regions {
            *region = self.region(*region)?;
        }
        for connection in &mut moved.connections {
            for position in &mut connection.path {
                *position = self.position(*position)?;
            }
        }
        let mut context = context.clone();
        context.known_region = self.region(context.known_region)?;
        for input in &mut context.input_levers {
            *input = self.position(*input)?;
        }
        Ok((moved, context))
    }
}
