//! Bounded caller policy over the common checked construction transitions.
//! Candidate pruning is deliberately incomplete; it never establishes impossibility.
#[path = "generation/candidates.rs"]
mod candidates;
#[path = "generation/policy.rs"]
mod policy;

use super::{
    CheckedPrefix, ConstructionPlanningError, ConstructionSite, HypotheticalConstructionPlan,
    Ledger, Result, cells, xyz,
};
use crate::survival_error::SurvivalErrorCode;
use crate::survival_navigation::TravelBounds;
use rmcp::schemars;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use voxrig::checked_survival::CapturedSurvivalScene;
use voxrig::{BlockFace, NativeBlockState};

#[derive(Clone, Copy, Debug, Serialize, serde::Deserialize, rmcp::schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchLimits {
    /// Includes refused extensions. One check may call multiple native guards.
    pub candidate_checks: usize,
    pub expanded: usize,
    pub frontier: usize,
    pub actions: usize,
}
impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            candidate_checks: 100_000,
            expanded: 512,
            frontier: 16,
            actions: 256,
        }
    }
}
impl SearchLimits {
    fn validate(self) -> Result<()> {
        if !(1..=1_000_000).contains(&self.candidate_checks)
            || !(1..=4096).contains(&self.expanded)
            || !(1..=64).contains(&self.frontier)
            || !(1..=512).contains(&self.actions)
        {
            return Err(ConstructionPlanningError::new(
                SurvivalErrorCode::InvalidSearchLimits,
                "require 1..1000000 candidate checks, 1..4096 expansions, 1..64 frontier, 1..512 actions",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructionSearch {
    pub candidate_checks: usize,
    pub expanded: usize,
    pub rejected: usize,
    pub pruned: usize,
    pub frontier_peak: usize,
    pub complete_checks: usize,
    pub action_limited_nodes: usize,
    pub cleanup_searches: usize,
    pub completed_access_cleanups: usize,
    pub best_remaining_permanent: usize,
    pub best_remaining_targets: Vec<[i32; 3]>,
    pub highest_hypothetical_feet: f64,
    pub best_remaining_temporary: Vec<[i32; 3]>,
    /// Bounded progress samples for diagnosing policy starvation and cycles.
    pub progress: Vec<SearchProgress>,
    /// Bounded samples, not an exhaustive explanation or a proof of impossibility.
    pub refusal_examples: Vec<ConstructionPlanningError>,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchProgress {
    pub candidate_checks: usize,
    pub actions: usize,
    pub position: [f64; 3],
    pub remaining_permanent: usize,
    pub remaining_temporary: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GenerationFailure {
    PlanningTaskFailed {
        reason: String,
    },
    InvalidInput {
        error: ConstructionPlanningError,
    },
    UnsupportedTarget {
        position: [i32; 3],
        state: NativeBlockState,
        reason: String,
    },
    InitialSceneRefused {
        error: ConstructionPlanningError,
    },
    InsufficientMaterials {
        missing: BTreeMap<String, usize>,
    },
    NoCompletePlanWithinLimits {
        reason: SearchStop,
        search: Box<ConstructionSearch>,
    },
}

/// Async-client entry point. Only owned detached data enters the CPU worker;
/// live client I/O and action authority remain on their existing executor.
/// Dropping the awaiter discards the result, but an already started bounded
/// search continues read-only until its limits are reached.
pub async fn generate_construction_plan_async(
    scene: CapturedSurvivalScene,
    site: ConstructionSite,
    supplied: BTreeMap<String, usize>,
    temporary_material: String,
    limits: SearchLimits,
) -> std::result::Result<GeneratedConstructionPlan, GenerationFailure> {
    limits
        .validate()
        .map_err(|error| GenerationFailure::InvalidInput { error })?;
    tokio::task::spawn_blocking(move || {
        generate_construction_plan(&scene, &site, &supplied, &temporary_material, limits)
    })
    .await
    .map_err(|error| GenerationFailure::PlanningTaskFailed {
        reason: error.to_string(),
    })?
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchStop {
    CandidateBudget,
    ExpansionBudget,
    CandidateFrontierExhausted,
}

#[derive(Clone, Debug, Serialize)]
pub struct GeneratedConstructionPlan {
    pub plan: HypotheticalConstructionPlan,
    pub search: ConstructionSearch,
}

#[derive(Clone)]
struct Node<'a> {
    checked: CheckedPrefix<'a>,
    cleaning: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PoseKind {
    Center,
    Edge,
    Overhang,
}
#[derive(Clone, Copy)]
struct WorkPose {
    position: [f64; 3],
    kind: PoseKind,
}

struct Search<'a> {
    limits: SearchLimits,
    supplied: &'a BTreeMap<String, usize>,
    temporary_material: &'a str,
    temporary_cells: Vec<[i32; 3]>,
    stats: ConstructionSearch,
}

fn missing_materials(
    ledger: &Ledger<'_>,
    supplied: &BTreeMap<String, usize>,
) -> BTreeMap<String, usize> {
    // Reserve unbuilt permanent targets, including when temporary and permanent
    // materials coincide. Removing a temporary block never refunds consumption.
    let mut required = ledger.materials.required_supplied.clone();
    for state in ledger.remaining.values() {
        *required.entry(state.name.clone()).or_default() += 1;
    }
    required
        .into_iter()
        .filter_map(|(name, count)| {
            let missing = count.saturating_sub(supplied.get(&name).copied().unwrap_or(0));
            (missing > 0).then_some((name, missing))
        })
        .collect()
}

const FACES: [(BlockFace, [i32; 3]); 6] = [
    (BlockFace::Up, [0, 1, 0]),
    (BlockFace::North, [0, 0, -1]),
    (BlockFace::South, [0, 0, 1]),
    (BlockFace::West, [-1, 0, 0]),
    (BlockFace::East, [1, 0, 0]),
    (BlockFace::Down, [0, -1, 0]),
];

fn rotation(eye: [f64; 3], point: [f64; 3]) -> [f32; 2] {
    let d: [f64; 3] = std::array::from_fn(|i| point[i] - eye[i]);
    [
        (-d[0]).atan2(d[2]).to_degrees() as f32,
        (-d[1]).atan2(d[0].hypot(d[2])).to_degrees() as f32,
    ]
}
fn eye(node: &Node<'_>) -> [f64; 3] {
    let source = node.checked.scene.source();
    std::array::from_fn(|i| {
        node.checked.scenario.position()[i] + source.eye_position[i] - source.position[i]
    })
}
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>()
}
fn center(p: [i32; 3]) -> [f64; 3] {
    p.map(|n| f64::from(n) + 0.5)
}
fn within(bounds: TravelBounds, p: [f64; 3]) -> bool {
    (0..3).all(|i| bounds.min[i] <= p[i] && p[i] <= bounds.max[i])
}

/// Produce a complete hypothetical plan or bounded-search diagnostics. This is
/// read-only and grants no adoption, inventory receipt or execution authority.
/// This function is CPU-bound. Async clients must use the off-thread entry point
/// above so planning cannot starve keepalive and observation tasks.
pub fn generate_construction_plan(
    scene: &CapturedSurvivalScene,
    site: &ConstructionSite,
    supplied: &BTreeMap<String, usize>,
    temporary_material: &str,
    limits: SearchLimits,
) -> std::result::Result<GeneratedConstructionPlan, GenerationFailure> {
    limits
        .validate()
        .map_err(|error| GenerationFailure::InvalidInput { error })?;
    if !temporary_material.starts_with("minecraft:") || temporary_material.len() > 128 {
        return Err(GenerationFailure::InvalidInput {
            error: ConstructionPlanningError::new(
                SurvivalErrorCode::InvalidTemporaryMaterial,
                "use a canonical minecraft material name",
            ),
        });
    }
    // Native hypothetical placement currently synthesizes property-free cubes.
    // Do not spend a search budget attempting an exact state it cannot produce.
    if let Some((position, state)) = site
        .structure
        .iter()
        .find(|(_, s)| !s.properties.is_empty())
    {
        return Err(GenerationFailure::UnsupportedTarget {
            position: *position,
            state: state.clone(),
            reason: "native hypothetical cube placement produces property-free states".into(),
        });
    }
    let missing = missing_materials(&Ledger::new(site), supplied);
    if !missing.is_empty() {
        return Err(GenerationFailure::InsufficientMaterials { missing });
    }
    let checked = CheckedPrefix::new(scene, site)
        .map_err(|error| GenerationFailure::InitialSceneRefused { error })?;
    let temporary_cells = site
        .scope
        .temporary
        .iter()
        .flat_map(|r| cells(*r).map(xyz))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let search = Search {
        limits,
        supplied,
        temporary_material,
        temporary_cells,
        stats: ConstructionSearch {
            best_remaining_permanent: site.structure.len(),
            best_remaining_targets: site.structure.keys().copied().collect(),
            highest_hypothetical_feet: scene.source().position[1],
            frontier_peak: 1,
            ..Default::default()
        },
    };
    search.run(checked)
}

#[cfg(test)]
mod tests {
    use super::super::{PlacementPurpose, air};
    use super::*;
    use voxrig::checked_survival::HypotheticalBlockEdit;

    #[test]
    fn temporary_consumption_reserves_unbuilt_permanent_materials_and_never_refunds() {
        let site = ConstructionSite::from_grounded(
            &super::super::tests::design(),
            super::super::tests::scope(),
        )
        .unwrap();
        let mut ledger = Ledger::new(&site);
        let supplied = BTreeMap::from([("minecraft:cobblestone".into(), 49)]);
        assert!(missing_materials(&ledger, &supplied).is_empty());
        let edit = HypotheticalBlockEdit {
            position: [-3, 0, 2],
            before: air(),
            after: NativeBlockState {
                name: "minecraft:cobblestone".into(),
                properties: Default::default(),
            },
        };
        ledger.place(PlacementPurpose::Temporary, &edit).unwrap();
        assert_eq!(
            missing_materials(&ledger, &supplied)["minecraft:cobblestone"],
            1
        );
        ledger
            .remove(&HypotheticalBlockEdit {
                position: edit.position,
                before: edit.after,
                after: air(),
            })
            .unwrap();
        assert_eq!(
            missing_materials(&ledger, &supplied)["minecraft:cobblestone"],
            1
        );
        assert_eq!(ledger.remaining.len(), 49);
    }

    #[test]
    fn extreme_search_budgets_refuse_before_candidate_generation() {
        assert!(SearchLimits::default().validate().is_ok());
        for limits in [
            SearchLimits {
                candidate_checks: 0,
                ..Default::default()
            },
            SearchLimits {
                candidate_checks: usize::MAX,
                ..Default::default()
            },
            SearchLimits {
                expanded: usize::MAX,
                ..Default::default()
            },
            SearchLimits {
                frontier: 0,
                ..Default::default()
            },
            SearchLimits {
                frontier: 65,
                ..Default::default()
            },
            SearchLimits {
                actions: 513,
                ..Default::default()
            },
        ] {
            assert_eq!(
                limits.validate().unwrap_err().code,
                SurvivalErrorCode::InvalidSearchLimits
            );
        }
    }
}
