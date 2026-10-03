//! Read-only candidate construction checks. No adoption, durable job or action authority.
//! The caller selects a sequence; native geometry verifies each ordered dependency.
use crate::survival_error::SurvivalErrorCode;
use crate::survival_navigation::{self, RouteRequest, TravelBounds};
use dustroute_library::world_edit::WorldEditScope;
use dustroute_translate::{
    building::GeneratedGroundedBuildingDesign,
    snapshot::{LiteralSnapshotIndex, MinecraftSnapshot},
    world::{Pos, Region},
};
use rmcp::schemars;
use serde::Serialize;
use std::collections::BTreeMap;
use voxrig::checked_survival::{
    CapturedSurvivalScene, HypotheticalBlockEdit, HypotheticalMovementPreview,
    HypotheticalPlacement, HypotheticalReconnectBoundary, StandingContext, SurvivalControl,
    SurvivalMotionContract, SurvivalScenario,
};
use voxrig::{BlockFace, NativeBlockState};

/// Proposed execution footprint, separate from immutable final Blueprint geometry.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize, rmcp::schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConstructionScope {
    pub observed: Region,
    pub edits: WorldEditScope,
    /// Temporary works need explicit coverage here AND in `edits`.
    /// This first version refuses permanent-air/structure overlap entirely.
    pub temporary: Vec<Region>,
    pub travel: TravelBounds,
    /// Required safe feet volume after the final temporary removal.
    pub retreat: TravelBounds,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructionPlanningError {
    pub code: SurvivalErrorCode,
    pub action: Option<usize>,
    pub position: Option<[i32; 3]>,
    pub detail: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub missing_materials: BTreeMap<String, usize>,
}
impl ConstructionPlanningError {
    fn new(code: SurvivalErrorCode, detail: impl ToString) -> Self {
        Self {
            code,
            action: None,
            position: None,
            detail: detail.to_string(),
            missing_materials: BTreeMap::new(),
        }
    }
    fn at(mut self, position: [i32; 3]) -> Self {
        self.position = Some(position);
        self
    }
}
impl std::fmt::Display for ConstructionPlanningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for ConstructionPlanningError {}
type Result<T> = std::result::Result<T, ConstructionPlanningError>;
fn native_error(e: voxrig::Error) -> ConstructionPlanningError {
    ConstructionPlanningError::new(SurvivalErrorCode::NativeGeometryRefused, e)
}
fn pos(p: [i32; 3]) -> Pos {
    Pos::new(p[0], p[1], p[2])
}
fn xyz(p: Pos) -> [i32; 3] {
    [p.x, p.y, p.z]
}
fn air() -> NativeBlockState {
    NativeBlockState {
        name: "minecraft:air".into(),
        properties: BTreeMap::new(),
    }
}
fn literal(index: &LiteralSnapshotIndex<'_>, p: Pos) -> NativeBlockState {
    index.get(p).map_or_else(air, |b| NativeBlockState {
        name: b.name.clone(),
        properties: b.properties.clone(),
    })
}
fn cells(r: Region) -> impl Iterator<Item = Pos> {
    (r.min.x..=r.max.x).flat_map(move |x| {
        (r.min.y..=r.max.y).flat_map(move |y| (r.min.z..=r.max.z).map(move |z| Pos::new(x, y, z)))
    })
}
fn intersects(a: Region, b: Region) -> bool {
    a.min.x <= b.max.x
        && b.min.x <= a.max.x
        && a.min.y <= b.max.y
        && b.min.y <= a.max.y
        && a.min.z <= b.max.z
        && b.min.z <= a.max.z
}

