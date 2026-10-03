//! Checked survival contract selected from the client's immutable version adapter.
//!
//! Capability discovery describes implemented semantics, never current readiness,
//! permission, observation freshness, or authority to replay a serialized plan.
//! Java 1.16.1's established controls remain available through its own API; they
//! do not implement this checked contract. Data types currently share the native
//! 1.21.11 representation and retain version/session evidence. Future adapters
//! must implement the same contract or declare a different one explicitly.
//! The legacy survival module remains source-compatible:
//! ```
//! let _: Option<voxrig::survival::SurvivalState> = None;
//! ```
//! Creative/command shortcuts are not part of this surface:
//! ```compile_fail
//! async fn bypass(ops: &voxrig::checked_survival::Operations) {
//!     ops.send_command("setblock 0 0 0 stone").await.unwrap();
//! }
//! ```
//!
//! Route selection, building designs, resource reservations and durable jobs
//! belong to the caller. This surface exposes no commands or creative controls.

use crate::versions::java_1_21_11::operations as native;
use crate::{
    BlockFace, Client, ConnectionConfig, MinecraftVersion, NativeBlockState, Region, Result,
};
use std::time::Duration;

pub use crate::versions::java_1_21_11::players::PlayerObservations;

pub use native::{
    CapturedSurvivalScene, HypotheticalAimRequirement, HypotheticalBlockEdit,
    HypotheticalMovementPreview, HypotheticalPlacement, HypotheticalReconnectBoundary,
    InventorySlot, InventorySwap, InventorySwapObservation, LocalPlayerState,
    MAX_SURVIVAL_CONTROL_TICKS, MiningIntent, MiningInventoryChange, MiningInventoryChangeKind,
    MiningRecord, MiningRecoveryAttempt, MiningRecoveryBoundary, MiningRecoveryEvidence,
    MiningRecoveryMethod, MiningRecoveryTarget, MiningRetirementStatus, MiningStatus,
    OperationHistory, PlacementIntent, PlacementStatus, PlayerState, PredictedMotionFrame,
    StandingContext, StandingPositionBasis, SurvivalControl, SurvivalInput, SurvivalMotionContract,
    SurvivalMotionRecheck, SurvivalMotionRecord, SurvivalMotionStatus, SurvivalMovementPreview,
    SurvivalScenario, TerminalClearance,
};

/// Versioned semantics, separate from a wire protocol number.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurvivalContract {
    /// Bounded dry full-cube walking/jumping with independent endpoint observation,
    /// plain-stack main/hotbar swaps, passive-cube placement, and empty-hand
    /// dirt/stone mining with explicit vanilla retirement and fresh recovery.
    /// No entities, fluids, gathering, tools, sprinting or crouching.
    ObservedDryCubeV1,
    /// Fully dispatched dry-cube model endpoints and fresh received geometry,
    /// explicitly without independent spatial corroboration or a physical error
    /// bound. Corrections/interruption invalidate continuation. Other supported
    /// inventory, placement and mining/recovery restrictions remain unchanged.
    PredictedDryCubeV1,
}

/// Static adapter support. A supported contract still checks each live action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct SurvivalCapabilities {
    /// Exact immutable wire/registry version.
    pub version: MinecraftVersion,
    /// None means this checked contract is unavailable, not that all legacy
    /// player controls are unavailable.
    pub checked_contract: Option<SurvivalContract>,
    /// Audited direct vanilla same-profile fresh mining recovery. This does not
    /// imply observer-free movement or permission to reuse the old connection.
    pub same_profile_mining_recovery: bool,
    /// Explicit observer-free model contract. None means unavailable for this version.
    pub prediction_based_contract: Option<SurvivalContract>,
}
impl SurvivalCapabilities {
    /// Discover implementation support without opening a connection.
    pub const fn for_version(version: MinecraftVersion) -> Self {
        Self {
            version,
            checked_contract: match version {
                MinecraftVersion::Java1_16_1 => None,
                MinecraftVersion::Java1_21_11 => Some(SurvivalContract::ObservedDryCubeV1),
            },
            same_profile_mining_recovery: matches!(version, MinecraftVersion::Java1_21_11),
            prediction_based_contract: match version {
                MinecraftVersion::Java1_16_1 => None,
                MinecraftVersion::Java1_21_11 => Some(SurvivalContract::PredictedDryCubeV1),
            },
        }
    }
}

