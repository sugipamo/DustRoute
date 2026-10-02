//! Bounded empty-hand cube mining. Cancellation never authorizes a safe abort.
use super::*;
use std::time::Duration;

#[cfg(test)]
mod tests;

/// One owning connection's removal intent, stored before sending START.
/// Serialized inspection cannot recreate a capability on another connection.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MiningIntent {
    /// Owning connection.
    pub connection_id: u64,
    /// Receive boundary before START.
    pub after_sequence: u64,
    /// Globally increasing interaction sequence for START.
    pub start_sequence: i32,
    /// Owning dimension.
    pub dimension: String,
    /// Exact requested removal cell.
    pub target: [i32; 3],
    /// Native entry face ID 0..5.
    pub face_id: u8,
    /// Complete received predecessor; never inferred from the item registry.
    pub baseline: crate::NativeBlockState,
    /// Received stationary feet position before submission.
    pub position: [f64; 3],
    /// Empty main-hand slot selected by an ordered send or server update.
    pub selection: HotbarSelection,
    /// Receive sequence which established that slot as empty.
    pub held_receive_sequence: u64,
    /// Local scheduling estimate only; no server-tick or cancellation guarantee.
    pub estimated_wait_ms: u64,
}
/// Separate send attempt retained even if its caller is cancelled.
#[derive(Clone, Debug, Serialize)]
pub struct MiningSend {
    /// Connection-global interaction sequence.
    pub sequence: i32,
    /// Receive boundary before the send attempt.
    pub after_sequence: u64,
    /// Entire packet write completed; never a server acceptance receipt.
    pub dispatched: bool,
}
/// A target-specific received update, independent of global cache revision.
#[derive(Clone, Debug, Serialize)]
pub struct MiningTargetReceipt {
    /// Exact native state from a block/section packet applied to the loaded target.
    pub state: crate::NativeBlockState,
    /// That packet's receive ordinal.
    pub receive_sequence: u64,
}
/// Retained observation of the requested result; not a current world snapshot
/// and does not prove which actor removed it.
#[derive(Clone, Debug, Serialize)]
pub struct MiningRemoval {
    /// Original intent and received predecessor.
    pub intent: MiningIntent,
    /// Fresh received air at the exact target.
    pub target_receipt: MiningTargetReceipt,
    /// False: observed air does not establish that server delayed mining has
    /// cleared. No next mutation is authorized on this connection yet.
    pub continuation_validated: bool,
}
/// Last mining attempt, retained on errors, timeouts, context changes and closure.
#[derive(Clone, Debug, Serialize)]
pub struct MiningRecord {
    /// Stored before the first possible mutation.
    pub intent: MiningIntent,
    /// Complete START frame was dispatched.
    pub start_dispatched: bool,
    /// At most one explicit FINISH attempt, retained before I/O.
    pub finish: Option<MiningSend>,
    /// At most one explicit ABORT attempt; never resolves delayed mining by itself.
    pub abort: Option<MiningSend>,
    /// Latest target-specific received packet after START.
    pub target_receipt: Option<MiningTargetReceipt>,
    /// Latched context/conflict reason. Later air cannot silently clear it.
    pub requires_inspection: Option<String>,
    /// Result established by a read-only observation. Even Some does not currently
    /// authorize in-session continuation; that release remains unimplemented.
    pub removal: Option<MiningRemoval>,
}
/// Explicit result states; a pending timeout is never a safe cancellation.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MiningStatus {
    /// START attempted, without a FINISH attempt or a confirmed result.
    Mining {
        /// Retained start and target evidence.
        record: MiningRecord,
    },
    /// FINISH attempted; ABORT/acknowledgement do not authorize another mutation.
    PendingAfterFinish {
        /// Retained finish/abort attempts; still unresolved.
        record: MiningRecord,
    },
    /// Exact target received as air after submission, with preserved context.
    ObservedRemoved {
        /// Received result, without attribution of its actor.
        observation: MiningRemoval,
    },
    /// Changed/missing context or target requires inspection; intent remains pending.
    RequiresInspection {
        /// Original intent and latched diagnosis.
        record: MiningRecord,
    },
}