/// Checked final-state/site contract. Not Deserialize and not an adoption receipt.
#[derive(Clone, Debug)]
pub struct ConstructionSite {
    baseline: MinecraftSnapshot,
    expected: MinecraftSnapshot,
    structure: BTreeMap<[i32; 3], NativeBlockState>,
    scope: ConstructionScope,
    initial_temporary: BTreeMap<[i32; 3], NativeBlockState>,
}
impl ConstructionSite {
    pub(crate) fn scope(&self) -> &ConstructionScope {
        &self.scope
    }
    /// Reuse grounded Blueprint geometry without asserting adoption/live readiness.
    pub fn from_grounded(
        design: &GeneratedGroundedBuildingDesign,
        scope: ConstructionScope,
    ) -> Result<Self> {
        Self::from_parts(
            &design.baseline,
            &design.expected,
            scope,
            &design
                .specification
                .design
                .spaces
                .iter()
                .map(|s| s.region)
                .collect::<Vec<_>>(),
            Some(design.protected_ground),
            false,
        )
    }

    /// Bounded temporary work with exact restoration of the declared initial site.
    /// This supplies no Blueprint adoption and permits no permanent placement.
    pub fn temporary_work(baseline: &MinecraftSnapshot, scope: ConstructionScope) -> Result<Self> {
        Self::from_parts(baseline, baseline, scope, &[], None, true)
    }

