//! Boss bars, scoreboard, teams, titles, tab list, and world-border state.

use crate::versions::java_1_16_1::protocol::{get_string, get_varint};
use anyhow::{Context, Result, bail};
use byteorder::{BigEndian, ReadBytesExt};
use std::{collections::HashMap, io::Cursor};

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `BossBar`.
pub struct BossBar {
    /// The `uuid` value.
    pub uuid: [u8; 16],
    /// The `title_json` value.
    pub title_json: String,
    /// The `health` value.
    pub health: f32,
    /// The `color` value.
    pub color: i32,
    /// The `divisions` value.
    pub divisions: i32,
    /// The `flags` value.
    pub flags: u8,
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `ScoreboardObjective`.
pub struct ScoreboardObjective {
    /// The `name` value.
    pub name: String,
    /// The `display_json` value.
    pub display_json: String,
    /// The `render_type` value.
    pub render_type: i32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `Team`.
pub struct Team {
    /// The `name` value.
    pub name: String,
    /// The `display_json` value.
    pub display_json: String,
    /// The `friendly_flags` value.
    pub friendly_flags: i8,
    /// The `name_tag_visibility` value.
    pub name_tag_visibility: String,
    /// The `collision_rule` value.
    pub collision_rule: String,
    /// The `color` value.
    pub color: i32,
    /// The `prefix_json` value.
    pub prefix_json: String,
    /// The `suffix_json` value.
    pub suffix_json: String,
    /// The `members` value.
    pub members: Vec<String>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `TitleState`.
pub struct TitleState {
    /// The `title_json` value.
    pub title_json: Option<String>,
    /// The `subtitle_json` value.
    pub subtitle_json: Option<String>,
    /// The `action_bar_json` value.
    pub action_bar_json: Option<String>,
    /// The `fade_in` value.
    pub fade_in: i32,
    /// The `stay` value.
    pub stay: i32,
    /// The `fade_out` value.
    pub fade_out: i32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// State and protocol data represented by `WorldBorder`.
pub struct WorldBorder {
    /// The `center_x` value.
    pub center_x: f64,
    /// The `center_z` value.
    pub center_z: f64,
    /// The `old_diameter` value.
    pub old_diameter: f64,
    /// The `diameter` value.
    pub diameter: f64,
    /// The `transition_millis` value.
    pub transition_millis: i64,
    /// The `portal_boundary` value.
    pub portal_boundary: i32,
    /// The `warning_time` value.
    pub warning_time: i32,
    /// The `warning_blocks` value.
    pub warning_blocks: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
/// State and protocol data represented by `UiState`.
pub struct UiState {
    /// The `boss_bars` value.
    pub boss_bars: HashMap<[u8; 16], BossBar>,
    /// The `objectives` value.
    pub objectives: HashMap<String, ScoreboardObjective>,
    /// The `display_objectives` value.
    pub display_objectives: HashMap<i8, String>,
    /// The `scores` value.
    pub scores: HashMap<(String, String), i32>,
    /// The `teams` value.
    pub teams: HashMap<String, Team>,
    /// The `title` value.
    pub title: TitleState,
    /// The `tab_header_json` value.
    pub tab_header_json: String,
    /// The `tab_footer_json` value.
    pub tab_footer_json: String,
    /// The `world_border` value.
    pub world_border: WorldBorder,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Possible values represented by `UiUpdateKind`.
pub enum UiUpdateKind {
    /// The `BossBar` variant.
    BossBar,
    /// The `Objective` variant.
    Objective,
    /// The `DisplayObjective` variant.
    DisplayObjective,
    /// The `Score` variant.
    Score,
    /// The `Team` variant.
    Team,
    /// The `Title` variant.
    Title,
    /// The `TabList` variant.
    TabList,
    /// The `WorldBorder` variant.
    WorldBorder,
}

impl UiState {
    pub(crate) fn apply_boss_bar(&mut self, payload: &[u8]) -> Result<()> {
        let mut rest = payload;
        let uuid = take_uuid(&mut rest)?;
        let action = get_varint(&mut rest)?;
        match action {
            0 => {
                let title_json = get_string(&mut rest)?;
                let health = take_f32(&mut rest)?;
                let color = get_varint(&mut rest)?;
                let divisions = get_varint(&mut rest)?;
                let flags = take_u8(&mut rest)?;
                self.boss_bars.insert(
                    uuid,
                    BossBar {
                        uuid,
                        title_json,
                        health,
                        color,
                        divisions,
                        flags,
                    },
                );
            }
            1 => {
                self.boss_bars.remove(&uuid);
            }
            2 => {
                if let Some(bar) = self.boss_bars.get_mut(&uuid) {
                    bar.health = take_f32(&mut rest)?;
                }
            }
            3 => {
                if let Some(bar) = self.boss_bars.get_mut(&uuid) {
                    bar.title_json = get_string(&mut rest)?;
                }
            }
            4 => {
                let color = get_varint(&mut rest)?;
                let divisions = get_varint(&mut rest)?;
                if let Some(bar) = self.boss_bars.get_mut(&uuid) {
                    bar.color = color;
                    bar.divisions = divisions;
                }
            }
            5 => {
                let flags = take_u8(&mut rest)?;
                if let Some(bar) = self.boss_bars.get_mut(&uuid) {
                    bar.flags = flags;
                }
            }
            _ => bail!("unknown boss bar action {action}"),
        }
        Ok(())
    }
    pub(crate) fn apply_objective(&mut self, payload: &[u8]) -> Result<()> {
        let mut r = payload;
        let name = get_string(&mut r)?;
        let action = take_i8(&mut r)?;
        match action {
            0 | 2 => {
                let display_json = get_string(&mut r)?;
                let render_type = get_varint(&mut r)?;
                self.objectives.insert(
                    name.clone(),
                    ScoreboardObjective {
                        name,
                        display_json,
                        render_type,
                    },
                );
            }
            1 => {
                self.objectives.remove(&name);
                self.display_objectives.retain(|_, v| v != &name);
                self.scores.retain(|(_, objective), _| objective != &name);
            }
            _ => bail!("unknown objective action {action}"),
        }
        Ok(())
    }
    pub(crate) fn apply_display(&mut self, p: &[u8]) -> Result<()> {
        let mut r = p;
        let slot = take_i8(&mut r)?;
        let name = get_string(&mut r)?;
        if name.is_empty() {
            self.display_objectives.remove(&slot);
        } else {
            self.display_objectives.insert(slot, name);
        }
        Ok(())
    }
    pub(crate) fn apply_score(&mut self, p: &[u8]) -> Result<()> {
        let mut r = p;
        let item = get_string(&mut r)?;
        let action = get_varint(&mut r)?;
        let objective = get_string(&mut r)?;
        if action == 1 {
            self.scores.remove(&(item, objective));
        } else {
            let value = get_varint(&mut r)?;
            self.scores.insert((item, objective), value);
        }
        Ok(())
    }
    pub(crate) fn apply_team(&mut self, p: &[u8]) -> Result<()> {
        let mut r = p;
        let key = get_string(&mut r)?;
        let mode = take_i8(&mut r)?;
        if mode == 1 {
            self.teams.remove(&key);
            return Ok(());
        }
        if mode == 0 || mode == 2 {
            let display_json = get_string(&mut r)?;
            let friendly_flags = take_i8(&mut r)?;
            let name_tag_visibility = get_string(&mut r)?;
            let collision_rule = get_string(&mut r)?;
            let color = get_varint(&mut r)?;
            let prefix_json = get_string(&mut r)?;
            let suffix_json = get_string(&mut r)?;
            let old = self.teams.remove(&key);
            let members = old.map_or_else(Vec::new, |t| t.members);
            self.teams.insert(
                key.clone(),
                Team {
                    name: key.clone(),
                    display_json,
                    friendly_flags,
                    name_tag_visibility,
                    collision_rule,
                    color,
                    prefix_json,
                    suffix_json,
                    members,
                },
            );
        }
        if mode == 0 || mode == 3 || mode == 4 {
            let count = get_varint(&mut r)?;
            if !(0..=4096).contains(&count) {
                bail!("invalid team member count {count}");
            }
            let mut players = Vec::with_capacity(count as usize);
            for _ in 0..count {
                players.push(get_string(&mut r)?);
            }
            if let Some(team) = self.teams.get_mut(&key) {
                if mode == 4 {
                    team.members.retain(|p| !players.contains(p));
                } else {
                    for player in players {
                        if !team.members.contains(&player) {
                            team.members.push(player);
                        }
                    }
                }
            }
        }
        if !(0..=4).contains(&mode) {
            bail!("unknown team mode {mode}");
        }
        Ok(())
    }
    pub(crate) fn apply_title(&mut self, p: &[u8]) -> Result<()> {
        let mut r = p;
        match get_varint(&mut r)? {
            0 => self.title.title_json = Some(get_string(&mut r)?),
            1 => self.title.subtitle_json = Some(get_string(&mut r)?),
            2 => self.title.action_bar_json = Some(get_string(&mut r)?),
            3 => {
                let mut c = Cursor::new(r);
                self.title.fade_in = c.read_i32::<BigEndian>()?;
                self.title.stay = c.read_i32::<BigEndian>()?;
                self.title.fade_out = c.read_i32::<BigEndian>()?;
            }
            4 => self.title = TitleState::default(),
            5 => {
                self.title.title_json = None;
                self.title.subtitle_json = None;
                self.title.action_bar_json = None;
            }
            a => bail!("unknown title action {a}"),
        }
        Ok(())
    }
    pub(crate) fn apply_tab(&mut self, p: &[u8]) -> Result<()> {
        let mut r = p;
        self.tab_header_json = get_string(&mut r)?;
        self.tab_footer_json = get_string(&mut r)?;
        Ok(())
    }
    pub(crate) fn apply_border(&mut self, p: &[u8]) -> Result<()> {
        let mut r = p;
        match get_varint(&mut r)? {
            0 => self.world_border.diameter = take_f64(&mut r)?,
            1 => {
                self.world_border.old_diameter = take_f64(&mut r)?;
                self.world_border.diameter = take_f64(&mut r)?;
                self.world_border.transition_millis = get_varlong(&mut r)?;
            }
            2 => {
                self.world_border.center_x = take_f64(&mut r)?;
                self.world_border.center_z = take_f64(&mut r)?;
            }
            3 => {
                self.world_border.center_x = take_f64(&mut r)?;
                self.world_border.center_z = take_f64(&mut r)?;
                self.world_border.old_diameter = take_f64(&mut r)?;
                self.world_border.diameter = take_f64(&mut r)?;
                self.world_border.transition_millis = get_varlong(&mut r)?;
                self.world_border.portal_boundary = get_varint(&mut r)?;
                self.world_border.warning_time = get_varint(&mut r)?;
                self.world_border.warning_blocks = get_varint(&mut r)?;
            }
            4 => self.world_border.warning_time = get_varint(&mut r)?,
            5 => self.world_border.warning_blocks = get_varint(&mut r)?,
            a => bail!("unknown world border action {a}"),
        }
        Ok(())
    }
}
fn take_u8(r: &mut &[u8]) -> Result<u8> {
    let v = *r.first().context("truncated UI packet")?;
    *r = &r[1..];
    Ok(v)
}
fn take_i8(r: &mut &[u8]) -> Result<i8> {
    Ok(take_u8(r)? as i8)
}
fn take_uuid(r: &mut &[u8]) -> Result<[u8; 16]> {
    if r.len() < 16 {
        bail!("truncated UUID")
    };
    let mut v = [0; 16];
    v.copy_from_slice(&r[..16]);
    *r = &r[16..];
    Ok(v)
}
fn take_f32(r: &mut &[u8]) -> Result<f32> {
    let mut c = Cursor::new(*r);
    let v = c.read_f32::<BigEndian>()?;
    *r = &r[4..];
    Ok(v)
}
fn take_f64(r: &mut &[u8]) -> Result<f64> {
    let mut c = Cursor::new(*r);
    let v = c.read_f64::<BigEndian>()?;
    *r = &r[8..];
    Ok(v)
}
fn get_varlong(r: &mut &[u8]) -> Result<i64> {
    let mut value = 0u64;
    for i in 0..10 {
        let b = take_u8(r)?;
        value |= u64::from(b & 0x7f) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(value as i64);
        }
    }
    bail!("varlong too long")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_16_1::protocol::{put_string, put_varint};