/// Session-bound checked operations. Clones share native guards and history.
/// No public constructor, deserialization or raw-operation escape hatch.
#[derive(Clone)]
pub struct Operations {
    client: Client,
    native: native::Operations,
}
impl Client {
    /// Static adapter capabilities; no I/O and no current-state admission.
    pub fn survival_capabilities(&self) -> SurvivalCapabilities {
        SurvivalCapabilities::for_version(self.version())
    }
    /// Select checked survival semantics, refusing unsupported versions before I/O.
    /// The legacy version-specific API continues to coexist.
    pub fn survival(&self) -> Result<Operations> {
        Ok(Operations {
            client: self.clone(),
            native: self.java_1_21_11_operations()?,
        })
    }
}
impl Operations {
    /// Received player identities and poses for observer setup; missing entries
    /// are not evidence of retirement or absence from the server.
    pub async fn visible_players(&self) -> Result<PlayerObservations> {
        self.native.visible_players().await
    }
    /// Contract and version of this handle; not permission to perform an action.
    pub fn capabilities(&self) -> SurvivalCapabilities {
        self.client.survival_capabilities()
    }
    /// Inspect received player state; unavailable values remain explicit.
    pub async fn player_state(&self) -> Result<PlayerState> {
        self.native.player_state().await
    }
    /// Read retained operation evidence, including after local closure.
    pub async fn operation_history(&self) -> OperationHistory {
        self.native.operation_history().await
    }
    /// Check current stationary geometry and position provenance.
    pub async fn standing_context(&self) -> Result<StandingContext> {
        self.native.standing_context().await
    }
    /// Capture a bounded read-only scene; no action authority is created.
    pub async fn capture_survival_scene(&self, region: Region) -> Result<CapturedSurvivalScene> {
        self.native.capture_survival_scene(region).await
    }
    /// Recheck capture provenance against current native state.
    pub async fn validate_survival_scene(&self, scene: &CapturedSurvivalScene) -> Result<()> {
        self.native.validate_survival_scene(scene).await
    }
    /// Predict caller-supplied controls without sending packets.
    pub async fn preview_survival_path(
        &self,
        controls: &[SurvivalControl],
    ) -> Result<SurvivalMovementPreview> {
        self.native.preview_survival_path(controls).await
    }
    /// Predict bounded inputs at one heading without sending packets.
    pub async fn preview_survival_motion(
        &self,
        yaw: f32,
        inputs: &[SurvivalInput],
    ) -> Result<SurvivalMovementPreview> {
        self.native.preview_survival_motion(yaw, inputs).await
    }
    /// Read a running or completed bounded movement and its retained evidence.
    pub async fn survival_motion(&self) -> Option<SurvivalMotionRecord> {
        self.native.survival_motion().await
    }
    /// Reassess a prepared motion observation; never resends controls.
    pub async fn observe_survival_motion_recheck(
        &self,
        token: &SurvivalMotionRecheck,
    ) -> Result<StandingContext> {
        self.native.observe_survival_motion_recheck(token).await
    }
    /// Submit an ordinary main-inventory/hotbar swap with before-I/O intent.
    pub async fn swap_player_hotbar(&self, main_slot: u8, hotbar: u8) -> Result<InventorySwap> {
        self.native.swap_player_hotbar(main_slot, hotbar).await
    }
    /// Read-only bounded inventory wait; cancellation retains the original intent.
    pub async fn wait_inventory_swap(
        &self,
        submission: &InventorySwap,
        maximum_wait: Duration,
    ) -> Result<InventorySwapObservation> {
        self.native
            .wait_inventory_swap(submission, maximum_wait)
            .await
    }
    /// Select an ordinary hotbar slot; dispatch is not server acceptance.
    pub async fn select_hotbar(&self, slot: u8) -> Result<()> {
        self.native.select_hotbar(slot).await
    }
    /// Submit look rotation through the unchanged native mutation guards.
    pub async fn look(&self, rotation: [f32; 2]) -> Result<()> {
        self.native.look(rotation).await
    }
    /// Place a supported held passive cube with native aim, reach and state checks.
    pub async fn place_survival_cube(
        &self,
        support: [i32; 3],
        face: BlockFace,
    ) -> Result<PlacementIntent> {
        self.native.place_survival_cube(support, face).await
    }
    /// Observe placement target, material and processed-sequence evidence.
    pub async fn observe_survival_placement(
        &self,
        intent: &PlacementIntent,
    ) -> Result<PlacementStatus> {
        self.native.observe_survival_placement(intent).await
    }
    /// Wait without replaying an uncertain placement.
    pub async fn wait_survival_placement(
        &self,
        intent: &PlacementIntent,
        maximum_wait: Duration,
    ) -> Result<PlacementStatus> {
        self.native
            .wait_survival_placement(intent, maximum_wait)
            .await
    }
    /// Start supported empty-hand removal; the old session remains guarded afterward.
    pub async fn start_survival_mining(
        &self,
        target: [i32; 3],
        face: BlockFace,
    ) -> Result<MiningIntent> {
        self.native.start_survival_mining(target, face).await
    }
    /// Explicitly send finish for the original intent; no continuation is implied.
    pub async fn finish_survival_mining(&self, intent: &MiningIntent) -> Result<i32> {
        self.native.finish_survival_mining(intent).await
    }
    /// Explicitly send abort; dispatch does not release the mining guard.
    pub async fn abort_survival_mining(&self, intent: &MiningIntent) -> Result<i32> {
        self.native.abort_survival_mining(intent).await
    }
    /// Observe the original mining operation without repeating it.
    pub async fn observe_survival_mining(&self, intent: &MiningIntent) -> Result<MiningStatus> {
        self.native.observe_survival_mining(intent).await
    }
    /// Bounded read-only wait; timeout/cancellation retains the mining intent.
    pub async fn wait_survival_mining(
        &self,
        intent: &MiningIntent,
        maximum_wait: Duration,
    ) -> Result<MiningStatus> {
        self.native.wait_survival_mining(intent, maximum_wait).await
    }
    /// Schedule one removal; cancellation never clears or retries its intent.
    pub async fn dig_survival_cube(
        &self,
        target: [i32; 3],
        face: BlockFace,
        maximum_wait: Duration,
    ) -> Result<MiningStatus> {
        self.native
            .dig_survival_cube(target, face, maximum_wait)
            .await
    }
    /// Revalidate and execute a preview with an independent same-endpoint observer.
    pub async fn start_previewed_survival_motion(
        &self,
        expected: &SurvivalMovementPreview,
        observer: &Self,
    ) -> Result<SurvivalMotionRecord> {
        self.native
            .start_previewed_survival_motion(expected, &observer.native)
            .await
    }
    /// Execute caller-selected bounded controls with independent endpoint observation.
    pub async fn start_survival_path(
        &self,
        controls: &[SurvivalControl],
        observer: &Self,
    ) -> Result<SurvivalMotionRecord> {
        self.native
            .start_survival_path(controls, &observer.native)
            .await
    }
    /// Revalidate and execute with model-based continuation, without an observer.
    /// The endpoint is not a received pose or independently measured position.
    pub async fn start_previewed_predicted_survival_motion(
        &self,
        expected: &SurvivalMovementPreview,
    ) -> Result<SurvivalMotionRecord> {
        self.native
            .start_previewed_predicted_survival_motion(expected)
            .await
    }
    /// Execute bounded controls under the explicit prediction-based contract.
    pub async fn start_predicted_survival_path(
        &self,
        controls: &[SurvivalControl],
    ) -> Result<SurvivalMotionRecord> {
        self.native.start_predicted_survival_path(controls).await
    }
    /// Execute one heading of bounded inputs with independent endpoint observation.
    pub async fn start_survival_motion(
        &self,
        yaw: f32,
        inputs: &[SurvivalInput],
        observer: &Self,
    ) -> Result<SurvivalMotionRecord> {
        self.native
            .start_survival_motion(yaw, inputs, &observer.native)
            .await
    }
    /// Prepare an explicit read-only reassessment after diagnosis of a failed run.
    pub async fn prepare_survival_motion_recheck(
        &self,
        run_id: u64,
        observer: &Self,
    ) -> Result<SurvivalMotionRecheck> {
        self.native
            .prepare_survival_motion_recheck(run_id, &observer.native)
            .await
    }
    /// Register exact independent retirement evidence before any local closure.
    /// Returns a reusable in-process coordinator, not a durable job or retry policy.
    /// Only direct unmodified vanilla semantics are admitted by this contract.
    pub async fn prepare_mining_retirement(
        &self,
        intent: &MiningIntent,
        observer: &Self,
    ) -> Result<MiningRetirement> {
        let watch = self
            .native
            .prepare_survival_mining_retirement(intent, &observer.native)
            .await?;
        Ok(MiningRetirement {
            source: self.clone(),
            observer: observer.clone(),
            watch,
        })
    }
    /// Explicit one-profile recovery for the audited direct vanilla endpoint.
    /// No observer, world edits or reconnect are performed by preparation.
    pub async fn prepare_mining_profile_recovery(
        &self,
        intent: &MiningIntent,
    ) -> Result<MiningProfileRecovery> {
        let watch = self
            .native
            .prepare_survival_mining_profile_recovery(intent)
            .await?;
        Ok(MiningProfileRecovery {
            source: self.clone(),
            watch,
        })
    }
}

