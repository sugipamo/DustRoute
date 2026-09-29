//! Player inventory, container windows, item stacks, and click transactions.

use crate::versions::java_1_16_1::{
    protocol::{get_varint, put_varint},
    world::skip_nbt,
};
use anyhow::{Context, Result, bail};
use byteorder::{BigEndian, ReadBytesExt};
use std::{collections::HashMap, io::Cursor};

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `ItemStack`.
pub struct ItemStack {
    /// The `item_id` value.
    pub item_id: i32,
    /// The `count` value.
    pub count: i8,
    /// Complete optional-NBT payload, including its root tag byte.
    pub nbt: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
/// State and protocol data represented by `InventoryState`.
pub struct InventoryState {
    /// The `windows` value.
    pub windows: HashMap<i8, Vec<Option<ItemStack>>>,
    /// The `cursor` value.
    pub cursor: Option<ItemStack>,
    /// The `selected_hotbar` value.
    pub selected_hotbar: u8,
    /// The `open_window` value.
    pub open_window: Option<OpenWindow>,
    /// The `properties` value.
    pub properties: HashMap<(i8, i16), i16>,
    /// The `pending_clicks` value.
    pub pending_clicks: HashMap<(i8, i16), PendingClick>,
    /// The `merchant_offers` value.
    pub merchant_offers: Option<MerchantOffers>,
    pub(crate) next_actions: HashMap<i8, i16>,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `MerchantOffer`.
pub struct MerchantOffer {
    /// The `input` value.
    pub input: ItemStack,
    /// The `output` value.
    pub output: ItemStack,
    /// The `second_input` value.
    pub second_input: Option<ItemStack>,
    /// The `disabled` value.
    pub disabled: bool,
    /// The `uses` value.
    pub uses: i32,
    /// The `max_uses` value.
    pub max_uses: i32,
    /// The `xp` value.
    pub xp: i32,
    /// The `special_price` value.
    pub special_price: i32,
    /// The `price_multiplier` value.
    pub price_multiplier: f32,
    /// The `demand` value.
    pub demand: i32,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `MerchantOffers`.
pub struct MerchantOffers {
    /// The `window_id` value.
    pub window_id: i8,
    /// The `offers` value.
    pub offers: Vec<MerchantOffer>,
    /// The `villager_level` value.
    pub villager_level: i32,
    /// The `experience` value.
    pub experience: i32,
    /// The `regular_villager` value.
    pub regular_villager: bool,
    /// The `can_restock` value.
    pub can_restock: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `OpenWindow`.
pub struct OpenWindow {
    /// The `id` value.
    pub id: i8,
    /// The `window_type` value.
    pub window_type: i32,
    /// Raw JSON chat component supplied by the server.
    pub title_json: String,
    /// The `entity_id` value.
    pub entity_id: Option<i32>,
    /// The `declared_slots` value.
    pub declared_slots: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i8)]
/// Possible values represented by `ClickMode`.
pub enum ClickMode {
    /// Documentation for this public variant.
    Normal = 0,
    /// Documentation for this public variant.
    Shift = 1,
    /// Documentation for this public variant.
    Hotbar = 2,
    /// Documentation for this public variant.
    Middle = 3,
    /// Documentation for this public variant.
    Drop = 4,
    /// Documentation for this public variant.
    Drag = 5,
    /// Documentation for this public variant.
    Double = 6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
/// Possible values represented by `EquipmentSlot`.
pub enum EquipmentSlot {
    /// Documentation for this public variant.
    Helmet = 5,
    /// Documentation for this public variant.
    Chestplate = 6,
    /// Documentation for this public variant.
    Leggings = 7,
    /// Documentation for this public variant.
    Boots = 8,
    /// Documentation for this public variant.
    Offhand = 45,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `PendingClick`.
pub struct PendingClick {
    /// The `window_id` value.
    pub window_id: i8,
    /// The `action` value.
    pub action: i16,
    /// The `slot` value.
    pub slot: i16,
    /// The `button` value.
    pub button: i8,
    /// The `mode` value.
    pub mode: ClickMode,
    /// The `slot_before` value.
    pub slot_before: Option<ItemStack>,
    /// The `cursor_before` value.
    pub cursor_before: Option<ItemStack>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// State and protocol data represented by `WindowTransaction`.
pub struct WindowTransaction {
    /// The `window_id` value.
    pub window_id: i8,
    /// The `action` value.
    pub action: i16,
    /// The `accepted` value.
    pub accepted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// State and protocol data represented by `WindowProperty`.
pub struct WindowProperty {
    /// The `window_id` value.
    pub window_id: i8,
    /// The `property` value.
    pub property: i16,
    /// The `value` value.
    pub value: i16,
}

impl InventoryState {
    /// Performs the `player_slots` operation.
    pub fn player_slots(&self) -> &[Option<ItemStack>] {
        self.windows.get(&0).map_or(&[], Vec::as_slice)
    }

