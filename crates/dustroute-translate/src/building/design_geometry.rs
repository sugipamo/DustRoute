//! Expand bounded caller-authored claims without order-dependent overwrites.
use super::design::BuildingDesignError;
use super::geometry::Geometry;
use crate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
use dustroute_library::building::{BuildingDesignRequest, BuildingDesignShape, BuildingMaterial};
use dustroute_minecraft::{Pos, Region};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn volume(region: Region) -> Result<usize, BuildingDesignError> {
    let dimensions = [
        i64::from(region.max.x) - i64::from(region.min.x) + 1,
        i64::from(region.max.y) - i64::from(region.min.y) + 1,
        i64::from(region.max.z) - i64::from(region.min.z) + 1,
    ];
    if dimensions.iter().any(|d| *d <= 0) {
        return Err(BuildingDesignError::new(
            "invalid_region",
            "region min must not exceed max",
        ));
    }
    let count = dimensions
        .into_iter()
        .try_fold(1_i64, i64::checked_mul)
        .filter(|n| *n <= 8192)
        .ok_or_else(|| {
            BuildingDesignError::new(
                "known_cell_budget",
                "each region must contain at most 8192 cells",
            )
        })?;
    Ok(count as usize)
}

pub(super) fn contains_region(outer: Region, inner: Region) -> bool {
    outer.contains(inner.min) && outer.contains(inner.max)
}

pub(super) fn intersects(a: Region, b: Region) -> bool {
    a.min.x <= b.max.x
        && b.min.x <= a.max.x
        && a.min.y <= b.max.y
        && b.min.y <= a.max.y
        && a.min.z <= b.max.z
        && b.min.z <= a.max.z
}

pub(super) fn interior(region: Region, p: Pos) -> bool {
    p.x > region.min.x
        && p.x < region.max.x
        && p.y > region.min.y
        && p.y < region.max.y
        && p.z > region.min.z
        && p.z < region.max.z
}

fn cells(region: Region) -> impl Iterator<Item = Pos> {
    (region.min.x..=region.max.x).flat_map(move |x| {
        (region.min.y..=region.max.y)
            .flat_map(move |y| (region.min.z..=region.max.z).map(move |z| Pos::new(x, y, z)))
    })
}

fn named(name: &str, names: &mut BTreeSet<String>) -> Result<(), BuildingDesignError> {
    if name.is_empty()
        || name.len() > 32
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-'))
        || matches!(name, "building" | "parent" | "empty" | "clearance" | "root")
    {
        return Err(BuildingDesignError::new("invalid_name",
            "part/space/component names require 1..32 lowercase letters, digits, _ or -; building, parent, empty, clearance and root are reserved").at(name, None));
    }
    if !names.insert(name.into()) {
        return Err(
            BuildingDesignError::new("duplicate_name", "design names must be unique")
                .at(name, None),
        );
    }
    Ok(())
}

