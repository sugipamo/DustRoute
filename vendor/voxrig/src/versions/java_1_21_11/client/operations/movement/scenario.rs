//! Detached hypothetical scenes. No session, sender, observation or action authority.
use super::*;
use std::collections::BTreeMap;

const MAX_CELLS: usize = 32768;
const MAX_EDITS: usize = 256;
const MAX_TICKS: usize = 4096;

/// Required position evidence for a hypothetical interaction, not an actual
/// receipt. Real interaction admission always reads its own fresh standing basis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HypotheticalAimRequirement {
    /// Error obtained from the original captured standing evidence.
    CapturedPosition {
        /// Per-axis uncertainty about the model's eye coordinates.
        horizontal_error: [f64; 3],
    },
    /// A future new connection must supply a fresh received standing position.
    /// This is a planning obligation, not a received pose or mutation authority.
    ReceivedAfterReconnect,
    /// Future fully dispatched prediction under the explicit model contract.
    /// The reserve is a geometric planning policy, not a measured position error.
    PredictedEndpoint {
        /// Horizontal model-space reserve required for subsequent interaction.
        planning_reserve: [f64; 3],
    },
    /// Future motion must satisfy the same independent observation contract as
    /// actual motion. This variant cannot stand in for that later observation.
    IndependentlyObservedEndpoint {
        /// Maximum per-axis error of the independent position packet.
        max_packet_error: f64,
        /// Maximum discrepancy between predicted and independently seen positions.
        max_prediction_discrepancy: f64,
        /// Sum of the two limits in horizontal axes; floor contact supplies Y.
        horizontal_error: [f64; 3],
    },
}
impl HypotheticalAimRequirement {
    fn after_observed_motion() -> Self {
        Self::IndependentlyObservedEndpoint {
            max_packet_error: endpoint::MAX_PACKET_ERROR,
            max_prediction_discrepancy: endpoint::MAX_DISCREPANCY,
            horizontal_error: [endpoint::MAX_AIM_ERROR, 0.0, endpoint::MAX_AIM_ERROR],
        }
    }
    fn error(self) -> [f64; 3] {
        match self {
            Self::ReceivedAfterReconnect => [0.0; 3],
            Self::PredictedEndpoint { planning_reserve } => planning_reserve,
            Self::CapturedPosition { horizontal_error }
            | Self::IndependentlyObservedEndpoint {
                horizontal_error, ..
            } => horizontal_error,
        }
    }
    /// Compare this prospective obligation with an actual freshly checked basis.
    /// This grants no authority and cannot replace native standing admission.
    pub fn validate_standing(&self, standing: &StandingContext) -> Result<()> {
        let basis = &standing.position_basis;
        let kind_matches = match self {
            Self::ReceivedAfterReconnect => matches!(basis, StandingPositionBasis::Received { .. }),
            Self::CapturedPosition { .. } => {
                !matches!(basis, StandingPositionBasis::Predicted { .. })
            }
            Self::PredictedEndpoint { .. } => {
                matches!(basis, StandingPositionBasis::Predicted { .. })
            }
            Self::IndependentlyObservedEndpoint {
                max_packet_error,
                max_prediction_discrepancy,
                ..
            } => {
                if let StandingPositionBasis::PredictedAndObserved {
                    predicted,
                    observed,
                    ..
                } = basis
                {
                    (0..3).all(|i| {
                        observed.motion.position_error[i] <= *max_packet_error
                            && (predicted.position[i] - observed.position[i]).abs()
                                <= *max_prediction_discrepancy
                    })
                } else {
                    false
                }
            }
        };
        let reserve = self.error();
        if !kind_matches
            || (0..3).any(|i| {
                !reserve[i].is_finite()
                    || reserve[i] < 0.0
                    || basis.geometry_reserve()[i] > reserve[i]
            })
        {
            return Err(invalid(
                "actual standing basis differs from the hypothetical evidence contract",
            ));
        }
        Ok(())
    }
}

/// A future connection-reset obligation. No reconnect, receipt or action token
/// is created by this value; callers still perform and verify their own lifecycle.
#[derive(Clone, Debug, Serialize)]
pub struct HypotheticalReconnectBoundary {
    expected_position: [f64; 3],
    dimension: String,
}
impl HypotheticalReconnectBoundary {
    /// Compare an actually captured new-connection standing scene with the
    /// planned reset. The caller supplies the actual retired connection identity
    /// and separately proves retirement, world contents and capture freshness.
    /// Success is a comparison only, never authority to start a native operation.
    pub fn validate_received_start(
        &self,
        fresh: &CapturedSurvivalScene,
        retired_connection_id: u64,
    ) -> Result<()> {
        let now = fresh.source();
        if now.connection_id == retired_connection_id
            || now.dimension != self.dimension
            || now.position != self.expected_position
            || !matches!(now.position_basis, StandingPositionBasis::Received { .. })
        {
            return Err(invalid(
                "received reconnect start differs from hypothetical boundary",
            ));
        }
        validate_initial(now)?;
        Ok(())
    }
}