fn unavailable(message: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::State, anyhow::anyhow!("{message}"))
}
fn owning(state: &State, intent: &MiningIntent, connection_id: u64) -> Result<()> {
    if intent.connection_id != connection_id
        || state.mining.as_ref().is_none_or(|m| m.intent != *intent)
    {
        return Err(unavailable(
            "mining intent belongs to another connection or attempt",
        ));
    }
    Ok(())
}
fn empty_hand(state: &State) -> Result<(HotbarSelection, u64)> {
    let inventory = &state.operations.inventory;
    let selection = state
        .operations
        .selected_hotbar
        .clone()
        .filter(|s| s.dispatched)
        .ok_or_else(|| unavailable("main-hand selection unavailable or incomplete"))?;
    let slot = 36 + usize::from(selection.slot);
    if inventory.window_id != Some(0)
        || inventory.cursor != InventorySlot::Empty
        || inventory.unsupported_components
        || inventory.pending_swap.is_some()
        || !inventory.pending_creative.is_empty()
        || inventory.slots[slot] != InventorySlot::Empty
    {
        return Err(unavailable(
            "mining requires a received empty selected hand and supported player screen",
        ));
    }
    let sequence = inventory.slot_sequences[slot]
        .ok_or_else(|| unavailable("selected empty-hand receive evidence unavailable"))?;
    Ok((selection, sequence))
}
fn prepared(
    state: &mut State,
    connection_id: u64,
    tick: u64,
    target: [i32; 3],
    face: u8,
) -> Result<(StandingContext, crate::NativeBlockState)> {
    if state.operations.game_mode != Some(GameMode::Survival) {
        return Err(unavailable("mining requires received survival mode"));
    }
    let standing = survival::context(state, connection_id, tick)?;
    if !standing.on_ground
        || standing.submerged
        || standing.support.contains(&target)
        || !standing
            .player
            .health
            .as_ref()
            .is_some_and(|h| h.health > 0.0)
    {
        return Err(unavailable(
            "mining requires known healthy dry ground and may not remove its foot support",
        ));
    }
    if standing.player.block_break_speed.map(|v| v.value) != Some(1.0)
        || standing.player.mining_efficiency.map(|v| v.value) != Some(0.0)
        || !standing.player.effect_updates.is_empty()
    {
        return Err(unavailable(
            "modified or unavailable mining conditions need inspection",
        ));
    }
    // An empty effect map is NOT a complete-list fence. The estimated wait below
    // is scheduling only; received removal remains the acceptance criterion.
    empty_hand(state)?;
    let hit = super::super::raycast::stationary_outline_hit(state, standing.eye_position, 4.5)?
        .ok_or_else(|| unavailable("no available native outline hit for mining"))?;
    survival::uncertain_target(state, &standing, &hit)?;
    if hit.position != target || hit.face.map(|f| f as u8) != Some(face) {
        return Err(unavailable(
            "target/face differs from the current first native outline hit",
        ));
    }
    admitted_material(&hit.state)?;
    Ok((standing, hit.state))
}
// Shared material admission for real mining and hypothetical removal geometry.
pub(super) fn admitted_material(state: &crate::NativeBlockState) -> Result<()> {
    if !matches!(state.name.as_str(), "minecraft:dirt" | "minecraft:stone")
        || !state.properties.is_empty()
    {
        return Err(Error::new(
            ErrorKind::Unsupported,
            anyhow::anyhow!("empty-hand mining currently admits dirt and stone only"),
        ));
    }
    Ok(())
}
fn packet(intent: &MiningIntent, action: u8, sequence: i32) -> Vec<u8> {
    let mut payload = vec![action];
    payload.extend(pack_position(intent.target).to_be_bytes());
    payload.push(intent.face_id);
    put_varint(&mut payload, sequence);
    payload
}

impl Operations {
    /// Begin one bounded stationary empty-hand dirt/stone removal. No other
    /// mutation is allowed afterward, even on observed removal, until a separate
    /// continuation boundary is implemented and verified. Caller owns
    /// site/edit permissions; this native API cannot infer Blueprint permissions.
    pub async fn start_survival_mining(
        &self,
        target: [i32; 3],
        face: crate::BlockFace,
    ) -> Result<MiningIntent> {
        let mut state = self.bot.session.state.lock().await;
        self.mutable(&state)?;
        let tick = self.bot.session.started.elapsed().as_millis() as u64 / 50;
        let (standing, baseline) =
            prepared(&mut state, self.bot.session.id, tick, target, face as u8)?;
        let (selection, held_receive_sequence) = empty_hand(&state)?;
        let intent = MiningIntent {
            connection_id: self.bot.session.id,
            after_sequence: state.sequence,
            start_sequence: self.next_sequence()?,
            dimension: standing.dimension,
            target,
            face_id: face as u8,
            position: standing.position,
            selection,
            held_receive_sequence,
            estimated_wait_ms: if baseline.name == "minecraft:dirt" {
                1100
            } else {
                8500
            },
            baseline,
        };
        state.mining = Some(MiningRecord {
            intent: intent.clone(),
            start_dispatched: false,
            finish: None,
            abort: None,
            target_receipt: None,
            requires_inspection: None,
            removal: None,
        });
        self.bot
            .session
            .send(
                ids::play_serverbound::BLOCK_DIG,
                &packet(&intent, 0, intent.start_sequence),
            )
            .await?;
        state
            .mining
            .as_mut()
            .expect("stored mining intent")
            .start_dispatched = true;
        Ok(intent)
    }

