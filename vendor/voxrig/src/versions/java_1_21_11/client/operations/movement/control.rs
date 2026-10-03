//! Connection-owned bounded controls. Dropping a caller's wait does not drop input.
use super::*;
use crate::versions::java_1_21_11::client::players::{
    ObservedPlayer, PlayerMotionStatus, PlayerMotionWatch,
};
use std::time::Duration;

/// How a bounded dry-cube endpoint may be used by subsequent checked operations.
/// Neither contract is a server acknowledgement that motion has stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurvivalMotionContract {
    /// Requires a fresh same-instance position from a distinct client.
    IndependentlyObserved,
    /// Uses fully dispatched controls, the model and currently received geometry.
    /// No independent position or actual server-position error bound is available.
    Predicted,
}

/// No phase means server-confirmed stopped motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurvivalMotionStatus {
    /// The bounded input sequence is being dispatched at native tick spacing.
    Running,
    /// Locally settled; awaiting a same-instance position observation.
    AwaitingObservation,
    /// Prediction and observation agree, subject to fresh standing geometry checks.
    Observed,
    /// Fully dispatched and locally settled under the explicit prediction contract.
    /// Fresh native standing/geometry checks are still required before interaction.
    Predicted,
    /// Failure, correction, changed context or missing observation. Never auto-replay.
    RequiresInspection,
}
impl SurvivalMotionStatus {
    /// A candidate for fresh standing admission, not authority by itself.
    pub fn is_continuation_candidate(self) -> bool {
        matches!(self, Self::Observed | Self::Predicted)
    }
}
/// Diagnostic control record retained before the first packet; cannot be imported.
#[derive(Clone, Debug, Serialize)]
pub struct SurvivalMotionRecord {
    /// Connection-local monotonically increasing run identity.
    pub run_id: u64,
    /// Owning connection, distinct from the observer.
    pub connection_id: u64,
    /// Owning native world generation.
    pub generation: u64,
    /// Prediction made before starting the run, against the then-received geometry.
    pub preview: SurvivalMovementPreview,
    /// Last tick whose input and position frames were fully dispatched.
    pub dispatched_ticks: u16,
    /// Before-I/O tick intent; may exceed dispatched_ticks after interruption.
    pub attempted_tick: u16,
    /// Current phase; elapsed time alone cannot produce Observed.
    pub status: SurvivalMotionStatus,
    /// Declared endpoint evidence contract; never inferred from missing packets.
    pub contract: SurvivalMotionContract,
    /// Independent observer identity.
    pub observer_connection_id: Option<u64>,
    /// Initial exact target lifetime watch.
    pub initial_watch: Option<PlayerMotionWatch>,
    /// New position boundary after the final dispatch; not a server-time fence.
    pub final_watch: Option<PlayerMotionWatch>,
    /// Exact same-instance observation used for continuation.
    pub observed: Option<ObservedPlayer>,
    /// Retained failure; further controls require explicit investigation.
    pub problem: Option<String>,
    /// Latest explicit observation-only reassessment; original problem is retained.
    pub recheck: Option<SurvivalMotionRecheck>,
    // Live in-process guards; diagnostic JSON can never restore these.
    #[serde(skip)]
    received_pose_sequence: u64,
    #[serde(skip)]
    observer_session: Option<Weak<Session>>,
}
/// Common standing provenance. Packet velocity and model velocity stay distinct.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StandingPositionBasis {
    /// Own native position packet with zero resolved packet velocity.
    Received {
        /// Position packet ordinal; no server-rest acknowledgement is implied.
        receive_sequence: u64,
    },
    /// A fully dispatched dry-cube prediction, without independent corroboration.
    Predicted {
        /// Connection-local run identity.
        run_id: u64,
        /// Simulated rest and next-tick gravity velocity.
        predicted: PredictedMotionFrame,
        /// Last own pose receipt before this run, not a receipt of its endpoint.
        received_pose_sequence: u64,
        /// Model-space construction reserve, not a measured physical error bound.
        planning_reserve: [f64; 3],
    },
    /// Locally settled dry-cube model corroborated by a later observer position.
    PredictedAndObserved {
        /// Connection-local run.
        run_id: u64,
        /// Simulated rest and next-tick gravity velocity.
        predicted: PredictedMotionFrame,
        /// Independent observer connection.
        observer_connection_id: u64,
        /// Full position, quantization, identity and ground/velocity receipt evidence.
        observed: Box<ObservedPlayer>,
    },
}
impl StandingPositionBasis {
    pub(in super::super::super) fn geometry_reserve(&self) -> [f64; 3] {
        match self {
            Self::Received { .. } => [0.0; 3],
            Self::Predicted {
                planning_reserve, ..
            } => *planning_reserve,
            Self::PredictedAndObserved {
                predicted,
                observed,
                ..
            } => std::array::from_fn(|i| {
                if i == 1 {
                    0.0
                } else {
                    observed.motion.position_error[i]
                        + (observed.position[i] - predicted.position[i]).abs()
                }
            }),
        }
    }
}
fn stable_context(state: &State, r: &SurvivalMotionRecord) -> Result<()> {
    let before = &r.preview.initial.player;
    let now = &state.operations.local_player;
    if state.loading.generation != r.generation
        || state.world.dimension.as_ref().map(|d| &d.0) != Some(&r.preview.initial.dimension)
        || state
            .motion
            .received_pose
            .as_ref()
            .map(|p| p.receive_sequence)
            != Some(r.received_pose_sequence)
        || state.operations.game_mode != Some(GameMode::Survival)
        || state.operations.requested_flying
        || state.operations.abilities.is_some_and(|a| a & 2 != 0)
        || now.velocity != before.velocity
        || now.pose != before.pose
        || now.scale != before.scale
        || now.movement_speed != before.movement_speed
        || now.gravity != before.gravity
        || now.jump_strength != before.jump_strength
        || now.step_height != before.step_height
        || now.movement_efficiency != before.movement_efficiency
        || now.motion_interruption.is_some()
        || !now.effect_updates.is_empty()
        || now.health.as_ref().is_some_and(|h| h.health <= 0.0)
    {
        return Err(invalid(
            "survival motion context changed; correction/impulse/world/posture/attributes require replanning",
        ));
    }
    Ok(())
}
pub(in super::super::super) fn standing_basis(state: &State) -> Result<StandingPositionBasis> {
    if let Some(r) = &state.survival_motion {
        if !r.status.is_continuation_candidate() {
            return Err(invalid(
                "survival motion has no settled continuation candidate; inspect retained run",
            ));
        }
        if r.status == SurvivalMotionStatus::Predicted {
            return predicted_basis(state, r);
        }
        let watch = r
            .final_watch
            .as_ref()
            .ok_or_else(|| invalid("motion observation watch missing"))?;
        return observed_basis(state, r, watch);
    }

    if state
        .motion
        .received_position(state.loading.generation, state.position)
        && state.operations.local_player.velocity.map(|v| v.value) == Some([0.0; 3])
    {
        Ok(StandingPositionBasis::Received {
            receive_sequence: state
                .motion
                .received_pose
                .as_ref()
                .unwrap()
                .receive_sequence,
        })
    } else {
        Err(invalid(
            "stationary context requires a received zero-velocity pose or a settled declared motion contract",
        ))
    }
}
fn dispatched_rest(state: &State, r: &SurvivalMotionRecord) -> Result<()> {
    stable_context(state, r)?;
    let predicted = r.preview.frames.last().expect("nonempty validated run");
    if state.motion.position_basis != PositionBasis::Submitted
        || state.position != Some(predicted.position)
        || !predicted.resting
        || r.dispatched_ticks != predicted.tick
        || r.attempted_tick != predicted.tick
        || state.motion.last_submission.as_ref().is_none_or(|s| {
            !s.dispatched
                || s.superseded_at.is_some()
                || s.generation != r.generation
                || s.position != predicted.position
        })
    {
        return Err(invalid(
            "survival motion result superseded or not fully dispatched",
        ));
    }
    Ok(())
}
fn predicted_basis(state: &State, r: &SurvivalMotionRecord) -> Result<StandingPositionBasis> {
    if r.contract != SurvivalMotionContract::Predicted {
        return Err(invalid(
            "independent motion cannot become prediction-only continuation",
        ));
    }
    dispatched_rest(state, r)?;
    Ok(StandingPositionBasis::Predicted {
        run_id: r.run_id,
        predicted: r.preview.frames.last().unwrap().clone(),
        received_pose_sequence: r.received_pose_sequence,
        planning_reserve: [TERMINAL_MARGIN, 0.0, TERMINAL_MARGIN],
    })
}
fn observed_basis(
    state: &State,
    r: &SurvivalMotionRecord,
    watch: &PlayerMotionWatch,
) -> Result<StandingPositionBasis> {
    if r.contract != SurvivalMotionContract::IndependentlyObserved {
        return Err(invalid(
            "predicted motion has no independent observation obligation",
        ));
    }
    dispatched_rest(state, r)?;
    let predicted = r.preview.frames.last().expect("nonempty validated run");
    // Nonblocking second lock: reciprocal observers cannot deadlock, and a
    // busy/closed/reconfigured observer does not silently keep authority.
    let observer = r
        .observer_session
        .as_ref()
        .and_then(Weak::upgrade)
        .ok_or_else(|| invalid("motion observer closed"))?;
    observer.check_outbound()?;
    let observed_state = observer.state.try_lock().map_err(|_| {
        invalid("motion observer is busy; read standing context again before mutation")
    })?;
    observer.check(&observed_state)?;

    if matches!(
        super::super::super::players::evaluate_motion(
            &observed_state,
            r.initial_watch
                .as_ref()
                .ok_or_else(|| invalid("initial observer watch missing"))?
        ),
        PlayerMotionStatus::RequiresInspection { .. }
    ) {
        return Err(invalid("original motion observer lifetime changed"));
    }
    let super::super::super::players::PlayerMotionStatus::PositionUpdated { player: current } =
        super::super::super::players::evaluate_motion(&observed_state, watch)
    else {
        return Err(invalid("motion observer world/target lifetime changed"));
    };
    if !matches_endpoint(r, &current) {
        return Err(invalid(
            "motion observer position/posture conflicts with settled prediction",
        ));
    }
    Ok(StandingPositionBasis::PredictedAndObserved {
        run_id: r.run_id,
        predicted: predicted.clone(),
        observer_connection_id: r
            .observer_connection_id
            .ok_or_else(|| invalid("observer identity missing"))?,
        observed: Box::new(current),
    })
}

