//! World effects and one microstep transaction. Every effect is applied to a
//! cloned State; rejected work preserves the world, queues, IDs and trace prefix.
//! State and trace are committed together only after carrier validation.
use super::*;
use std::collections::BTreeSet;

impl<P: Clone + Eq> State<P> {
    fn apply_carriers(&mut self, effects: Vec<CarrierEffect>) -> Result<(), RuntimeError> {
        let mut touched = BTreeSet::new();
        for effect in effects {
            let position = effect.position();
            self.check_position(position)?;
            if !touched.insert(position) {
                return Err(RuntimeError::CarrierConflict(position));
            }
            let existing = self.carriers.get(&position).copied();
            let (after, history) = match effect {
                CarrierEffect::Stage { block, .. } => {
                    if existing.is_some()
                        || self.staged_carriers.contains_key(&position)
                        || block.kind != BlockKind::MovingPiston
                        || block.piston_entity.is_none()
                    {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    self.staged_carriers.insert(position, *block);
                    (None, None)
                }
                CarrierEffect::Install { history, .. } => {
                    if existing.is_some() {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    if let Some(block) = self.staged_carriers.remove(&position)
                        && self.world.get(position) != Some(&block)
                    {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    (Some(self.carrier_id()?), Some(history))
                }
                CarrierEffect::Update {
                    expected, history, ..
                } => {
                    if existing != Some(expected) {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    (Some(expected.id), Some(history))
                }
                CarrierEffect::Retire { expected, .. } => {
                    if existing != Some(expected) {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    (None, None)
                }
                CarrierEffect::Replace {
                    expected, history, ..
                } => {
                    if existing != Some(expected) {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    (Some(self.carrier_id()?), Some(history))
                }
            };
            if let (Some(id), Some(history)) = (after, history) {
                if history.last_progress > history.progress
                    || history.saved_world_time > self.time.game_tick
                {
                    return Err(RuntimeError::Invalid("inconsistent motion history".into()));
                }
                self.carriers.insert(position, CarrierState { id, history });
            } else {
                self.carriers.remove(&position);
            }
        }
        Ok(())
    }

    pub(super) fn validate_carriers(
        &self,
        before: &Self,
        delta: Option<&WorldDelta>,
    ) -> Result<(), RuntimeError> {
        // The previous state was validated and only WorldDelta can write the
        // world. Include removed metadata too: retiring a carrier without
        // removing its moving block must still be rejected.
        let positions = delta
            .into_iter()
            .flat_map(WorldDelta::changed_positions)
            .chain(before.carriers.keys().copied())
            .chain(self.carriers.keys().copied())
            .chain(before.staged_carriers.keys().copied())
            .chain(self.staged_carriers.keys().copied())
            .collect::<BTreeSet<_>>();
        for position in positions {
            let Some(block) = self.world.get(position) else {
                continue;
            };
            if block.kind == BlockKind::MovingPiston
                && (block.piston_entity.is_none() || !self.carriers.contains_key(&position))
                && !self.staged_carriers.contains_key(&position)
            {
                return Err(RuntimeError::CarrierConflict(position));
            }
        }
        for (position, planned) in &self.staged_carriers {
            let mut bare = planned.clone();
            bare.piston_entity = None;
            if self.world.get(*position) != Some(&bare)
                || self.carriers.contains_key(position)
                || self.stack.is_empty()
            {
                return Err(RuntimeError::CarrierConflict(*position));
            }
        }
        for (position, carrier) in &self.carriers {
            if !self
                .world
                .get(*position)
                .is_some_and(|b| b.kind == BlockKind::MovingPiston && b.piston_entity.is_some())
            {
                return Err(RuntimeError::CarrierConflict(*position));
            }
            // Changing a payload/body identity requires a new lifetime token.
            if before
                .carriers
                .get(position)
                .is_some_and(|c| c.id == carrier.id)
                && before.world.get(*position) != self.world.get(*position)
            {
                return Err(RuntimeError::CarrierConflict(*position));
            }
        }
        Ok(())
    }

    pub(super) fn apply_outcome(
        &mut self,
        parent: &Invocation<P>,
        outcome: RuntimeOutcome<P>,
    ) -> Result<(), RuntimeError> {
        if let Some(delta) = outcome.delta {
            for pos in delta
                .changed_positions()
                .chain(delta.moves.iter().flat_map(|m| [m.from, m.to]))
            {
                self.check_position(pos)?;
            }
            let identities: Vec<_> = delta
                .changed_positions()
                .chain(delta.moves.iter().flat_map(|m| [m.from, m.to]))
                .map(|pos| (pos, self.world.get(pos).map(BlockIdentity::of)))
                .collect();
            self.apply_world_delta(&delta)?;
            for (pos, before) in identities {
                if before != self.world.get(pos).map(BlockIdentity::of)
                    && self.outputs.contains_key(&pos)
                {
                    Arc::make_mut(&mut self.outputs).remove(&pos);
                }
            }
        }
        for effect in outcome.outputs {
            self.check_position(effect.position)?;
            if effect.value > 15
                || self.world.get(effect.position).map(BlockIdentity::of) != Some(effect.block)
            {
                return Err(RuntimeError::Invalid(
                    "invalid stored output assignment".into(),
                ));
            }
            if self.outputs.get(&effect.position).copied().unwrap_or(0) != effect.value {
                let outputs = Arc::make_mut(&mut self.outputs);
                if effect.value == 0 {
                    outputs.remove(&effect.position);
                } else {
                    outputs.insert(effect.position, effect.value);
                }
            }
        }
        if !outcome.histories.is_empty() {
            history::apply(
                Arc::make_mut(&mut self.histories),
                outcome.histories,
                self.region,
                self.time.game_tick,
            )?;
        }
        self.apply_carriers(outcome.carriers)?;
        for queued in outcome.queued {
            self.enqueue(queued, Some(parent.id))?;
        }
        if let Some(payload) = outcome.continuation {
            let id = self.id()?;
            self.stack.push(Invocation {
                id,
                root: parent.root,
                cause: Some(parent.id),
                time: parent.time,
                kind: InvocationKind::Continuation,
                depth: parent.depth,
                call: RuntimeCall {
                    target: parent.call.target,
                    payload,
                },
            });
        }
        // Allocate in request order and push in reverse so each child's entire
        // nested chain executes before the next sibling and parent continuation.
        let mut calls = Vec::with_capacity(outcome.callbacks.len());
        for call in outcome.callbacks {
            self.check_position(call.target)?;
            let depth = parent
                .depth
                .checked_add(1)
                .ok_or(RuntimeError::Limit("call depth"))?;
            if depth > self.limits.max_call_depth {
                return Err(RuntimeError::Limit("call depth"));
            }
            calls.push(Invocation {
                id: self.id()?,
                root: parent.root,
                cause: Some(parent.id),
                time: parent.time,
                kind: InvocationKind::Callback,
                depth,
                call,
            });
        }
        self.stack.extend(calls.into_iter().rev());
        self.check_pending_limit()
    }

    /// The sole world-write boundary in a staged invocation. An empty delta
    /// still validates its shape/moves but needs no detached world allocation.
    pub(super) fn apply_world_delta(&mut self, delta: &WorldDelta) -> Result<(), RuntimeError> {
        let result = if delta.changes.is_empty() {
            delta.validate(&self.world)
        } else {
            delta.apply(Arc::make_mut(&mut self.world))
        };
        result.map_err(|e| RuntimeError::Invalid(e.to_string()))
    }
}

impl<A: RuntimeAdapter> SynchronousWorldRuntime<A> {
    pub(super) fn try_microstep(
        &mut self,
    ) -> Result<Option<RuntimeRecord<A::Payload>>, RuntimeError> {
        if self.pending_count() == 0 {
            if !self.state.carriers.is_empty() || !self.state.staged_carriers.is_empty() {
                return Err(RuntimeError::UnfinishedMotion);
            }
            self.status = TraceStatus::Complete;
            return Ok(None);
        }
        if self.state.processed >= self.state.limits.max_microsteps {
            return Err(RuntimeError::Limit("microsteps"));
        }
        let mut staged = self.state.clone();
        let (invocation, result) = if let Some(frame) = staged.stack.pop() {
            (frame, DeliveryResult::Executed)
        } else {
            let delivery = staged.take_delivery().expect("pending root");
            let result = staged.delivery_result(&delivery)?;
            (delivery.invocation, result)
        };
        let outcome = if result == DeliveryResult::Executed {
            A::handle(&invocation, staged.view())?
        } else {
            RuntimeOutcome::default()
        };
        let delta = outcome.delta.clone();
        staged.apply_outcome(&invocation, outcome)?;
        staged.validate_carriers(&self.state, delta.as_ref())?;
        staged.processed += 1;
        let positions: BTreeSet<_> = self
            .state
            .carriers
            .keys()
            .chain(staged.carriers.keys())
            .copied()
            .collect();
        let carrier_changes = positions
            .into_iter()
            .filter_map(|pos| {
                let before = self.state.carriers.get(&pos).copied();
                let after = staged.carriers.get(&pos).copied();
                (before != after).then_some((pos, before, after))
            })
            .collect();
        let output_changes = if Arc::ptr_eq(&self.state.outputs, &staged.outputs) {
            Vec::new()
        } else {
            self.state
                .outputs
                .keys()
                .chain(staged.outputs.keys())
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter_map(|pos| {
                    let before = self.state.outputs.get(&pos).copied().unwrap_or(0);
                    let after = staged.outputs.get(&pos).copied().unwrap_or(0);
                    (before != after).then_some((pos, before, after))
                })
                .collect()
        };
        let history_changes = if Arc::ptr_eq(&self.state.histories, &staged.histories) {
            Vec::new()
        } else {
            self.state
                .histories
                .keys()
                .chain(staged.histories.keys())
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter_map(|key| {
                    let before = self.state.histories.get(&key).cloned();
                    let after = staged.histories.get(&key).cloned();
                    (before != after).then_some(HistoryChange {
                        key: key.0,
                        position: key.1,
                        before,
                        after,
                    })
                })
                .collect()
        };
        let record = RuntimeRecord {
            invocation,
            result,
            delta,
            carrier_changes,
            output_changes,
            history_changes,
        };
        self.state = staged;
        self.trace.push(record.clone());
        self.status = TraceStatus::InProgress;
        Ok(Some(record))
    }
}
