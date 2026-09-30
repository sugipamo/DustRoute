//! Bounded geometry authoring. No block mutation or separate physics engine.
use crate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
use crate::world::{Pos, Region};
use dustroute_library::building::{
    BuildingEntrance, BuildingMaterial, BuildingRequest, BuildingRoof,
};
use std::collections::BTreeMap;

pub(super) struct Geometry {
    pub region: Region,
    pub initial: MinecraftSnapshot,
    pub parts: Vec<(&'static str, Vec<MinecraftSnapshotBlock>)>,
    pub entrance: BuildingEntrance,
}

pub(super) fn expand(request: &BuildingRequest) -> Result<Geometry, String> {
    if request.namespace.is_empty()
        || request.namespace.len() > 64
        || !request.namespace.bytes().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-' | b'.')
        })
    {
        return Err("building namespace must contain 1..64 lowercase ID characters".into());
    }
    if !(3..=16).contains(&request.width)
        || !(3..=16).contains(&request.depth)
        || !(3..=16).contains(&request.height)
    {
        return Err("building outside dimensions must each be 3..16 blocks".into());
    }
    let roof = matches!(request.roof, BuildingRoof::Flat);
    let wall_height = request.height - 1 - u16::from(roof);
    let entrance = request.entrance.unwrap_or(BuildingEntrance {
        offset: (request.width - 1) / 2,
        width: 1,
        height: 2,
    });
    if entrance.offset == 0
        || entrance.width == 0
        || entrance.height < 2
        || entrance.height > wall_height
        || u32::from(entrance.offset) + u32::from(entrance.width) >= u32::from(request.width)
    {
        return Err("entrance must fit between wall corners, be at least two blocks high and remain below the roof".into());
    }
    let (w, d, h) = (
        i32::from(request.width),
        i32::from(request.depth),
        i32::from(request.height),
    );
    let block = |pos, material: BuildingMaterial| MinecraftSnapshotBlock {
        pos,
        name: material.native_name().into(),
        properties: Default::default(),
    };
    let mut floor = Vec::new();
    let mut walls = Vec::new();
    let mut ceiling = Vec::new();
    for x in 0..w {
        for z in 0..d {
            floor.push(block(Pos::new(x, 0, z), request.floor_material));
            if roof {
                ceiling.push(block(Pos::new(x, h - 1, z), request.roof_material));
            }
            if x != 0 && x != w - 1 && z != 0 && z != d - 1 {
                continue;
            }
            for y in 1..=i32::from(wall_height) {
                if z == 0
                    && x >= i32::from(entrance.offset)
                    && x < i32::from(entrance.offset) + i32::from(entrance.width)
                    && y <= i32::from(entrance.height)
                {
                    continue;
                }
                walls.push(block(Pos::new(x, y, z), request.wall_material));
            }
        }
    }
    let mut parts = vec![("floor", floor), ("walls", walls)];
    if roof {
        parts.push(("roof", ceiling));
    }
    let blocks: BTreeMap<_, _> = parts
        .iter()
        .flat_map(|(_, blocks)| blocks.iter().cloned())
        .map(|b| (b.pos, b))
        .collect();
    if blocks.len() > 256 {
        return Err("generated building exceeds the 256-block custom Assembly budget".into());
    }
    let region = Region::new(Pos::new(-1, -1, -1), Pos::new(w, h, d));
    Ok(Geometry {
        region,
        initial: MinecraftSnapshot {
            min: region.min,
            max: region.max,
            blocks: blocks.into_values().collect(),
        },
        parts,
        entrance,
    })
}