    /// Attempt FINISH once for the original target. An early finish can schedule
    /// a delayed server break. No elapsed duration or acknowledgement resolves it.
    pub async fn finish_survival_mining(&self, intent: &MiningIntent) -> Result<i32> {
        self.mining_send(intent, false).await
    }
    /// Attempt ABORT once. This stops ordinary native mining, but does not prove
    /// that a delayed FINISH was cleared; pending intent remains until observation.
    pub async fn abort_survival_mining(&self, intent: &MiningIntent) -> Result<i32> {
        self.mining_send(intent, true).await
    }
    async fn mining_send(&self, intent: &MiningIntent, abort: bool) -> Result<i32> {
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        owning(&state, intent, self.bot.session.id)?;
        let record = state.mining.as_ref().expect("owning intent");
        if !record.start_dispatched
            || record.removal.is_some()
            || record.requires_inspection.is_some()
            || (if abort {
                record.abort.is_some()
            } else {
                record.finish.is_some() || record.abort.is_some()
            })
            || state.world.dimension.as_ref().map(|d| &d.0) != Some(&intent.dimension)
        {
            return Err(unavailable(
                "mining send cannot resume/replay this stage or changed context",
            ));
        }
        if !abort {
            let tick = self.bot.session.started.elapsed().as_millis() as u64 / 50;
            let validation = prepared(
                &mut state,
                self.bot.session.id,
                tick,
                intent.target,
                intent.face_id,
            );
            match validation {
                Ok((standing, block))
                    if block == intent.baseline
                        && standing.position == intent.position
                        && empty_hand(&state)?.0 == intent.selection => {}
                outcome => {
                    let reason = match outcome {
                        Err(e) => e.to_string(),
                        _ => "mining baseline or player context changed".into(),
                    };
                    mining_world_changed(&mut state, &reason);
                    return Err(unavailable(reason));
                }
            }
        }
        let sequence = self.next_sequence()?;
        let attempt = MiningSend {
            sequence,
            after_sequence: state.sequence,
            dispatched: false,
        };
        let record = state.mining.as_mut().expect("owning intent");
        if abort {
            record.abort = Some(attempt);
        } else {
            record.finish = Some(attempt);
        }
        self.bot
            .session
            .send(
                ids::play_serverbound::BLOCK_DIG,
                &packet(intent, if abort { 1 } else { 2 }, sequence),
            )
            .await?;
        let record = state.mining.as_mut().expect("owning intent");
        (if abort {
            &mut record.abort
        } else {
            &mut record.finish
        })
        .as_mut()
        .expect("send attempt")
        .dispatched = true;
        Ok(sequence)
    }

    /// Read-only result reconciliation. Once removal is observed, that result is
    /// retained as history; use a fresh world query for the current target.
    /// Exact target freshness is required for first confirmation; cached air,
    /// unrelated packets, acknowledgements and ABORT never stand in for removal.
    pub async fn observe_survival_mining(&self, intent: &MiningIntent) -> Result<MiningStatus> {
        let mut state = self.bot.session.state.lock().await;
        self.bot.session.check(&state)?;
        owning(&state, intent, self.bot.session.id)?;
        Ok(status(&mut state))
    }
    /// Wait without resending. Timeout returns a pending status and retains the
    /// intent; a cancelled future does the same. Closed connections expose history.
    pub async fn wait_survival_mining(
        &self,
        intent: &MiningIntent,
        maximum_wait: Duration,
    ) -> Result<MiningStatus> {
        if maximum_wait.is_zero() || maximum_wait > Duration::from_secs(30) {
            return Err(invalid(
                "mining wait must be greater than zero and at most 30 seconds",
            ));
        }
        let result = timeout(maximum_wait, async {
            loop {
                let notified = self.bot.session.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let observed = self.observe_survival_mining(intent).await?;
                if matches!(
                    observed,
                    MiningStatus::ObservedRemoved { .. } | MiningStatus::RequiresInspection { .. }
                ) {
                    return Ok(observed);
                }
                notified.await;
            }
        })
        .await;
        match result {
            Ok(result) => result,
            Err(_) => self.observe_survival_mining(intent).await,
        }
    }
    /// Convenience scheduling only. Cancellation never sends an automatic abort
    /// or clears the intent. Resume by inspecting the stored stage, not replaying.
    pub async fn dig_survival_cube(
        &self,
        target: [i32; 3],
        face: crate::BlockFace,
        maximum_wait: Duration,
    ) -> Result<MiningStatus> {
        if maximum_wait.is_zero() || maximum_wait > Duration::from_secs(30) {
            return Err(invalid(
                "mining wait must be greater than zero and at most 30 seconds",
            ));
        }
        let intent = self.start_survival_mining(target, face).await?;
        tokio::time::sleep(Duration::from_millis(intent.estimated_wait_ms)).await;
        let observed = self.observe_survival_mining(&intent).await?;
        if matches!(
            observed,
            MiningStatus::ObservedRemoved { .. } | MiningStatus::RequiresInspection { .. }
        ) {
            return Ok(observed);
        }
        self.finish_survival_mining(&intent).await?;
        self.wait_survival_mining(&intent, maximum_wait).await
    }
}