/// Original native miner binding; history cannot reconstruct this coordinator.
/// Clones and independent-retirement handles share one before-I/O login claim.
#[derive(Clone)]
pub struct MiningProfileRecovery {
    source: Operations,
    watch: native::MiningProfileRecoveryWatch,
}
impl MiningProfileRecovery {
    /// Close before reconnecting. This call alone does not establish retirement.
    pub async fn close_source(&self) -> Result<()> {
        self.source.client.disconnect().await
    }
    /// Retained source history, including a cancelled or failed recovery claim.
    pub async fn source_history(&self) -> OperationHistory {
        self.source.operation_history().await
    }
    /// Explicit once-only login using the original endpoint/profile/version.
    /// New same-profile login and fresh admission establish the recovery boundary.
    /// No Blueprint, permission or old native operation is transferred.
    pub async fn reconnect(
        &self,
        config: ConnectionConfig,
        target: MiningRecoveryTarget,
    ) -> Result<RecoveredSurvivalClient> {
        let recovered = self
            .source
            .native
            .reconnect_survival_mining_profile(&self.watch, config, target)
            .await?;
        let client = recovered.client();
        Ok(RecoveredSurvivalClient {
            operations: client.survival()?,
            client,
            evidence: recovered.evidence,
        })
    }
}