    fn from_parts(
        before: &MinecraftSnapshot,
        after: &MinecraftSnapshot,
        scope: ConstructionScope,
        spaces: &[Region],
        protected_ground: Option<Region>,
        temporary_only: bool,
    ) -> Result<Self> {
        let bad =
            |e: &str| ConstructionPlanningError::new(SurvivalErrorCode::InvalidSiteContract, e);
        let known = scope.observed;
        let mut volume = 1i64;
        for (lo, hi) in xyz(known.min).into_iter().zip(xyz(known.max)) {
            let side = i64::from(hi) - i64::from(lo) + 1;
            if !(1..=64).contains(&side) || lo < -29_999_984 || hi > 29_999_984 {
                return Err(bad(
                    "observed axes must be ordered, bounded and at most 64 cells",
                ));
            }
            volume *= side;
        }
        if volume > 32768 {
            return Err(bad("observed region exceeds 32768 cells"));
        }
        scope.edits.validate(known).map_err(|e| bad(&e))?;
        survival_navigation::validate(&RouteRequest {
            travel: scope.travel,
            goal: scope.retreat,
            max_previews: 2,
        })
        .map_err(|e| bad(&format!("invalid travel/retreat: {e:?}")))?;
        if (0..3).any(|i| {
            scope.travel.min[i] < f64::from(xyz(known.min)[i])
                || scope.travel.max[i] > f64::from(xyz(known.max)[i]) + 1.0
        }) {
            return Err(bad("travel lies outside observed geometry"));
        }
        let baseline = LiteralSnapshotIndex::new(before).map_err(|e| bad(&e.to_string()))?;
        let expected = LiteralSnapshotIndex::new(after).map_err(|e| bad(&e.to_string()))?;
        let changed = baseline
            .changed_positions(&expected)
            .map_err(|e| bad(&e.to_string()))?;
        let blueprint = Region::new(before.min, before.max);
        if !known.contains(blueprint.min) || !known.contains(blueprint.max) {
            return Err(bad(
                "observations must contain the complete Blueprint region",
            ));
        }
        if let Some(protected_ground) = protected_ground {
            if protected_ground
                != Region::new(
                    blueprint.min,
                    Pos::new(blueprint.max.x, blueprint.min.y, blueprint.max.z),
                )
            {
                return Err(bad(
                    "protected ground must be the bounded Blueprint bottom plane",
                ));
            }
            if cells(protected_ground).any(|p| scope.edits.allows_change(p)) {
                return Err(bad("existing ground must remain outside editable space"));
            }
        }
        // Validate public generated data rather than trusting cached material counts.
        let mut structure = BTreeMap::new();
        for p in changed {
            let after = literal(&expected, p);
            if literal(&baseline, p) != air() || after == air() || !scope.edits.allows_change(p) {
                return Err(
                    bad("structure must add cubes in permitted initially empty cells").at(xyz(p)),
                );
            }
            structure.insert(xyz(p), after);
        }
        if (!temporary_only && structure.is_empty()) || structure.len() > 64 {
            return Err(bad("requires 1..64 permanent placements"));
        }
        if scope.temporary.len() > 64 {
            return Err(bad("at most 64 temporary regions"));
        }
        for r in &scope.temporary {
            if r.min.x > r.max.x
                || r.min.y > r.max.y
                || r.min.z > r.max.z
                || !known.contains(r.min)
                || !known.contains(r.max)
            {
                return Err(bad("temporary region is unordered or outside observations"));
            }
            if spaces.iter().any(|s| intersects(*r, *s)) {
                return Err(bad("temporary region overlaps declared permanent air"));
            }
            if cells(*r).any(|p| {
                !scope.edits.allows_change(p)
                    || structure.contains_key(&xyz(p))
                    || (blueprint.contains(p) && literal(&baseline, p) != air())
            }) {
                return Err(bad(
                    "temporary region overlaps structure, protected terrain or existing solids",
                ));
            }
        }
        Ok(Self {
            baseline: before.clone(),
            expected: after.clone(),
            structure,
            scope,
            initial_temporary: BTreeMap::new(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementPurpose {
    Permanent,
    Temporary,
}

/// Proposed actions only. Native hit/collision checks have not yet run.
#[derive(Clone, Debug)]
pub enum ConstructionAction {
    Move {
        controls: Vec<SurvivalControl>,
    },
    Place {
        purpose: PlacementPurpose,
        support: [i32; 3],
        face: BlockFace,
        rotation: [f32; 2],
        material: String,
    },
    RemoveTemporary {
        target: [i32; 3],
        face: BlockFace,
        rotation: [f32; 2],
    },
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HypotheticalConstructionStep {
    Move {
        prediction: Box<HypotheticalMovementPreview>,
    },
    Place {
        purpose: PlacementPurpose,
        placement: HypotheticalPlacement,
    },
    RemoveTemporary {
        edit: HypotheticalBlockEdit,
        face_id: u8,
        rotation: [f32; 2],
        /// Common execution retires and reconnects after every mining attempt.
        reconnect: HypotheticalReconnectBoundary,
    },
}
#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
pub struct ConstructionMaterials {
    pub permanent: BTreeMap<String, usize>,
    /// Every placement consumes supplied inventory; removal never credits drops.
    pub temporary_without_recovery: BTreeMap<String, usize>,
    /// Maximum simultaneously placed temporary blocks, by material.
    pub peak_temporary_in_world: BTreeMap<String, usize>,
    pub required_supplied: BTreeMap<String, usize>,
}
/// Geometric/order/material evidence, not adoption or standalone action authority.
/// The caller-authorized executor must revalidate it; it is not deserializable.
#[derive(Clone, Debug, Serialize)]
pub struct HypotheticalConstructionPlan {
    motion_contract: SurvivalMotionContract,
    source: StandingContext,
    scope: ConstructionScope,
    baseline: MinecraftSnapshot,
    expected: MinecraftSnapshot,
    steps: Vec<HypotheticalConstructionStep>,
    materials: ConstructionMaterials,
    final_position: [f64; 3],
    initial_temporary: Vec<TemporaryBlock>,
}

/// Historical ownership is conditional on an exact fresh observation and the
/// explicitly approved temporary footprint; an identical state is not attribution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporaryBlock {
    pub position: [i32; 3],
    pub state: NativeBlockState,
}
impl HypotheticalConstructionPlan {
    pub(crate) fn final_position(&self) -> [f64; 3] {
        self.final_position
    }
    pub(crate) fn motion_contract(&self) -> SurvivalMotionContract {
        self.motion_contract
    }
    pub(crate) fn source(&self) -> &StandingContext {
        &self.source
    }
    pub(crate) fn scope(&self) -> &ConstructionScope {
        &self.scope
    }
    pub(crate) fn baseline(&self) -> &MinecraftSnapshot {
        &self.baseline
    }
    pub(crate) fn expected(&self) -> &MinecraftSnapshot {
        &self.expected
    }
    pub fn steps(&self) -> &[HypotheticalConstructionStep] {
        &self.steps
    }
    pub fn materials(&self) -> &ConstructionMaterials {
        &self.materials
    }
    pub(crate) fn initial_temporary(&self) -> &[TemporaryBlock] {
        &self.initial_temporary
    }
}

#[derive(Clone)]
struct Ledger<'a> {
    site: &'a ConstructionSite,
    remaining: BTreeMap<[i32; 3], NativeBlockState>,
    temporary: BTreeMap<[i32; 3], NativeBlockState>,
    materials: ConstructionMaterials,
}
impl<'a> Ledger<'a> {
    fn new(site: &'a ConstructionSite) -> Self {
        let mut materials = ConstructionMaterials::default();
        for state in site.initial_temporary.values() {
            *materials
                .peak_temporary_in_world
                .entry(state.name.clone())
                .or_default() += 1;
        }
        Self {
            site,
            remaining: site.structure.clone(),
            temporary: site.initial_temporary.clone(),
            materials,
        }
    }
    fn place(&mut self, purpose: PlacementPurpose, edit: &HypotheticalBlockEdit) -> Result<()> {
        let fail = |detail| {
            ConstructionPlanningError::new(SurvivalErrorCode::PlacementOutsidePlan, detail)
                .at(edit.position)
        };
        if edit.before != air()
            || edit.after == air()
            || !self.site.scope.edits.allows_change(pos(edit.position))
        {
            return Err(fail("placement must consume permitted exact air"));
        }
        match purpose {
            PlacementPurpose::Permanent => {
                if self.remaining.get(&edit.position) != Some(&edit.after) {
                    return Err(fail("not an unbuilt exact permanent target"));
                }
                self.remaining.remove(&edit.position);
                *self
                    .materials
                    .permanent
                    .entry(edit.after.name.clone())
                    .or_default() += 1;
            }
            PlacementPurpose::Temporary => {
                if !self
                    .site
                    .scope
                    .temporary
                    .iter()
                    .any(|r| r.contains(pos(edit.position)))
                    || self.temporary.contains_key(&edit.position)
                {
                    return Err(fail(
                        "temporary placement outside its explicit footprint or already occupied",
                    ));
                }
                self.temporary.insert(edit.position, edit.after.clone());
                *self
                    .materials
                    .temporary_without_recovery
                    .entry(edit.after.name.clone())
                    .or_default() += 1;
                let active = self
                    .temporary
                    .values()
                    .filter(|b| b.name == edit.after.name)
                    .count();
                let peak = self
                    .materials
                    .peak_temporary_in_world
                    .entry(edit.after.name.clone())
                    .or_default();
                *peak = (*peak).max(active);
            }
        }
        *self
            .materials
            .required_supplied
            .entry(edit.after.name.clone())
            .or_default() += 1;
        Ok(())
    }
    fn remove(&mut self, edit: &HypotheticalBlockEdit) -> Result<()> {
        if self.temporary.get(&edit.position) != Some(&edit.before) || edit.after != air() {
            return Err(ConstructionPlanningError::new(
                SurvivalErrorCode::RemovalOutsideTemporaryWorks,
                "only an exact previously planned temporary cube may be removed",
            )
            .at(edit.position));
        }
        self.temporary.remove(&edit.position);
        Ok(())
    }
    fn finish(self, supplied: &BTreeMap<String, usize>) -> Result<ConstructionMaterials> {
        if !self.remaining.is_empty() || !self.temporary.is_empty() {
            return Err(ConstructionPlanningError::new(
                SurvivalErrorCode::IncompleteSequence,
                format!(
                    "{} permanent targets unbuilt; {} temporary cubes not removed",
                    self.remaining.len(),
                    self.temporary.len()
                ),
            ));
        }
        let missing: BTreeMap<_, _> = self
            .materials
            .required_supplied
            .iter()
            .filter_map(|(name, required)| {
                let shortage = required.saturating_sub(supplied.get(name).copied().unwrap_or(0));
                (shortage > 0).then_some((name.clone(), shortage))
            })
            .collect();
        if !missing.is_empty() {
            let mut error = ConstructionPlanningError::new(
                SurvivalErrorCode::InsufficientSuppliedMaterials,
                format!("missing without assuming drops: {missing:?}"),
            );
            error.missing_materials = missing;
            return Err(error);
        }
        Ok(self.materials)
    }
}

/// Validate a complete candidate before mutation. `supplied` is a proposed budget,
/// not an inventory receipt. Actual execution must check adoption, permissions,
/// fresh scene/inventory and every live action, including explicit mining recovery.
pub fn preview_construction_sequence(
    scene: &CapturedSurvivalScene,
    site: &ConstructionSite,
    actions: &[ConstructionAction],
    supplied: &BTreeMap<String, usize>,
) -> Result<HypotheticalConstructionPlan> {
    if actions.is_empty() || actions.len() > 512 {
        return Err(ConstructionPlanningError::new(
            SurvivalErrorCode::InvalidSequence,
            "requires 1..512 actions",
        ));
    }
    let mut prefix = CheckedPrefix::new(scene, site)?;
    for action in actions {
        prefix = prefix.after(action)?;
    }
    prefix.finish(supplied)
}

/// Private branchable checker shared by authored candidates and generated search.
/// A failed extension cannot mutate its predecessor or grant execution authority.
#[derive(Clone)]
struct CheckedPrefix<'a> {
    scene: &'a CapturedSurvivalScene,
    ledger: Ledger<'a>,
    scenario: SurvivalScenario,
    steps: Vec<HypotheticalConstructionStep>,
}
impl<'a> CheckedPrefix<'a> {
    fn new(scene: &'a CapturedSurvivalScene, site: &'a ConstructionSite) -> Result<Self> {
        if scene.region().min != xyz(site.scope.observed.min)
            || scene.region().max != xyz(site.scope.observed.max)
        {
            return Err(ConstructionPlanningError::new(
                SurvivalErrorCode::SceneScopeMismatch,
                "capture and declared observation bounds differ",
            ));
        }
        let scenario = scene.scenario_with_motion_contract(SurvivalMotionContract::Predicted);
        let baseline = LiteralSnapshotIndex::new(&site.baseline).expect("checked site baseline");
        for p in cells(Region::new(site.baseline.min, site.baseline.max)) {
            if scenario.block(xyz(p)).map_err(native_error)? != literal(&baseline, p) {
                return Err(ConstructionPlanningError::new(
                    SurvivalErrorCode::SiteBaselineMismatch,
                    "observed cell differs from declared predecessor",
                )
                .at(xyz(p)));
            }
        }
        // Even a sequence without motion must start/end inside the declared body scope.
        let idle = [SurvivalControl {
            yaw: 0.0,
            input: Default::default(),
        }; 3];
        let initial = scenario.preview_path(&idle).map_err(native_error)?;
        if !survival_navigation::hypothetical_admissible(&initial, site.scope.travel) {
            return Err(ConstructionPlanningError::new(
                SurvivalErrorCode::TravelScopeMismatch,
                "initial body is not safely within travel bounds",
            ));
        }
        Ok(Self {
            scene,
            ledger: Ledger::new(site),
            scenario,
            steps: Vec::new(),
        })
    }

    fn after(self, action: &ConstructionAction) -> Result<Self> {
        let index = self.steps.len();
        (|| {
            if index >= 512 {
                return Err(ConstructionPlanningError::new(
                    SurvivalErrorCode::InvalidSequence,
                    "at most 512 actions",
                ));
            }
            let mut next = self;
            let site = next.ledger.site;
            let ledger = &mut next.ledger;
            let mut scenario = next.scenario;
            let step = match action {
                ConstructionAction::Move { controls } => {
                    let prediction = scenario.preview_path(controls).map_err(native_error)?;
                    if !survival_navigation::hypothetical_admissible(&prediction, site.scope.travel)
                    {
                        return Err(ConstructionPlanningError::new(
                            SurvivalErrorCode::UnsafePlannedMotion,
                            "body leaves travel bounds or cannot stop safely",
                        ));
                    }
                    scenario = scenario.after_path(controls).map_err(native_error)?;
                    HypotheticalConstructionStep::Move {
                        prediction: Box::new(prediction),
                    }
                }
                ConstructionAction::Place {
                    purpose,
                    support,
                    face,
                    rotation,
                    material,
                } => {
                    let placement = scenario
                        .preview_cube_placement(*support, *face, *rotation, material)
                        .map_err(native_error)?;
                    ledger.place(*purpose, &placement.edit)?;
                    scenario = scenario
                        .after_edits(std::slice::from_ref(&placement.edit))
                        .map_err(native_error)?;
                    HypotheticalConstructionStep::Place {
                        purpose: *purpose,
                        placement,
                    }
                }
                ConstructionAction::RemoveTemporary {
                    target,
                    face,
                    rotation,
                } => {
                    let edit = scenario
                        .preview_cube_removal(*target, *face, *rotation)
                        .map_err(native_error)?;
                    ledger.remove(&edit)?;
                    scenario = scenario
                        .after_edits(std::slice::from_ref(&edit))
                        .map_err(native_error)?;
                    let (after_reconnect, reconnect) =
                        scenario.after_expected_reconnect().map_err(native_error)?;
                    scenario = after_reconnect;
                    HypotheticalConstructionStep::RemoveTemporary {
                        reconnect,
                        edit,
                        face_id: *face as u8,
                        rotation: *rotation,
                    }
                }
            };
            next.scenario = scenario;
            next.steps.push(step);
            Ok(next)
        })()
        .map_err(|mut error: ConstructionPlanningError| {
            error.action = Some(index);
            error
        })
    }

    fn finish(self, supplied: &BTreeMap<String, usize>) -> Result<HypotheticalConstructionPlan> {
        if self.steps.is_empty()
            && self.ledger.site.initial_temporary.is_empty()
            && !self.ledger.site.structure.is_empty()
        {
            return Err(ConstructionPlanningError::new(
                SurvivalErrorCode::InvalidSequence,
                "requires at least one action",
            ));
        }
        let Self {
            scene,
            ledger,
            scenario,
            steps,
        } = self;
        let site = ledger.site;
        let idle = [SurvivalControl {
            yaw: 0.0,
            input: Default::default(),
        }; 3];
        let materials = ledger.finish(supplied)?;
        let final_position = scenario.position();
        if (0..3).any(|i| {
            final_position[i] < site.scope.retreat.min[i]
                || final_position[i] > site.scope.retreat.max[i]
        }) {
            return Err(ConstructionPlanningError::new(
                SurvivalErrorCode::RetreatNotReached,
                "sequence must end in the declared safe feet volume",
            ));
        }
        let end = scenario.preview_path(&idle).map_err(native_error)?;
        if !survival_navigation::hypothetical_admissible(&end, site.scope.travel) {
            return Err(ConstructionPlanningError::new(
                SurvivalErrorCode::UnsafeFinalStanding,
                "cleanup must preserve safe standing clearance",
            ));
        }
        let expected =
            LiteralSnapshotIndex::new(&site.expected).expect("checked expected snapshot");
        for p in cells(Region::new(site.expected.min, site.expected.max)) {
            if scenario.block(xyz(p)).map_err(native_error)? != literal(&expected, p) {
                return Err(ConstructionPlanningError::new(
                    SurvivalErrorCode::FinalGeometryMismatch,
                    "candidate does not reproduce exact final geometry",
                )
                .at(xyz(p)));
            }
        }
        Ok(HypotheticalConstructionPlan {
            motion_contract: SurvivalMotionContract::Predicted,
            source: scene.source().clone(),
            scope: site.scope.clone(),
            baseline: site.baseline.clone(),
            expected: site.expected.clone(),
            steps,
            materials,
            final_position,
            initial_temporary: site
                .initial_temporary
                .iter()
                .map(|(&position, state)| TemporaryBlock {
                    position,
                    state: state.clone(),
                })
                .collect(),
        })
    }
}

#[cfg(test)]
#[path = "survival_construction/tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "survival_construction/native_roof_trial.rs"]
pub(crate) mod native_roof_trial;

#[path = "survival_construction/generation.rs"]
pub mod generation;

#[path = "survival_construction/continuation.rs"]
pub(crate) mod continuation;