    #[test]
    fn scoreboard_create_score_and_remove_are_stateful() {
        let mut state = UiState::default();
        let mut objective = Vec::new();
        put_string(&mut objective, "kills");
        objective.push(0);
        put_string(&mut objective, r#"{"text":"Kills"}"#);
        put_varint(&mut objective, 0);
        state.apply_objective(&objective).unwrap();
        let mut score = Vec::new();
        put_string(&mut score, "Alice");
        put_varint(&mut score, 0);
        put_string(&mut score, "kills");
        put_varint(&mut score, 3);
        state.apply_score(&score).unwrap();
        assert_eq!(
            state.scores.get(&("Alice".into(), "kills".into())),
            Some(&3)
        );
        let mut remove = Vec::new();
        put_string(&mut remove, "kills");
        remove.push(1);
        state.apply_objective(&remove).unwrap();
        assert!(state.objectives.is_empty());
        assert!(state.scores.is_empty());
    }

    #[test]
    fn boss_bar_incremental_updates_preserve_other_fields() {
        let mut state = UiState::default();
        let mut add = vec![7; 16];
        put_varint(&mut add, 0);
        put_string(&mut add, r#"{"text":"Boss"}"#);
        add.extend(0.5f32.to_be_bytes());
        put_varint(&mut add, 2);
        put_varint(&mut add, 4);
        add.push(1);
        state.apply_boss_bar(&add).unwrap();
        let mut health = vec![7; 16];
        put_varint(&mut health, 2);
        health.extend(0.25f32.to_be_bytes());
        state.apply_boss_bar(&health).unwrap();
        let bar = &state.boss_bars[&[7; 16]];
        assert_eq!(bar.health, 0.25);
        assert_eq!(bar.color, 2);
    }
}
