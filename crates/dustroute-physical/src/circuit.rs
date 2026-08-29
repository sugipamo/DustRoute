use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{Block, Pos};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ComponentId(pub usize);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct NetId(pub usize);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct FragmentId(pub usize);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    Dust,
    WeakPower,
    StrongPower,
    DirectionalInput,
    DirectionalOutput,
    DirectSource,
    Control,
    Support,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalComponent {
    pub id: ComponentId,
    pub pos: Pos,
    pub block: Block,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PhysicalConnection {
    pub source: ComponentId,
    pub sink: ComponentId,
    pub kind: ConnectionKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalNet {
    pub id: NetId,
    pub components: BTreeSet<ComponentId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalFragment {
    pub id: FragmentId,
    pub net: NetId,
    pub components: BTreeSet<ComponentId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GapEvidence {
    Nearby {
        left: ComponentId,
        right: ComponentId,
        manhattan_distance: u32,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GapCandidate {
    pub left: FragmentId,
    pub right: FragmentId,
    pub evidence: Vec<GapEvidence>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalCircuit {
    pub components: Vec<PhysicalComponent>,
    pub connections: BTreeSet<PhysicalConnection>,
    pub nets: Vec<PhysicalNet>,
    pub fragments: Vec<PhysicalFragment>,
}

impl PhysicalCircuit {
    #[must_use]
    pub fn from_parts(
        mut components: Vec<PhysicalComponent>,
        connections: impl IntoIterator<Item = PhysicalConnection>,
    ) -> Self {
        components.sort_by_key(|component| (component.pos, component.id));
        let connections: BTreeSet<_> = connections.into_iter().collect();
        let mut union_find = UnionFind::new(components.len());
        let indices: BTreeMap<_, _> = components
            .iter()
            .enumerate()
            .map(|(index, component)| (component.id, index))
            .collect();
        for connection in &connections {
            if let (Some(source), Some(sink)) = (
                indices.get(&connection.source),
                indices.get(&connection.sink),
            ) {
                union_find.union(*source, *sink);
            }
        }
        let mut groups: BTreeMap<usize, BTreeSet<ComponentId>> = BTreeMap::new();
        for (index, component) in components.iter().enumerate() {
            groups
                .entry(union_find.find(index))
                .or_default()
                .insert(component.id);
        }
        let nets: Vec<_> = groups
            .into_values()
            .enumerate()
            .map(|(id, components)| PhysicalNet {
                id: NetId(id),
                components,
            })
            .collect();
        let fragments = nets
            .iter()
            .enumerate()
            .map(|(id, net)| PhysicalFragment {
                id: FragmentId(id),
                net: net.id,
                components: net.components.clone(),
            })
            .collect();
        Self {
            components,
            connections,
            nets,
            fragments,
        }
    }

    #[must_use]
    pub fn gap_candidates(&self, max_manhattan_distance: u32) -> Vec<GapCandidate> {
        let by_id: BTreeMap<_, _> = self
            .components
            .iter()
            .map(|component| (component.id, component))
            .collect();
        let mut candidates = Vec::new();
        for (left_index, left) in self.fragments.iter().enumerate() {
            for right in self.fragments.iter().skip(left_index + 1) {
                let nearest = left
                    .components
                    .iter()
                    .flat_map(|left_id| {
                        right.components.iter().map(|right_id| {
                            let a = by_id[left_id].pos;
                            let b = by_id[right_id].pos;
                            (
                                *left_id,
                                *right_id,
                                a.x.abs_diff(b.x) + a.y.abs_diff(b.y) + a.z.abs_diff(b.z),
                            )
                        })
                    })
                    .min_by_key(|(_, _, distance)| *distance);
                if let Some((left_component, right_component, distance)) =
                    nearest.filter(|(_, _, distance)| *distance <= max_manhattan_distance)
                {
                    candidates.push(GapCandidate {
                        left: left.id,
                        right: right.id,
                        evidence: vec![GapEvidence::Nearby {
                            left: left_component,
                            right: right_component,
                            manhattan_distance: distance,
                        }],
                    });
                }
            }
        }
        candidates
    }

    #[must_use]
    pub fn discover_nearby_fragments(
        &self,
        seed: FragmentId,
        max_manhattan_distance: u32,
    ) -> BTreeSet<FragmentId> {
        let candidates = self.gap_candidates(max_manhattan_distance);
        let mut adjacency: BTreeMap<FragmentId, Vec<FragmentId>> = BTreeMap::new();
        for candidate in candidates {
            adjacency
                .entry(candidate.left)
                .or_default()
                .push(candidate.right);
            adjacency
                .entry(candidate.right)
                .or_default()
                .push(candidate.left);
        }
        let mut discovered = BTreeSet::from([seed]);
        let mut queue = VecDeque::from([seed]);
        while let Some(fragment) = queue.pop_front() {
            for next in adjacency.get(&fragment).into_iter().flatten() {
                if discovered.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        discovered
    }
}

#[derive(Clone, Debug)]
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, value: usize) -> usize {
        if self.parent[value] != value {
            self.parent[value] = self.find(self.parent[value]);
        }
        self.parent[value]
    }

    fn union(&mut self, left: usize, right: usize) {
        let left = self.find(left);
        let right = self.find(right);
        if left == right {
            return;
        }
        match self.rank[left].cmp(&self.rank[right]) {
            std::cmp::Ordering::Less => self.parent[left] = right,
            std::cmp::Ordering::Greater => self.parent[right] = left,
            std::cmp::Ordering::Equal => {
                self.parent[right] = left;
                self.rank[left] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockKind;

    fn component(id: usize, x: i32) -> PhysicalComponent {
        PhysicalComponent {
            id: ComponentId(id),
            pos: Pos::new(x, 64, 0),
            block: Block::new(BlockKind::RedstoneWire),
        }
    }

    #[test]
    fn unions_only_verified_connections() {
        let circuit = PhysicalCircuit::from_parts(
            vec![component(0, 0), component(1, 1), component(2, 3)],
            [PhysicalConnection {
                source: ComponentId(0),
                sink: ComponentId(1),
                kind: ConnectionKind::Dust,
            }],
        );
        assert_eq!(circuit.fragments.len(), 2);
        assert_eq!(circuit.fragments[0].components.len(), 2);
        assert_eq!(circuit.fragments[1].components.len(), 1);
    }

    #[test]
    fn proximity_discovers_broken_fragments_without_unioning_them() {
        let circuit = PhysicalCircuit::from_parts(
            vec![component(0, 0), component(1, 1), component(2, 3)],
            [PhysicalConnection {
                source: ComponentId(0),
                sink: ComponentId(1),
                kind: ConnectionKind::Dust,
            }],
        );
        let nearby = circuit.discover_nearby_fragments(FragmentId(0), 2);
        assert_eq!(nearby, BTreeSet::from([FragmentId(0), FragmentId(1)]));
        assert_eq!(circuit.fragments.len(), 2);
    }
}
