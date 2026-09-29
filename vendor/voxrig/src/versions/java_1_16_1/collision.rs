use crate::versions::java_1_16_1::physics::Aabb;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RelativeAabb([f64; 6]);

impl RelativeAabb {
    pub fn at(self, x: i32, y: i32, z: i32) -> Aabb {
        Aabb {
            min_x: f64::from(x) + self.0[0],
            min_y: f64::from(y) + self.0[1],
            min_z: f64::from(z) + self.0[2],
            max_x: f64::from(x) + self.0[3],
            max_y: f64::from(y) + self.0[4],
            max_z: f64::from(z) + self.0[5],
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlockRange {
    name: String,
    min_state_id: usize,
    max_state_id: usize,
}

#[derive(Deserialize)]
struct ShapeData {
    blocks: HashMap<String, Value>,
    shapes: HashMap<String, Vec<[f64; 6]>>,
}

struct CollisionRegistry {
    by_state: Vec<Vec<RelativeAabb>>,
    names_by_state: Vec<String>,
}
static REGISTRY: OnceLock<CollisionRegistry> = OnceLock::new();

pub(crate) fn shapes_for(state_id: i32) -> &'static [RelativeAabb] {
    if state_id < 0 {
        return &[];
    }
    REGISTRY
        .get_or_init(load)
        .by_state
        .get(state_id as usize)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub(crate) fn block_name(state_id: i32) -> Option<&'static str> {
    if state_id < 0 {
        return None;
    }
    REGISTRY
        .get_or_init(load)
        .names_by_state
        .get(state_id as usize)
        .map(String::as_str)
}

fn load() -> CollisionRegistry {
    let ranges: Vec<BlockRange> =
        serde_json::from_str(include_str!("../../../data/block_state_ranges.json"))
            .expect("bundled block ranges must be valid");
    let data: ShapeData =
        serde_json::from_str(include_str!("../../../data/block_collision_shapes.json"))
            .expect("bundled collision shapes must be valid");
    let max_state = ranges.iter().map(|r| r.max_state_id).max().unwrap_or(0);
    let mut by_state = vec![Vec::new(); max_state + 1];
    let mut names_by_state = vec![String::new(); max_state + 1];
    for range in ranges {
        let Some(mapping) = data.blocks.get(&range.name) else {
            continue;
        };
        for (offset, slot) in by_state[range.min_state_id..=range.max_state_id]
            .iter_mut()
            .enumerate()
        {
            names_by_state[range.min_state_id + offset] = range.name.clone();
            let shape_id = match mapping {
                Value::Number(number) => number.as_u64(),
                Value::Array(ids) => ids.get(offset).and_then(Value::as_u64),
                _ => None,
            };
            let Some(shape_id) = shape_id else {
                continue;
            };
            if let Some(boxes) = data.shapes.get(&shape_id.to_string()) {
                *slot = boxes.iter().copied().map(RelativeAabb).collect();
            }
        }
    }
    CollisionRegistry {
        by_state,
        names_by_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_distinguishes_air_full_blocks_and_slabs() {
        assert!(shapes_for(0).is_empty());
        assert_eq!(
            shapes_for(1),
            &[RelativeAabb([0.0, 0.0, 0.0, 1.0, 1.0, 1.0])]
        );
        let ranges: Vec<BlockRange> =
            serde_json::from_str(include_str!("../../../data/block_state_ranges.json")).unwrap();
        let slab = ranges.iter().find(|r| r.name == "oak_slab").unwrap();
        assert!(
            shapes_for(slab.min_state_id as i32)
                .iter()
                .any(|shape| shape.0[4] == 0.5 || shape.0[1] == 0.5)
        );
        assert!(shapes_for(3968).iter().any(|shape| shape.0[4] == 1.5));
    }

    #[test]
    fn every_block_range_has_a_complete_shape_mapping() {
        let ranges: Vec<BlockRange> =
            serde_json::from_str(include_str!("../../../data/block_state_ranges.json")).unwrap();
        let data: ShapeData =
            serde_json::from_str(include_str!("../../../data/block_collision_shapes.json"))
                .unwrap();
        for range in ranges {
            let mapping = data
                .blocks
                .get(&range.name)
                .unwrap_or_else(|| panic!("missing collision mapping for {}", range.name));
            if let Value::Array(ids) = mapping {
                assert!(
                    ids.len() > range.max_state_id - range.min_state_id,
                    "not enough state shapes for {}",
                    range.name
                );
            }
        }
    }
}
