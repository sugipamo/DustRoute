//! Recipe-book, statistic, and advancement state received from the server.

use crate::versions::java_1_16_1::{
    inventory::{ItemStack, read_slot},
    protocol::{get_string, get_varint},
};
use anyhow::{Result, bail};
use byteorder::{BigEndian, ReadBytesExt};
use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `RecipeBookState`.
pub struct RecipeBookState {
    /// The `crafting_open` value.
    pub crafting_open: bool,
    /// The `crafting_filter` value.
    pub crafting_filter: bool,
    /// The `smelting_open` value.
    pub smelting_open: bool,
    /// The `smelting_filter` value.
    pub smelting_filter: bool,
    /// The `unlocked` value.
    pub unlocked: HashSet<String>,
    /// The `displayed` value.
    pub displayed: HashSet<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `StatisticsState`.
pub struct StatisticsState {
    /// Raw protocol `(category_id, statistic_id) -> value` entries.
    pub values: HashMap<(i32, i32), i32>,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `AdvancementDisplay`.
pub struct AdvancementDisplay {
    /// The `title` value.
    pub title: crate::text_component::ProtocolText,
    /// The `description` value.
    pub description: crate::text_component::ProtocolText,
    /// The `icon` value.
    pub icon: ItemStack,
    /// The `frame_type` value.
    pub frame_type: i32,
    /// The `background` value.
    pub background: Option<String>,
    /// The `show_toast` value.
    pub show_toast: bool,
    /// The `hidden` value.
    pub hidden: bool,
    /// The `x` value.
    pub x: f32,
    /// The `y` value.
    pub y: f32,
}
#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `AdvancementDefinition`.
pub struct AdvancementDefinition {
    /// The `id` value.
    pub id: String,
    /// The `parent` value.
    pub parent: Option<String>,
    /// The `display` value.
    pub display: Option<AdvancementDisplay>,
    /// The `criteria` value.
    pub criteria: Vec<String>,
    /// The `requirements` value.
    pub requirements: Vec<Vec<String>>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `AdvancementProgress`.
pub struct AdvancementProgress {
    /// The `criteria` value.
    pub criteria: HashMap<String, Option<i64>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
/// State and protocol data represented by `AdvancementState`.
pub struct AdvancementState {
    /// The `definitions` value.
    pub definitions: HashMap<String, AdvancementDefinition>,
    /// The `progress` value.
    pub progress: HashMap<String, AdvancementProgress>,
    /// The `selected_tab` value.
    pub selected_tab: Option<String>,
}

impl AdvancementState {
    pub(crate) fn apply(&mut self, payload: &[u8]) -> Result<()> {
        let mut rest = payload;
        if take_bool(&mut rest)? {
            self.definitions.clear();
            self.progress.clear();
        }
        let count = bounded_count(&mut rest, "advancement", 16_384)?;
        for _ in 0..count {
            let id = get_string(&mut rest)?;
            let parent = take_bool(&mut rest)?
                .then(|| get_string(&mut rest))
                .transpose()?;
            let display = if take_bool(&mut rest)? {
                let title = crate::text_component::ProtocolText::received(get_string(&mut rest)?);
                let description =
                    crate::text_component::ProtocolText::received(get_string(&mut rest)?);
                let icon = read_slot(&mut rest)?
                    .ok_or_else(|| anyhow::anyhow!("advancement display has no icon"))?;
                let frame_type = get_varint(&mut rest)?;
                let flags = read_i32(&mut rest)?;
                let background = if flags & 1 != 0 {
                    Some(get_string(&mut rest)?)
                } else {
                    None
                };
                let x = read_f32(&mut rest)?;
                let y = read_f32(&mut rest)?;
                Some(AdvancementDisplay {
                    title,
                    description,
                    icon,
                    frame_type,
                    background,
                    show_toast: flags & 2 != 0,
                    hidden: flags & 4 != 0,
                    x,
                    y,
                })
            } else {
                None
            };
            let criteria = read_strings(&mut rest)?;
            let requirement_count = bounded_count(&mut rest, "requirement", 16_384)?;
            let mut requirements = Vec::with_capacity(requirement_count);
            for _ in 0..requirement_count {
                requirements.push(read_strings(&mut rest)?);
            }
            self.definitions.insert(
                id.clone(),
                AdvancementDefinition {
                    id,
                    parent,
                    display,
                    criteria,
                    requirements,
                },
            );
        }
        for id in read_strings(&mut rest)? {
            self.definitions.remove(&id);
            self.progress.remove(&id);
        }
        let count = bounded_count(&mut rest, "advancement progress", 16_384)?;
        for _ in 0..count {
            let id = get_string(&mut rest)?;
            let n = bounded_count(&mut rest, "criterion progress", 16_384)?;
            let mut criteria = HashMap::with_capacity(n);
            for _ in 0..n {
                let key = get_string(&mut rest)?;
                let done = take_bool(&mut rest)?;
                criteria.insert(
                    key,
                    if done {
                        Some(read_i64(&mut rest)?)
                    } else {
                        None
                    },
                );
            }
            self.progress.insert(id, AdvancementProgress { criteria });
        }
        Ok(())
    }
}

impl RecipeBookState {
    pub(crate) fn apply(&mut self, payload: &[u8]) -> Result<()> {
        let mut rest = payload;
        let action = get_varint(&mut rest)?;
        self.crafting_open = take_bool(&mut rest)?;
        self.crafting_filter = take_bool(&mut rest)?;
        self.smelting_open = take_bool(&mut rest)?;
        self.smelting_filter = take_bool(&mut rest)?;
        let recipes = read_strings(&mut rest)?;
        match action {
            0 => {
                self.unlocked = recipes.into_iter().collect();
                self.displayed = read_strings(&mut rest)?.into_iter().collect();
            }
            1 => self.unlocked.extend(recipes),
            2 => {
                for recipe in recipes {
                    self.unlocked.remove(&recipe);
                    self.displayed.remove(&recipe);
                }
            }
            _ => bail!("unknown recipe-book action {action}"),
        }
        Ok(())
    }
}

impl StatisticsState {
    pub(crate) fn apply(&mut self, payload: &[u8]) -> Result<()> {
        let mut rest = payload;
        let count = get_varint(&mut rest)?;
        if !(0..=1_000_000).contains(&count) {
            bail!("invalid statistics count {count}");
        }
        for _ in 0..count {
            let category = get_varint(&mut rest)?;
            let statistic = get_varint(&mut rest)?;
            let value = get_varint(&mut rest)?;
            self.values.insert((category, statistic), value);
        }
        Ok(())
    }
}

fn read_strings(rest: &mut &[u8]) -> Result<Vec<String>> {
    let count = bounded_count(rest, "string list", 16_384)?;
    (0..count).map(|_| get_string(rest)).collect()
}
fn bounded_count(rest: &mut &[u8], kind: &str, max: i32) -> Result<usize> {
    let count = get_varint(rest)?;
    if !(0..=max).contains(&count) || count as usize > rest.len() {
        bail!("invalid {kind} count {count}")
    }
    Ok(count as usize)
}
fn read_i32(rest: &mut &[u8]) -> Result<i32> {
    let mut c = Cursor::new(*rest);
    let v = c.read_i32::<BigEndian>()?;
    *rest = &rest[4..];
    Ok(v)
}
fn read_i64(rest: &mut &[u8]) -> Result<i64> {
    let mut c = Cursor::new(*rest);
    let v = c.read_i64::<BigEndian>()?;
    *rest = &rest[8..];
    Ok(v)
}
fn read_f32(rest: &mut &[u8]) -> Result<f32> {
    let mut c = Cursor::new(*rest);
    let v = c.read_f32::<BigEndian>()?;
    *rest = &rest[4..];
    Ok(v)
}
fn take_bool(rest: &mut &[u8]) -> Result<bool> {
    let value = *rest
        .first()
        .ok_or_else(|| anyhow::anyhow!("truncated recipe-book flags"))?
        != 0;
    *rest = &rest[1..];
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_16_1::protocol::{put_string, put_varint};
    #[test]
    fn recipe_unlock_add_and_remove_updates_sets() {
        let mut state = RecipeBookState::default();
        let mut add = Vec::new();
        put_varint(&mut add, 1);
        add.extend([1, 0, 0, 0]);
        put_varint(&mut add, 1);
        put_string(&mut add, "minecraft:stick");
        state.apply(&add).unwrap();
        assert!(state.unlocked.contains("minecraft:stick"));
        let mut remove = Vec::new();
        put_varint(&mut remove, 2);
        remove.extend([1, 0, 0, 0]);
        put_varint(&mut remove, 1);
        put_string(&mut remove, "minecraft:stick");
        state.apply(&remove).unwrap();
        assert!(state.unlocked.is_empty());
    }

    #[test]
    fn advancement_packet_keeps_display_requirements_and_progress() {
        use byteorder::WriteBytesExt;
        let mut packet = vec![1];
        put_varint(&mut packet, 1);
        put_string(&mut packet, "minecraft:test");
        packet.push(0);
        packet.push(1);
        put_string(&mut packet, r#"{"text":"Test"}"#);
        put_string(&mut packet, r#"{"text":"Description"}"#);
        crate::versions::java_1_16_1::inventory::write_slot(
            &mut packet,
            Some(&ItemStack {
                item_id: 1,
                count: 1,
                nbt: None,
            }),
        );
        put_varint(&mut packet, 0);
        packet.write_i32::<BigEndian>(3).unwrap();
        put_string(
            &mut packet,
            "minecraft:textures/gui/advancements/backgrounds/stone.png",
        );
        packet.write_f32::<BigEndian>(1.5).unwrap();
        packet.write_f32::<BigEndian>(-2.0).unwrap();
        put_varint(&mut packet, 1);
        put_string(&mut packet, "criterion");
        put_varint(&mut packet, 1);
        put_varint(&mut packet, 1);
        put_string(&mut packet, "criterion");
        put_varint(&mut packet, 0);
        put_varint(&mut packet, 1);
        put_string(&mut packet, "minecraft:test");
        put_varint(&mut packet, 1);
        put_string(&mut packet, "criterion");
        packet.push(1);
        packet.write_i64::<BigEndian>(1234).unwrap();
        let mut state = AdvancementState::default();
        state.apply(&packet).unwrap();
        let definition = &state.definitions["minecraft:test"];
        assert_eq!(definition.requirements, vec![vec!["criterion".to_string()]]);
        assert!(definition.display.as_ref().unwrap().show_toast);
        assert_eq!(
            state.progress["minecraft:test"].criteria["criterion"],
            Some(1234)
        );
    }
}
