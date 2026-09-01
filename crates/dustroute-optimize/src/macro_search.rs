use std::collections::BTreeMap;

use dustroute_library::{Catalog, Component, ComponentId, ComponentKind, PhysicalMetrics};
use dustroute_translate::{BlockKind, FunctionalNetworkModel, World};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedMacroMetrics {
    pub bounding_size: [usize; 3],
    pub occupied_blocks: usize,
    pub dust_blocks: usize,
    pub repeater_count: usize,
}

impl ObservedMacroMetrics {
    #[must_use]
    pub fn from_world(world: &World) -> Self {
        let bounding_size = world.bounds().map_or([0; 3], |(low, high)| {
            [
                usize::try_from(high.x - low.x + 1).unwrap_or(usize::MAX),
                usize::try_from(high.y - low.y + 1).unwrap_or(usize::MAX),
                usize::try_from(high.z - low.z + 1).unwrap_or(usize::MAX),
            ]
        });
        Self {
            bounding_size,
            occupied_blocks: world.iter().count(),
            dust_blocks: world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::RedstoneWire)
                .count(),
            repeater_count: world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::Repeater)
                .count(),
        }
    }

    #[must_use]
    pub const fn bounding_volume(self) -> usize {
        self.bounding_size[0]
            .saturating_mul(self.bounding_size[1])
            .saturating_mul(self.bounding_size[2])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroReplacementCandidate {
    pub component_id: ComponentId,
    pub name: String,
    pub kind: ComponentKind,
    pub layout_reference: String,
    pub physical: PhysicalMetrics,
    /// For each observed input/output index, the matching macro port name.
    pub input_ports: Vec<String>,
    pub output_ports: Vec<String>,
    pub saved_blocks: usize,
    pub saved_volume: usize,
    /// Matching the stable function is necessary but not sufficient. The cell
    /// must be realized in context and its input transitions rechecked.
    pub requires_contextual_transition_verification: bool,
}

#[must_use]
pub fn find_verified_macro_replacements(
    model: &FunctionalNetworkModel,
    catalog: &Catalog,
    edition: &str,
    version: &str,
    current: ObservedMacroMetrics,
) -> Vec<MacroReplacementCandidate> {
    let mut candidates = catalog
        .search(&dustroute_library::ComponentQuery {
            input_count: Some(model.truth_table.inputs.len()),
            output_count: Some(model.truth_table.outputs.len()),
            require_physical: true,
            require_automatic_replacement: true,
            ..dustroute_library::ComponentQuery::default()
        })
        .into_iter()
        .filter(|component| compatible(component, edition, version))
        .filter_map(|component| {
            let (input_ports, output_ports) = truth_table_mapping(component, model)?;
            let physical = component.physical.clone()?;
            let layout_reference = component.layout_reference.clone()?;
            let candidate_volume = volume(physical.bounding_size);
            let current_volume = current.bounding_volume();
            let improves = physical.occupied_blocks < current.occupied_blocks
                || candidate_volume < current_volume;
            improves.then(|| MacroReplacementCandidate {
                component_id: component.id.clone(),
                name: component.name.clone(),
                kind: component.kind,
                layout_reference,
                input_ports,
                output_ports,
                saved_blocks: current
                    .occupied_blocks
                    .saturating_sub(physical.occupied_blocks),
                saved_volume: current_volume.saturating_sub(candidate_volume),
                physical,
                requires_contextual_transition_verification: true,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| {
        (
            candidate.physical.occupied_blocks,
            volume(candidate.physical.bounding_size),
            candidate
                .physical
                .delay_redstone_ticks
                .unwrap_or(usize::MAX),
            candidate.component_id.clone(),
        )
    });
    candidates
}

#[must_use]
pub fn find_builtin_verified_macro_replacements(
    model: &FunctionalNetworkModel,
    edition: &str,
    version: &str,
    current: ObservedMacroMetrics,
) -> Vec<MacroReplacementCandidate> {
    find_verified_macro_replacements(
        model,
        &dustroute_library::builtin_catalog(),
        edition,
        version,
        current,
    )
}

fn compatible(component: &Component, edition: &str, version: &str) -> bool {
    component
        .compatibility
        .as_ref()
        .is_some_and(|compatibility| {
            compatibility.edition == edition
                && !compatibility
                    .incompatible_versions
                    .iter()
                    .any(|item| item == version)
                && (compatibility.versions.is_empty()
                    || compatibility.versions.iter().any(|item| item == version))
        })
}

fn truth_table_mapping(
    component: &Component,
    model: &FunctionalNetworkModel,
) -> Option<(Vec<String>, Vec<String>)> {
    if component.logical.stateful {
        return None;
    }
    let input_count = model.truth_table.inputs.len();
    let output_count = model.truth_table.outputs.len();
    if component.logical.input_names().len() != input_count
        || component.logical.output_names().len() != output_count
        || input_count > 6
        || output_count > 6
    {
        return None;
    }
    let actual = component
        .logical
        .truth_table
        .iter()
        .filter(|row| row.len() == input_count + model.truth_table.outputs.len())
        .map(|row| (row[..input_count].to_vec(), row[input_count..].to_vec()))
        .collect::<BTreeMap<_, _>>();
    for input_mapping in permutations(input_count) {
        for output_mapping in permutations(output_count) {
            let matches = model.truth_table.rows.iter().all(|row| {
                let mut candidate_inputs = vec![false; input_count];
                for (observed, candidate) in input_mapping.iter().copied().enumerate() {
                    candidate_inputs[candidate] = row.inputs[observed];
                }
                let Some(candidate_outputs) = actual.get(&candidate_inputs) else {
                    return false;
                };
                output_mapping
                    .iter()
                    .copied()
                    .enumerate()
                    .all(|(observed, candidate)| {
                        row.outputs[observed] == candidate_outputs[candidate]
                    })
            });
            if matches {
                let input_names = component.logical.input_names();
                let output_names = component.logical.output_names();
                return Some((
                    input_mapping
                        .iter()
                        .map(|index| input_names[*index].to_owned())
                        .collect(),
                    output_mapping
                        .iter()
                        .map(|index| output_names[*index].to_owned())
                        .collect(),
                ));
            }
        }
    }
    None
}

fn permutations(count: usize) -> Vec<Vec<usize>> {
    fn visit(prefix: &mut Vec<usize>, unused: &mut Vec<usize>, result: &mut Vec<Vec<usize>>) {
        if unused.is_empty() {
            result.push(prefix.clone());
            return;
        }
        for index in 0..unused.len() {
            let value = unused.remove(index);
            prefix.push(value);
            visit(prefix, unused, result);
            prefix.pop();
            unused.insert(index, value);
        }
    }
    let mut result = Vec::new();
    visit(&mut Vec::new(), &mut (0..count).collect(), &mut result);
    result
}

const fn volume(size: [usize; 3]) -> usize {
    size[0].saturating_mul(size[1]).saturating_mul(size[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_library::{DUSTROUTE_COMPACT_XOR_ID, REDSTONE_COMPILER_XOR_ID, builtin_catalog};
    use dustroute_translate::{RegionBounds, analyze_world_region, derive_functional_network};

    #[test]
    fn shared_xor_function_finds_only_the_smaller_minecraft_verified_macro() {
        let world = dustroute_translate::compiled_xor_cell().unwrap().world;
        let (low, high) = world.bounds().unwrap();
        let analysis = analyze_world_region(&world, RegionBounds::new(low, high));
        let model = derive_functional_network(&world, &analysis, 8, 64).unwrap();
        let candidates = find_verified_macro_replacements(
            &model,
            &builtin_catalog(),
            "java",
            "1.21.11",
            ObservedMacroMetrics::from_world(&world),
        );

        assert_eq!(candidates.len(), 1, "{candidates:#?}");
        assert_eq!(
            candidates[0].component_id.as_str(),
            DUSTROUTE_COMPACT_XOR_ID
        );
        assert!(candidates[0].saved_blocks > 0);
        assert!(candidates[0].saved_volume > 0);
        assert_eq!(candidates[0].input_ports, ["a", "b"]);
        assert_eq!(candidates[0].output_ports, ["out"]);
        assert!(candidates[0].requires_contextual_transition_verification);
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.component_id.as_str() != REDSTONE_COMPILER_XOR_ID)
        );
    }
}
