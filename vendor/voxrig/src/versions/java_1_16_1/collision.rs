use crate::versions::java_1_16_1::physics::Aabb;
use std::sync::OnceLock;

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
    use crate::tables::java_1_16_1::{COLLISION, STATES};
    crate::tables::validate_shapes(STATES, &COLLISION);
    let by_state = COLLISION
        .state_shapes
        .iter()
        .map(|&id| {
            COLLISION.shapes[id]
                .iter()
                .copied()
                .map(RelativeAabb)
                .collect()
        })
        .collect();
    let mut names_by_state = Vec::with_capacity(COLLISION.state_shapes.len());
    for block in STATES {
        for _ in block.min_state_id..=block.max_state_id {
            names_by_state.push(block.name.to_owned());
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
        let slab = crate::tables::java_1_16_1::STATES
            .iter()
            .find(|r| r.name == "oak_slab")
            .unwrap();
        assert!(
            shapes_for(slab.min_state_id)
                .iter()
                .any(|shape| shape.0[4] == 0.5 || shape.0[1] == 0.5)
        );
        assert!(shapes_for(3968).iter().any(|shape| shape.0[4] == 1.5));
    }

    #[test]
    fn every_block_range_has_a_complete_shape_mapping() {
        use crate::tables::java_1_16_1::{COLLISION, STATES};
        crate::tables::validate_shapes(STATES, &COLLISION);
        for block in STATES {
            for id in block.min_state_id..=block.max_state_id {
                assert_eq!(block_name(id), Some(block.name));
            }
        }
    }
}