    /// Performs the `selected_item` operation.
    pub fn selected_item(&self) -> Option<&ItemStack> {
        self.player_slots()
            .get(36 + usize::from(self.selected_hotbar))
            .and_then(Option::as_ref)
    }
}

impl ItemStack {
    /// Performs the `name` operation.
    pub fn name(&self) -> Option<&'static str> {
        crate::versions::java_1_16_1::registry::item_name(self.item_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `SlotUpdate`.
pub struct SlotUpdate {
    /// The `window_id` value.
    pub window_id: i8,
    /// The `slot` value.
    pub slot: i16,
    /// The `item` value.
    pub item: Option<ItemStack>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// State and protocol data represented by `ItemCollected`.
pub struct ItemCollected {
    /// The `collected_entity_id` value.
    pub collected_entity_id: i32,
    /// The `collector_entity_id` value.
    pub collector_entity_id: i32,
    /// The `count` value.
    pub count: i32,
}

pub(crate) fn read_slot(input: &mut &[u8]) -> Result<Option<ItemStack>> {
    let present = *input.first().context("missing slot presence")? != 0;
    *input = &input[1..];
    if !present {
        return Ok(None);
    }
    let item_id = get_varint(input)?;
    let count = *input.first().context("missing item count")? as i8;
    *input = &input[1..];
    let nbt = if input.first() == Some(&0) {
        *input = &input[1..];
        None
    } else {
        let original = *input;
        let mut cursor = Cursor::new(original);
        skip_nbt(&mut cursor)?;
        let length = cursor.position() as usize;
        *input = &original[length..];
        Some(original[..length].to_vec())
    };
    Ok(Some(ItemStack {
        item_id,
        count,
        nbt,
    }))
}

pub(crate) fn parse_merchant_offers(payload: &[u8]) -> Result<MerchantOffers> {
    let mut rest = payload;
    let raw_window_id = get_varint(&mut rest)?;
    let window_id = i8::try_from(raw_window_id).context("merchant window ID does not fit i8")?;
    let count = usize::from(*rest.first().context("missing merchant offer count")?);
    rest = &rest[1..];
    let mut offers = Vec::with_capacity(count);
    for _ in 0..count {
        let input = read_slot(&mut rest)?.context("merchant offer missing first input")?;
        let output = read_slot(&mut rest)?.context("merchant offer missing output")?;
        let has_second = *rest.first().context("missing second input flag")? != 0;
        rest = &rest[1..];
        let second_input = if has_second {
            read_slot(&mut rest)?
        } else {
            None
        };
        let disabled = *rest.first().context("missing trade disabled flag")? != 0;
        rest = &rest[1..];
        let mut cursor = Cursor::new(rest);
        let uses = cursor.read_i32::<BigEndian>()?;
        let max_uses = cursor.read_i32::<BigEndian>()?;
        let xp = cursor.read_i32::<BigEndian>()?;
        let special_price = cursor.read_i32::<BigEndian>()?;
        let price_multiplier = cursor.read_f32::<BigEndian>()?;
        let demand = cursor.read_i32::<BigEndian>()?;
        rest = &rest[cursor.position() as usize..];
        offers.push(MerchantOffer {
            input,
            output,
            second_input,
            disabled,
            uses,
            max_uses,
            xp,
            special_price,
            price_multiplier,
            demand,
        });
    }
    let villager_level = get_varint(&mut rest)?;
    let experience = get_varint(&mut rest)?;
    let regular_villager = *rest.first().context("missing regular-villager flag")? != 0;
    let can_restock = *rest.get(1).context("missing can-restock flag")? != 0;
    Ok(MerchantOffers {
        window_id,
        offers,
        villager_level,
        experience,
        regular_villager,
        can_restock,
    })
}

pub(crate) fn write_slot(output: &mut Vec<u8>, item: Option<&ItemStack>) {
    let Some(item) = item else {
        output.push(0);
        return;
    };
    output.push(1);
    put_varint(output, item.item_id);
    output.push(item.count as u8);
    if let Some(nbt) = &item.nbt {
        output.extend_from_slice(nbt);
    } else {
        output.push(0);
    }
}

pub(crate) fn parse_window_items(payload: &[u8]) -> Result<(i8, Vec<Option<ItemStack>>)> {
    let window_id = *payload.first().context("missing window ID")? as i8;
    let mut cursor = Cursor::new(&payload[1..]);
    let count = cursor.read_i16::<BigEndian>()?;
    if !(0..=4096).contains(&count) {
        bail!("invalid window slot count {count}");
    }
    let mut rest = &payload[1 + cursor.position() as usize..];
    let mut items = Vec::with_capacity(count as usize);
    for _ in 0..count {
        items.push(read_slot(&mut rest)?);
    }
    Ok((window_id, items))
}

pub(crate) fn parse_set_slot(payload: &[u8]) -> Result<SlotUpdate> {
    let window_id = *payload.first().context("missing window ID")? as i8;
    let mut cursor = Cursor::new(&payload[1..]);
    let slot = cursor.read_i16::<BigEndian>()?;
    let mut rest = &payload[1 + cursor.position() as usize..];
    Ok(SlotUpdate {
        window_id,
        slot,
        item: read_slot(&mut rest)?,
    })
}

pub(crate) fn apply_slot(state: &mut InventoryState, update: &SlotUpdate) -> Result<()> {
    if update.window_id == -1 && update.slot == -1 {
        state.cursor = update.item.clone();
        return Ok(());
    }
    if update.slot < 0 {
        return Ok(());
    }
    let slots = state.windows.entry(update.window_id).or_default();
    let index = update.slot as usize;
    if index >= 4096 {
        bail!("invalid slot index {index}");
    }
    if slots.len() <= index {
        slots.resize(index + 1, None);
    }
    slots[index] = update.item.clone();
    sync_player_slot_from_window(state, update.window_id, index);
    Ok(())
}

pub(crate) fn sync_player_inventory_from_window(state: &mut InventoryState, window_id: i8) {
    if window_id == 0 {
        return;
    }
    let Some(window) = state.windows.get(&window_id) else {
        return;
    };
    let Some(player_start) = window.len().checked_sub(36) else {
        return;
    };
    let appended = window[player_start..].to_vec();
    let player = state.windows.entry(0).or_insert_with(|| vec![None; 46]);
    if player.len() < 46 {
        player.resize(46, None);
    }
    for (relative, item) in appended.into_iter().enumerate() {
        let player_slot = if relative < 27 {
            9 + relative
        } else {
            36 + relative - 27
        };
        player[player_slot] = item;
    }
}

fn sync_player_slot_from_window(state: &mut InventoryState, window_id: i8, index: usize) {
    if window_id == 0 {
        return;
    }
    let Some(window) = state.windows.get(&window_id) else {
        return;
    };
    let Some(player_start) = window.len().checked_sub(36) else {
        return;
    };
    if index < player_start {
        return;
    }
    let relative = index - player_start;
    let player_slot = if relative < 27 {
        9 + relative
    } else if relative < 36 {
        36 + relative - 27
    } else {
        return;
    };
    let item = window[index].clone();
    let player = state.windows.entry(0).or_insert_with(|| vec![None; 46]);
    if player.len() < 46 {
        player.resize(46, None);
    }
    player[player_slot] = item;
}

pub(crate) fn predict_normal_click(
    state: &mut InventoryState,
    window_id: i8,
    slot: i16,
    button: i8,
) -> Result<()> {
    if slot < 0 {
        return Ok(());
    }
    let index = slot as usize;
    let InventoryState {
        windows, cursor, ..
    } = state;
    let slots = windows.get_mut(&window_id).context("window has no slots")?;
    let target = slots.get_mut(index).context("slot is outside the window")?;
    match button {
        0 if cursor
            .as_ref()
            .zip(target.as_ref())
            .is_some_and(|(held, item)| held.item_id == item.item_id && held.nbt == item.nbt) =>
        {
            let held = cursor.as_mut().expect("matched above");
            let item = target.as_mut().expect("matched above");
            let capacity =
                crate::versions::java_1_16_1::registry::item_stack_size(item.item_id) - item.count;
            let transferred = capacity.max(0).min(held.count);
            item.count += transferred;
            held.count -= transferred;
            if held.count == 0 {
                *cursor = None;
            }
        }
        0 => std::mem::swap(target, cursor),
        1 if cursor.is_none() && target.is_some() => {
            let mut item = target.take().expect("checked above");
            let taken = (item.count + 1) / 2;
            let mut held = item.clone();
            held.count = taken;
            item.count -= taken;
            if item.count > 0 {
                *target = Some(item);
            }
            *cursor = Some(held);
        }
        1 if cursor.is_some() && target.is_none() => {
            let held = cursor.as_mut().expect("checked above");
            let mut placed = held.clone();
            placed.count = 1;
            held.count -= 1;
            *target = Some(placed);
            if held.count == 0 {
                *cursor = None;
            }
        }
        1 if cursor
            .as_ref()
            .zip(target.as_ref())
            .is_some_and(|(held, item)| {
                held.item_id == item.item_id && held.nbt == item.nbt && item.count < 64
            }) =>
        {
            let held = cursor.as_mut().expect("matched above");
            let item = target.as_mut().expect("matched above");
            item.count += 1;
            held.count -= 1;
            if held.count == 0 {
                *cursor = None;
            }
        }
        1 => std::mem::swap(target, cursor),
        _ => bail!("normal click button must be 0 or 1"),
    }
    Ok(())
}

pub(crate) fn rollback_click(state: &mut InventoryState, pending: &PendingClick) {
    state.cursor = pending.cursor_before.clone();
    if pending.slot >= 0 {
        if let Some(slot) = state
            .windows
            .get_mut(&pending.window_id)
            .and_then(|slots| slots.get_mut(pending.slot as usize))
        {
            *slot = pending.slot_before.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_16_1::protocol::put_varint;
    use byteorder::WriteBytesExt;

    #[test]
    fn slot_preserves_raw_nbt() {
        let mut bytes = vec![1];
        put_varint(&mut bytes, 5);
        bytes.push(3);
        // Root compound with an empty name and immediate end tag.
        bytes.extend([10, 0, 0, 0]);
        let mut input = bytes.as_slice();
        let item = read_slot(&mut input).unwrap().unwrap();
        assert_eq!(item.item_id, 5);
        assert_eq!(item.count, 3);
        assert_eq!(item.nbt, Some(vec![10, 0, 0, 0]));
        assert!(input.is_empty());
    }

    #[test]
    fn window_items_and_cursor_updates_are_independent() {
        let mut payload = vec![0];
        payload.write_i16::<BigEndian>(2).unwrap();
        payload.push(0);
        payload.push(1);
        put_varint(&mut payload, 17);
        payload.extend([4, 0]);
        let (window, slots) = parse_window_items(&payload).unwrap();
        let mut state = InventoryState::default();
        state.windows.insert(window, slots);
        apply_slot(
            &mut state,
            &SlotUpdate {
                window_id: -1,
                slot: -1,
                item: Some(ItemStack {
                    item_id: 1,
                    count: 1,
                    nbt: None,
                }),
            },
        )
        .unwrap();
        assert_eq!(state.player_slots()[1].as_ref().unwrap().count, 4);
        assert_eq!(state.cursor.as_ref().unwrap().item_id, 1);
    }

    #[test]
    fn merchant_offers_preserve_costs_and_server_flags() {
        use byteorder::WriteBytesExt;
        let input = ItemStack {
            item_id: 1,
            count: 3,
            nbt: None,
        };
        let output = ItemStack {
            item_id: 2,
            count: 1,
            nbt: None,
        };
        let mut packet = Vec::new();
        put_varint(&mut packet, 4);
        packet.push(1);
        write_slot(&mut packet, Some(&input));
        write_slot(&mut packet, Some(&output));
        packet.push(0);
        packet.push(1);
        for value in [2, 7, 5, -1] {
            packet.write_i32::<BigEndian>(value).unwrap();
        }
        packet.write_f32::<BigEndian>(0.2).unwrap();
        packet.write_i32::<BigEndian>(3).unwrap();
        put_varint(&mut packet, 2);
        put_varint(&mut packet, 11);
        packet.extend([1, 0]);
        let parsed = parse_merchant_offers(&packet).unwrap();
        assert_eq!(parsed.window_id, 4);
        assert_eq!(parsed.offers[0].input, input);
        assert_eq!(parsed.offers[0].output, output);
        assert!(parsed.offers[0].disabled);
        assert_eq!(parsed.offers[0].special_price, -1);
        assert!(parsed.regular_villager);
        assert!(!parsed.can_restock);
    }
}