/// Immutable complete native scene, captured atomically with standing parameters.
/// This may seed hypothetical planning; it does not authorize future execution.
#[derive(Clone, Debug)]
pub struct CapturedSurvivalScene {
    initial: StandingContext,
    generation: u64,
    region: crate::Region,
    blocks: Arc<BTreeMap<[i32; 3], crate::NativeBlockState>>,
}
/// Explicit hypothetical predecessor and successor; neither is a live write.
#[derive(Clone, Debug, Serialize)]
pub struct HypotheticalBlockEdit {
    /// Cell inside the captured scene.
    pub position: [i32; 3],
    /// Exact required scenario state before this edit.
    pub before: crate::NativeBlockState,
    /// Admitted passive cube or air; no dynamic callback is assumed.
    pub after: crate::NativeBlockState,
}
/// Detached branch of a captured scene; no deserialization/action authority.
#[derive(Clone, Debug)]
pub struct SurvivalScenario {
    scene: CapturedSurvivalScene,
    model: Model,
    edits: usize,
    ticks: usize,
    clearance_error: [f64; 3],
    aim_requirement: HypotheticalAimRequirement,
    motion_contract: SurvivalMotionContract,
    origin: Arc<()>,
}
/// Future geometry prediction, deliberately incompatible with live input APIs.
/// ```compile_fail
/// use voxrig::versions::java_1_21_11::operations::{Operations, HypotheticalMovementPreview};
/// async fn cannot_execute(api: &Operations, observer: &Operations, future: &HypotheticalMovementPreview) {
///     api.start_previewed_survival_motion(future, observer).await.unwrap();
/// }
/// ```
#[derive(Clone, Debug, Serialize)]
pub struct HypotheticalMovementPreview {
    #[serde(skip)]
    origin: Arc<()>,
    /// Original capture provenance, not the hypothetical player's current pose.
    pub source: StandingContext,
    /// Initial model frame (tick zero), including the native velocity phase.
    pub initial_frame: PredictedMotionFrame,
    /// Hypothetical starting feet; never a received StandingContext.
    pub initial_position: [f64; 3],
    /// Native standing bounds at the hypothetical start, including uncertainty.
    pub initial_bounds: [f64; 6],
    /// Prospective evidence required at the start; never an actual new receipt.
    pub initial_aim_requirement: HypotheticalAimRequirement,
    /// Required endpoint evidence contract, distinct from the predicted frames.
    pub endpoint_contract: SurvivalMotionContract,
    /// Number of preceding hypothetical cell edits.
    pub preceding_edits: usize,
    /// Exact inputs used by the shared native model.
    pub controls: Vec<SurvivalControl>,
    /// Frames against this branch's geometry.
    pub frames: Vec<PredictedMotionFrame>,
    /// Same conservative stop check as a live preview.
    pub terminal_clearance: TerminalClearance,
}
impl HypotheticalMovementPreview {
    /// Whether both predictions start from the identical immutable scenario.
    pub fn shares_origin(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.origin, &other.origin)
    }
}
/// Geometric possibility only: no material reservation, action sequence or receipt.
#[derive(Clone, Debug, Serialize)]
pub struct HypotheticalPlacement {
    /// Exact proposed cell change, usable only in a detached scenario.
    pub edit: HypotheticalBlockEdit,
    /// Requested supporting cell.
    pub support: [i32; 3],
    /// Native face ID.
    pub face_id: u8,
    /// Native yaw/pitch used for visibility/reach checks.
    pub rotation: [f32; 2],
    /// Native face cursor.
    pub cursor: [f32; 3],
    /// Required position evidence for this hypothetical target/face/cursor.
    pub aim_requirement: HypotheticalAimRequirement,
}
impl GeometryView for CapturedSurvivalScene {
    fn block(&self, p: [i32; 3]) -> Result<crate::NativeBlockState> {
        self.blocks
            .get(&p)
            .cloned()
            .ok_or_else(|| invalid("geometry lies outside the complete captured scene"))
    }
}
fn admitted(block: &crate::NativeBlockState) -> Result<()> {
    super::super::super::super::state_id(block)?;
    if !matches!(
        block.name.as_str(),
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
    ) && !survival::DRY_CUBES.contains(&block.name.as_str())
    {
        return Err(invalid(
            "hypothetical scenes require admitted dry cubes and air",
        ));
    }
    Ok(())
}
fn capture(
    state: &mut State,
    id: u64,
    tick: u64,
    region: crate::Region,
) -> Result<CapturedSurvivalScene> {
    let mut volume = 1usize;
    for i in 0..3 {
        let side = i64::from(region.max[i]) - i64::from(region.min[i]) + 1;
        if !(1..=64).contains(&side) || region.min[i] < -29_999_984 || region.max[i] > 29_999_984 {
            return Err(invalid(
                "capture requires ordered bounded coordinates and axes at most 64 cells",
            ));
        }
        volume = volume
            .checked_mul(side as usize)
            .filter(|n| *n <= MAX_CELLS)
            .ok_or_else(|| invalid("capture exceeds 32768 cells"))?;
    }
    if state.operations.game_mode != Some(GameMode::Survival) {
        return Err(invalid("capture requires survival mode"));
    }
    let initial = survival::context(state, id, tick)?;
    validate_initial(&initial)?;
    let mut blocks = BTreeMap::new();
    for x in region.min[0]..=region.max[0] {
        for y in region.min[1]..=region.max[1] {
            for z in region.min[2]..=region.max[2] {
                let p = [x, y, z];
                let block = state.block(p)?;
                admitted(&block)?;
                blocks.insert(p, block);
            }
        }
    }
    let scene = CapturedSurvivalScene {
        initial,
        generation: state.loading.generation,
        region,
        blocks: Arc::new(blocks),
    };
    // Require the initial standing halo as well as the caller's visible cells.
    survival::standing_geometry(
        &scene,
        scene.initial.position,
        scene.initial.position_basis.geometry_reserve(),
    )?;
    Ok(scene)
}
impl Operations {
    /// Capture complete static geometry and native starting parameters without I/O.
    pub async fn capture_survival_scene(
        &self,
        region: crate::Region,
    ) -> Result<CapturedSurvivalScene> {
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        capture(
            &mut state,
            self.bot.session.id,
            self.bot.session.started.elapsed().as_millis() as u64 / 50,
            region,
        )
    }
    /// Check original capture provenance against current standing/world state.
    /// This is read-only and does not promote any hypothetical result to authority.
    pub async fn validate_survival_scene(&self, scene: &CapturedSurvivalScene) -> Result<()> {
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        if state.operations.game_mode != Some(GameMode::Survival) {
            return Err(invalid("captured scene requires current survival mode"));
        }
        let now = survival::context(
            &mut state,
            self.bot.session.id,
            self.bot.session.started.elapsed().as_millis() as u64 / 50,
        )?;
        let old = &scene.initial;
        if scene.generation != state.loading.generation
            || now.connection_id != old.connection_id
            || now.world_revision != old.world_revision
            || now.dimension != old.dimension
            || now.position != old.position
            || now.bounds != old.bounds
            || now.player != old.player
            || Model::from_context(&now).frame.velocity != Model::from_context(old).frame.velocity
        {
            return Err(invalid("captured scene is stale; capture and plan again"));
        }
        Ok(())
    }
}
impl CapturedSurvivalScene {
    /// Original immutable received/observed starting provenance.
    pub fn source(&self) -> &StandingContext {
        &self.initial
    }
    /// Complete captured cell bounds, including geometry halos.
    pub fn region(&self) -> crate::Region {
        self.region
    }
    /// Start a detached branch, with the native model's original velocity.
    pub fn scenario(&self) -> SurvivalScenario {
        let contract = if matches!(
            self.initial.position_basis,
            StandingPositionBasis::Predicted { .. }
        ) {
            SurvivalMotionContract::Predicted
        } else {
            SurvivalMotionContract::IndependentlyObserved
        };
        self.scenario_with_motion_contract(contract)
    }
    /// Declare future endpoint evidence without inventing any actual receipt.
    pub fn scenario_with_motion_contract(
        &self,
        motion_contract: SurvivalMotionContract,
    ) -> SurvivalScenario {
        SurvivalScenario {
            scene: self.clone(),
            model: Model::from_context(&self.initial),
            edits: 0,
            ticks: 0,
            clearance_error: self.initial.position_basis.geometry_reserve(),
            aim_requirement: match self.initial.position_basis {
                StandingPositionBasis::Predicted {
                    planning_reserve, ..
                } => HypotheticalAimRequirement::PredictedEndpoint { planning_reserve },
                _ => HypotheticalAimRequirement::CapturedPosition {
                    horizontal_error: self.initial.position_basis.geometry_reserve(),
                },
            },
            motion_contract,
            origin: Arc::new(()),
        }
    }
}
impl SurvivalScenario {
    /// Check a hypothetical removal's native hit and retained foot support.
    /// Dirt/stone match current real mining admission. This proves no timing,
    /// empty-hand inventory, item recovery or safe continuation after real mining.
    pub fn preview_cube_removal(
        &self,
        target: [i32; 3],
        face: crate::BlockFace,
        rotation: [f32; 2],
    ) -> Result<HypotheticalBlockEdit> {
        validate_pose(self.position(), rotation)?;
        let p = self.position();
        let eye = [p[0], p[1] + f64::from(1.62f32), p[2]];
        let hit = super::super::super::raycast::outline_hit_in(eye, rotation, 4.5, |p| {
            self.scene.block(p).map_err(anyhow::Error::from)
        })?
        .ok_or_else(|| invalid("hypothetical removal has no native target hit"))?;
        survival::uncertain_target_in(
            &self.scene,
            eye,
            self.aim_requirement.error(),
            rotation,
            &hit,
        )?;
        if hit.position != target || hit.face.map(|f| f as u8) != Some(face as u8) {
            return Err(invalid(
                "hypothetical removal differs from first native target face",
            ));
        }
        super::super::mining::admitted_material(&hit.state)?;
        let edit = HypotheticalBlockEdit {
            position: target,
            before: hit.state,
            after: crate::NativeBlockState {
                name: "minecraft:air".into(),
                properties: Default::default(),
            },
        };
        self.after_edits(std::slice::from_ref(&edit))?;
        Ok(edit)
    }
    /// Whether the preview was produced from this exact immutable scenario.
    /// Serialized diagnostics cannot recreate this in-memory identity.
    pub fn matches_preview(&self, preview: &HypotheticalMovementPreview) -> bool {
        Arc::ptr_eq(&self.origin, &preview.origin)
    }
    /// Current hypothetical feet position; not a received standing observation.
    pub fn position(&self) -> [f64; 3] {
        self.model.frame.position
    }
    /// Read-only requirement for subsequent hypothetical placement or removal.
    /// It is deliberately distinct from `StandingPositionBasis` observations.
    pub fn aim_requirement(&self) -> HypotheticalAimRequirement {
        self.aim_requirement
    }
    /// Current hypothetical native state; missing geometry refuses.
    pub fn block(&self, p: [i32; 3]) -> Result<crate::NativeBlockState> {
        self.scene.block(p)
    }
    /// Predict without modifying this branch; uses the same model as live preview.
    pub fn preview_path(
        &self,
        controls: &[SurvivalControl],
    ) -> Result<HypotheticalMovementPreview> {
        let (_, result) = self.advance(controls)?;
        Ok(result)
    }
    fn advance(
        &self,
        controls: &[SurvivalControl],
    ) -> Result<(Model, HypotheticalMovementPreview)> {
        if self.ticks.saturating_add(controls.len()) > MAX_TICKS {
            return Err(invalid("scenario exceeds 4096 motion ticks"));
        }
        let mut model = self.model.clone();
        let initial_frame = model.initial_frame();
        let frames = predict(&self.scene, &mut model, controls)?;
        let result = HypotheticalMovementPreview {
            origin: self.origin.clone(),
            source: self.scene.initial.clone(),
            initial_frame,
            initial_position: self.position(),
            initial_bounds: survival::standing_geometry(
                &self.scene,
                self.position(),
                self.clearance_error,
            )?
            .bounds,
            initial_aim_requirement: self.aim_requirement,
            endpoint_contract: self.motion_contract,
            preceding_edits: self.edits,
            controls: controls.to_vec(),
            terminal_clearance: clearance(&self.scene, frames.last().unwrap()),
            frames,
        };
        Ok((model, result))
    }
    /// Fork at a newly predicted safe stop. No live player state is modified.
    pub fn after_path(&self, controls: &[SurvivalControl]) -> Result<Self> {
        let (model, result) = self.advance(controls)?;
        if !matches!(
            result.terminal_clearance,
            TerminalClearance::Admitted { .. }
        ) {
            return Err(invalid(
                "hypothetical path must end with terminal clearance",
            ));
        }
        Ok(Self {
            origin: Arc::new(()),
            model,
            ticks: self.ticks + controls.len(),
            clearance_error: [TERMINAL_MARGIN, 0.0, TERMINAL_MARGIN],
            aim_requirement: match self.motion_contract {
                SurvivalMotionContract::IndependentlyObserved => {
                    HypotheticalAimRequirement::after_observed_motion()
                }
                SurvivalMotionContract::Predicted => {
                    HypotheticalAimRequirement::PredictedEndpoint {
                        planning_reserve: [TERMINAL_MARGIN, 0.0, TERMINAL_MARGIN],
                    }
                }
            },
            ..self.clone()
        })
    }
    /// Fork with the native model's new-connection initialization at these feet.
    /// This explicitly plans a lifecycle boundary; it does not perform a reset,
    /// establish received evidence or turn a future scene into live authority.
    /// The returned obligation must be checked against an actual fresh capture.
    pub fn after_expected_reconnect(&self) -> Result<(Self, HypotheticalReconnectBoundary)> {
        terminal_clearance(&self.scene, &self.model.frame)?;
        let boundary = HypotheticalReconnectBoundary {
            expected_position: self.position(),
            dimension: self.scene.initial.dimension.clone(),
        };
        let next = Self {
            model: Model::new(self.position()),
            origin: Arc::new(()),
            aim_requirement: HypotheticalAimRequirement::ReceivedAfterReconnect,
            // A future exact receipt is not permission to shrink standing margins.
            clearance_error: [TERMINAL_MARGIN, 0.0, TERMINAL_MARGIN],
            ..self.clone()
        };
        Ok((next, boundary))
    }
    /// Fork with explicit edits. A batch is atomic and cannot remove current foot
    /// support, intersect the player or change cells outside the captured scene.
    /// This does not establish mining timing, reach, materials or edit permissions.
    pub fn after_edits(&self, edits: &[HypotheticalBlockEdit]) -> Result<Self> {
        if edits.is_empty() || self.edits.saturating_add(edits.len()) > MAX_EDITS {
            return Err(invalid(
                "scenario requires edits within the 256-edit lifetime budget",
            ));
        }
        let mut next = self.clone();
        next.origin = Arc::new(());
        let mut seen = std::collections::BTreeSet::new();
        for edit in edits {
            if !seen.insert(edit.position) || self.scene.block(edit.position)? != edit.before {
                return Err(invalid(
                    "hypothetical edit has a duplicate or mismatched predecessor",
                ));
            }
            admitted(&edit.after)?;
            // Grass is observed ground, not an admitted synthesized placement.
            if edit.after.name == "minecraft:grass_block" && edit.before != edit.after {
                return Err(invalid("stateful grass placement is not modeled"));
            }
            Arc::make_mut(&mut next.scene.blocks).insert(edit.position, edit.after.clone());
        }
        let standing =
            survival::standing_geometry(&next.scene, self.position(), self.clearance_error)?;
        if standing.support.is_empty() {
            return Err(invalid("hypothetical edit removes player support"));
        }
        next.edits += edits.len();
        Ok(next)
    }
    /// Native face/reach/body check against this future geometry. Inventory and
    /// actual result evidence remain exclusively the real placement API's work.
    pub fn preview_cube_placement(
        &self,
        support: [i32; 3],
        face: crate::BlockFace,
        rotation: [f32; 2],
        material: &str,
    ) -> Result<HypotheticalPlacement> {
        let item = default_item(material, 1)?;
        let after = crate::NativeBlockState {
            name: item.name,
            properties: Default::default(),
        };
        admitted(&after)?;
        if !survival::DRY_CUBES.contains(&after.name.as_str())
            || after.name == "minecraft:grass_block"
        {
            return Err(invalid("placement requires an admitted passive cube"));
        }
        let standing =
            survival::standing_geometry(&self.scene, self.position(), self.clearance_error)?;
        if standing.support.is_empty() {
            return Err(invalid("hypothetical placement requires standing support"));
        }
        let g = super::super::placement::placement_geometry(
            &self.scene,
            self.position(),
            standing.bounds,
            self.aim_requirement.error(),
            rotation,
            support,
            face as u8,
        )?;
        Ok(HypotheticalPlacement {
            edit: HypotheticalBlockEdit {
                position: g.target,
                before: g.before,
                after,
            },
            support,
            face_id: face as u8,
            rotation,
            cursor: g.cursor,
            aim_requirement: self.aim_requirement,
        })
    }
}
