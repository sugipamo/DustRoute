//! Original pinned sources are independent fixtures for generated Rust data.
use super::*;
use crate::block_state::{NativeBlockState, StateRegistry};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Block {
    name: String,
    min_state_id: i32,
    max_state_id: i32,
    states: Vec<Property>,
}
#[derive(Deserialize)]
struct Property {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    num_values: u32,
    values: Option<Vec<String>>,
}
#[test]
fn every_native_state_keeps_the_pinned_property_order_and_id_in_both_adapters() {
    for (input, definitions) in [
        (include_str!("../../data/blocks.json"), java_1_16_1::STATES),
        (
            include_str!("../../data/java_1_21_11/blocks.json"),
            java_1_21_11::STATES,
        ),
    ] {
        let blocks: Vec<Block> = serde_json::from_str(input).unwrap();
        assert_eq!(blocks.len(), definitions.len());
        let registry = StateRegistry::new(definitions).unwrap();
        for (index, block) in blocks.iter().enumerate() {
            assert_eq!(registry.block_name(index as i32), Some(block.name.as_str()));
            for id in block.min_state_id..=block.max_state_id {
                let mut offset = (id - block.min_state_id) as u32;
                let mut properties = BTreeMap::new();
                for p in block.states.iter().rev() {
                    let index = offset % p.num_values;
                    offset /= p.num_values;
                    let value = match (&p.values, p.kind.as_str()) {
                        (Some(values), _) => values[index as usize].clone(),
                        (None, "bool") => (index == 0).to_string(),
                        (None, "int") => index.to_string(),
                        _ => panic!("unsupported pinned property"),
                    };
                    properties.insert(p.name.clone(), value);
                }
                let expected = NativeBlockState {
                    name: format!("minecraft:{}", block.name),
                    properties,
                };
                assert_eq!(registry.decode(id).unwrap(), expected, "state {id}");
                assert_eq!(registry.encode(&expected).unwrap(), id);
            }
        }
        assert!(registry.decode(-1).is_err());
        assert!(
            registry
                .decode(blocks.last().unwrap().max_state_id + 1)
                .is_err()
        );
    }
}
#[derive(Deserialize)]
struct Shapes {
    state_shapes: Vec<usize>,
    shapes: Vec<Vec<[f64; 6]>>,
}
#[derive(Deserialize)]
struct Outline {
    state_shapes: Vec<Option<[usize; 2]>>,
    shapes: Vec<Vec<[f64; 6]>>,
    registry_fnv64: String,
}
fn boxes_equal(actual: &[&[[f64; 6]]], expected: &[Vec<[f64; 6]>]) {
    assert_eq!(actual.len(), expected.len());
    for (i, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(a.len(), b.len(), "shape {i}");
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits), "shape {i}");
        }
    }
}
#[test]
fn collision_and_outline_preserve_every_coordinate_bit_mapping_and_unsupported_state() {
    let source: Shapes = serde_json::from_str(include_str!(
        "../../data/java_1_21_11/collision_shapes.json"
    ))
    .unwrap();
    assert_eq!(java_1_21_11::COLLISION.state_shapes, source.state_shapes);
    boxes_equal(java_1_21_11::COLLISION.shapes, &source.shapes);
    let outline: Outline =
        serde_json::from_str(include_str!("../../data/java_1_21_11/outline_shapes.json")).unwrap();
    assert_eq!(java_1_21_11::OUTLINE.state_shapes, outline.state_shapes);
    assert_eq!(java_1_21_11::OUTLINE.registry_fnv64, outline.registry_fnv64);
    boxes_equal(java_1_21_11::OUTLINE.shapes, &outline.shapes);
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Mapping {
        Common(usize),
        PerState(Vec<usize>),
    }
    #[derive(Deserialize)]
    struct Legacy {
        blocks: HashMap<String, Mapping>,
        shapes: HashMap<usize, Vec<[f64; 6]>>,
    }
    let legacy: Legacy =
        serde_json::from_str(include_str!("../../data/block_collision_shapes.json")).unwrap();
    for block in java_1_16_1::STATES {
        for id in block.min_state_id..=block.max_state_id {
            let old_id = match &legacy.blocks[block.name] {
                Mapping::Common(id) => *id,
                Mapping::PerState(ids) => ids[(id - block.min_state_id) as usize],
            };
            assert_eq!(java_1_16_1::COLLISION.state_shapes[id as usize], old_id);
        }
    }
    for (id, shape) in legacy.shapes {
        boxes_equal(&[java_1_16_1::COLLISION.shapes[id]], &[shape]);
    }
}
#[test]
fn recipes_keep_all_shaped_shapeless_and_output_grid_data() {
    use crate::versions::java_1_16_1::registry::{RawRecipe, recipes_for_output};
    let expected: HashMap<i32, Vec<RawRecipe>> =
        serde_json::from_str(include_str!("../../data/recipes.json")).unwrap();
    let actual_count: usize = expected
        .keys()
        .map(|&id| recipes_for_output(id).len())
        .sum();
    assert_eq!(actual_count, java_1_16_1::RECIPES.len());
    for (id, recipes) in expected {
        assert_eq!(recipes_for_output(id), recipes);
    }
    assert!(recipes_for_output(-1).is_empty());
}

