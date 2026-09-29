//! Native moving-piston chunk data. This is not general block-entity simulation.
use super::{
    reconstruction::{CarrierRole, Direction, MotionProgress},
    wire::Reader,
};
use crate::NativeBlockState;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};

// Java 1.21.11 BlockEntityType bootstrap order (Yarn build.6), also checked by live capture.
pub(crate) const PISTON_TYPE: i32 = 11;

#[derive(Clone, Debug)]
pub(crate) struct PistonData {
    pub carried: NativeBlockState,
    pub direction: Direction,
    pub extending: bool,
    pub role: CarrierRole,
    pub progress: MotionProgress,
}

impl PistonData {
    pub fn read(r: &mut Reader<'_>) -> Result<Option<Self>> {
        match r.u8()? {
            0 => return Ok(None),
            10 => {}
            _ => bail!("piston NBT must be a compound"),
        }
        let (mut carried, mut facing, mut progress, mut extending, mut source) =
            (None, None, None, None, None);
        let mut seen = BTreeSet::new();
        while let Some((kind, key)) = r.nbt_field()? {
            if seen.len() >= 64 || !seen.insert(key.clone()) {
                bail!("duplicate/oversized piston NBT");
            }
            match (key.as_str(), kind) {
                ("blockState", 10) => carried = Some(block_state(r)?),
                ("facing", 1) => facing = Direction::from_id(r.u8()?),
                ("facing", 3) => facing = u8::try_from(r.i32()?).ok().and_then(Direction::from_id),
                ("progress", 5) => {
                    progress = Some(match r.f32()? {
                        0.0 => MotionProgress::Start,
                        0.5 => MotionProgress::Half,
                        1.0 => MotionProgress::Full,
                        _ => bail!("unsupported piston progress"),
                    })
                }
                ("extending", 1) => extending = Some(r.bool()?),
                ("source", 1) => source = Some(r.bool()?),
                _ => r.skip_nbt_value(kind, 1)?,
            }
        }
        let carried = carried.context("missing piston blockState")?;
        super::state_id(&carried)?;
        let direction = facing.context("missing/invalid piston facing")?;
        let extending = extending.context("missing piston extending")?;
        let role = match (source.context("missing piston source")?, extending) {
            (false, _) => CarrierRole::Payload,
            (true, true) => CarrierRole::Head,
            (true, false) => CarrierRole::Body,
        };
        match role {
            CarrierRole::Head if carried.name != "minecraft:piston_head" => {
                bail!("invalid source head")
            }
            CarrierRole::Body
                if !matches!(
                    carried.name.as_str(),
                    "minecraft:piston" | "minecraft:sticky_piston"
                ) =>
            {
                bail!("invalid source body")
            }
            _ => {}
        }
        if role != CarrierRole::Payload
            && carried.properties.get("facing").map(String::as_str) != Some(direction.name())
        {
            bail!("source facing mismatch");
        }
        Ok(Some(Self {
            carried,
            direction,
            extending,
            role,
            progress: progress.context("missing piston progress")?,
        }))
    }
}

fn block_state(r: &mut Reader<'_>) -> Result<NativeBlockState> {
    let mut name = None;
    let mut properties = BTreeMap::new();
    let mut seen = BTreeSet::new();
    while let Some((kind, key)) = r.nbt_field()? {
        if seen.len() >= 64 || !seen.insert(key.clone()) {
            bail!("duplicate/oversized blockState NBT");
        }
        match (key.as_str(), kind) {
            ("Name", 8) => name = Some(r.nbt_string()?),
            ("Properties", 10) => {
                while let Some((kind, property)) = r.nbt_field()? {
                    if kind != 8 || properties.len() >= 64 {
                        bail!("invalid blockState properties");
                    }
                    if properties.insert(property, r.nbt_string()?).is_some() {
                        bail!("duplicate state property");
                    }
                }
            }
            _ => r.skip_nbt_value(kind, 2)?,
        }
    }
    Ok(NativeBlockState {
        name: name.context("missing blockState name")?,
        properties,
    })
}
