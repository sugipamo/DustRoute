//! Detached hypothetical scenes. No session, sender, observation or action authority.
use super::*;
use std::collections::BTreeMap;

const MAX_CELLS: usize = 32768;
const MAX_EDITS: usize = 256;
const MAX_TICKS: usize = 4096;

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
    error: [f64; 3],
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
    /// Hypothetical starting feet; never a received StandingContext.
    pub initial_position: [f64; 3],
    /// Native standing bounds at the hypothetical start, including uncertainty.
    pub initial_bounds: [f64; 6],
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
        scene.initial.position_basis.horizontal_error(),
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
        SurvivalScenario {
            scene: self.clone(),
            model: Model::from_context(&self.initial),
            edits: 0,
            ticks: 0,
            error: self.initial.position_basis.horizontal_error(),
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
        survival::uncertain_target_in(&self.scene, eye, self.error, rotation, &hit)?;
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
        let frames = predict(&self.scene, &mut model, controls)?;
        let result = HypotheticalMovementPreview {
            origin: self.origin.clone(),
            source: self.scene.initial.clone(),
            initial_position: self.position(),
            initial_bounds: survival::standing_geometry(&self.scene, self.position(), self.error)?
                .bounds,
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
            error: [TERMINAL_MARGIN, 0.0, TERMINAL_MARGIN],
            ..self.clone()
        })
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
        let standing = survival::standing_geometry(&next.scene, self.position(), self.error)?;
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
        let standing = survival::standing_geometry(&self.scene, self.position(), self.error)?;
        if standing.support.is_empty() {
            return Err(invalid("hypothetical placement requires standing support"));
        }
        let g = super::super::placement::placement_geometry(
            &self.scene,
            self.position(),
            standing.bounds,
            self.error,
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
        })
    }
}