/// Explicit retirement phases bound to the original miner and exact observer.
/// Retain this handle across cancelled waits. It cannot be restored from JSON.
/// Clones share the native once-only reconnect guard; dropping never reconnects.
#[derive(Clone)]
pub struct MiningRetirement {
    source: Operations,
    observer: Operations,
    watch: native::MiningRetirementWatch,
}
impl MiningRetirement {
    /// Close the original sender after registering the watch. Local closure alone
    /// is not retirement. Cancellation never clears history or the watch.
    pub async fn close_source(&self) -> Result<()> {
        self.source.client.disconnect().await
    }
    /// Original before-I/O intent and subsequent history, even if observation fails.
    pub async fn source_history(&self) -> OperationHistory {
        self.source.operation_history().await
    }
    /// Inspect exact removal/local closure, conflicts and whether reconnect began.
    /// An observer error is returned intact; original history remains accessible.
    pub async fn observe(&self) -> Result<MiningRetirementStatus> {
        self.source
            .native
            .observe_survival_mining_retirement(&self.watch, &self.observer.native)
            .await
    }
    /// Read-only bounded wait. Pending or cancellation is resumable through this
    /// handle, without reconstructing the miner/observer/watch combination.
    pub async fn wait(&self, maximum_wait: Duration) -> Result<MiningRetirementStatus> {
        self.source
            .native
            .wait_survival_mining_retirement(&self.watch, &self.observer.native, maximum_wait)
            .await
    }
    /// Explicit once-only reconnect after exact retirement. Requires the original
    /// endpoint/profile/version and declared air or original target. Revalidates
    /// fresh loading, health, inventory, position, dimension and target before
    /// exposing operations. Failure/cancellation cannot authorize another login.
    /// No caller plan, permission or resource reservation is transferred.
    pub async fn reconnect(
        &self,
        config: ConnectionConfig,
        expected_target: NativeBlockState,
    ) -> Result<RecoveredSurvivalClient> {
        let recovered = self
            .source
            .native
            .reconnect_survival_mining(&self.watch, &self.observer.native, config, expected_target)
            .await?;
        let client = recovered.client();
        let operations = client.survival()?;
        Ok(RecoveredSurvivalClient {
            client,
            operations,
            evidence: recovered.evidence,
        })
    }
}

/// Fresh validated connection; caller must build a new permission-checked plan.
pub struct RecoveredSurvivalClient {
    /// Same new session for generic observation, tracing and explicit closure.
    pub client: Client,
    /// Checked operations on that session, with no inherited job authority.
    pub operations: Operations,
    /// Old history, retirement receipt and new baselines for caller diagnosis.
    pub evidence: MiningRecoveryEvidence,
}
