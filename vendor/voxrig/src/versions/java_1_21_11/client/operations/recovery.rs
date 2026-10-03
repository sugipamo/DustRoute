//! Shared native fresh-miner admission and once-only recovery ownership.
use super::*;
use crate::diagnostic_projection::diagnostic_record;

fn unavailable(message: &str) -> Error {
    Error::new(ErrorKind::State, anyhow::anyhow!("{message}"))
}

/// Fresh connection and stationary target observation after validated retirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MiningRecoveryMethod {
    /// Exact independently received profile removal before reconnect.
    IndependentRemoval,
    /// New same-profile login success under audited direct vanilla semantics.
    SameProfileLogin,
}
/// Caller-declared condition on the newly received target. No edit is sent.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "state", rename_all = "snake_case")]
pub enum MiningRecoveryTarget {
    /// Require the exact original baseline or an exact ordinary air state.
    Exact(crate::NativeBlockState),
    /// Admit only the original baseline or ordinary air for caller reconciliation.
    OriginalOrAir,
}
impl MiningRecoveryTarget {
    fn air(state: &crate::NativeBlockState) -> bool {
        state.properties.is_empty()
            && matches!(
                state.name.as_str(),
                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
            )
    }
    pub(super) fn validate(&self, intent: &MiningIntent) -> Result<()> {
        if let Self::Exact(state) = self
            && *state != intent.baseline
            && !Self::air(state)
        {
            return Err(invalid(
                "recovery requires air or the original supported target baseline",
            ));
        }
        Ok(())
    }
    fn allows(&self, state: &crate::NativeBlockState, intent: &MiningIntent) -> bool {
        match self {
            Self::Exact(expected) => state == expected,
            Self::OriginalOrAir => *state == intent.baseline || Self::air(state),
        }
    }
}
/// Original connection's retained before-I/O claim, not a login result.
#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq)]
pub struct MiningRecoveryAttempt {
    /// Selected native retirement semantics.
    pub method: MiningRecoveryMethod,
    /// Declared fresh target condition.
    pub target: MiningRecoveryTarget,
}
diagnostic_record! {
    /// Evidence establishing which native recovery boundary was used.
    #[derive(Clone, Debug, Serialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    pub enum MiningRecoveryBoundary => RecordedMiningRecoveryBoundary {
        /// Independent exact UUID removal after the source closed.
        IndependentRemoval {
            /// Observer's retained receipt and once-only recovery claim.
            receipt: Box<MiningRetirementRecord>,
        },
        /// Audited vanilla login excludes the old same-UUID player before success.
        /// This is a version-specific lifecycle interpretation, not a general server ACK.
        SameProfileLogin {
            /// Authenticated UUID received again on the new connection.
            uuid: [u8; 16],
            /// Exact authenticated profile name.
            name: String,
        },
    }
    diagnostic_serde { #[serde(tag = "kind", rename_all = "snake_case")] }
}
diagnostic_record! {
    /// Closed original history and fresh native baseline with explicit lifecycle evidence.
    #[derive(Clone, Debug, Serialize)]
    pub struct MiningRecoveryEvidence => RecordedMiningRecoveryEvidence {
        /// Current-generation PLAYER_LOADED dispatched before exposing new operations.
        /// This is not server acceptance of a subsequent game action.
        pub interaction_ready: bool,
        /// Old mining state is retained; it is never imported into the new session.
        pub old_history: OperationHistory,
        /// Native lifecycle boundary; independent and same-profile evidence stay distinct.
        pub boundary: MiningRecoveryBoundary,
        /// New, live connection identity.
        pub connection_id: u64,
        /// Entire new stationary context; contains no inherited position/health data.
        pub standing: StandingContext,
        /// Newly loaded exact target contents matching the caller's declared condition.
        pub target: crate::NativeBlockState,
        /// New connection's receive boundary; incomparable to the old ordinal.
        pub receive_sequence: u64,
    }
    diagnostic_serde {}
}
/// Explicit recovery result. The original connection remains closed and blocked.
pub struct MiningRecovery {
    /// Operations on the fresh connection after loading notification and new
    /// site/player validation. Original mining authority is never imported.
    pub operations: Operations,
    /// Evidence for diagnosis and a new plan; not permission to replay an old job.
    pub evidence: MiningRecoveryEvidence,
}
impl MiningRecovery {
    /// Handle to this already validated connection for observation, tracing and
    /// explicit disconnection. Cloning the handle does not reconnect or release
    /// any operation guard; it uses the same version adapter and session.
    pub fn client(&self) -> crate::Client {
        crate::Client::from_java_1_21_11(self.operations.bot.clone())
    }
}

