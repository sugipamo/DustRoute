//! Structured map-item updates and retained 128×128 color buffers.

use crate::versions::java_1_16_1::protocol::{get_string, get_varint};
use anyhow::{Context, Result, bail};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `MapIcon`.
pub struct MapIcon {
    /// The `kind` value.
    pub kind: i32,
    /// The `x` value.
    pub x: i8,
    /// The `z` value.
    pub z: i8,
    /// The `direction` value.
    pub direction: u8,
    /// The `display_name` value.
    pub display_name: Option<crate::text_component::ProtocolText>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `MapData`.
pub struct MapData {
    /// The `id` value.
    pub id: i32,
    /// The `scale` value.
    pub scale: i8,
    /// The `tracking_position` value.
    pub tracking_position: bool,
    /// The `locked` value.
    pub locked: bool,
    /// The `icons` value.
    pub icons: Vec<MapIcon>,
    /// 128×128 vanilla map colors, indexed as `y * 128 + x`.
    pub colors: Arc<[u8; 16_384]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `MapUpdate`.
pub struct MapUpdate {
    /// The `id` value.
    pub id: i32,
    /// The `scale` value.
    pub scale: i8,
    /// The `tracking_position` value.
    pub tracking_position: bool,
    /// The `locked` value.
    pub locked: bool,
    /// The `icons` value.
    pub icons: Vec<MapIcon>,
    /// The `rectangle` value.
    pub rectangle: Option<MapRectangle>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `MapRectangle`.
pub struct MapRectangle {
    /// The `x` value.
    pub x: u8,
    /// The `y` value.
    pub y: u8,
    /// The `width` value.
    pub width: u8,
    /// The `height` value.
    pub height: u8,
    /// The `colors` value.
    pub colors: Vec<u8>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `MapStore`.
pub struct MapStore {
    /// The `maps` value.
    pub maps: HashMap<i32, MapData>,
}

impl MapStore {
    pub(crate) fn apply(&mut self, update: &MapUpdate, max_maps: usize) -> Result<()> {
        if !self.maps.contains_key(&update.id) && self.maps.len() >= max_maps {
            bail!("map cache limit of {max_maps} exceeded");
        }
        let map = self.maps.entry(update.id).or_insert_with(|| MapData {
            id: update.id,
            scale: update.scale,
            tracking_position: update.tracking_position,
            locked: update.locked,
            icons: Vec::new(),
            colors: Arc::new([0; 16_384]),
        });
        map.scale = update.scale;
        map.tracking_position = update.tracking_position;
        map.locked = update.locked;
        map.icons = update.icons.clone();
        if let Some(rectangle) = &update.rectangle {
            let colors = Arc::make_mut(&mut map.colors);
            for row in 0..usize::from(rectangle.height) {
                let source = row * usize::from(rectangle.width);
                let destination = (usize::from(rectangle.y) + row) * 128 + usize::from(rectangle.x);
                colors[destination..destination + usize::from(rectangle.width)].copy_from_slice(
                    &rectangle.colors[source..source + usize::from(rectangle.width)],
                );
            }
        }
        Ok(())
    }
}

pub(crate) fn parse_map_update(payload: &[u8]) -> Result<MapUpdate> {
    let mut rest = payload;
    let id = get_varint(&mut rest)?;
    let scale = take_i8(&mut rest)?;
    let tracking_position = take_bool(&mut rest)?;
    let locked = take_bool(&mut rest)?;
    let icon_count = get_varint(&mut rest)?;
    if !(0..=4096).contains(&icon_count) {
        bail!("invalid map icon count {icon_count}");
    }
    let mut icons = Vec::with_capacity(icon_count as usize);
    for _ in 0..icon_count {
        let kind = get_varint(&mut rest)?;
        let x = take_i8(&mut rest)?;
        let z = take_i8(&mut rest)?;
        let direction = take_u8(&mut rest)?;
        let display_name = if take_bool(&mut rest)? {
            Some(crate::text_component::ProtocolText::received(get_string(
                &mut rest,
            )?))
        } else {
            None
        };
        icons.push(MapIcon {
            kind,
            x,
            z,
            direction,
            display_name,
        });
    }
    let width = take_u8(&mut rest)?;
    let rectangle = if width == 0 {
        None
    } else {
        let height = take_u8(&mut rest)?;
        let x = take_u8(&mut rest)?;
        let y = take_u8(&mut rest)?;
        if usize::from(x) + usize::from(width) > 128 || usize::from(y) + usize::from(height) > 128 {
            bail!("map rectangle is outside 128x128 bounds");
        }
        let length = get_varint(&mut rest)?;
        let expected = usize::from(width) * usize::from(height);
        if length < 0 || length as usize != expected || rest.len() < expected {
            bail!("invalid map color data length {length}, expected {expected}");
        }
        let colors = rest[..expected].to_vec();
        Some(MapRectangle {
            x,
            y,
            width,
            height,
            colors,
        })
    };
    Ok(MapUpdate {
        id,
        scale,
        tracking_position,
        locked,
        icons,
        rectangle,
    })
}

fn take_u8(rest: &mut &[u8]) -> Result<u8> {
    let value = *rest.first().context("truncated map packet")?;
    *rest = &rest[1..];
    Ok(value)
}

fn take_i8(rest: &mut &[u8]) -> Result<i8> {
    Ok(take_u8(rest)? as i8)
}
fn take_bool(rest: &mut &[u8]) -> Result<bool> {
    Ok(take_u8(rest)? != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_16_1::protocol::put_varint;

    #[test]
    fn partial_map_updates_are_applied_without_copying_unchanged_snapshots() {
        let mut packet = Vec::new();
        put_varint(&mut packet, 7);
        packet.extend([2, 1, 0]);
        put_varint(&mut packet, 0);
        packet.extend([2, 1, 3, 4]);
        put_varint(&mut packet, 2);
        packet.extend([9, 10]);
        let update = parse_map_update(&packet).unwrap();
        let mut store = MapStore::default();
        store.apply(&update, 128).unwrap();
        let map = &store.maps[&7];
        assert_eq!(map.colors[4 * 128 + 3], 9);
        assert_eq!(map.colors[4 * 128 + 4], 10);
    }
}