impl Operations {
    /// Start one finite dry-cube walk/jump sequence which must end in released,
    /// predicted rest. A connection-owned task completes it even if the caller
    /// drops its wait. No route search, sprint, sneak, fluid or entity collisions.
    /// Requires a distinct observer on the same direct vanilla endpoint. Missing
    /// observation or changed conditions latch inspection; no automatic replay.
    pub async fn start_survival_motion(
        &self,
        yaw: f32,
        inputs: &[SurvivalInput],
        observer: &Operations,
    ) -> Result<SurvivalMotionRecord> {
        self.start_control_path(&fixed_controls(yaw, inputs)?, Some(observer), None)
            .await
    }
    /// Start a caller-selected multi-heading path, revalidating current geometry.
    pub async fn start_survival_path(
        &self,
        controls: &[SurvivalControl],
        observer: &Operations,
    ) -> Result<SurvivalMotionRecord> {
        self.start_control_path(controls, Some(observer), None)
            .await
    }
    /// Recompute an earlier preview under the send-intent lock. Changed initial
    /// context, generation, world revision or predicted frames refuse before I/O.
    /// A supplied preview is a constraint, never imported action authority.
    pub async fn start_previewed_survival_motion(
        &self,
        expected: &SurvivalMovementPreview,
        observer: &Operations,
    ) -> Result<SurvivalMotionRecord> {
        self.start_control_path(&expected.controls, Some(observer), Some(expected))
            .await
    }
    /// Explicit model-based continuation. No observer, receipt or error bound is
    /// substituted; corrected, interrupted or unsupported motion requires inspection.
    pub async fn start_predicted_survival_path(
        &self,
        controls: &[SurvivalControl],
    ) -> Result<SurvivalMotionRecord> {
        self.start_control_path(controls, None, None).await
    }
    /// Revalidate a preview and dispatch under the prediction-only contract.
    pub async fn start_previewed_predicted_survival_motion(
        &self,
        expected: &SurvivalMovementPreview,
    ) -> Result<SurvivalMotionRecord> {
        self.start_control_path(&expected.controls, None, Some(expected))
            .await
    }
    async fn start_control_path(
        &self,
        controls: &[SurvivalControl],
        observer: Option<&Operations>,
        expected: Option<&SurvivalMovementPreview>,
    ) -> Result<SurvivalMotionRecord> {
        if observer.is_some_and(|o| self.bot.session.id == o.bot.session.id) {
            return Err(invalid("motion requires an independent observer"));
        }
        let identity = {
            let state = self.bot.session.state.lock().await;
            self.mutable(&state)?;
            state
                .identity
                .clone()
                .ok_or_else(|| invalid("motion login identity unavailable"))?
        };
        let (observer_dimension, initial_watch) = if let Some(observer) = observer {
            let dimension = {
                let state = observer.bot.session.state.lock().await;
                observer.ready(&state)?;
                let other = state
                    .identity
                    .as_ref()
                    .ok_or_else(|| invalid("observer identity unavailable"))?;
                if other.server != identity.server
                    || other.uuid == identity.uuid
                    || state.players.profile_name(&identity.uuid) != Some(identity.name.as_str())
                {
                    return Err(invalid(
                        "motion observer must know the exact profile on the same endpoint",
                    ));
                }
                state
                    .world
                    .dimension
                    .as_ref()
                    .map(|d| d.0.clone())
                    .ok_or_else(|| invalid("observer dimension unavailable"))?
            };
            (
                Some(dimension),
                Some(observer.watch_player_motion(identity.uuid).await?),
            )
        } else {
            (None, None)
        };
        let mut state = self.bot.session.state.lock().await;
        self.mutable(&state)?;
        if state.operations.inventory.pending_swap.is_some() {
            return Err(invalid("inventory swap unresolved"));
        }
        let tick = self.bot.session.started.elapsed().as_millis() as u64 / 50;
        let preview = preview(&mut state, self.bot.session.id, tick, controls)?;
        if let Some(expected) = expected {
            if expected.generation != preview.generation
                || expected.initial.connection_id != preview.initial.connection_id
                || expected.initial.world_revision != preview.initial.world_revision
                || expected.initial.dimension != preview.initial.dimension
                || expected.initial.position != preview.initial.position
                || expected.initial.player != preview.initial.player
                || expected.frames != preview.frames
            {
                return Err(invalid(
                    "motion preview changed; replan before submitting controls",
                ));
            }
        }
        if observer_dimension
            .as_ref()
            .is_some_and(|d| d != &preview.initial.dimension)
        {
            return Err(invalid("observer dimension differs"));
        }
        if !preview.frames.last().is_some_and(|f| f.resting) {
            return Err(invalid("motion inputs must end in predicted released rest"));
        }
        terminal_clearance(&*state, preview.frames.last().unwrap())?;
        let run_id = state
            .survival_motion
            .as_ref()
            .map_or(Some(1), |r| r.run_id.checked_add(1))
            .ok_or_else(|| invalid("motion run IDs exhausted"))?;
        let record = SurvivalMotionRecord {
            run_id,
            connection_id: self.bot.session.id,
            generation: state.loading.generation,
            preview,
            dispatched_ticks: 0,
            attempted_tick: 0,
            status: SurvivalMotionStatus::Running,
            contract: if observer.is_some() {
                SurvivalMotionContract::IndependentlyObserved
            } else {
                SurvivalMotionContract::Predicted
            },
            observer_connection_id: observer.map(|o| o.bot.session.id),
            initial_watch,
            final_watch: None,
            observed: None,
            problem: None,
            recheck: None,
            observer_session: observer.map(|o| Arc::downgrade(&o.bot.session)),
            received_pose_sequence: state
                .motion
                .received_pose
                .as_ref()
                .unwrap()
                .receive_sequence,
        };
        state.survival_motion = Some(record.clone());
        let owner = self.clone();
        let observer = observer.cloned();
        let running = record.clone();
        // No await between retaining the intent and spawning its finite owner.
        tokio::spawn(async move {
            if let Err(error) = owner
                .run_survival_motion(&running, observer.as_ref(), identity.uuid)
                .await
            {
                let mut state = owner.bot.session.state.lock().await;
                if let Some(r) = state
                    .survival_motion
                    .as_mut()
                    .filter(|r| r.run_id == running.run_id)
                {
                    r.status = SurvivalMotionStatus::RequiresInspection;
                    r.problem = Some(error.to_string());
                }
                owner.bot.session.changed.notify_waiters();
            }
        });
        Ok(record)
    }
    /// Inspect the retained run, including failure after connection closure. The
    /// returned diagnostic record cannot be imported to authorize another client.
    pub async fn survival_motion(&self) -> Option<SurvivalMotionRecord> {
        let mut state = self.bot.session.state.lock().await;
        let problem = state
            .survival_motion
            .as_ref()
            .filter(|r| r.status.is_continuation_candidate())
            .and_then(|r| dispatched_rest(&state, r).err());
        if let Some(problem) = problem {
            let r = state.survival_motion.as_mut().unwrap();
            r.status = SurvivalMotionStatus::RequiresInspection;
            r.problem.get_or_insert_with(|| problem.to_string());
        }
        state.survival_motion.clone()
    }
    async fn run_survival_motion(
        &self,
        run: &SurvivalMotionRecord,
        observer: Option<&Operations>,
        uuid: [u8; 16],
    ) -> Result<()> {
        let mut model = Model::from_context(&run.preview.initial);
        let mut interval = tokio::time::interval(Duration::from_millis(50));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        for (control, expected) in run.preview.controls.iter().zip(&run.preview.frames) {
            let input = control.input;
            interval.tick().await;
            // Observer lifetime is checked before each send, without two locks.
            if let Some(observer) = observer {
                if let PlayerMotionStatus::RequiresInspection { reason } = observer
                    .observe_player_motion(run.initial_watch.as_ref().unwrap())
                    .await?
                {
                    return Err(invalid(&reason));
                }
            }
            let mut state = self.bot.session.state.lock().await;
            self.ready(&state)?;
            stable_context(&state, run)?;
            if state
                .survival_motion
                .as_ref()
                .is_none_or(|r| r.run_id != run.run_id || r.status != SurvivalMotionStatus::Running)
            {
                return Err(invalid("motion run superseded"));
            }
            let tick = self.bot.session.started.elapsed().as_millis() as u64 / 50;
            let State {
                reconstruction,
                world,
                ..
            } = &mut *state;
            reconstruction.advance(world, tick);
            if state.reconstruction.issue.is_some()
                || !state.reconstruction.recovery_chunks.is_empty()
            {
                return Err(invalid("motion reconstruction incomplete"));
            }
            let proposed = model.intent(input, control.yaw);
            let boxes = geometry(&*state, model.frame.position, proposed)?;
            model.advance(input, proposed, &boxes);
            if model.frame.position != expected.position
                || model.frame.velocity != expected.velocity
                || model.frame.on_ground != expected.on_ground
            {
                return Err(invalid(
                    "motion geometry changed the predicted path; replan before further input",
                ));
            }
            let rotation = [control.yaw, 0.0];
            let sequence = state.sequence;
            state
                .motion
                .begin(run.generation, sequence, expected.position, rotation)?;
            state.survival_motion.as_mut().unwrap().attempted_tick = expected.tick;
            self.bot
                .session
                .send(ids::play_serverbound::PLAYER_INPUT, &[input_bits(input)])
                .await?;
            let payload = position_packet(expected, rotation);
            self.bot
                .session
                .send(ids::play_serverbound::POSITION_LOOK, &payload)
                .await?;
            state.position = Some(expected.position);
            state.rotation = rotation;
            state.motion.dispatched();
            state.survival_motion.as_mut().unwrap().dispatched_ticks = expected.tick;
        }
        if observer.is_none() {
            let mut state = self.bot.session.state.lock().await;
            self.ready(&state)?;
            let basis = predicted_basis(&state, state.survival_motion.as_ref().unwrap())?;
            let tick = self.bot.session.started.elapsed().as_millis() as u64 / 50;
            let context =
                survival::context_with_basis(&mut state, self.bot.session.id, tick, basis)?;
            validate_initial(&context)?;
            state.survival_motion.as_mut().unwrap().status = SurvivalMotionStatus::Predicted;
            self.bot.session.changed.notify_waiters();
            return Ok(());
        }
        let observer = observer.unwrap();
        // Register after dispatch, still not a cross-connection causal/time fence.
        let watch = observer.watch_player_motion(uuid).await?;
        if let PlayerMotionStatus::RequiresInspection { reason } = observer
            .observe_player_motion(run.initial_watch.as_ref().unwrap())
            .await?
        {
            return Err(invalid(&reason));
        }
        {
            let mut state = self.bot.session.state.lock().await;
            stable_context(&state, run)?;
            let r = state.survival_motion.as_mut().unwrap();
            r.status = SurvivalMotionStatus::AwaitingObservation;
            r.final_watch = Some(watch.clone());
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            self.ready(&*self.bot.session.state.lock().await)?;
            if let PlayerMotionStatus::RequiresInspection { reason } = observer
                .observe_player_motion(run.initial_watch.as_ref().unwrap())
                .await?
            {
                return Err(invalid(&reason));
            }
            match observer.observe_player_motion(&watch).await? {
                PlayerMotionStatus::RequiresInspection { reason } => return Err(invalid(&reason)),
                PlayerMotionStatus::PositionUpdated { player }
                    if matches_endpoint(run, &player) =>
                {
                    let mut state = self.bot.session.state.lock().await;
                    stable_context(&state, run)?;
                    let basis =
                        observed_basis(&state, state.survival_motion.as_ref().unwrap(), &watch)?;
                    let tick = self.bot.session.started.elapsed().as_millis() as u64 / 50;
                    if !survival::context_with_basis(&mut state, self.bot.session.id, tick, basis)?
                        .on_ground
                    {
                        return Err(invalid("observed motion has no conservative floor support"));
                    }
                    let r = state.survival_motion.as_mut().unwrap();
                    r.observed = Some(player);
                    r.status = SurvivalMotionStatus::Observed;
                    self.bot.session.changed.notify_waiters();
                    return Ok(());
                }
                _ => {}
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(invalid(
                    "no fresh matching endpoint observation; inspect motion without replay",
                ));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
fn matches_endpoint(run: &SurvivalMotionRecord, player: &ObservedPlayer) -> bool {
    let end = run.preview.frames.last().unwrap();
    player.pose == Some(super::super::super::players::PlayerPose::Standing)
        && player.scale == 1.0
        && (0..3).all(|i| {
            endpoint::axis_matches(
                end.position[i],
                player.position[i],
                player.motion.position_error[i],
            )
        })
}
fn input_bits(input: SurvivalInput) -> u8 {
    u8::from(input.forward > 0)
        | (u8::from(input.forward < 0) << 1)
        | (u8::from(input.strafe > 0) << 2)
        | (u8::from(input.strafe < 0) << 3)
        | (u8::from(input.jump) << 4)
}

fn position_packet(frame: &PredictedMotionFrame, rotation: [f32; 2]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(33);
    for v in frame.position {
        payload.extend(v.to_be_bytes());
    }
    for v in rotation {
        payload.extend(v.to_be_bytes());
    }
    payload.push(u8::from(frame.on_ground) | (u8::from(frame.horizontal_collision) << 1));
    payload
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_packets_match_native_codecs() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../data/java_1_21_11/dry_movement.json"
        ))
        .unwrap();
        for c in fixture["input_packets"].as_array().unwrap() {
            assert_eq!(
                input_bits(SurvivalInput {
                    forward: c["forward"].as_i64().unwrap() as i8,
                    strafe: c["strafe"].as_i64().unwrap() as i8,
                    jump: c["jump"].as_bool().unwrap()
                }),
                c["bits"].as_u64().unwrap() as u8
            );
        }
        for c in fixture["position_packets"].as_array().unwrap() {
            let mut frame = Model::new([0.5, 64.0, -2.5]).frame;
            frame.on_ground = c["ground"].as_bool().unwrap();
            frame.horizontal_collision = c["collision"].as_bool().unwrap();
            assert_eq!(
                hex::encode(position_packet(&frame, [35.57, -22.12])),
                c["hex"].as_str().unwrap()
            );
        }
    }
}

/// An in-process, exact-run observation fence for explicit reassessment. No
/// deserialization or movement authority; creating it sends no player controls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SurvivalMotionRecheck {
    connection_id: u64,
    run_id: u64,
    attempt: u64,
    watch: PlayerMotionWatch,
}
fn recheck_eligible(state: &State, run_id: u64) -> Result<&SurvivalMotionRecord> {
    let r = state
        .survival_motion
        .as_ref()
        .filter(|r| r.run_id == run_id && r.status == SurvivalMotionStatus::RequiresInspection)
        .ok_or_else(|| invalid("recheck requires the exact failed motion run"))?;
    stable_context(state, r)?;
    let end = r.preview.frames.last().unwrap();
    if !end.resting
        || r.dispatched_ticks != end.tick
        || r.attempted_tick != end.tick
        || state.motion.position_basis != PositionBasis::Submitted
        || state.position != Some(end.position)
        || state.motion.last_submission.as_ref().is_none_or(|s| {
            !s.dispatched
                || s.superseded_at.is_some()
                || s.generation != r.generation
                || s.position != end.position
        })
    {
        return Err(invalid(
            "interrupted or superseded movement cannot be recovered by observation alone",
        ));
    }
    Ok(r)
}
impl Operations {
    /// Register a fresh observation fence for a fully dispatched, predicted-rest
    /// failed run. Does not move, clear failure, reconnect, or bypass standing.
    /// Caller must first diagnose the failure; no automatic retry is performed.
    pub async fn prepare_survival_motion_recheck(
        &self,
        run_id: u64,
        observer: &Operations,
    ) -> Result<SurvivalMotionRecheck> {
        let uuid = {
            let state = self.bot.session.state.lock().await;
            self.ready(&state)?;
            let r = recheck_eligible(&state, run_id)?;
            if r.contract != SurvivalMotionContract::IndependentlyObserved
                || r.observer_connection_id != Some(observer.bot.session.id)
            {
                return Err(invalid(
                    "motion recheck requires the original observer connection",
                ));
            }
            state
                .identity
                .as_ref()
                .ok_or_else(|| invalid("identity unavailable"))?
                .uuid
        };
        let watch = observer.watch_player_motion(uuid).await?;
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        let r = recheck_eligible(&state, run_id)?;
        let attempt = r
            .recheck
            .as_ref()
            .map_or(Some(1), |t| t.attempt.checked_add(1))
            .ok_or_else(|| invalid("recheck attempts exhausted"))?;
        let token = SurvivalMotionRecheck {
            connection_id: self.bot.session.id,
            run_id,
            attempt,
            watch,
        };
        state.survival_motion.as_mut().unwrap().recheck = Some(token.clone());
        Ok(token)
    }
    /// Reassess using a newer same-instance position and current geometry. A
    /// missing/conflicting observation or unsuitable geometry returns an error
    /// and leaves RequiresInspection intact. Safe to poll: no controls are sent.
    /// Success restores only the original predicted-and-observed basis, retaining
    /// the original problem and reassessment token in history.
    pub async fn observe_survival_motion_recheck(
        &self,
        token: &SurvivalMotionRecheck,
    ) -> Result<StandingContext> {
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        let r = recheck_eligible(&state, token.run_id)?;
        if token.connection_id != self.bot.session.id || r.recheck.as_ref() != Some(token) {
            return Err(invalid(
                "motion recheck belongs to another connection/run/attempt",
            ));
        }
        let basis = observed_basis(&state, r, &token.watch)?;
        let tick = self.bot.session.started.elapsed().as_millis() as u64 / 50;
        let context = survival::context_with_basis(&mut state, self.bot.session.id, tick, basis)?;
        if !context.on_ground {
            return Err(invalid("motion recheck lacks conservative floor support"));
        }
        let StandingPositionBasis::PredictedAndObserved { observed, .. } = &context.position_basis
        else {
            unreachable!()
        };
        let r = state.survival_motion.as_mut().unwrap();
        r.final_watch = Some(token.watch.clone());
        r.observed = Some((**observed).clone());
        r.status = SurvivalMotionStatus::Observed;
        Ok(context)
    }
}
