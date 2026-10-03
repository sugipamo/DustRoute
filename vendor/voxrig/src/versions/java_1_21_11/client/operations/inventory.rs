//! Ordinary player-inventory swaps. Submitted clicks never predict received slots.
use super::*;
use crate::diagnostic_projection::diagnostic_record;
use std::time::Duration;

diagnostic_record! {
    /// One submitted SWAP click, tied to this connection and received baseline.
    /// It is not an acknowledgement and cannot be restored from serialized history.
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct InventorySwap => RecordedInventorySwap {
        /// Owning native connection.
        pub connection_id: u64,
        /// Receive boundary immediately before submission.
        pub after_sequence: u64,
        /// Received screen revision sent with the click.
        pub screen_revision: i32,
        /// Main-inventory screen slot, 9..35.
        pub main_slot: u8,
        /// Hotbar index, 0..8 (screen slot 36..44).
        pub hotbar: u8,
        /// Received main-inventory stack before submission.
        pub main_before: InventorySlot,
        /// Received hotbar stack before submission.
        pub hotbar_before: InventorySlot,
    }
    diagnostic_serde {}
}

diagnostic_record! {
    /// Both swap destinations were received after submission with the expected stacks.
    /// This is client receive evidence, not an independent server-side receipt.
    #[derive(Clone, Debug, Serialize)]
    pub struct InventorySwapObservation => RecordedInventorySwapObservation {
        /// Original submission, with its received predecessor.
        pub submission: InventorySwap,
        /// Receive boundary at verification.
        pub receive_sequence: u64,
        /// Received main destination after submission.
        pub main_sequence: u64,
        /// Received hotbar destination after submission.
        pub hotbar_sequence: u64,
    }
    diagnostic_serde {}
}

fn unavailable(message: &str) -> Error {
    Error::new(ErrorKind::State, anyhow::anyhow!("{message}"))
}

fn prepare(
    inventory: &Inventory,
    connection_id: u64,
    after_sequence: u64,
    main_slot: u8,
    hotbar: u8,
) -> Result<(InventorySwap, Vec<u8>)> {
    if !(9..=35).contains(&main_slot) || hotbar > 8 {
        return Err(invalid("swap requires main slot 9..35 and hotbar 0..8"));
    }
    if inventory.pending_swap.is_some() || !inventory.pending_creative.is_empty() {
        return Err(unavailable("prior inventory mutation needs inspection"));
    }
    if inventory.window_id != Some(0)
        || inventory.cursor != InventorySlot::Empty
        || inventory.unsupported_components
    {
        return Err(unavailable(
            "received player screen and empty supported cursor required",
        ));
    }
    let screen_revision = inventory
        .screen_revision
        .ok_or_else(|| unavailable("player screen revision unavailable"))?;
    let main_before = inventory.slots[usize::from(main_slot)].clone();
    let hotbar_before = inventory.slots[36 + usize::from(hotbar)].clone();
    if matches!(main_before, InventorySlot::Unavailable)
        || matches!(hotbar_before, InventorySlot::Unavailable)
    {
        return Err(unavailable("swap slot contents unavailable"));
    }
    for stack in [&main_before, &hotbar_before] {
        if let InventorySlot::Item { item } = stack {
            if items()
                .iter()
                .find(|definition| definition.id == item.item_id)
                .is_none_or(|definition| {
                    item.count <= 0
                        || item.count > definition.stack_size
                        || item.name != format!("minecraft:{}", definition.name)
                })
            {
                return Err(invalid(
                    "ordinary swap requires a valid default stack within its native stack limit",
                ));
            }
        }
    }
    if main_before == hotbar_before {
        return Err(invalid("identical slot contents do not require a swap"));
    }
    let submission = InventorySwap {
        connection_id,
        after_sequence,
        screen_revision,
        main_slot,
        hotbar,
        main_before,
        hotbar_before,
    };
    // Native 1.21.11: container 0, received revision, clicked screen slot,
    // hotbar button, SWAP (2), empty modified-hash map, empty cursor hash.
    // Do not advertise predicted hashes: the native handler sends the actual
    // changed slots against its retained client baseline. A revision mismatch
    // causes a resync AFTER applying the click, not a rejection before mutation.
    let mut payload = vec![0];
    put_varint(&mut payload, screen_revision);
    payload.extend(i16::from(main_slot).to_be_bytes());
    payload.extend([hotbar, 2, 0, 0]);
    Ok((submission, payload))
}

