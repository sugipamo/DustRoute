//! Ordered movement groups with native slime/honey attachment rules.
use super::*;

pub(super) struct MovementPlan {
    pub moved: Vec<Pos>,
    pub destroyed: Vec<Pos>,
}

/// Iteration of the native position map used for vacated-source shape updates.
/// At most twelve entries; retain insertion order within each Java HashMap bucket.
pub(super) fn position_map_order(inserted: &[Pos]) -> ApplyResult<Vec<Pos>> {
    let hash = |p: Pos| {
        let h = p[1]
            .wrapping_add(p[2].wrapping_mul(31))
            .wrapping_mul(31)
            .wrapping_add(p[0]) as u32;
        h ^ (h >> 16)
    };
    let mut capacity = 16u32;
    let mut order = Vec::new();
    for p in inserted {
        order.push(*p);
        if order
            .iter()
            .filter(|q| hash(**q) & (capacity - 1) == hash(*p) & (capacity - 1))
            .count()
            >= 9
        {
            if capacity >= 64 {
                return Err(ReconstructionIssue::Limit);
            }
            capacity *= 2;
        }
        order.sort_by_key(|p| hash(*p) & (capacity - 1));
    }
    Ok(order)
}

struct Planner<'a> {
    runtime: &'a mut Reconstruction,
    world: &'a World,
    piston: Pos,
    movement: Direction,
    plan: MovementPlan,
    calls: usize,
}

pub(super) fn adhesive(kind: Kind) -> bool {
    matches!(kind, Kind::Slime | Kind::Honey)
}
fn attached(a: Kind, b: Kind) -> bool {
    !matches!(
        (a, b),
        (Kind::Slime, Kind::Honey) | (Kind::Honey, Kind::Slime)
    ) && (adhesive(a) || adhesive(b))
}

impl MovementPlan {
    pub fn calculate(
        runtime: &mut Reconstruction,
        world: &World,
        piston: Pos,
        start: Pos,
        movement: Direction,
        extending: bool,
    ) -> ApplyResult<Option<Self>> {
        let mut planner = Planner {
            runtime,
            world,
            piston,
            movement,
            plan: Self {
                moved: vec![],
                destroyed: vec![],
            },
            calls: 0,
        };
        let (s, k) = planner.runtime.kind(world, start)?;
        if k == Kind::Air {
            return Ok(Some(planner.plan));
        }
        if !Reconstruction::movable(&s, k) {
            if rules::destroyed_by_piston(k) && extending {
                planner.plan.destroyed.push(start);
                return Ok(Some(planner.plan));
            }
            return Ok(None);
        }
        if !planner.line(start)? {
            return Ok(None);
        }
        let mut i = 0;
        while i < planner.plan.moved.len() {
            if !planner.branches(planner.plan.moved[i])? {
                return Ok(None);
            }
            i += 1;
        }
        Ok(Some(planner.plan))
    }
}

impl Planner<'_> {
    fn line(&mut self, start: Pos) -> ApplyResult<bool> {
        self.calls += 1;
        if self.calls > 512 {
            return Err(ReconstructionIssue::Limit);
        }
        let (s, mut kind) = self.runtime.kind(self.world, start)?;
        if kind == Kind::Air
            || start == self.piston
            || self.plan.moved.contains(&start)
            || !Reconstruction::movable(&s, kind)
        {
            return Ok(true);
        }
        let mut length = 1;
        if self.plan.moved.len() + length > 12 {
            return Ok(false);
        }
        while adhesive(kind) {
            let p = self.movement.opposite().offset(start, length as i32);
            let (s, behind) = self.runtime.kind(self.world, p)?;
            if p == self.piston
                || behind == Kind::Air
                || !attached(kind, behind)
                || !Reconstruction::movable(&s, behind)
            {
                break;
            }
            length += 1;
            if self.plan.moved.len() + length > 12 {
                return Ok(false);
            }
            kind = behind;
        }
        for n in (0..length).rev() {
            self.plan
                .moved
                .push(self.movement.opposite().offset(start, n as i32));
        }
        for (added, distance) in (length..).zip(1..=13) {
            let p = self.movement.offset(start, distance);
            if let Some(index) = self.plan.moved.iter().position(|v| *v == p) {
                // Move the newly discovered tail before the line it collides with.
                self.plan.moved[index..].rotate_right(added);
                for i in 0..=index + added {
                    if !self.branches(self.plan.moved[i])? {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
            let (s, k) = self.runtime.kind(self.world, p)?;
            if k == Kind::Air {
                return Ok(true);
            }
            if p == self.piston {
                return Ok(false);
            }
            if rules::destroyed_by_piston(k) {
                if !self.plan.destroyed.contains(&p) {
                    self.plan.destroyed.push(p);
                }
                return Ok(true);
            }
            if !Reconstruction::movable(&s, k) || self.plan.moved.len() == 12 {
                return Ok(false);
            }
            self.plan.moved.push(p);
        }
        Err(ReconstructionIssue::Limit)
    }
    fn branches(&mut self, p: Pos) -> ApplyResult<bool> {
        let (_, kind) = self.runtime.kind(self.world, p)?;
        if !adhesive(kind) {
            return Ok(true);
        }
        for side in Direction::ALL {
            if side == self.movement || side == self.movement.opposite() {
                continue;
            }
            let q = side.offset(p, 1);
            let (_, neighbor) = self.runtime.kind(self.world, q)?;
            if attached(kind, neighbor) && !self.line(q)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
