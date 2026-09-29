//! Embedded Minecraft 1.16.1 registry and mining-time queries.

use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlockData {
    name: String,
    hardness: Option<f64>,
    min_state_id: i32,
    max_state_id: i32,
    #[serde(default)]
    diggable: bool,
    material: Option<String>,
    harvest_tools: Option<HashMap<String, bool>>,
}

#[derive(Clone, Debug, Deserialize)]
struct ItemData {
    id: i32,
    name: String,
    #[serde(rename = "stackSize")]
    stack_size: i8,
}

#[derive(Clone, Debug, Deserialize)]
struct EntityData {
    id: i32,
    name: String,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct SoundData {
    id: i32,
    name: String,
}

struct Registry {
    blocks: Vec<BlockData>,
    items: HashMap<i32, (String, i8)>,
    materials: HashMap<String, HashMap<String, f64>>,
    recipes: HashMap<String, Vec<RawRecipe>>,
    entities: HashMap<i32, (String, f64, f64)>,
    sounds: HashMap<i32, String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// State and protocol data represented by `MiningInfo`.
pub struct MiningInfo {
    /// The `diggable` value.
    pub diggable: bool,
    /// The `harvestable` value.
    pub harvestable: bool,
    /// The `effective_tool` value.
    pub effective_tool: bool,
    /// The `predicted_ticks` value.
    pub predicted_ticks: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// State and protocol data represented by `RecipeResult`.
pub struct RecipeResult {
    /// The `id` value.
    pub id: i32,
    /// The `count` value.
    pub count: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// State and protocol data represented by `RawRecipe`.
pub struct RawRecipe {
    /// The `result` value.
    pub result: RecipeResult,
    #[serde(default)]
    /// The `ingredients` value.
    pub ingredients: Option<Vec<i32>>,
    #[serde(default)]
    /// The `in_shape` value.
    pub in_shape: Option<Vec<Vec<Option<i32>>>>,
    #[serde(default)]
    /// The `out_shape` value.
    pub out_shape: Option<Vec<Vec<Option<i32>>>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        blocks: serde_json::from_str(include_str!("../../../data/blocks.json"))
            .expect("embedded 1.16.1 blocks registry must be valid"),
        items: serde_json::from_str::<Vec<ItemData>>(include_str!("../../../data/items.json"))
            .expect("embedded 1.16.1 items registry must be valid")
            .into_iter()
            .map(|item| (item.id, (item.name, item.stack_size)))
            .collect(),
        materials: serde_json::from_str(include_str!("../../../data/materials.json"))
            .expect("embedded 1.16.1 materials registry must be valid"),
        recipes: serde_json::from_str(include_str!("../../../data/recipes.json"))
            .expect("embedded 1.16.1 recipes registry must be valid"),
        entities: serde_json::from_str::<Vec<EntityData>>(include_str!(
            "../../../data/entities.json"
        ))
        .expect("embedded 1.16.1 entities registry must be valid")
        .into_iter()
        .map(|entity| (entity.id, (entity.name, entity.width, entity.height)))
        .collect(),
        sounds: serde_json::from_str::<Vec<SoundData>>(include_str!("../../../data/sounds.json"))
            .expect("embedded 1.16.1 sounds registry must be valid")
            .into_iter()
            .map(|sound| (sound.id, sound.name))
            .collect(),
    })
}

/// Performs the `recipes_for_output` operation.
pub fn recipes_for_output(item_id: i32) -> &'static [RawRecipe] {
    registry()
        .recipes
        .get(&item_id.to_string())
        .map_or(&[], Vec::as_slice)
}

/// Performs the `entity_name` operation.
pub fn entity_name(id: i32) -> Option<&'static str> {
    registry()
        .entities
        .get(&id)
        .map(|(name, _, _)| name.as_str())
}

/// Performs the `entity_dimensions` operation.
pub fn entity_dimensions(id: i32) -> Option<(f64, f64)> {
    registry()
        .entities
        .get(&id)
        .map(|(_, width, height)| (*width, *height))
}

/// Performs the `sound_name` operation.
pub fn sound_name(id: i32) -> Option<&'static str> {
    registry().sounds.get(&id).map(String::as_str)
}

/// Performs the `item_name` operation.
pub fn item_name(id: i32) -> Option<&'static str> {
    registry().items.get(&id).map(|(name, _)| name.as_str())
}

pub(crate) fn item_stack_size(id: i32) -> i8 {
    registry().items.get(&id).map_or(64, |(_, size)| *size)
}

/// Performs the `block_name_from_state` operation.
pub fn block_name_from_state(state_id: i32) -> Option<&'static str> {
    registry()
        .blocks
        .iter()
        .find(|block| (block.min_state_id..=block.max_state_id).contains(&state_id))
        .map(|block| block.name.as_str())
}

/// Performs the `mining_info` operation.
pub fn mining_info(state_id: i32, tool_id: Option<i32>) -> Option<MiningInfo> {
    let registry = registry();
    let block = registry
        .blocks
        .iter()
        .find(|block| (block.min_state_id..=block.max_state_id).contains(&state_id))?;
    if !block.diggable {
        return Some(MiningInfo {
            diggable: false,
            harvestable: false,
            effective_tool: false,
            predicted_ticks: None,
        });
    }
    let hardness = block.hardness?;
    if hardness <= 0.0 {
        return Some(MiningInfo {
            diggable: true,
            harvestable: true,
            effective_tool: true,
            predicted_ticks: Some(0),
        });
    }
    let tool_key = tool_id.map(|id| id.to_string());
    let tool_speed = block
        .material
        .as_ref()
        .and_then(|material| registry.materials.get(material))
        .and_then(|tools| tool_key.as_ref().and_then(|key| tools.get(key)))
        .copied()
        .unwrap_or(1.0);
    let harvestable = block.harvest_tools.as_ref().is_none_or(|tools| {
        tool_key
            .as_ref()
            .is_some_and(|key| tools.get(key).copied().unwrap_or(false))
    });
    let damage_per_tick = tool_speed / hardness / if harvestable { 30.0 } else { 100.0 };
    Some(MiningInfo {
        diggable: true,
        harvestable,
        effective_tool: tool_speed > 1.0,
        predicted_ticks: Some((1.0 / damage_per_tick).ceil() as u64),
    })
}

pub(crate) fn mining_ticks(state_id: i32, tool_id: Option<i32>) -> Option<u64> {
    mining_info(state_id, tool_id)?.predicted_ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_ids_and_vanilla_mining_times() {
        assert_eq!(item_name(1), Some("stone"));
        assert_eq!(block_name_from_state(1), Some("stone"));
        assert_eq!(mining_ticks(1, None), Some(150));
        assert_eq!(mining_ticks(1, Some(589)), Some(23));
        assert!(!recipes_for_output(589).is_empty());
        assert_eq!(entity_name(5), Some("blaze"));
        assert_eq!(sound_name(0), Some("ambient.cave"));
    }
}
