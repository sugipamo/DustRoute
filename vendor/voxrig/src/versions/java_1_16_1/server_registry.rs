//! Runtime command-tree, tag, and recipe declarations sent by the server.

use crate::versions::java_1_16_1::{
    inventory::{ItemStack, read_slot},
    protocol::{get_string, get_varint},
};
use anyhow::{Context, Result, bail};
use byteorder::{BigEndian, ReadBytesExt};
use std::{collections::HashMap, io::Cursor};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Possible values represented by `CommandNodeKind`.
pub enum CommandNodeKind {
    /// The `Root` variant.
    Root,
    /// The `Literal` variant.
    Literal,
    /// The `Argument` variant.
    Argument,
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `CommandNode`.
pub struct CommandNode {
    /// The `kind` value.
    pub kind: CommandNodeKind,
    /// The `executable` value.
    pub executable: bool,
    /// The `children` value.
    pub children: Vec<i32>,
    /// The `redirect` value.
    pub redirect: Option<i32>,
    /// The `name` value.
    pub name: Option<String>,
    /// The `parser` value.
    pub parser: Option<String>,
    /// The `parser_properties` value.
    pub parser_properties: Vec<u8>,
    /// The `suggestion_type` value.
    pub suggestion_type: Option<String>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `CommandTree`.
pub struct CommandTree {
    /// The `nodes` value.
    pub nodes: Vec<CommandNode>,
    /// The `root_index` value.
    pub root_index: i32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `ServerTags`.
pub struct ServerTags {
    /// The `blocks` value.
    pub blocks: HashMap<String, Vec<i32>>,
    /// The `items` value.
    pub items: HashMap<String, Vec<i32>>,
    /// The `fluids` value.
    pub fluids: HashMap<String, Vec<i32>>,
    /// The `entities` value.
    pub entities: HashMap<String, Vec<i32>>,
}

/// Public type alias `Ingredient`.
pub type Ingredient = Vec<Option<ItemStack>>;
#[derive(Clone, Debug, PartialEq)]
/// Possible values represented by `ServerRecipeData`.
pub enum ServerRecipeData {
    /// The `Shapeless` variant.
    Shapeless {
        /// The `group` value carried by this variant.
        group: String,
        /// The `ingredients` value carried by this variant.
        ingredients: Vec<Ingredient>,
        /// The `result` value carried by this variant.
        result: ItemStack,
    },
    /// The `Shaped` variant.
    Shaped {
        /// The `width` value carried by this variant.
        width: i32,
        /// The `height` value carried by this variant.
        height: i32,
        /// The `group` value carried by this variant.
        group: String,
        /// The `ingredients` value carried by this variant.
        ingredients: Vec<Ingredient>,
        /// The `result` value carried by this variant.
        result: ItemStack,
    },
    /// The `Cooking` variant.
    Cooking {
        /// The `group` value carried by this variant.
        group: String,
        /// The `ingredient` value carried by this variant.
        ingredient: Ingredient,
        /// The `result` value carried by this variant.
        result: ItemStack,
        /// The `experience` value carried by this variant.
        experience: f32,
        /// The `cook_time` value carried by this variant.
        cook_time: i32,
    },
    /// The `Stonecutting` variant.
    Stonecutting {
        /// The `group` value carried by this variant.
        group: String,
        /// The `ingredient` value carried by this variant.
        ingredient: Ingredient,
        /// The `result` value carried by this variant.
        result: ItemStack,
    },
    /// The `Smithing` variant.
    Smithing {
        /// The `base` value carried by this variant.
        base: Ingredient,
        /// The `addition` value carried by this variant.
        addition: Ingredient,
        /// The `result` value carried by this variant.
        result: ItemStack,
    },
    /// The `Special` variant.
    Special,
}
#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `ServerRecipe`.
pub struct ServerRecipe {
    /// The `kind` value.
    pub kind: String,
    /// The `id` value.
    pub id: String,
    /// The `data` value.
    pub data: ServerRecipeData,
}
#[derive(Clone, Debug, Default, PartialEq)]
/// State and protocol data represented by `ServerRecipes`.
pub struct ServerRecipes {
    /// The `recipes` value.
    pub recipes: HashMap<String, ServerRecipe>,
}

pub(crate) fn parse_command_tree(payload: &[u8]) -> Result<CommandTree> {
    let mut rest = payload;
    let count = bounded_count(&mut rest, "command node", 16_384)?;
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        let flags = take_u8(&mut rest)?;
        let kind = match flags & 3 {
            0 => CommandNodeKind::Root,
            1 => CommandNodeKind::Literal,
            2 => CommandNodeKind::Argument,
            v => bail!("unknown command node kind {v}"),
        };
        let children = read_varints(&mut rest)?;
        let redirect = if flags & 8 != 0 {
            Some(get_varint(&mut rest)?)
        } else {
            None
        };
        let name = if kind != CommandNodeKind::Root {
            Some(get_string(&mut rest)?)
        } else {
            None
        };
        let (parser, parser_properties) = if kind == CommandNodeKind::Argument {
            let parser = get_string(&mut rest)?;
            let before = rest.len();
            skip_parser_properties(&parser, &mut rest)?;
            (Some(parser), rest_at(payload, before, rest.len()))
        } else {
            (None, Vec::new())
        };
        let suggestion_type = if flags & 16 != 0 {
            Some(get_string(&mut rest)?)
        } else {
            None
        };
        nodes.push(CommandNode {
            kind,
            executable: flags & 4 != 0,
            children,
            redirect,
            name,
            parser,
            parser_properties,
            suggestion_type,
        });
    }
    let root_index = get_varint(&mut rest)?;
    if root_index < 0 || root_index as usize >= nodes.len() {
        bail!("command root index {root_index} out of range")
    }
    Ok(CommandTree { nodes, root_index })
}

// Reconstructs the consumed suffix without retaining a borrow into the packet.
fn rest_at(payload: &[u8], before: usize, after: usize) -> Vec<u8> {
    let consumed = before - after;
    payload[payload.len() - before..payload.len() - before + consumed].to_vec()
}
fn skip_parser_properties(parser: &str, rest: &mut &[u8]) -> Result<()> {
    match parser {
        "brigadier:float" => skip_bounds(rest, 4)?,
        "brigadier:double" => skip_bounds(rest, 8)?,
        "brigadier:integer" => skip_bounds(rest, 4)?,
        "brigadier:long" => skip_bounds(rest, 8)?,
        "brigadier:string" => {
            get_varint(rest)?;
        }
        "minecraft:entity" | "minecraft:score_holder" | "minecraft:range" => {
            take_u8(rest)?;
        }
        "minecraft:resource_or_tag" | "minecraft:resource" => {
            get_string(rest)?;
        }
        "brigadier:bool"
        | "minecraft:game_profile"
        | "minecraft:block_pos"
        | "minecraft:column_pos"
        | "minecraft:vec3"
        | "minecraft:vec2"
        | "minecraft:block_state"
        | "minecraft:block_predicate"
        | "minecraft:item_stack"
        | "minecraft:item_predicate"
        | "minecraft:color"
        | "minecraft:component"
        | "minecraft:message"
        | "minecraft:nbt"
        | "minecraft:nbt_path"
        | "minecraft:objective"
        | "minecraft:objective_criteria"
        | "minecraft:operation"
        | "minecraft:particle"
        | "minecraft:angle"
        | "minecraft:rotation"
        | "minecraft:scoreboard_slot"
        | "minecraft:swizzle"
        | "minecraft:team"
        | "minecraft:item_slot"
        | "minecraft:resource_location"
        | "minecraft:mob_effect"
        | "minecraft:function"
        | "minecraft:entity_anchor"
        | "minecraft:int_range"
        | "minecraft:float_range"
        | "minecraft:item_enchantment"
        | "minecraft:entity_summon"
        | "minecraft:dimension"
        | "minecraft:nbt_compound_tag"
        | "minecraft:time"
        | "minecraft:uuid" => {}
        _ => bail!("unsupported command parser {parser}"),
    }
    Ok(())
}
fn skip_bounds(rest: &mut &[u8], width: usize) -> Result<()> {
    let flags = take_u8(rest)?;
    let fields = usize::from(flags & 1 != 0) + usize::from(flags & 2 != 0);
    let bytes = fields * width;
    if rest.len() < bytes {
        bail!("truncated command parser bounds")
    }
    *rest = &rest[bytes..];
    Ok(())
}
pub(crate) fn parse_tags(payload: &[u8]) -> Result<ServerTags> {
    let mut rest = payload;
    Ok(ServerTags {
        blocks: read_tags(&mut rest)?,
        items: read_tags(&mut rest)?,
        fluids: read_tags(&mut rest)?,
        entities: read_tags(&mut rest)?,
    })
}
pub(crate) fn parse_recipes(payload: &[u8]) -> Result<ServerRecipes> {
    let mut rest = payload;
    let count = bounded_count(&mut rest, "recipe", 65_536)?;
    let mut recipes = HashMap::with_capacity(count);
    for _ in 0..count {
        let kind = get_string(&mut rest)?;
        let id = get_string(&mut rest)?;
        let data = match kind.as_str() {
            "minecraft:crafting_shapeless" => {
                let group = get_string(&mut rest)?;
                let n = bounded_count(&mut rest, "ingredient", 1_024)?;
                let mut ingredients = Vec::with_capacity(n);
                for _ in 0..n {
                    ingredients.push(read_ingredient(&mut rest)?);
                }
                let result = required_slot(&mut rest)?;
                ServerRecipeData::Shapeless {
                    group,
                    ingredients,
                    result,
                }
            }
            "minecraft:crafting_shaped" => {
                let width = get_varint(&mut rest)?;
                let height = get_varint(&mut rest)?;
                if width < 0 || height < 0 || width.saturating_mul(height) > 10_000 {
                    bail!("invalid shaped recipe dimensions {width}x{height}")
                }
                let group = get_string(&mut rest)?;
                let mut ingredients = Vec::with_capacity((width * height) as usize);
                for _ in 0..width * height {
                    ingredients.push(read_ingredient(&mut rest)?);
                }
                let result = required_slot(&mut rest)?;
                ServerRecipeData::Shaped {
                    width,
                    height,
                    group,
                    ingredients,
                    result,
                }
            }
            "minecraft:smelting"
            | "minecraft:blasting"
            | "minecraft:smoking"
            | "minecraft:campfire_cooking" => {
                let group = get_string(&mut rest)?;
                let ingredient = read_ingredient(&mut rest)?;
                let result = required_slot(&mut rest)?;
                let experience = read_f32(&mut rest)?;
                let cook_time = get_varint(&mut rest)?;
                ServerRecipeData::Cooking {
                    group,
                    ingredient,
                    result,
                    experience,
                    cook_time,
                }
            }
            "minecraft:stonecutting" => {
                let group = get_string(&mut rest)?;
                let ingredient = read_ingredient(&mut rest)?;
                let result = required_slot(&mut rest)?;
                ServerRecipeData::Stonecutting {
                    group,
                    ingredient,
                    result,
                }
            }
            "minecraft:smithing" => {
                let base = read_ingredient(&mut rest)?;
                let addition = read_ingredient(&mut rest)?;
                let result = required_slot(&mut rest)?;
                ServerRecipeData::Smithing {
                    base,
                    addition,
                    result,
                }
            }
            special if special.starts_with("minecraft:crafting_special_") => {
                ServerRecipeData::Special
            }
            _ => bail!("unsupported declared recipe type {kind}"),
        };
        recipes.insert(id.clone(), ServerRecipe { kind, id, data });
    }
    Ok(ServerRecipes { recipes })
}
fn read_ingredient(rest: &mut &[u8]) -> Result<Ingredient> {
    let count = bounded_count(rest, "ingredient option", 1_024)?;
    (0..count).map(|_| read_slot(rest)).collect()
}
fn required_slot(rest: &mut &[u8]) -> Result<ItemStack> {
    read_slot(rest)?.context("declared recipe result is empty")
}
fn read_f32(rest: &mut &[u8]) -> Result<f32> {
    let mut c = Cursor::new(*rest);
    let value = c.read_f32::<BigEndian>()?;
    *rest = &rest[4..];
    Ok(value)
}
fn read_tags(rest: &mut &[u8]) -> Result<HashMap<String, Vec<i32>>> {
    let count = bounded_count(rest, "tag", 65_536)?;
    let mut tags = HashMap::with_capacity(count);
    for _ in 0..count {
        tags.insert(get_string(rest)?, read_varints(rest)?);
    }
    Ok(tags)
}
fn read_varints(rest: &mut &[u8]) -> Result<Vec<i32>> {
    let count = bounded_count(rest, "varint list", 262_144)?;
    (0..count).map(|_| get_varint(rest)).collect()
}
fn bounded_count(rest: &mut &[u8], kind: &str, max: i32) -> Result<usize> {
    let count = get_varint(rest)?;
    if !(0..=max).contains(&count) || count as usize > rest.len() {
        bail!("invalid {kind} count {count}")
    }
    Ok(count as usize)
}
fn take_u8(rest: &mut &[u8]) -> Result<u8> {
    let value = *rest.first().context("truncated registry packet")?;
    *rest = &rest[1..];
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_16_1::{
        inventory::write_slot,
        protocol::{put_string, put_varint},
    };
    #[test]
    fn command_tree_parses_literal_nodes() {
        let mut p = Vec::new();
        put_varint(&mut p, 2);
        p.push(0);
        put_varint(&mut p, 1);
        put_varint(&mut p, 1);
        p.push(5);
        put_varint(&mut p, 0);
        put_string(&mut p, "help");
        put_varint(&mut p, 0);
        let tree = parse_command_tree(&p).unwrap();
        assert_eq!(tree.nodes[1].name.as_deref(), Some("help"));
        assert!(tree.nodes[1].executable);
    }
    #[test]
    fn declared_shapeless_recipe_is_structured() {
        let mut p = Vec::new();
        put_varint(&mut p, 1);
        put_string(&mut p, "minecraft:crafting_shapeless");
        put_string(&mut p, "minecraft:test");
        put_string(&mut p, "");
        put_varint(&mut p, 1);
        put_varint(&mut p, 1);
        write_slot(
            &mut p,
            Some(&ItemStack {
                item_id: 1,
                count: 1,
                nbt: None,
            }),
        );
        write_slot(
            &mut p,
            Some(&ItemStack {
                item_id: 2,
                count: 1,
                nbt: None,
            }),
        );
        let recipes = parse_recipes(&p).unwrap();
        match &recipes.recipes["minecraft:test"].data {
            ServerRecipeData::Shapeless {
                ingredients,
                result,
                ..
            } => {
                assert_eq!(ingredients.len(), 1);
                assert_eq!(result.item_id, 2)
            }
            _ => panic!("wrong recipe type"),
        }
    }

    #[test]
    fn declared_count_cannot_exceed_remaining_packet() {
        let mut payload = Vec::new();
        put_varint(&mut payload, 1_000_000);
        assert!(parse_recipes(&payload).is_err());
        assert!(parse_command_tree(&payload).is_err());
    }
}
