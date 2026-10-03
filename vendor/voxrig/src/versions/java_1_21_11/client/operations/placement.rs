//! One-shot ordinary passive-cube placement with independent world/material receipts.
use super::geometry::GeometryView;
use super::*;
use crate::diagnostic_projection::diagnostic_record;
use std::time::Duration;

diagnostic_record! {
    /// A single main-hand placement, retained before any packet write.
    #[derive(Clone, Debug, PartialEq, Serialize)]
    pub struct PlacementIntent => RecordedPlacementIntent {
        /// Owning live connection; serialized history cannot create an operation.
        pub connection_id: u64,
        /// Current loading/world generation.
        pub generation: u64,
        /// Receive boundary before submission.
        pub after_sequence: u64,
        /// Connection-global one-shot block-use sequence.
        pub sequence: i32,
        /// Observed dimension.
        pub dimension: String,
        /// Stationary feet position.
        pub position: [f64; 3],
        /// Nonreplaceable cube whose face is clicked.
        pub support: [i32; 3],
        /// Exact received predecessor at the clicked support.
        pub support_state: crate::NativeBlockState,
        /// Native face ID, 0..5.
        pub face_id: u8,
        /// Derived native face-hit cursor, local to support.
        pub cursor: [f32; 3],
        /// Adjacent air cell to receive the block.
        pub target: [i32; 3],
        /// Received target predecessor; never inferred air.
        pub before: crate::NativeBlockState,
        /// Expected passive block, with no state properties.
        pub expected: crate::NativeBlockState,
        /// Ordered main-hand selection used by this action.
        pub selection: HotbarSelection,
        /// Received component-free selected stack before placement.
        pub held_before: PlainItem,
        /// Per-slot receipt boundary for that predecessor.
        pub held_receive_sequence: u64,
    }
    diagnostic_serde {}
}
diagnostic_record! {
    /// Both received outcomes, not attribution to a particular actor.
    #[derive(Clone, Debug, Serialize)]
    pub struct PlacementObservation => RecordedPlacementObservation {
        /// Original operation and predecessors.
        pub intent: PlacementIntent,
        /// Fresh target-specific block/section update.
        pub target_receive_sequence: u64,
        /// Fresh selected-slot update showing exactly one material consumed.
        pub inventory_receive_sequence: u64,
        /// Exact remaining selected stack (possibly empty).
        pub held_after: InventorySlot,
        /// Native one-shot interaction sequence processed; never success by itself.
        pub acknowledged_sequence: i32,
    }
    diagnostic_serde {}
}
diagnostic_record! {
    /// Retained stage; timeout/cancellation never permits a blind second use.
    #[derive(Clone, Debug, Serialize)]
    pub struct PlacementRecord => RecordedPlacementRecord {
        /// Stored before I/O.
        pub intent: PlacementIntent,
        /// Complete frame submitted, not server acceptance.
        pub dispatched: bool,
        /// Latest exact target receipt.
        pub target_receive_sequence: Option<u64>,
        /// Latest receipt showing exactly one selected material consumed.
        pub inventory_receive_sequence: Option<u64>,
        /// Latched unexpected state/context.
        pub requires_inspection: Option<String>,
        /// Historical completed result. Each next operation revalidates its own site.
        pub observation: Option<PlacementObservation>,
    }
    diagnostic_serde {}
}
diagnostic_record! {
    /// Acknowledgement, target change and material consumption are distinct stages.
    #[derive(Clone, Debug, Serialize)]
    #[serde(tag = "status", rename_all = "snake_case")]
    pub enum PlacementStatus => RecordedPlacementStatus {
        /// One or more required receipts have not arrived. Do not resend.
        Pending {
            /// Retained unresolved attempt.
            record: PlacementRecord,
        },
        /// Both expected outcomes and processed sequence have been received.
        ObservedPlaced {
            /// Received placement and material outcome.
            observation: PlacementObservation,
        },
        /// A conflict requires fresh inspection, not automatic retry.
        RequiresInspection {
            /// Attempt and latched conflict.
            record: PlacementRecord,
        },
    }
    diagnostic_serde { #[serde(tag = "status", rename_all = "snake_case")] }
}
fn unavailable(message: &str) -> Error {
    Error::new(ErrorKind::State, anyhow::anyhow!("{message}"))
}
fn air(block: &crate::NativeBlockState) -> bool {
    matches!(
        block.name.as_str(),
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
    )
}
fn remaining(intent: &PlacementIntent) -> InventorySlot {
    if intent.held_before.count == 1 {
        InventorySlot::Empty
    } else {
        let mut item = intent.held_before.clone();
        item.count -= 1;
        InventorySlot::Item { item }
    }
}
fn offset(face: u8) -> [i32; 3] {
    [
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
        [-1, 0, 0],
        [1, 0, 0],
    ][usize::from(face)]
}
fn prepare(
    state: &mut State,
    connection_id: u64,
    tick: u64,
    support: [i32; 3],
    face: u8,
) -> Result<PlacementIntent> {
    if state.operations.game_mode != Some(GameMode::Survival) {
        return Err(unavailable("placement requires received survival mode"));
    }
    let standing = survival::context(state, connection_id, tick)?;
    if !standing.on_ground
        || standing.submerged
        || !standing
            .player
            .health
            .as_ref()
            .is_some_and(|h| h.health > 0.0)
    {
        return Err(unavailable(
            "placement requires healthy dry standing contact",
        ));
    }
    let selection = state
        .operations
        .selected_hotbar
        .clone()
        .filter(|s| s.dispatched)
        .ok_or_else(|| unavailable("selected hand unavailable or incomplete"))?;
    let inventory = &state.operations.inventory;
    let slot = 36 + usize::from(selection.slot);
    if inventory.window_id != Some(0)
        || inventory.cursor != InventorySlot::Empty
        || inventory.unsupported_components
        || inventory.pending_swap.is_some()
        || !inventory.pending_creative.is_empty()
    {
        return Err(unavailable(
            "placement requires supported player inventory with no pending transfer",
        ));
    }
    let InventorySlot::Item { item } = &inventory.slots[slot] else {
        return Err(unavailable(
            "selected building material unavailable or empty",
        ));
    };
    // This is a separate placement admission: stateful grass is valid ground,
    // but is not an exact state-free placement material.
    if !survival::DRY_CUBES.contains(&item.name.as_str()) || item.name == "minecraft:grass_block" {
        return Err(Error::new(
            ErrorKind::Unsupported,
            anyhow::anyhow!("placement requires an admitted passive cube material"),
        ));
    }
    let count = u8::try_from(item.count).map_err(|_| invalid("invalid material count"))?;
    if default_item(&item.name, count)? != *item {
        return Err(invalid(
            "material identity/count differs from native registry",
        ));
    }
    let held_before = item.clone();
    let held_receive_sequence = inventory.slot_sequences[slot]
        .ok_or_else(|| unavailable("held material receipt unavailable"))?;
    let geometry = placement_geometry(
        state,
        standing.position,
        standing.bounds,
        standing.position_basis.geometry_reserve(),
        state.rotation,
        support,
        face,
    )?;
    let PlacementGeometry {
        target,
        before,
        hit,
        cursor,
    } = geometry;
    let expected = crate::NativeBlockState {
        name: item.name.clone(),
        properties: Default::default(),
    };
    super::super::super::state_id(&expected)?;
    Ok(PlacementIntent {
        connection_id,
        generation: state.loading.generation,
        after_sequence: state.sequence,
        sequence: 0,
        dimension: standing.dimension,
        position: standing.position,
        support,
        support_state: hit.state,
        face_id: face,
        cursor,
        target,
        before,
        expected,
        selection,
        held_before,
        held_receive_sequence,
    })
}
pub(super) struct PlacementGeometry {
    pub target: [i32; 3],
    pub before: crate::NativeBlockState,
    pub hit: super::super::raycast::BlockHit,
    pub cursor: [f32; 3],
}
// Shared native targeting and uncertainty checks; no inventory or authority.
pub(super) fn placement_geometry(
    view: &impl GeometryView,
    position: [f64; 3],
    bounds: [f64; 6],
    error: [f64; 3],
    rotation: [f32; 2],
    support: [i32; 3],
    face: u8,
) -> Result<PlacementGeometry> {
    validate_pose(position, rotation)?;
    if face > 5 {
        return Err(invalid("invalid placement face"));
    }
    let eye = [position[0], position[1] + f64::from(1.62f32), position[2]];
    let hit = super::super::raycast::outline_hit_in(eye, rotation, 4.5, |p| {
        view.block(p).map_err(anyhow::Error::from)
    })?
    .ok_or_else(|| unavailable("no first native outline hit for placement"))?;
    survival::uncertain_target_in(view, eye, error, rotation, &hit)?;
    if hit.position != support
        || hit.face.map(|f| f as u8) != Some(face)
        || !survival::DRY_CUBES.contains(&hit.state.name.as_str())
    {
        return Err(unavailable(
            "placement support must be the first hit on an admitted passive cube face",
        ));
    }
    let d = offset(face);
    let target = std::array::from_fn(|i| support[i] + d[i]);
    let before = view.block(target)?;
    if !air(&before)
        || (0..3)
            .all(|i| bounds[i] < f64::from(target[i] + 1) && bounds[i + 3] > f64::from(target[i]))
    {
        return Err(unavailable(
            "placement requires known empty air outside the standing body",
        ));
    }
    let cursor = super::super::raycast::hit_cursor_in(rotation, eye, &hit);
    Ok(PlacementGeometry {
        target,
        before,
        hit,
        cursor,
    })
}
fn packet(intent: &PlacementIntent) -> Vec<u8> {
    let mut p = vec![0]; // Main hand.
    p.extend(pack_position(intent.support).to_be_bytes());
    put_varint(&mut p, i32::from(intent.face_id));
    for v in intent.cursor {
        p.extend(v.to_be_bytes());
    }
    p.extend([0, 0]); // Outside shape, not against world border.
    put_varint(&mut p, intent.sequence);
    p
}
fn owning(state: &State, intent: &PlacementIntent, id: u64) -> Result<()> {
    if intent.connection_id != id || state.placement.as_ref().is_none_or(|p| p.intent != *intent) {
        return Err(unavailable(
            "placement intent belongs to another connection or attempt",
        ));
    }
    Ok(())
}
impl Operations {
    /// Submit one ordinary stationary placement into adjacent observed air.
    /// Caller owns site/edit permissions; this API grants no Blueprint authority.
    /// No automatic retry, local prediction, item creation or mode change.
    pub async fn place_survival_cube(
        &self,
        support: [i32; 3],
        face: crate::BlockFace,
    ) -> Result<PlacementIntent> {
        let mut state = self.bot.session.state.lock().await;
        self.mutable(&state)?;
        let mut intent = prepare(
            &mut state,
            self.bot.session.id,
            self.bot.session.started.elapsed().as_millis() as u64 / 50,
            support,
            face as u8,
        )?;
        intent.sequence = self.next_sequence()?;
        state.placement = Some(PlacementRecord {
            intent: intent.clone(),
            dispatched: false,
            target_receive_sequence: None,
            inventory_receive_sequence: None,
            requires_inspection: None,
            observation: None,
        });
        self.bot
            .session
            .send(ids::play_serverbound::BLOCK_PLACE, &packet(&intent))
            .await?;
        state.placement.as_mut().expect("intent stored").dispatched = true;
        Ok(intent)
    }
    /// Read received progress. Successful observation releases the one-shot gate;
    /// pending/conflicting observations never do. Closure preserves history.
    pub async fn observe_survival_placement(
        &self,
        intent: &PlacementIntent,
    ) -> Result<PlacementStatus> {
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        owning(&state, intent, self.bot.session.id)?;
        if let Some(observation) = &state.placement.as_ref().expect("owner").observation {
            return Ok(PlacementStatus::ObservedPlaced {
                observation: observation.clone(),
            });
        }
        placement_context_received(&mut state);
        let tick = self.bot.session.started.elapsed().as_millis() as u64 / 50;
        if let Err(e) = survival::context(&mut state, self.bot.session.id, tick) {
            state
                .placement
                .as_mut()
                .expect("owner")
                .requires_inspection
                .get_or_insert_with(|| e.to_string());
        }
        let cell = state.reconstruction.cell(&state.world, intent.target);
        let State {
            placement,
            operations,
            ..
        } = &mut *state;
        let record = placement.as_mut().expect("owner");
        if cell.moving.is_some() || cell.state.is_none() {
            record
                .requires_inspection
                .get_or_insert_with(|| "placement target unavailable or moving".into());
        }
        if record.requires_inspection.is_some() {
            return Ok(PlacementStatus::RequiresInspection {
                record: record.clone(),
            });
        }
        let inventory = &operations.inventory;
        let slot = 36 + usize::from(intent.selection.slot);
        if record.dispatched
            && record
                .target_receive_sequence
                .is_some_and(|s| s > intent.after_sequence)
            && cell.state.as_ref() == Some(&intent.expected)
            && inventory.slot_sequences[slot].is_some_and(|s| s > intent.after_sequence)
            && inventory.slots[slot] == remaining(intent)
            && operations.ack.is_some_and(|s| s >= intent.sequence)
        {
            let observation = PlacementObservation {
                intent: intent.clone(),
                target_receive_sequence: record.target_receive_sequence.unwrap(),
                inventory_receive_sequence: inventory.slot_sequences[slot].unwrap(),
                held_after: inventory.slots[slot].clone(),
                acknowledged_sequence: operations.ack.unwrap(),
            };
            record.observation = Some(observation.clone());
            return Ok(PlacementStatus::ObservedPlaced { observation });
        }
        Ok(PlacementStatus::Pending {
            record: record.clone(),
        })
    }
    /// Bounded read-only wait; timeout/cancellation retains the original intent.
    pub async fn wait_survival_placement(
        &self,
        intent: &PlacementIntent,
        maximum_wait: Duration,
    ) -> Result<PlacementStatus> {
        if maximum_wait.is_zero() || maximum_wait > Duration::from_secs(30) {
            return Err(invalid(
                "placement wait must be greater than zero and at most 30 seconds",
            ));
        }
        match timeout(maximum_wait, async {
            loop {
                let notified = self.bot.session.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let result = self.observe_survival_placement(intent).await?;
                if !matches!(result, PlacementStatus::Pending { .. }) {
                    return Ok(result);
                }
                notified.await;
            }
        })
        .await
        {
            Ok(result) => result,
            Err(_) => self.observe_survival_placement(intent).await,
        }
    }
}
/// Called after every successfully applied receive packet, so a conflicting
/// intermediate inventory/world state cannot be erased by a later matching one.
pub(in crate::versions::java_1_21_11::client) fn placement_context_received(state: &mut State) {
    let Some(r) = &mut state.placement else {
        return;
    };
    if r.observation.is_some() {
        return;
    }
    let i = &r.intent;
    let inventory = &state.operations.inventory;
    let slot = 36 + usize::from(i.selection.slot);
    let before = InventorySlot::Item {
        item: i.held_before.clone(),
    };
    let after = remaining(i);
    if inventory.slot_sequences[slot].is_some_and(|seq| seq > i.after_sequence) {
        if inventory.slots[slot] == after {
            r.inventory_receive_sequence = inventory.slot_sequences[slot];
        } else if r.inventory_receive_sequence.is_some() {
            r.requires_inspection.get_or_insert_with(|| {
                "consumed material changed again before placement confirmation".into()
            });
        }
    }
    if state.loading.generation != i.generation
        || !state.ready
        || state.position != Some(i.position)
        || state.world.dimension.as_ref().map(|d| &d.0) != Some(&i.dimension)
        || state.operations.game_mode != Some(GameMode::Survival)
        || state.operations.selected_hotbar.as_ref().map(|s| s.slot) != Some(i.selection.slot)
        || inventory.window_id != Some(0)
        || inventory.cursor != InventorySlot::Empty
        || inventory.unsupported_components
        || inventory.pending_swap.is_some()
        || (inventory.slots[slot] != before && inventory.slots[slot] != remaining(i))
    {
        r.requires_inspection
            .get_or_insert_with(|| "placement player/world/inventory context changed".into());
    }
}
pub(in crate::versions::java_1_21_11::client) fn placement_chunk_changed(
    state: &mut State,
    chunk: [i32; 2],
) {
    if let Some(r) = &mut state.placement
        && r.observation.is_none()
        && [r.intent.target, r.intent.support]
            .iter()
            .any(|p| [p[0].div_euclid(16), p[2].div_euclid(16)] == chunk)
    {
        r.requires_inspection
            .get_or_insert_with(|| "placement chunk unloaded or replaced".into());
    }
}
pub(in crate::versions::java_1_21_11::client) fn placement_received(
    state: &mut State,
    changes: &[([i32; 3], i32)],
) -> anyhow::Result<()> {
    let Some(r) = &mut state.placement else {
        return Ok(());
    };
    if r.observation.is_some() {
        return Ok(());
    }
    for (p, id) in changes {
        if *p != r.intent.target && *p != r.intent.support {
            continue;
        }
        let block = super::super::super::native_state(*id)?;
        if state.world.block(*p) != Some(*id)
            || (*p == r.intent.support && block != r.intent.support_state)
            || (*p == r.intent.target && block != r.intent.before && block != r.intent.expected)
        {
            r.requires_inspection.get_or_insert_with(|| {
                "placement target/support conflict or unavailable baseline".into()
            });
        }
        if *p == r.intent.target {
            // Once expected placement has arrived, its disappearance is a conflict.
            if r.target_receive_sequence.is_some() && block != r.intent.expected {
                r.requires_inspection.get_or_insert_with(|| {
                    "placed target changed before material confirmation".into()
                });
            }
            if block == r.intent.expected {
                r.target_receive_sequence = Some(state.sequence);
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
