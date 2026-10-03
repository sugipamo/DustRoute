//! Explicit vanilla player-retirement recovery. Air never releases the old miner.
use super::recovery::{connect_fresh_miner, identity};
use super::*;
use crate::diagnostic_projection::diagnostic_record;
use std::time::Duration;

diagnostic_record! {
    /// An in-process watch established while an independent observer knows the
    /// authenticated miner profile. Private fields prevent importing JSON as authority.
    #[derive(Clone, Debug, PartialEq, Serialize)]
    pub struct MiningRetirementWatch => RecordedMiningRetirementWatch {
        intent: MiningIntent,
        observer_connection_id: u64,
        after_sequence: u64,
        observer_dimension: String,
        miner_uuid: [u8; 16],
        miner_name: String,
    }
    diagnostic_serde {}
}
diagnostic_record! {
    /// Bounded observer history: one watch, with an exact player-info removal receipt.
    #[derive(Clone, Debug, Serialize)]
    pub struct MiningRetirementRecord => RecordedMiningRetirementRecord {
        /// Owning intent, authenticated identity and independent receive boundary.
        pub watch: MiningRetirementWatch,
        /// PLAYER_REMOVE containing that UUID, received after watch registration.
        pub removal_receive_sequence: Option<u64>,
        /// Latched observer context/rejoin conflict; a later removal cannot clear it.
        pub requires_inspection: Option<String>,
        /// An explicit reconnect has begun. Retained before I/O; never auto-retried.
        pub recovery_started: bool,
    }
    diagnostic_serde {}
}
/// Retirement is separate from the target's removal result.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MiningRetirementStatus {
    /// Missing exact removal receipt or the old local sender is still usable.
    Pending {
        /// Independent observer evidence retained across polling/timeout.
        record: MiningRetirementRecord,
        /// Whether local closure has been established; closure alone is insufficient.
        source_closed: bool,
    },
    /// Local source closed and independent PLAYER_REMOVE observed on the live
    /// observer. Valid only for direct, unmodified vanilla 1.21.11 semantics.
    Retired {
        /// Historical retirement evidence, not a current target observation.
        record: MiningRetirementRecord,
    },
    /// Changed observer context or miner rejoin requires a new investigation.
    RequiresInspection {
        /// Latched cause and original watch.
        record: MiningRetirementRecord,
    },
}
fn unavailable(message: &str) -> Error {
    Error::new(ErrorKind::State, anyhow::anyhow!("{message}"))
}
fn owns(state: &State, watch: &MiningRetirementWatch, owner: u64) -> Result<()> {
    if owner != watch.intent.connection_id
        || state
            .mining
            .as_ref()
            .is_none_or(|m| m.intent != watch.intent)
        || state
            .identity
            .as_ref()
            .is_none_or(|i| i.uuid != watch.miner_uuid || i.name != watch.miner_name)
    {
        return Err(unavailable(
            "retirement watch belongs to another miner/attempt",
        ));
    }
    Ok(())
}
fn watched<'a>(
    state: &'a State,
    watch: &MiningRetirementWatch,
) -> Result<&'a MiningRetirementRecord> {
    state
        .retirement
        .as_ref()
        .filter(|r| r.watch == *watch)
        .ok_or_else(|| unavailable("retirement watch is absent or superseded"))
}
impl Operations {
    /// Register before disconnecting, while the observer has the miner's exact
    /// authenticated profile. A missing entity/profile, cached absence, unrelated
    /// destroy packet or player name alone cannot prove retirement.
    /// Caller must use two connections to the same direct vanilla server; plugin
    /// tab-list filtering and proxies are outside this fence's admitted semantics.
    pub async fn prepare_survival_mining_retirement(
        &self,
        intent: &MiningIntent,
        observer: &Operations,
    ) -> Result<MiningRetirementWatch> {
        if self.bot.session.id == observer.bot.session.id {
            return Err(unavailable("retirement requires an independent observer"));
        }
        // Never hold two session locks: reciprocal callers cannot deadlock.
        let miner = {
            let state = self.bot.session.state.lock().await;
            if intent.connection_id != self.bot.session.id
                || state.mining.as_ref().is_none_or(|m| m.intent != *intent)
            {
                return Err(unavailable(
                    "mining intent belongs to another connection/attempt",
                ));
            }
            identity(&state)?
        };
        let mut state = observer.bot.session.state.lock().await;
        observer.ready(&state)?;
        let observer_identity = identity(&state)?;
        if observer_identity.server != miner.server || observer_identity.uuid == miner.uuid {
            return Err(unavailable(
                "observer must be a distinct profile on the same server endpoint",
            ));
        }
        if state.players.profile_name(&miner.uuid) != Some(miner.name.as_str()) {
            return Err(unavailable(
                "observer has no exact authenticated miner profile baseline",
            ));
        }
        if state.retirement.as_ref().is_some_and(|r| {
            r.removal_receive_sequence.is_none()
                || r.watch.intent.connection_id == intent.connection_id
        }) {
            return Err(unavailable(
                "observer already has an unresolved retirement watch",
            ));
        }
        let watch = MiningRetirementWatch {
            intent: intent.clone(),
            observer_connection_id: observer.bot.session.id,
            after_sequence: state.sequence,
            observer_dimension: state
                .world
                .dimension
                .as_ref()
                .ok_or_else(|| unavailable("observer dimension unavailable"))?
                .0
                .clone(),
            miner_uuid: miner.uuid,
            miner_name: miner.name,
        };
        state.retirement = Some(MiningRetirementRecord {
            watch: watch.clone(),
            removal_receive_sequence: None,
            requires_inspection: None,
            recovery_started: false,
        });
        Ok(watch)
    }
    /// Read-only retirement reconciliation, available after source closure.
    /// The observer itself must remain live, ready and in its registered context.
    pub async fn observe_survival_mining_retirement(
        &self,
        watch: &MiningRetirementWatch,
        observer: &Operations,
    ) -> Result<MiningRetirementStatus> {
        {
            let state = self.bot.session.state.lock().await;
            owns(&state, watch, self.bot.session.id)?;
        }
        if observer.bot.session.id != watch.observer_connection_id {
            return Err(unavailable("retirement watch belongs to another observer"));
        }
        let state = observer.bot.session.state.lock().await;
        observer.ready(&state)?;
        let record = watched(&state, watch)?.clone();
        if record.requires_inspection.is_some()
            || state.world.dimension.as_ref().map(|d| &d.0) != Some(&watch.observer_dimension)
        {
            return Ok(MiningRetirementStatus::RequiresInspection { record });
        }
        let source_closed = self.bot.session.stopped.load(Ordering::Acquire);
        if source_closed && record.removal_receive_sequence.is_some() {
            Ok(MiningRetirementStatus::Retired { record })
        } else {
            Ok(MiningRetirementStatus::Pending {
                record,
                source_closed,
            })
        }
    }
    /// Bounded read-only wait. Timeout/cancellation preserves the observer watch.
    pub async fn wait_survival_mining_retirement(
        &self,
        watch: &MiningRetirementWatch,
        observer: &Operations,
        maximum_wait: Duration,
    ) -> Result<MiningRetirementStatus> {
        if maximum_wait.is_zero() || maximum_wait > Duration::from_secs(30) {
            return Err(invalid(
                "retirement wait must be greater than zero and at most 30 seconds",
            ));
        }
        let waited = timeout(maximum_wait, async {
            loop {
                let notified = observer.bot.session.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let status = self
                    .observe_survival_mining_retirement(watch, observer)
                    .await?;
                if !matches!(status, MiningRetirementStatus::Pending { .. }) {
                    return Ok(status);
                }
                // Source closure does not notify the observer. This bounded poll
                // is wake scheduling only; it is never evidence of retirement.
                tokio::select! {
                    _ = notified => {},
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {},
                }
            }
        })
        .await;
        match waited {
            Ok(result) => result,
            Err(_) => {
                self.observe_survival_mining_retirement(watch, observer)
                    .await
            }
        }
    }
    /// Explicit fresh-connection recovery, never an automatic retry. Requires
    /// validated retirement and the same endpoint/name/version. The caller must
    /// declare expected target contents and build a new permission-checked plan.
    /// Native loading notification and new player/site observations are required
    /// before exposing operations; every subsequent action needs its own result.
    pub async fn reconnect_survival_mining(
        &self,
        watch: &MiningRetirementWatch,
        observer: &Operations,
        config: ConnectionConfig,
        expected_target: crate::NativeBlockState,
    ) -> Result<MiningRecovery> {
        let miner = {
            let state = self.bot.session.state.lock().await;
            identity(&state)?
        };
        if config.version != MinecraftVersion::Java1_21_11
            || config.server != miner.server
            || config.username != miner.name
        {
            return Err(invalid(
                "mining recovery requires the original endpoint/name and Java 1.21.11",
            ));
        }
        let retirement = match self
            .observe_survival_mining_retirement(watch, observer)
            .await?
        {
            MiningRetirementStatus::Retired { record } => record,
            _ => {
                return Err(unavailable(
                    "independent miner retirement is not established",
                ));
            }
        };
        let target = MiningRecoveryTarget::Exact(expected_target);
        target.validate(&watch.intent)?;
        // Consume the reconnect stage under the observer lock. Parallel or
        // cancelled callers cannot start a second login from the same receipt.
        let retirement = {
            let mut state = observer.bot.session.state.lock().await;
            observer.ready(&state)?;
            let current = watched(&state, watch)?;
            if current.requires_inspection.is_some() || current.recovery_started {
                return Err(unavailable(
                    "retirement recovery already attempted or needs inspection",
                ));
            }
            if current.removal_receive_sequence != retirement.removal_receive_sequence {
                return Err(unavailable("retirement receipt changed before recovery"));
            }
            let current = state.retirement.as_mut().expect("watch checked");
            current.recovery_started = true;
            current.clone()
        };
        self.claim_mining_recovery(
            &watch.intent,
            &miner,
            MiningRecoveryMethod::IndependentRemoval,
            target.clone(),
        )
        .await?;
        let old_history = self.operation_history().await;
        let fresh = connect_fresh_miner(config, &miner, &watch.intent, &target).await?;
        let evidence = fresh.evidence(
            old_history,
            MiningRecoveryBoundary::IndependentRemoval {
                receipt: Box::new(retirement),
            },
        );
        let operations = fresh.operations;
        {
            let state = observer.bot.session.state.lock().await;
            observer.ready(&state)?;
            if watched(&state, watch)?.requires_inspection.is_some() {
                return Err(unavailable("observer context changed during recovery"));
            }
        }
        Ok(MiningRecovery {
            operations,
            evidence,
        })
    }
}

pub(in crate::versions::java_1_21_11::client) fn retirement_received(
    state: &mut State,
    id: i32,
    bytes: &[u8],
) -> anyhow::Result<()> {
    let Some(record) = &mut state.retirement else {
        return Ok(());
    };
    if id == ids::play_clientbound::PLAYER_REMOVE {
        let mut reader = Reader::new(bytes);
        for _ in 0..reader.count(1024)? {
            let uuid: [u8; 16] = reader.take(16)?.try_into()?;
            if uuid == record.watch.miner_uuid && state.sequence > record.watch.after_sequence {
                record
                    .removal_receive_sequence
                    .get_or_insert(state.sequence);
            }
        }
        reader.end()?;
    } else if id == ids::play_clientbound::PLAYER_INFO
        && record.removal_receive_sequence.is_some()
        && !record.recovery_started
        && state
            .players
            .profile_name(&record.watch.miner_uuid)
            .is_some()
    {
        record
            .requires_inspection
            .get_or_insert_with(|| "miner profile reappeared after retirement receipt".into());
    }
    Ok(())
}
