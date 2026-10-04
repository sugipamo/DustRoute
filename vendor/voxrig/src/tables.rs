//! Compile-time native data. Generation reads pinned source data outside the client.
use crate::block_state::BlockDefinition;

#[rustfmt::skip]
pub(crate) mod java_1_16_1;
#[rustfmt::skip]
pub(crate) mod java_1_21_11;

pub(crate) struct ItemDefinition {
    pub id: i32,
    pub name: &'static str,
    pub stack_size: i32,
}
pub(crate) struct MiningBlock {
    pub name: &'static str,
    pub hardness: Option<f64>,
    pub min_state_id: i32,
    pub max_state_id: i32,
    pub diggable: bool,
    pub material: Option<&'static str>,
    pub harvest_tools: Option<&'static [(i32, bool)]>,
}
pub(crate) struct Recipe {
    pub output: i32,
    pub result: (i32, i32),
    pub ingredients: Option<&'static [i32]>,
    pub in_shape: Option<&'static [&'static [Option<i32>]]>,
    pub out_shape: Option<&'static [&'static [Option<i32>]]>,
}
pub(crate) struct CollisionShapes {
    pub state_shapes: &'static [usize],
    pub shapes: &'static [&'static [[f64; 6]]],
}
pub(crate) struct OutlineShapes {
    pub state_shapes: &'static [Option<[usize; 2]>],
    pub shapes: &'static [&'static [[f64; 6]]],
    #[cfg(test)]
    pub registry_fnv64: &'static str,
}

pub(crate) fn validate_shapes(states: &[BlockDefinition], shapes: &CollisionShapes) {
    assert_eq!(
        shapes.state_shapes.len(),
        states.last().unwrap().max_state_id as usize + 1
    );
    assert!(
        shapes
            .state_shapes
            .iter()
            .all(|&id| id < shapes.shapes.len())
    );
}

#[cfg(test)]
mod tests;