fn observed(
    inventory: &Inventory,
    submission: &InventorySwap,
    receive_sequence: u64,
) -> Option<InventorySwapObservation> {
    if inventory.pending_swap.as_ref() != Some(submission)
        || inventory.window_id != Some(0)
        || inventory.cursor != InventorySlot::Empty
        || inventory.unsupported_components
    {
        return None;
    }
    let main = usize::from(submission.main_slot);
    let hotbar = 36 + usize::from(submission.hotbar);
    let main_sequence = inventory.slot_sequences[main]?;
    let hotbar_sequence = inventory.slot_sequences[hotbar]?;
    if main_sequence <= submission.after_sequence
        || hotbar_sequence <= submission.after_sequence
        || inventory.slots[main] != submission.hotbar_before
        || inventory.slots[hotbar] != submission.main_before
    {
        return None;
    }
    Some(InventorySwapObservation {
        submission: submission.clone(),
        receive_sequence,
        main_sequence,
        hotbar_sequence,
    })
}

impl Operations {
    /// Submit one ordinary SWAP click in received survival mode. Both occupied
    /// and empty hotbar destinations are supported for plain stacks.
    /// The pending marker is set before writing and remains on errors/cancellation.
    /// Call `wait_inventory_swap` to establish the received result; do not retry
    /// an unresolved click, since a stale revision does not prevent its execution.
    pub async fn swap_player_hotbar(&self, main_slot: u8, hotbar: u8) -> Result<InventorySwap> {
        let mut state = self.bot.session.state.lock().await;
        self.mutable(&state)?;
        if state.operations.game_mode != Some(GameMode::Survival) {
            return Err(unavailable("swap requires received survival mode"));
        }
        let (submission, payload) = prepare(
            &state.operations.inventory,
            self.bot.session.id,
            state.sequence,
            main_slot,
            hotbar,
        )?;
        state.operations.inventory.pending_swap = Some(submission.clone());
        self.bot
            .session
            .send(ids::play_serverbound::WINDOW_CLICK, &payload)
            .await?;
        Ok(submission)
    }

