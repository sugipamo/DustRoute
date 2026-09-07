use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{Block, Pos};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ComponentId(pub usize);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct NetId(pub usize);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct FragmentId(pub usize);

/// Preferred name for a Union-Find group used only for physical traversal and
/// nearby-fragment discovery. It is not a logical circuit or signal-flow unit.
pub type PhysicalTraversalGroupId = FragmentId;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    Dust,
    DustRise,
    DustFallThroughConductor,
    WeakPower,
    StrongPower,
    DirectionalInput,
    DirectionalOutput,
    DirectSource,
    Control,
    /// A block-state transition observed at the front face of an Observer.
    ObserverInput,
    /// The strong pulse emitted from an Observer's back face.
    ObserverOutput,
    /// A direct redstone input into a Piston. Mechanical movement is modeled
    /// separately from this electrical trigger.
    PistonInput,
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
pub struct PhysicalTraversalGroup {
    pub id: FragmentId,
    pub nets: BTreeSet<NetId>,
    pub components: BTreeSet<ComponentId>,
}

/// Compatibility name. New APIs should say `PhysicalTraversalGroup` so callers
/// do not mistake undirected membership for functional circuit identity.
pub type PhysicalFragment = PhysicalTraversalGroup;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GapEvidence {
    Nearby {
        left: ComponentId,
        right: ComponentId,
        manhattan_distance: u32,
    },
    MissingInlineBlock {
        position: Pos,
    },
    InvalidSupport {
        component: ComponentId,
        expected_support: Pos,
    },
    DirectionMismatch {
        component: ComponentId,
        toward: ComponentId,
    },
    SuspectedUnexpectedConnection {
        component: ComponentId,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GapCandidate {
    pub left: FragmentId,
    pub right: FragmentId,
    pub evidence: Vec<GapEvidence>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct VerifiedTopology {
    pub components: Vec<PhysicalComponent>,
    pub connections: BTreeSet<PhysicalConnection>,
    pub nets: Vec<PhysicalNet>,
    pub fragments: Vec<PhysicalFragment>,
}

impl VerifiedTopology {
    /// Undirected groups used to bound physical discovery. These groups do not
    /// imply common signal direction, driveability, or logical function.
    #[must_use]
    pub fn physical_traversal_groups(&self) -> &[PhysicalTraversalGroup] {
        &self.fragments
    }

    #[must_use]
    pub fn from_parts(
        mut components: Vec<PhysicalComponent>,
        connections: impl IntoIterator<Item = PhysicalConnection>,
    ) -> Self {
        components.sort_by_key(|component| (component.pos, component.id));
        let connections: BTreeSet<_> = connections.into_iter().collect();
        let indices: BTreeMap<_, _> = components
            .iter()
            .enumerate()
            .map(|(index, component)| (component.id, index))
            .collect();
        let nets: Vec<_> = connection_groups(&components, &connections, &indices, |kind| {
            matches!(
                kind,
                ConnectionKind::Dust
                    | ConnectionKind::DustRise
                    | ConnectionKind::DustFallThroughConductor
                    | ConnectionKind::WeakPower
                    | ConnectionKind::StrongPower
            )
        })
        .into_values()
        .enumerate()
        .map(|(id, components)| PhysicalNet {
            id: NetId(id),
            components,
        })
        .collect();
        let fragments = connection_groups(&components, &connections, &indices, |kind| {
            kind != ConnectionKind::Support
        })
        .into_values()
        .enumerate()
        .map(|(id, components)| PhysicalFragment {
            id: FragmentId(id),
            nets: nets
                .iter()
                .filter(|net| !net.components.is_disjoint(&components))
                .map(|net| net.id)
                .collect(),
            components,
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
                    .filter(|left_id| by_id[left_id].block.kind.is_redstone_related())
                    .flat_map(|left_id| {
                        right
                            .components
                            .iter()
                            .filter(|right_id| by_id[right_id].block.kind.is_redstone_related())
                            .map(|right_id| {
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

fn connection_groups(
    components: &[PhysicalComponent],
    connections: &BTreeSet<PhysicalConnection>,
    indices: &BTreeMap<ComponentId, usize>,
    include: impl Fn(ConnectionKind) -> bool,
) -> BTreeMap<usize, BTreeSet<ComponentId>> {
    let mut union_find = UnionFind::new(components.len());
    for connection in connections
        .iter()
        .filter(|connection| include(connection.kind))
    {
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
    groups
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

    fn support(id: usize, x: i32) -> PhysicalComponent {
        PhysicalComponent {
            id: ComponentId(id),
            pos: Pos::new(x, 64, 0),
            block: Block::new(BlockKind::Solid),
        }
    }

    #[test]
    fn unions_only_verified_connections() {
        let circuit = VerifiedTopology::from_parts(
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
        let circuit = VerifiedTopology::from_parts(
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

    #[test]
    fn directional_devices_separate_nets_without_splitting_fragments() {
        let circuit = VerifiedTopology::from_parts(
            vec![component(0, 0), component(1, 1), component(2, 2)],
            [
                PhysicalConnection {
                    source: ComponentId(0),
                    sink: ComponentId(1),
                    kind: ConnectionKind::DirectionalInput,
                },
                PhysicalConnection {
                    source: ComponentId(1),
                    sink: ComponentId(2),
                    kind: ConnectionKind::DirectionalOutput,
                },
            ],
        );
        assert_eq!(circuit.nets.len(), 3);
        assert_eq!(circuit.fragments.len(), 1);
        assert_eq!(circuit.fragments[0].nets.len(), 3);
    }

    #[test]
    fn structural_supports_do_not_create_proximity_candidates() {
        let circuit = VerifiedTopology::from_parts(
            vec![
                component(0, 0),
                support(1, 4),
                support(2, 5),
                component(3, 10),
            ],
            [
                PhysicalConnection {
                    source: ComponentId(0),
                    sink: ComponentId(1),
                    kind: ConnectionKind::Support,
                },
                PhysicalConnection {
                    source: ComponentId(2),
                    sink: ComponentId(3),
                    kind: ConnectionKind::Support,
                },
            ],
        );
        assert!(circuit.gap_candidates(2).is_empty());
    }
}
