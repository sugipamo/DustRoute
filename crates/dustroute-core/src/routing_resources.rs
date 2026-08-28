use std::collections::BTreeSet;

use crate::world::Pos;

#[must_use]
pub fn horizontal_neighbors(pos: Pos) -> BTreeSet<Pos> {
    [
        pos.offset(1, 0, 0),
        pos.offset(-1, 0, 0),
        pos.offset(0, 0, 1),
        pos.offset(0, 0, -1),
    ]
    .into_iter()
    .collect()
}

#[must_use]
pub fn electrical_keepout_for_wire(pos: Pos) -> BTreeSet<Pos> {
    let horizontal = horizontal_neighbors(pos);
    horizontal
        .iter()
        .flat_map(|neighbor| {
            [
                *neighbor,
                neighbor.offset(0, 1, 0),
                neighbor.offset(0, -1, 0),
            ]
        })
        .collect()
}

#[must_use]
pub fn branch_stair_clearances(branch: &[Pos]) -> BTreeSet<Pos> {
    branch
        .windows(2)
        .filter_map(|pair| {
            let [a, b] = pair else { return None };
            if a.y == b.y {
                None
            } else {
                let lower = if a.y < b.y { a } else { b };
                Some(lower.offset(0, 1, 0))
            }
        })
        .collect()
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RoutingResources {
    pub conductors: BTreeSet<Pos>,
    pub supports: BTreeSet<Pos>,
    pub electrical_keepout: BTreeSet<Pos>,
    pub stair_clearance: BTreeSet<Pos>,
    pub terminals: BTreeSet<Pos>,
}

impl RoutingResources {
    #[must_use]
    pub fn from_conductors(
        conductors: impl IntoIterator<Item = Pos>,
        stair_clearance: impl IntoIterator<Item = Pos>,
        terminals: impl IntoIterator<Item = Pos>,
    ) -> Self {
        let conductors: BTreeSet<_> = conductors.into_iter().collect();
        let supports = conductors.iter().map(|pos| pos.offset(0, -1, 0)).collect();
        let electrical_keepout = conductors
            .iter()
            .flat_map(|pos| electrical_keepout_for_wire(*pos))
            .collect();
        Self {
            conductors,
            supports,
            electrical_keepout,
            stair_clearance: stair_clearance.into_iter().collect(),
            terminals: terminals.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn merged(mut self, other: &Self) -> Self {
        self.conductors.extend(&other.conductors);
        self.supports.extend(&other.supports);
        self.electrical_keepout.extend(&other.electrical_keepout);
        self.stair_clearance.extend(&other.stair_clearance);
        self.terminals.extend(&other.terminals);
        self
    }

    #[must_use]
    pub fn blocked_conductors(&self) -> BTreeSet<Pos> {
        self.conductors
            .union(&self.electrical_keepout)
            .copied()
            .chain(self.stair_clearance.iter().copied())
            .chain(self.terminals.iter().copied())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_roles_remain_separate() {
        let resources = RoutingResources::from_conductors(
            [Pos::new(3, 2, 4), Pos::new(4, 2, 4)],
            [Pos::new(3, 3, 4)],
            [Pos::new(2, 2, 4)],
        );
        assert!(resources.supports.contains(&Pos::new(3, 1, 4)));
        assert!(resources.electrical_keepout.contains(&Pos::new(3, 2, 3)));
        assert!(resources.blocked_conductors().contains(&Pos::new(2, 2, 4)));
    }
}
