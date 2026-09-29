//! Explicit native block rules admitted by the first client-piston implementation.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Air,
    Solid,
    Glass,
    Observer,
    PowerBlock,
    Wire,
    Gate { repeater: bool },
    Slime,
    Honey,
    Stairs,
    Lever,
    Piston { sticky: bool },
    Head,
    Moving,
    Blocked,
}
const RULES: &[(&str, Kind)] = &[
    ("air", Kind::Air),
    ("cave_air", Kind::Air),
    ("void_air", Kind::Air),
    ("stone", Kind::Solid),
    ("cobblestone", Kind::Solid),
    ("quartz_block", Kind::Solid),
    ("smooth_quartz", Kind::Solid),
    ("glass", Kind::Glass),
    ("slime_block", Kind::Slime),
    ("honey_block", Kind::Honey),
    ("redstone_block", Kind::PowerBlock),
    ("observer", Kind::Observer),
    ("redstone_wire", Kind::Wire),
    ("repeater", Kind::Gate { repeater: true }),
    ("comparator", Kind::Gate { repeater: false }),
    ("redstone_lamp", Kind::Solid),
    ("stone_stairs", Kind::Stairs),
    ("cobblestone_stairs", Kind::Stairs),
    ("quartz_stairs", Kind::Stairs),
    ("smooth_quartz_stairs", Kind::Stairs),
    ("lever", Kind::Lever),
    ("stone_button", Kind::Lever),
    ("polished_blackstone_button", Kind::Lever),
    ("oak_button", Kind::Lever),
    ("spruce_button", Kind::Lever),
    ("birch_button", Kind::Lever),
    ("jungle_button", Kind::Lever),
    ("acacia_button", Kind::Lever),
    ("dark_oak_button", Kind::Lever),
    ("mangrove_button", Kind::Lever),
    ("cherry_button", Kind::Lever),
    ("pale_oak_button", Kind::Lever),
    ("bamboo_button", Kind::Lever),
    ("crimson_button", Kind::Lever),
    ("warped_button", Kind::Lever),
    ("white_wool", Kind::Solid),
    ("orange_wool", Kind::Solid),
    ("magenta_wool", Kind::Solid),
    ("light_blue_wool", Kind::Solid),
    ("yellow_wool", Kind::Solid),
    ("lime_wool", Kind::Solid),
    ("pink_wool", Kind::Solid),
    ("gray_wool", Kind::Solid),
    ("light_gray_wool", Kind::Solid),
    ("cyan_wool", Kind::Solid),
    ("purple_wool", Kind::Solid),
    ("blue_wool", Kind::Solid),
    ("brown_wool", Kind::Solid),
    ("green_wool", Kind::Solid),
    ("red_wool", Kind::Solid),
    ("black_wool", Kind::Solid),
    ("piston", Kind::Piston { sticky: false }),
    ("sticky_piston", Kind::Piston { sticky: true }),
    ("piston_head", Kind::Head),
    ("moving_piston", Kind::Moving),
    ("obsidian", Kind::Blocked),
    ("bedrock", Kind::Blocked),
];
pub(super) fn destroyed_by_piston(kind: Kind) -> bool {
    matches!(kind, Kind::Lever | Kind::Wire | Kind::Gate { .. })
}
pub(super) fn solid_block(kind: Kind) -> bool {
    matches!(kind, Kind::Solid | Kind::Slime | Kind::Blocked)
}
pub(super) fn classify(s: &NativeBlockState) -> Option<Kind> {
    if s.properties
        .get("waterlogged")
        .is_some_and(|v| v != "false")
    {
        return None;
    }
    let name = s.name.strip_prefix("minecraft:")?;
    RULES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, kind)| *kind)
}
pub(super) fn state(name: &str, properties: &[(&str, &str)]) -> NativeBlockState {
    NativeBlockState {
        name: format!("minecraft:{name}"),
        properties: properties
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}
pub(super) fn facing(s: &NativeBlockState) -> ApplyResult<Direction> {
    s.properties
        .get("facing")
        .and_then(|n| Direction::from_name(n))
        .ok_or(ReconstructionIssue::InvalidAction)
}
impl Reconstruction {
    pub(super) fn head_supported(
        &mut self,
        world: &World,
        p: Pos,
        head: &NativeBlockState,
    ) -> ApplyResult<bool> {
        let dir = facing(head)?;
        let (base, kind) = self.kind(world, dir.opposite().offset(p, 1))?;
        if !matches!(kind, Kind::Piston { .. } | Kind::Moving) || facing(&base)? != dir {
            return Ok(false);
        }
        Ok(match kind {
            Kind::Moving => true,
            Kind::Piston { sticky } => {
                base.properties.get("extended").is_some_and(|v| v == "true")
                    && head
                        .properties
                        .get("type")
                        .is_some_and(|v| v == if sticky { "sticky" } else { "normal" })
            }
            _ => false,
        })
    }
    pub(super) fn stair_shape(
        &mut self,
        world: &World,
        p: Pos,
        mut s: NativeBlockState,
    ) -> ApplyResult<NativeBlockState> {
        let dir = facing(&s)?;
        let half = s
            .properties
            .get("half")
            .ok_or(ReconstructionIssue::InvalidAction)?
            .clone();
        let mut shape = "straight";
        for (side, outer) in [(dir, true), (dir.opposite(), false)] {
            let (neighbor, kind) = self.kind(world, side.offset(p, 1))?;
            if kind != Kind::Stairs || neighbor.properties.get("half") != Some(&half) {
                continue;
            }
            let other = facing(&neighbor)?;
            if other == dir || other == dir.opposite() {
                continue;
            }
            let adjacent = if outer { other.opposite() } else { other };
            let (guard, guard_kind) = self.kind(world, adjacent.offset(p, 1))?;
            if guard_kind == Kind::Stairs
                && guard.properties.get("half") == Some(&half)
                && facing(&guard)? == dir
            {
                continue;
            }
            shape = match (outer, other == dir.left()) {
                (true, true) => "outer_left",
                (true, false) => "outer_right",
                (false, true) => "inner_left",
                (false, false) => "inner_right",
            };
            break;
        }
        s.properties.insert("shape".into(), shape.into());
        Ok(s)
    }
    pub(super) fn lever_support(p: Pos, lever: &NativeBlockState) -> ApplyResult<(Pos, Direction)> {
        let attachment = match lever.properties.get("face").map(String::as_str) {
            Some("floor") => Direction::Down,
            Some("ceiling") => Direction::Up,
            Some("wall") => facing(lever)?.opposite(),
            _ => return Err(ReconstructionIssue::InvalidAction),
        };
        Ok((attachment.offset(p, 1), attachment.opposite()))
    }
    pub(super) fn lever_supported(
        &mut self,
        world: &World,
        p: Pos,
        lever: &NativeBlockState,
    ) -> ApplyResult<bool> {
        let (support, face) = Self::lever_support(p, lever)?;
        self.full_face(world, support, face)
    }
    pub(super) fn full_face(
        &mut self,
        world: &World,
        support: Pos,
        face: Direction,
    ) -> ApplyResult<bool> {
        let (s, kind) = self.kind(world, support)?;
        if kind == Kind::Moving {
            return self.moving_full_face(support, face);
        }
        Self::state_full_face(&s, kind, support, face)
    }
    fn state_full_face(
        s: &NativeBlockState,
        kind: Kind,
        support: Pos,
        face: Direction,
    ) -> ApplyResult<bool> {
        Ok(match kind {
            Kind::Solid
            | Kind::Blocked
            | Kind::Slime
            | Kind::Glass
            | Kind::Observer
            | Kind::PowerBlock => true,
            Kind::Piston { .. } => {
                s.properties.get("extended").is_some_and(|v| v == "false")
                    || face == facing(s)?.opposite()
            }
            Kind::Stairs => {
                let dir = facing(s)?;
                let base = if s.properties.get("half").is_some_and(|v| v == "top") {
                    Direction::Up
                } else {
                    Direction::Down
                };
                let shape = s.properties.get("shape").map(String::as_str).unwrap_or("");
                face == base
                    || match shape {
                        "straight" => face == dir,
                        "inner_left" => face == dir || face == dir.left(),
                        "inner_right" => face == dir || face == dir.left().opposite(),
                        "outer_left" | "outer_right" => false,
                        _ => return Err(ReconstructionIssue::InvalidAction),
                    }
            }
            Kind::Head => {
                return Err(ReconstructionIssue::UnsupportedBlock {
                    position: support,
                    name: s.name.clone(),
                });
            }
            _ => false,
        })
    }
    fn moving_full_face(&self, p: Pos, face: Direction) -> ApplyResult<bool> {
        let Some(motion) = self.moving.get(&p) else {
            // The native setBlockState callbacks run before addBlockEntity.
            // During that local transaction the extension has empty collision.
            // Unknown received carriers are rejected by read() before this query.
            return Ok(false);
        };
        if motion.role == CarrierRole::Body {
            // Retraction retains the stationary extended base collision shape;
            // the head fills its missing quarter when progress reaches one.
            let carried_facing = facing(&motion.carried)?;
            if carried_facing != motion.direction {
                return Err(ReconstructionIssue::InvalidAction);
            }
            return Ok(face == carried_facing.opposite() || motion.progress == MotionProgress::Full);
        }
        if motion.role == CarrierRole::Head {
            return Ok(motion.progress == MotionProgress::Full && face == motion.direction);
        }
        let kind =
            classify(&motion.carried).ok_or_else(|| ReconstructionIssue::UnsupportedBlock {
                position: p,
                name: motion.carried.name.clone(),
            })?;
        if motion.progress == MotionProgress::Full {
            return Self::state_full_face(&motion.carried, kind, p, face);
        }
        if motion.progress == MotionProgress::Start {
            return Ok(false);
        }
        match kind {
            Kind::Solid
            | Kind::Glass
            | Kind::Observer
            | Kind::PowerBlock
            | Kind::Slime
            | Kind::Piston { .. } => {
                // A half-translated full cube covers only the face toward the
                // cell it came from. Other faces have only partial coverage.
                Ok(face
                    == if motion.extending {
                        motion.direction.opposite()
                    } else {
                        motion.direction
                    })
            }
            Kind::Honey => Ok(false),
            _ => Err(ReconstructionIssue::UnsupportedBlock {
                position: p,
                name: motion.carried.name.clone(),
            }),
        }
    }
}
