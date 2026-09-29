//! Structured chat messages and the server-maintained player list.

use crate::versions::java_1_16_1::protocol::{get_string, get_varint};
use anyhow::{Context, Result, bail};
use std::collections::HashMap;

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `ChatMessage`.
pub struct ChatMessage {
    /// Raw JSON chat component; rendering and natural-language conversion are caller concerns.
    pub json: String,
    /// The `position` value.
    pub position: i8,
    /// The `sender` value.
    pub sender: [u8; 16],
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `PlayerProperty`.
pub struct PlayerProperty {
    /// The `name` value.
    pub name: String,
    /// The `value` value.
    pub value: String,
    /// The `signature` value.
    pub signature: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `PlayerListEntry`.
pub struct PlayerListEntry {
    /// The `uuid` value.
    pub uuid: [u8; 16],
    /// The `name` value.
    pub name: String,
    /// The `properties` value.
    pub properties: Vec<PlayerProperty>,
    /// The `game_mode` value.
    pub game_mode: i32,
    /// The `latency` value.
    pub latency: i32,
    /// The `display_name_json` value.
    pub display_name_json: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `PlayerList`.
pub struct PlayerList {
    /// The `entries` value.
    pub entries: HashMap<[u8; 16], PlayerListEntry>,
}

fn read_uuid(rest: &mut &[u8]) -> Result<[u8; 16]> {
    if rest.len() < 16 {
        bail!("truncated player UUID");
    }
    let mut uuid = [0; 16];
    uuid.copy_from_slice(&rest[..16]);
    *rest = &rest[16..];
    Ok(uuid)
}

fn optional_string(rest: &mut &[u8]) -> Result<Option<String>> {
    let present = *rest.first().context("missing optional string presence")? != 0;
    *rest = &rest[1..];
    Ok(if present {
        Some(get_string(rest)?)
    } else {
        None
    })
}

pub(crate) fn parse_chat(payload: &[u8]) -> Result<ChatMessage> {
    let mut rest = payload;
    let json = get_string(&mut rest)?;
    let position = *rest.first().context("missing chat position")? as i8;
    rest = &rest[1..];
    Ok(ChatMessage {
        json,
        position,
        sender: read_uuid(&mut rest)?,
    })
}

pub(crate) fn apply_player_info(
    list: &mut PlayerList,
    payload: &[u8],
) -> Result<(i32, Vec<[u8; 16]>)> {
    let mut rest = payload;
    let action = get_varint(&mut rest)?;
    let count = get_varint(&mut rest)?;
    if !(0..=4096).contains(&count) {
        bail!("invalid player info count {count}");
    }
    let mut changed = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let uuid = read_uuid(&mut rest)?;
        changed.push(uuid);
        match action {
            0 => {
                let name = get_string(&mut rest)?;
                let property_count = get_varint(&mut rest)?;
                if !(0..=1024).contains(&property_count) {
                    bail!("invalid player property count {property_count}");
                }
                let mut properties = Vec::with_capacity(property_count as usize);
                for _ in 0..property_count {
                    properties.push(PlayerProperty {
                        name: get_string(&mut rest)?,
                        value: get_string(&mut rest)?,
                        signature: optional_string(&mut rest)?,
                    });
                }
                let game_mode = get_varint(&mut rest)?;
                let latency = get_varint(&mut rest)?;
                let display_name_json = optional_string(&mut rest)?;
                list.entries.insert(
                    uuid,
                    PlayerListEntry {
                        uuid,
                        name,
                        properties,
                        game_mode,
                        latency,
                        display_name_json,
                    },
                );
            }
            1 => {
                let game_mode = get_varint(&mut rest)?;
                if let Some(entry) = list.entries.get_mut(&uuid) {
                    entry.game_mode = game_mode;
                }
            }
            2 => {
                let latency = get_varint(&mut rest)?;
                if let Some(entry) = list.entries.get_mut(&uuid) {
                    entry.latency = latency;
                }
            }
            3 => {
                let display_name_json = optional_string(&mut rest)?;
                if let Some(entry) = list.entries.get_mut(&uuid) {
                    entry.display_name_json = display_name_json;
                }
            }
            4 => {
                list.entries.remove(&uuid);
            }
            _ => bail!("unknown player info action {action}"),
        }
    }
    Ok((action, changed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_16_1::protocol::put_string;

    #[test]
    fn chat_keeps_raw_json_and_sender() {
        let mut payload = Vec::new();
        put_string(&mut payload, r#"{\"text\":\"hello\"}"#);
        payload.push(0);
        payload.extend([7; 16]);
        let message = parse_chat(&payload).unwrap();
        assert_eq!(message.json, r#"{\"text\":\"hello\"}"#);
        assert_eq!(message.sender, [7; 16]);
    }
}
