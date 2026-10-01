//! Pure bounded partition phases; physical proofs remain in ElectricalWorkPlan.
use super::ElectricalWorkRegion;
use crate::piston_construction::limits;
use crate::piston_construction::order;
use dustroute_minecraft::{Pos, Region, World};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn plan(
    regions: Vec<Region>,
    changed: &BTreeSet<Pos>,
    original: &World,
    desired: &World,
) -> Result<Vec<ElectricalWorkRegion>, String> {
    let groups = subdivide(regions, changed)?;
    let edges = dependencies(&groups, original, desired)?;
    let (components, component) = components(&groups, &edges)?;
    Ok(order_components(
        &groups,
        &edges,
        &components,
        &component,
        original,
        desired,
    ))
}
fn subdivide(
    regions: Vec<Region>,
    changed: &BTreeSet<Pos>,
) -> Result<Vec<ElectricalWorkRegion>, String> {
    let mut groups = vec![];
    for r in regions {
        split(
            r,
            changed.iter().copied().filter(|p| r.contains(*p)).collect(),
            &mut groups,
        )?;
    }
    if groups.len() > limits::MAX_WORK_STAGES {
        return Err("automatic subdivision requires more than 64 stages; use fewer input regions or split the job".into());
    }
    Ok(groups)
}
fn dependencies(
    groups: &[ElectricalWorkRegion],
    original: &World,
    desired: &World,
) -> Result<Vec<u64>, String> {
    let owners = groups
        .iter()
        .enumerate()
        .flat_map(|(i, g)| g.changed_positions.iter().map(move |p| (*p, i)))
        .collect::<BTreeMap<_, _>>();
    let mut edges = vec![0_u64; groups.len()];
    let mut edge = |first: Pos, next: Pos| {
        if let (Some(a), Some(b)) = (owners.get(&first), owners.get(&next)) {
            if a != b {
                edges[*a] |= 1_u64 << b;
            }
        }
    };
    for (world, removing) in [(desired, false), (original, true)] {
        for (p, block) in world.iter() {
            if let Some(d) = block.support_offset {
                let support =
                    p.x.checked_add(d.x)
                        .zip(p.y.checked_add(d.y))
                        .zip(p.z.checked_add(d.z))
                        .map(|((x, y), z)| Pos::new(x, y, z))
                        .ok_or("support coordinate overflow")?;
                if removing {
                    edge(*p, support);
                } else {
                    edge(support, *p);
                }
            }
        }
    }
    for (observer, watched) in
        order::observer_predecessors(&order::ordered_blocks(desired), desired)?
    {
        edge(watched, observer);
    }
    Ok(edges)
}
fn components(
    groups: &[ElectricalWorkRegion],
    edges: &[u64],
) -> Result<(Vec<Vec<usize>>, Vec<usize>), String> {
    // At most 64 nodes: word-sized reachability takes O(R²), not a
    // permutation search. Mutually reachable nodes form one work stage.
    let mut reach = edges.to_vec();
    for (i, r) in reach.iter_mut().enumerate() {
        *r |= 1_u64 << i;
    }
    for via in 0..groups.len() {
        for i in 0..groups.len() {
            if reach[i] & (1_u64 << via) != 0 {
                reach[i] |= reach[via];
            }
        }
    }
    let mut components: Vec<Vec<usize>> = vec![];
    let mut component = vec![usize::MAX; groups.len()];
    for i in 0..groups.len() {
        if component[i] != usize::MAX {
            continue;
        }
        let members = (i..groups.len())
            .filter(|j| reach[i] & (1_u64 << j) != 0 && reach[*j] & (1_u64 << i) != 0)
            .collect::<Vec<_>>();
        for member in &members {
            component[*member] = components.len();
        }
        if members
            .iter()
            .map(|m| groups[*m].changed_positions.len())
            .sum::<usize>()
            > limits::MAX_STAGE_CHANGES
        {
            return Err("dependency cycle exceeds 64 changes; revise the partition or explicit intermediate design".into());
        }
        components.push(members);
    }
    Ok((components, component))
}
fn order_components(
    groups: &[ElectricalWorkRegion],
    edges: &[u64],
    components: &[Vec<usize>],
    component: &[usize],
    original: &World,
    desired: &World,
) -> Vec<ElectricalWorkRegion> {
    let mut outgoing = vec![BTreeSet::new(); components.len()];
    let mut indegree = vec![0; components.len()];
    for (a, links) in edges.iter().enumerate() {
        for b in 0..groups.len() {
            if links & (1_u64 << b) != 0
                && component[a] != component[b]
                && outgoing[component[a]].insert(component[b])
            {
                indegree[component[b]] += 1;
            }
        }
    }
    let mut ready = indegree
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect::<BTreeSet<_>>();
    let mut ordered = vec![];
    let priority = |i: usize| {
        components[i]
            .iter()
            .flat_map(|m| groups[*m].changed_positions.iter())
            .map(|p| {
                if let Some(block) = desired.get(*p) {
                    (
                        1_u8,
                        crate::piston_construction::policy::for_kind(block.kind).build as u8,
                    )
                } else {
                    (
                        0_u8,
                        original
                            .get(*p)
                            .and_then(|b| {
                                crate::piston_construction::policy::for_kind(b.kind).removal
                            })
                            .map_or(u8::MAX, |p| p as u8),
                    )
                }
            })
            .min()
            .unwrap_or((2, u8::MAX))
    };
    while let Some(i) = ready.iter().copied().min_by_key(|i| (priority(*i), *i)) {
        ready.remove(&i);
        let parts = components[i]
            .iter()
            .flat_map(|m| groups[*m].parts.clone())
            .collect::<Vec<_>>();
        let positions = components[i]
            .iter()
            .flat_map(|m| groups[*m].changed_positions.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let bounds = parts.iter().fold(parts[0], |a, b| {
            Region::new(
                Pos::new(
                    a.min.x.min(b.min.x),
                    a.min.y.min(b.min.y),
                    a.min.z.min(b.min.z),
                ),
                Pos::new(
                    a.max.x.max(b.max.x),
                    a.max.y.max(b.max.y),
                    a.max.z.max(b.max.z),
                ),
            )
        });
        ordered.push(ElectricalWorkRegion {
            region: bounds,
            parts,
            changed_positions: positions,
        });
        for next in &outgoing[i] {
            indegree[*next] -= 1;
            if indegree[*next] == 0 {
                ready.insert(*next);
            }
        }
    }
    ordered
}
fn split(
    r: Region,
    positions: Vec<Pos>,
    groups: &mut Vec<ElectricalWorkRegion>,
) -> Result<(), String> {
    if positions.is_empty() {
        return Ok(());
    }
    if positions.len() <= limits::MAX_STAGE_CHANGES {
        groups.push(ElectricalWorkRegion {
            region: r,
            parts: vec![r],
            changed_positions: positions,
        });
        return Ok(());
    }
    let coordinate = |p: Pos, axis: usize| match axis {
        0 => p.x,
        1 => p.y,
        _ => p.z,
    };
    let axis = (0..3)
        .max_by_key(|axis| {
            i64::from(
                positions
                    .iter()
                    .map(|p| coordinate(*p, *axis))
                    .max()
                    .unwrap(),
            ) - i64::from(
                positions
                    .iter()
                    .map(|p| coordinate(*p, *axis))
                    .min()
                    .unwrap(),
            )
        })
        .unwrap();
    let values = positions
        .iter()
        .map(|p| coordinate(*p, axis))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let cut = values[(values.len() - 1) / 2];
    let (left, right) = positions
        .into_iter()
        .partition(|p| coordinate(*p, axis) <= cut);
    let mut a = r;
    let mut b = r;
    match axis {
        0 => {
            a.max.x = cut;
            b.min.x = cut + 1;
        }
        1 => {
            a.max.y = cut;
            b.min.y = cut + 1;
        }
        _ => {
            a.max.z = cut;
            b.min.z = cut + 1;
        }
    }
    split(a, left, groups)?;
    split(b, right, groups)
}
