//! Client wire geometry. Power, repeater locking and scheduled redstone stay server-owned.
use super::*;

impl Reconstruction {
    fn is_wire(&mut self, world: &World, p: Pos) -> ApplyResult<bool> {
        Ok(self.read(world, p)?.name == "minecraft:redstone_wire")
    }
    fn wire_connection(
        &mut self,
        world: &World,
        p: Pos,
        side: Direction,
    ) -> ApplyResult<&'static str> {
        let (_, above) = self.kind(world, Direction::Up.offset(p, 1))?;
        let neighbor = side.offset(p, 1);
        let (s, kind) = self.kind(world, neighbor)?;
        if !rules::solid_block(above)
            && self.full_face(world, neighbor, Direction::Up)?
            && self.is_wire(world, Direction::Up.offset(neighbor, 1))?
        {
            return Ok(if self.full_face(world, neighbor, side.opposite())? {
                "up"
            } else {
                "side"
            });
        }
        let connects = match kind {
            Kind::Wire | Kind::Lever | Kind::PowerBlock | Kind::Gate { repeater: false } => true,
            Kind::Gate { repeater: true } => {
                let f = rules::facing(&s)?;
                f == side || f == side.opposite()
            }
            Kind::Observer => rules::facing(&s)? == side,
            _ => false,
        };
        if connects
            || (!rules::solid_block(kind)
                && self.is_wire(world, Direction::Down.offset(neighbor, 1))?)
        {
            Ok("side")
        } else {
            Ok("none")
        }
    }
    fn wire_layout(
        &mut self,
        world: &World,
        p: Pos,
        mut s: NativeBlockState,
        keep_dot: bool,
    ) -> ApplyResult<NativeBlockState> {
        for side in Direction::HORIZONTAL {
            s.properties.insert(
                side.name().into(),
                self.wire_connection(world, p, side)?.into(),
            );
        }
        let connected =
            |side: Direction| s.properties.get(side.name()).is_some_and(|v| v != "none");
        let n = connected(Direction::North);
        let e = connected(Direction::East);
        let south = connected(Direction::South);
        let w = connected(Direction::West);
        if keep_dot && !n && !e && !south && !w {
            return Ok(s);
        }
        if !n && !south {
            if !e {
                s.properties.insert("east".into(), "side".into());
            }
            if !w {
                s.properties.insert("west".into(), "side".into());
            }
        }
        if !e && !w {
            if !n {
                s.properties.insert("north".into(), "side".into());
            }
            if !south {
                s.properties.insert("south".into(), "side".into());
            }
        }
        Ok(s)
    }
    pub(super) fn wire_update(
        &mut self,
        world: &World,
        p: Pos,
        mut before: NativeBlockState,
        from: Direction,
    ) -> ApplyResult<NativeBlockState> {
        if from == Direction::Down {
            return Ok(
                if self.full_face(world, from.offset(p, 1), Direction::Up)? {
                    before
                } else {
                    state("air", &[])
                },
            );
        }
        if from == Direction::Up {
            let keep_dot = Direction::HORIZONTAL
                .iter()
                .all(|d| before.properties.get(d.name()).is_some_and(|v| v == "none"));
            return self.wire_layout(world, p, before, keep_dot);
        }
        let connection = self.wire_connection(world, p, from)?;
        let old = before
            .properties
            .get(from.name())
            .ok_or(ReconstructionIssue::InvalidAction)?;
        let fully_connected = Direction::HORIZONTAL
            .iter()
            .all(|d| before.properties.get(d.name()).is_some_and(|v| v != "none"));
        if (old != "none") == (connection != "none") && !fully_connected {
            before
                .properties
                .insert(from.name().into(), connection.into());
            Ok(before)
        } else {
            self.wire_layout(world, p, before, false)
        }
    }
    pub(super) fn prepare(
        &mut self,
        world: &World,
        p: Pos,
        s: &NativeBlockState,
        origin: StateOrigin,
        depth: usize,
    ) -> ApplyResult<()> {
        if s.name != "minecraft:redstone_wire" {
            return Ok(());
        }
        if depth > 128 {
            return Err(ReconstructionIssue::Limit);
        }
        for side in Direction::HORIZONTAL {
            if s.properties.get(side.name()).is_none_or(|v| v == "none") {
                continue;
            }
            let neighbor = side.offset(p, 1);
            if self.is_wire(world, neighbor)? {
                continue;
            }
            for vertical in [Direction::Down, Direction::Up] {
                let target = vertical.offset(neighbor, 1);
                if self.is_wire(world, target)? {
                    self.update_neighbor(world, target, side.opposite(), origin, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