    /// Wait for both received destination slots, without sending another click.
    /// Timeout or cancellation leaves the swap pending. An owning caller may
    /// resume this read-only wait on the same connection using the same submission.
    pub async fn wait_inventory_swap(
        &self,
        submission: &InventorySwap,
        maximum_wait: Duration,
    ) -> Result<InventorySwapObservation> {
        if maximum_wait.is_zero() || maximum_wait > Duration::from_secs(30) {
            return Err(invalid(
                "inventory wait must be greater than zero and at most 30 seconds",
            ));
        }
        if submission.connection_id != self.bot.session.id {
            return Err(unavailable(
                "inventory submission belongs to another connection",
            ));
        }
        timeout(maximum_wait, async {
            loop {
                let notified = self.bot.session.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                {
                    let mut state = self.bot.session.state.lock().await;
                    self.ready(&state)?;
                    if state.operations.game_mode != Some(GameMode::Survival)
                        || state.operations.inventory.pending_swap.as_ref() != Some(submission)
                    {
                        return Err(unavailable(
                            "inventory swap context changed; inspect received state",
                        ));
                    }
                    if let Some(result) =
                        observed(&state.operations.inventory, submission, state.sequence)
                    {
                        state.operations.inventory.pending_swap = None;
                        return Ok(result);
                    }
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| {
            Error::new(
                ErrorKind::Timeout,
                anyhow::anyhow!(
                    "inventory swap result not received; inspect pending swap without retry"
                ),
            )
        })?
    }
}

fn invalidate(inventory: &mut Inventory, window: Option<i32>) {
    inventory.window_id = window;
    inventory.screen_revision = None;
    inventory.cursor = InventorySlot::Unavailable;
    inventory.slots.fill(InventorySlot::Unavailable);
    inventory.slot_sequences.fill(None);
}

pub(super) fn receive(
    inventory: &mut Inventory,
    id: i32,
    payload: &[u8],
    sequence: u64,
) -> anyhow::Result<()> {
    use ids::play_clientbound as input;
    let mut r = Reader::new(payload);
    match id {
        input::OPEN_WINDOW => {
            let window = r.count(i32::MAX as usize)? as i32;
            if window == 0 {
                bail!("open container cannot use player screen ID");
            }
            r.count(i32::MAX as usize)?;
            r.skip_nbt()?;
            r.end()?;
            invalidate(inventory, Some(window));
        }
        input::CLOSE_WINDOW => {
            let window = r.varint()?;
            r.end()?;
            if inventory.window_id == Some(window) {
                invalidate(inventory, None);
            }
        }
        input::WINDOW_ITEMS => {
            let window = r.varint()?;
            let revision = r.count(i32::MAX as usize)? as i32;
            let count = r.count(1024)?;
            if window != 0 {
                invalidate(inventory, Some(window));
                return Ok(());
            }
            if count != 46 {
                bail!("unexpected player inventory size");
            }
            let mut slots = Vec::with_capacity(count);
            for _ in 0..count {
                let Some(value) = slot(&mut r)? else {
                    invalidate(inventory, inventory.window_id);
                    inventory.unsupported_components = true;
                    inventory.receive_sequence = Some(sequence);
                    return Ok(());
                };
                slots.push(value);
            }
            let Some(cursor) = slot(&mut r)? else {
                invalidate(inventory, inventory.window_id);
                inventory.unsupported_components = true;
                inventory.receive_sequence = Some(sequence);
                return Ok(());
            };
            r.end()?;
            inventory.slots = slots;
            inventory.slot_sequences.fill(Some(sequence));
            if inventory.window_id.is_none_or(|w| w == 0) {
                inventory.window_id = Some(0);
                inventory.screen_revision = Some(revision);
                inventory.cursor = cursor;
            }
            inventory.pending_creative.clear();
            inventory.unsupported_components = false;
            inventory.receive_sequence = Some(sequence);
        }
        input::SET_CURSOR_ITEM => {
            let value = slot(&mut r)?;
            inventory.cursor = if let Some(value) = value {
                r.end()?;
                value
            } else {
                inventory.unsupported_components = true;
                InventorySlot::Unavailable
            };
            inventory.receive_sequence = Some(sequence);
        }
        input::SET_SLOT | input::SET_PLAYER_INVENTORY => {
            let index = if id == input::SET_SLOT {
                let window = r.varint()?;
                let revision = r.count(i32::MAX as usize)? as i32;
                let index = r.u16()? as i16;
                if window != 0 {
                    invalidate(inventory, Some(window));
                    return Ok(());
                }
                if inventory.window_id == Some(0) {
                    inventory.screen_revision = Some(revision);
                }
                usize::try_from(index).context("negative player slot")?
            } else {
                match r.count(40)? {
                    v @ 0..=8 => v + 36,
                    v @ 9..=35 => v,
                    v @ 36..=39 => 44 - v,
                    40 => 45,
                    _ => unreachable!(),
                }
            };
            if index >= 46 {
                bail!("invalid player slot");
            }
            let value = slot(&mut r)?;
            inventory.slots[index] = if let Some(value) = value {
                r.end()?;
                inventory
                    .pending_creative
                    .retain(|s| 36 + usize::from(*s) != index);
                value
            } else {
                inventory.unsupported_components = true;
                InventorySlot::Unavailable
            };
            inventory.slot_sequences[index] = Some(sequence);
            inventory.receive_sequence = Some(sequence);
        }
        _ => unreachable!("inventory packet dispatch"),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