pub(super) fn expand(request: &BuildingDesignRequest) -> Result<Geometry, BuildingDesignError> {
    if request.namespace.is_empty()
        || request.namespace.len() > 64
        || !request.namespace.bytes().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-' | b'.')
        })
    {
        return Err(BuildingDesignError::new(
            "invalid_namespace",
            "namespace requires 1..64 lowercase ID characters",
        ));
    }
    if request.name.trim().is_empty() || request.name.len() > 256 {
        return Err(BuildingDesignError::new(
            "invalid_title",
            "design name requires 1..256 bytes",
        ));
    }
    volume(request.known_region)?;
    if request.parts.len() > 64
        || request.spaces.len() > 64
        || (request.parts.is_empty() && request.component.is_none())
    {
        return Err(BuildingDesignError::new(
            "part_budget",
            "at most 64 parts and 64 spaces; supply a part or component",
        ));
    }
    let mut names = BTreeSet::new();
    let mut claims: BTreeMap<Pos, (MinecraftSnapshotBlock, String)> = BTreeMap::new();
    let mut parts = Vec::new();
    let mut work = 0usize;
    for part in &request.parts {
        named(&part.name, &mut names)?;
        if part.shapes.is_empty() || part.shapes.len() > 256 || part.cutouts.len() > 64 {
            return Err(BuildingDesignError::new(
                "shape_budget",
                "supply 1..256 shapes and at most 64 cutouts per part",
            )
            .at(&part.name, None));
        }
        for r in &part.cutouts {
            volume(*r).map_err(|e| e.at(&part.name, None))?;
            if !contains_region(request.known_region, *r) {
                return Err(BuildingDesignError::new(
                    "outside_known_region",
                    "cutout is outside known_region",
                )
                .at(&part.name, Some(r.min)));
            }
        }
        let mut local = BTreeMap::new();
        let mut add = |p: Pos, material: BuildingMaterial| -> Result<(), BuildingDesignError> {
            if !interior(request.known_region, p) {
                return Err(BuildingDesignError::new(
                    "missing_air_guard",
                    "solid cells must lie inside the empty outer boundary",
                )
                .at(&part.name, Some(p)));
            }
            if part.cutouts.iter().any(|r| r.contains(p)) {
                return Ok(());
            }
            let block = MinecraftSnapshotBlock {
                pos: p,
                name: material.native_name().into(),
                properties: Default::default(),
            };
            if local.get(&p).is_some_and(|b| b != &block) {
                return Err(BuildingDesignError::new(
                    "material_conflict",
                    "different materials claim the same cell within this part",
                )
                .at(&part.name, Some(p)));
            }
            local.insert(p, block);
            Ok(())
        };
        for shape in &part.shapes {
            let count = match shape {
                BuildingDesignShape::Fill { region, .. }
                | BuildingDesignShape::Shell { region, .. } => {
                    let n = volume(*region).map_err(|e| e.at(&part.name, None))?;
                    if !contains_region(request.known_region, *region) {
                        return Err(BuildingDesignError::new(
                            "outside_known_region",
                            "shape is outside known_region",
                        )
                        .at(&part.name, Some(region.min)));
                    }
                    n
                }
                BuildingDesignShape::Blocks { positions, .. } => positions.len(),
            };
            work = work
                .checked_add(count)
                .filter(|n| *n <= 16384)
                .ok_or_else(|| {
                    BuildingDesignError::new(
                        "shape_work_budget",
                        "shape expansion exceeds 16384 cell claims",
                    )
                    .at(&part.name, None)
                })?;
            match shape {
                BuildingDesignShape::Fill { region, material } => {
                    for p in cells(*region) {
                        add(p, *material)?;
                    }
                }
                BuildingDesignShape::Shell { region, material } => {
                    for p in cells(*region) {
                        if !interior(*region, p) {
                            add(p, *material)?;
                        }
                    }
                }
                BuildingDesignShape::Blocks {
                    positions,
                    material,
                } => {
                    for p in positions {
                        add(*p, *material)?;
                    }
                }
            }
        }
        if local.is_empty() {
            return Err(
                BuildingDesignError::new("empty_part", "part has no blocks after cutouts")
                    .at(&part.name, None),
            );
        }
        for (p, block) in &local {
            if let Some((existing, owner)) = claims.get(p) {
                if existing != block {
                    return Err(BuildingDesignError::new(
                        "material_conflict",
                        format!("different material also claimed by part {owner}"),
                    )
                    .at(&part.name, Some(*p)));
                }
            } else {
                claims.insert(*p, (block.clone(), part.name.clone()));
            }
        }
        if claims.len() > 256 {
            return Err(BuildingDesignError::new(
                "block_budget",
                "design exceeds 256 unique non-air cells",
            ));
        }
        parts.push((part.name.clone(), local.into_values().collect()));
    }
    for space in &request.spaces {
        named(&space.name, &mut names)?;
        let n = volume(space.region).map_err(|e| e.at(&space.name, None))?;
        work = work.checked_add(n).filter(|n| *n <= 16384).ok_or_else(|| {
            BuildingDesignError::new(
                "shape_work_budget",
                "shape/space expansion exceeds 16384 cell claims",
            )
            .at(&space.name, None)
        })?;
        if !contains_region(request.known_region, space.region) {
            return Err(BuildingDesignError::new(
                "outside_known_region",
                "space is outside known_region",
            )
            .at(&space.name, Some(space.region.min)));
        }
        if let Some((p, (_, owner))) = claims.iter().find(|(p, _)| space.region.contains(**p)) {
            return Err(BuildingDesignError::new(
                "occupied_space",
                format!("permanent air space is occupied by part {owner}"),
            )
            .at(&space.name, Some(*p)));
        }
    }
    if let Some(component) = &request.component {
        named(&component.name, &mut names)?;
    }
    let region = request.known_region;
    Ok(Geometry {
        region,
        initial: MinecraftSnapshot {
            min: region.min,
            max: region.max,
            blocks: claims.into_values().map(|(b, _)| b).collect(),
        },
        parts,
        reserved: vec![],
    })
}