#[test]
fn item_mining_material_entity_and_sound_tables_match_every_pinned_field() {
    use crate::versions::java_1_16_1::registry;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Item {
        id: i32,
        name: String,
        stack_size: i32,
    }
    for (input, actual) in [
        (include_str!("../../data/items.json"), java_1_16_1::ITEMS),
        (
            include_str!("../../data/java_1_21_11/items.json"),
            java_1_21_11::ITEMS,
        ),
    ] {
        let expected: Vec<Item> = serde_json::from_str(input).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected) {
            assert_eq!(
                (a.id, a.name, a.stack_size),
                (e.id, e.name.as_str(), e.stack_size)
            );
        }
    }
    for item in java_1_16_1::ITEMS {
        assert_eq!(registry::item_name(item.id), Some(item.name));
        assert_eq!(registry::item_stack_size(item.id), item.stack_size as i8);
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Mining {
        name: String,
        hardness: Option<f64>,
        min_state_id: i32,
        max_state_id: i32,
        #[serde(default)]
        diggable: bool,
        material: Option<String>,
        harvest_tools: Option<BTreeMap<i32, bool>>,
    }
    let blocks: Vec<Mining> = serde_json::from_str(include_str!("../../data/blocks.json")).unwrap();
    assert_eq!(blocks.len(), java_1_16_1::MINING_BLOCKS.len());
    for (a, e) in java_1_16_1::MINING_BLOCKS.iter().zip(blocks) {
        assert_eq!(
            (
                a.name,
                a.min_state_id,
                a.max_state_id,
                a.diggable,
                a.material
            ),
            (
                e.name.as_str(),
                e.min_state_id,
                e.max_state_id,
                e.diggable,
                e.material.as_deref()
            )
        );
        assert_eq!(a.hardness.map(f64::to_bits), e.hardness.map(f64::to_bits));
        assert_eq!(
            a.harvest_tools
                .map(|v| v.iter().copied().collect::<BTreeMap<_, _>>()),
            e.harvest_tools
        );
        assert_eq!(
            registry::block_name_from_state(a.min_state_id),
            Some(a.name)
        );
        assert_eq!(
            registry::block_name_from_state(a.max_state_id),
            Some(a.name)
        );
    }
    let materials: BTreeMap<String, BTreeMap<i32, f64>> =
        serde_json::from_str(include_str!("../../data/materials.json")).unwrap();
    assert_eq!(materials.len(), java_1_16_1::MATERIALS.len());
    for (name, tools) in java_1_16_1::MATERIALS {
        let expected = &materials[*name];
        assert_eq!(tools.len(), expected.len());
        for &(id, speed) in *tools {
            assert_eq!(speed.to_bits(), expected[&id].to_bits());
        }
    }
    #[derive(Deserialize)]
    struct Entity {
        id: i32,
        name: String,
        width: f64,
        height: f64,
    }
    let entities: Vec<Entity> =
        serde_json::from_str(include_str!("../../data/entities.json")).unwrap();
    assert_eq!(entities.len(), java_1_16_1::ENTITIES.len());
    for (&(id, name, width, height), expected) in java_1_16_1::ENTITIES.iter().zip(entities) {
        assert_eq!((id, name), (expected.id, expected.name.as_str()));
        assert_eq!(
            [width.to_bits(), height.to_bits()],
            [expected.width.to_bits(), expected.height.to_bits()]
        );
        assert_eq!(registry::entity_name(id), Some(name));
        assert_eq!(registry::entity_dimensions(id), Some((width, height)));
    }
    #[derive(Deserialize)]
    struct Sound {
        id: i32,
        name: String,
    }
    let sounds: Vec<Sound> = serde_json::from_str(include_str!("../../data/sounds.json")).unwrap();
    assert_eq!(sounds.len(), java_1_16_1::SOUNDS.len());
    for (&(id, name), expected) in java_1_16_1::SOUNDS.iter().zip(sounds) {
        assert_eq!((id, name), (expected.id, expected.name.as_str()));
        assert_eq!(registry::sound_name(id), Some(name));
    }
    assert_eq!(registry::item_name(-1), None);
    assert_eq!(registry::entity_name(-1), None);
    assert_eq!(registry::sound_name(-1), None);
    assert_eq!(registry::block_name_from_state(-1), None);
}
