//! Embedded Minecraft 1.16.1 registry and mining-time queries.

use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

struct Registry {
    blocks: &'static [crate::tables::MiningBlock],
    items: HashMap<i32, (&'static str, i8)>,
    materials: HashMap<&'static str, HashMap<i32, f64>>,
    recipes: HashMap<i32, Vec<RawRecipe>>,
    entities: HashMap<i32, (&'static str, f64, f64)>,
    sounds: HashMap<i32, &'static str>,
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
    REGISTRY.get_or_init(|| {
        use crate::tables::java_1_16_1 as data;
        let mut recipes: HashMap<i32, Vec<RawRecipe>> = HashMap::new();
        let grid =
            |rows: &'static [&'static [Option<i32>]]| rows.iter().map(|row| row.to_vec()).collect();
        for recipe in data::RECIPES {
            recipes.entry(recipe.output).or_default().push(RawRecipe {
                result: RecipeResult {
                    id: recipe.result.0,
                    count: recipe.result.1,
                },
                ingredients: recipe.ingredients.map(<[i32]>::to_vec),
                in_shape: recipe.in_shape.map(grid),
                out_shape: recipe.out_shape.map(grid),
            });
        }
        Registry {
            blocks: data::MINING_BLOCKS,
            items: data::ITEMS
                .iter()
                .map(|i| (i.id, (i.name, i.stack_size as i8)))
                .collect(),
            materials: data::MATERIALS
                .iter()
                .map(|(name, tools)| (*name, tools.iter().copied().collect()))
                .collect(),
            recipes,
            entities: data::ENTITIES
                .iter()
                .map(|&(id, name, width, height)| (id, (name, width, height)))
                .collect(),
            sounds: data::SOUNDS.iter().copied().collect(),
        }
    })
}

/// Performs the `recipes_for_output` operation.
pub fn recipes_for_output(item_id: i32) -> &'static [RawRecipe] {
    registry().recipes.get(&item_id).map_or(&[], Vec::as_slice)
}

/// Performs the `entity_name` operation.
pub fn entity_name(id: i32) -> Option<&'static str> {
    registry().entities.get(&id).map(|(name, _, _)| *name)
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
    registry().sounds.get(&id).copied()
}

/// Performs the `item_name` operation.
pub fn item_name(id: i32) -> Option<&'static str> {
    registry().items.get(&id).map(|(name, _)| *name)
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
        .map(|block| block.name)
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
    let tool_speed = block
        .material
        .and_then(|material| registry.materials.get(material))
        .and_then(|tools| tool_id.as_ref().and_then(|key| tools.get(key)))
        .copied()
        .unwrap_or(1.0);
    let harvestable = block.harvest_tools.is_none_or(|tools| {
        tool_id.is_some_and(|id| tools.iter().any(|&(tool, harvests)| tool == id && harvests))
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
