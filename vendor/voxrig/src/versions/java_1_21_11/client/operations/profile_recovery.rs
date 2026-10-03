//! One-profile fresh mining recovery for direct unmodified vanilla 1.21.11.
//! Login success excludes the old same-UUID player; local closure alone does not.
use super::recovery::connect_fresh_miner;
use super::*;

/// Process-local original mining/identity binding. Private fields and no
/// Deserialize prevent reconstruction from history. Clones share the source guard.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MiningProfileRecoveryWatch {
    intent: MiningIntent,
    uuid: [u8; 16],
    name: String,
}

impl Operations {
    /// Prepare without closing, reconnecting or editing. No observer is required.
    /// The caller declares a direct unmodified vanilla endpoint and exclusively
    /// owns this profile; proxy/plugin/other-server semantics are not equivalent.
    pub async fn prepare_survival_mining_profile_recovery(
        &self,
        intent: &MiningIntent,
    ) -> Result<MiningProfileRecoveryWatch> {
        let state = self.bot.session.state.lock().await;
        if intent.connection_id != self.bot.session.id
            || state
                .mining
                .as_ref()
                .is_none_or(|m| m.intent != *intent || m.recovery_attempt.is_some())
        {
            return Err(invalid(
                "profile recovery belongs to another or already claimed mining attempt",
            ));
        }
        let identity = state
            .identity
            .as_ref()
            .ok_or_else(|| invalid("authenticated miner unavailable"))?;
        Ok(MiningProfileRecoveryWatch {
            intent: intent.clone(),
            uuid: identity.uuid,
            name: identity.name.clone(),
        })
    }

    /// Explicit once-only fresh login AFTER closing the source. The new native
    /// same-profile login is the lifecycle boundary, not a replay or old-session
    /// release. Validate new loading, identity, standing, inventory and target
    /// using the same admission as independent retirement recovery.
    pub async fn reconnect_survival_mining_profile(
        &self,
        watch: &MiningProfileRecoveryWatch,
        config: ConnectionConfig,
        target: MiningRecoveryTarget,
    ) -> Result<MiningRecovery> {
        let miner = self
            .bot
            .session
            .state
            .lock()
            .await
            .identity
            .clone()
            .ok_or_else(|| invalid("authenticated miner unavailable"))?;
        if config.version != MinecraftVersion::Java1_21_11
            || config.server != miner.server
            || config.username != miner.name
            || watch.uuid != miner.uuid
            || watch.name != miner.name
        {
            return Err(invalid(
                "profile recovery requires the original endpoint/profile/version",
            ));
        }
        target.validate(&watch.intent)?;
        self.claim_mining_recovery(
            &watch.intent,
            &miner,
            MiningRecoveryMethod::SameProfileLogin,
            target.clone(),
        )
        .await?;
        let old_history = self.operation_history().await;
        let fresh = connect_fresh_miner(config, &miner, &watch.intent, &target).await?;
        let evidence = fresh.evidence(
            old_history,
            MiningRecoveryBoundary::SameProfileLogin {
                uuid: miner.uuid,
                name: miner.name,
            },
        );
        Ok(MiningRecovery {
            operations: fresh.operations,
            evidence,
        })
    }
}