pub(super) fn identity(state: &State) -> Result<LoginIdentity> {
    state
        .identity
        .clone()
        .ok_or_else(|| unavailable("authenticated login identity unavailable"))
}
pub(super) struct FreshMiningConnection {
    pub(super) operations: Operations,
    standing: StandingContext,
    target: crate::NativeBlockState,
    receive_sequence: u64,
    interaction_ready: bool,
}
impl FreshMiningConnection {
    pub(super) fn evidence(
        &self,
        old_history: OperationHistory,
        boundary: MiningRecoveryBoundary,
    ) -> MiningRecoveryEvidence {
        MiningRecoveryEvidence {
            interaction_ready: self.interaction_ready,
            old_history,
            boundary,
            connection_id: self.operations.bot.session.id,
            standing: self.standing.clone(),
            target: self.target.clone(),
            receive_sequence: self.receive_sequence,
        }
    }
}
/// Both retirement methods share the same fresh loading/site/player admission.
pub(super) async fn connect_fresh_miner(
    config: ConnectionConfig,
    miner: &LoginIdentity,
    intent: &MiningIntent,
    target: &MiningRecoveryTarget,
) -> Result<FreshMiningConnection> {
    let bot = Bot::connect(config).await?;
    bot.wait_until_ready().await?;
    let operations = bot.operations();
    // Common loading covers the own chunk, not all inventory/health/site cells.
    // Wait for those received baselines; the timeout never certifies them.
    timeout(bot.session.limits.ready_timeout, async {
        loop {
            let notified = bot.session.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let state = bot.session.state.lock().await;
            operations.ready(&state)?;
            if state.operations.local_player.health.is_some()
                && !state
                    .operations
                    .inventory
                    .slots
                    .contains(&InventorySlot::Unavailable)
                && state.world.block(intent.target).is_some()
                && survival::standing_baselines_received(&state)?
            {
                return Ok::<(), Error>(());
            }
            drop(state);
            notified.await;
        }
    })
    .await
    .context("fresh recovery baselines timed out")??;
    let mut state = bot.session.state.lock().await;
    operations.ready(&state)?;
    operations.mutable(&state)?;
    if identity(&state)?.uuid != miner.uuid
        || identity(&state)?.name != miner.name
        || identity(&state)?.server != miner.server
        || state.operations.game_mode != Some(GameMode::Survival)
        || state.world.dimension.as_ref().map(|d| &d.0) != Some(&intent.dimension)
    {
        return Err(unavailable(
            "fresh miner identity, mode or dimension differs",
        ));
    }
    let tick = bot.session.started.elapsed().as_millis() as u64 / 50;
    let standing = survival::context(&mut state, bot.session.id, tick)?;
    let cell = state.reconstruction.cell(&state.world, intent.target);
    if !standing.on_ground
        || standing.submerged
        || !standing
            .player
            .health
            .as_ref()
            .is_some_and(|h| h.health > 0.0)
        || standing.player.block_break_speed.map(|v| v.value) != Some(1.0)
        || standing.player.mining_efficiency.map(|v| v.value) != Some(0.0)
        || !standing.player.effect_updates.is_empty()
        || cell.moving.is_some()
        || !cell
            .state
            .as_ref()
            .is_some_and(|state| target.allows(state, intent))
        || state.reconstruction.issue.is_some()
        || !state.reconstruction.recovery_chunks.is_empty()
        || state.operations.inventory.window_id != Some(0)
        || state.operations.inventory.cursor != InventorySlot::Empty
        || state.operations.inventory.unsupported_components
        || state
            .operations
            .inventory
            .slots
            .contains(&InventorySlot::Unavailable)
    {
        return Err(unavailable(
            "fresh survival site/player/inventory needs inspection and a new plan",
        ));
    }
    let fresh = FreshMiningConnection {
        operations: operations.clone(),
        standing,
        target: cell.state.expect("validated fresh target"),
        receive_sequence: state.sequence,
        interaction_ready: state.loading.notification_dispatched(),
    };
    drop(state);
    Ok(fresh)
}
impl Operations {
    /// Claim once on the source, shared by independent and single-profile recovery.
    /// Failed or cancelled login never releases this claim.
    pub(super) async fn claim_mining_recovery(
        &self,
        intent: &MiningIntent,
        miner: &LoginIdentity,
        method: MiningRecoveryMethod,
        target: MiningRecoveryTarget,
    ) -> Result<()> {
        let mut state = self.bot.session.state.lock().await;
        let authenticated = identity(&state)?;
        if !self.bot.session.stopped.load(Ordering::Acquire)
            || intent.connection_id != self.bot.session.id
            || authenticated.uuid != miner.uuid
            || authenticated.name != miner.name
            || authenticated.server != miner.server
        {
            return Err(unavailable(
                "recovery requires the closed original authenticated miner",
            ));
        }
        let record = state
            .mining
            .as_mut()
            .filter(|m| m.intent == *intent)
            .ok_or_else(|| unavailable("recovery belongs to another mining attempt"))?;
        if record.recovery_attempt.is_some() {
            return Err(unavailable(
                "mining recovery already attempted; no second login",
            ));
        }
        record.recovery_attempt = Some(MiningRecoveryAttempt { method, target });
        Ok(())
    }
}