fn status(state: &mut State) -> MiningStatus {
    let record = state.mining.as_mut().expect("owning intent checked");
    if let Some(observation) = &record.removal {
        return MiningStatus::ObservedRemoved {
            observation: observation.clone(),
        };
    }
    if state.world.dimension.as_ref().map(|d| &d.0) != Some(&record.intent.dimension)
        || !state.ready
    {
        record
            .requires_inspection
            .get_or_insert_with(|| "mining world context unavailable or changed".into());
    }
    let cell = state
        .reconstruction
        .cell(&state.world, record.intent.target);
    if cell.moving.is_some()
        || cell.state.is_none()
        || state.reconstruction.issue.is_some()
        || !state.reconstruction.recovery_chunks.is_empty()
    {
        record
            .requires_inspection
            .get_or_insert_with(|| "mining target reconstruction unavailable or moving".into());
    }
    if record.requires_inspection.is_some() {
        return MiningStatus::RequiresInspection {
            record: record.clone(),
        };
    }
    if let Some(receipt) = &record.target_receipt
        && receipt.receive_sequence > record.intent.after_sequence
        && matches!(
            receipt.state.name.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        )
        && cell.state.as_ref() == Some(&receipt.state)
    {
        let observation = MiningRemoval {
            intent: record.intent.clone(),
            target_receipt: receipt.clone(),
            continuation_validated: false,
        };
        record.removal = Some(observation.clone());
        return MiningStatus::ObservedRemoved { observation };
    }
    if record.finish.is_some() {
        MiningStatus::PendingAfterFinish {
            record: record.clone(),
        }
    } else {
        MiningStatus::Mining {
            record: record.clone(),
        }
    }
}

pub(in crate::versions::java_1_21_11::client) fn mining_world_changed(
    state: &mut State,
    reason: &str,
) {
    if let Some(watch) = &mut state.retirement {
        watch
            .requires_inspection
            .get_or_insert_with(|| reason.into());
    }
    if let Some(record) = &mut state.mining
        && record.removal.is_none()
    {
        record
            .requires_inspection
            .get_or_insert_with(|| reason.into());
    }
}
pub(in crate::versions::java_1_21_11::client) fn mining_chunk_changed(
    state: &mut State,
    chunk: [i32; 2],
) {
    if state.mining.as_ref().is_some_and(|m| {
        m.removal.is_none()
            && [
                m.intent.target[0].div_euclid(16),
                m.intent.target[2].div_euclid(16),
            ] == chunk
    }) {
        mining_world_changed(
            state,
            "target chunk unloaded or replaced; fresh inspection required",
        );
    }
}
pub(in crate::versions::java_1_21_11::client) fn mining_received(
    state: &mut State,
    changes: &[([i32; 3], i32)],
) -> anyhow::Result<()> {
    let Some(record) = &mut state.mining else {
        return Ok(());
    };
    if record.removal.is_some() {
        return Ok(());
    }
    for (position, id) in changes {
        if *position != record.intent.target {
            continue;
        }
        let block = super::super::super::native_state(*id)?;
        if state.world.block(*position) != Some(*id) {
            record
                .requires_inspection
                .get_or_insert_with(|| "target update without loaded baseline".into());
            continue;
        }
        if block != record.intent.baseline
            && !matches!(
                block.name.as_str(),
                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
            )
        {
            record
                .requires_inspection
                .get_or_insert_with(|| "target changed to an unexpected non-air state".into());
        }
        record.target_receipt = Some(MiningTargetReceipt {
            state: block,
            receive_sequence: state.sequence,
        });
    }
    Ok(())
}
