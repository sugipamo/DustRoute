//! Comparison for one pinned physical adapter, never a generic scheduler
//! optimization. No world block, pending root, tick section or carrier progress
//! is removed. Exact checkpoints and cumulative diagnostic budgets are unchanged.
use super::*;
use crate::time::piston_runtime::PistonEvent;
use std::cmp::Ordering;

pub const COMPARISON: &str = "dustroute.piston-electrical-root-comparison.v1";

/// Opaque, process-local representative of a complete physical root boundary.
/// Its ordering compares the full canonical record, not a hash. The selected
/// adapter never reads event/cause IDs, absolute positive tick numbers or trace
/// records. Computation counters are renewed per root; limits are retained.
#[derive(Clone, Debug)]
pub struct PistonBehaviorState {
    state: State<PistonEvent>,
    key: Vec<u8>,
}

impl PistonBehaviorState {
    pub const COMPARISON: &'static str = COMPARISON;
}
impl PartialEq for PistonBehaviorState {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for PistonBehaviorState {}
impl PartialOrd for PistonBehaviorState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PistonBehaviorState {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

impl<A: RuntimeAdapter<Payload = PistonEvent>> SynchronousWorldRuntime<A> {
    /// Supply the next External boundary before jumping to a later queued
    /// root. A world clock continues even with an empty event queue. Without
    /// this boundary an explorer would miss inputs before a carrier's next
    /// tick, or after an idle interval. This does not execute or delete work.
    pub(crate) fn piston_behavior_clock(&mut self) -> Result<bool, RuntimeError> {
        self.ensure_running()?;
        if !self.at_input_boundary() {
            return Err(RuntimeError::InputInsideCallback);
        }
        if !self.state.staged_carriers.is_empty() {
            return Err(RuntimeError::UnfinishedMotion);
        }
        if self
            .state
            .pending
            .first_key_value()
            .is_some_and(|(time, _)| time.game_tick == self.state.time.game_tick)
        {
            return Ok(false);
        }
        if self.state.pending.is_empty() && !self.state.carriers.is_empty() {
            return Err(RuntimeError::UnfinishedMotion);
        }
        self.state.time = RuntimeTime {
            game_tick: self
                .state
                .time
                .game_tick
                .checked_add(1)
                .ok_or(RuntimeError::ClockOverflow)?,
            section: TickSection::External,
        };
        Ok(true)
    }
    pub(crate) fn piston_behavior_state(&self) -> Result<PistonBehaviorState, RuntimeError> {
        self.ensure_running()?;
        if !self.at_input_boundary() {
            return Err(RuntimeError::InputInsideCallback);
        }
        if self.state.pending.is_empty() && !self.state.carriers.is_empty() {
            return Err(RuntimeError::UnfinishedMotion);
        }
        if !self.state.staged_carriers.is_empty() {
            return Err(RuntimeError::UnfinishedMotion);
        }
        let mut state = self.state.clone();
        let old_tick = state.time.game_tick;
        // Zero is a semantic constructor constant (fresh savedWorldTime = 0).
        // Positive epochs can share origin 1; moving them to zero is unsound.
        let epoch = u64::from(old_tick != 0);
        state.time.game_tick = epoch;
        state.processed = 0; // a computation limit per atomic root, not physics
        state.next_id = 0;
        state.next_carrier = 0;
        let mut carriers = BTreeMap::new();
        for carrier in state.carriers.values_mut() {
            let id = CarrierId(state.next_carrier);
            state.next_carrier += 1;
            carriers.insert(carrier.id, id);
            carrier.id = id;
            carrier.history.saved_world_time = if carrier.history.saved_world_time == old_tick {
                epoch
            } else {
                0
            };
        }
        let mut pending = BTreeMap::new();
        for (time, mut queue) in state.pending {
            let time = RuntimeTime {
                game_tick: time
                    .game_tick
                    .checked_sub(old_tick)
                    .and_then(|delay| epoch.checked_add(delay))
                    .ok_or(RuntimeError::ClockOverflow)?,
                section: time.section,
            };
            for delivery in &mut queue {
                let invocation = &mut delivery.invocation;
                // Only these queued payloads are emitted by the pinned adapter.
                // A new suspended operation cannot silently lose captured state.
                if !matches!(
                    (invocation.kind, &invocation.call.payload),
                    (
                        InvocationKind::External,
                        PistonEvent::Initialize
                            | PistonEvent::Input { .. }
                            | PistonEvent::Device {
                                callback: crate::device_program::Callback::Use,
                                source: None,
                                captured: None
                            }
                    ) | (
                        InvocationKind::ScheduledTick,
                        PistonEvent::Device {
                            callback: crate::device_program::Callback::Tick,
                            source: None,
                            captured: None
                        }
                    ) | (InvocationKind::BlockEvent, PistonEvent::Block { .. })
                        | (InvocationKind::CarrierTick, PistonEvent::CarrierTick)
                ) {
                    return Err(RuntimeError::Invalid(
                        "unsupported root payload in piston behavior comparison".into(),
                    ));
                }
                invocation.id = state.next_id;
                invocation.root = state.next_id;
                state.next_id += 1;
                invocation.cause = None; // diagnostic attribution, never read by this adapter
                invocation.time = time;
                if let Guard::Carrier(old) = delivery.guard {
                    let id = *carriers.entry(old).or_insert_with(|| {
                        let id = CarrierId(state.next_carrier);
                        state.next_carrier += 1;
                        id
                    });
                    // Keep retired deliveries and their input boundary. New
                    // lifetimes start above every referenced old token.
                    delivery.guard = Guard::Carrier(id);
                }
            }
            pending.insert(time, queue);
        }
        state.pending = pending;
        let key = serde_json::to_vec(&(
            COMPARISON,
            state.profile,
            state.adapter,
            state.region,
            state.time,
            state.world.iter().collect::<Vec<_>>(),
            state.pending.iter().collect::<Vec<_>>(),
            state.carriers.iter().collect::<Vec<_>>(),
            state.staged_carriers.iter().collect::<Vec<_>>(),
            state.next_id,
            state.next_carrier,
            state.limits,
        ))
        .map_err(|e| RuntimeError::Invalid(e.to_string()))?;
        Ok(PistonBehaviorState { state, key })
    }

    pub(crate) fn from_piston_behavior_state(
        state: &PistonBehaviorState,
    ) -> Result<Self, RuntimeError> {
        Self::check_restore_identity(&state.state, "behavior state")?;
        Ok(Self {
            state: state.state.clone(),
            status: if state.state.pending.is_empty() {
                TraceStatus::Complete
            } else {
                TraceStatus::InProgress
            },
            trace: Vec::new(),
            adapter: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::piston_runtime::{
        ElectricalPistonAdapter, ElectricalPistonRuntime, new_piston_runtime,
        schedule_electrical_input,
    };
    use crate::{BlockKind, Facing, PistonState, PistonVariant};

    const INPUT: Pos = Pos::new(-1, 1, 0);
    const OUT: Pos = Pos::new(2, 1, 0);

    fn scene() -> ElectricalPistonRuntime {
        let mut world = World::new();
        world.place(BlockKind::Solid, Pos::new(-1, 0, 0));
        let lever = world.place(BlockKind::Lever, INPUT);
        lever.powered = Some(false);
        lever.support_offset = Some(Pos::new(0, -1, 0));
        let piston = world.place(BlockKind::Piston, Pos::new(0, 1, 0));
        piston.facing = Some(Facing::East);
        piston.piston_state = Some(PistonState::Retracted);
        piston.piston_variant = Some(PistonVariant::Sticky);
        world.place(BlockKind::Solid, Pos::new(1, 1, 0));
        new_piston_runtime(
            world,
            Region::new(Pos::new(-3, -1, -3), Pos::new(5, 3, 3)),
            RuntimeLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn restoration_checks_runtime_profile_revision_and_concrete_adapter() {
        let original = scene();
        let state = original.behavior_state().unwrap();
        let checkpoint = original.checkpoint();
        for field in ["profile", "revision", "adapter_type"] {
            let mut representative = state.clone();
            let mut exact = checkpoint.clone();
            for inner in [&mut representative.state, &mut exact.state] {
                match field {
                    "profile" => inner.profile = "unsupported-runtime.v1",
                    "revision" => inner.adapter = "unsupported-adapter.v1",
                    _ => inner.adapter_type = TypeId::of::<()>(),
                }
            }
            assert!(
                matches!(
                    ElectricalPistonRuntime::from_behavior_state(&representative),
                    Err(RuntimeError::Invalid(message)) if message == "behavior state execution profile/adapter mismatch"
                ),
                "{field}"
            );
            assert!(
                matches!(
                    ElectricalPistonRuntime::from_checkpoint(&exact),
                    Err(RuntimeError::Invalid(message)) if message == "checkpoint execution profile/adapter mismatch"
                ),
                "{field}"
            );
        }
        assert_eq!(original.behavior_state().unwrap(), state);
        assert_eq!(original.checkpoint(), checkpoint);
    }

    #[test]
    fn positive_epochs_and_retired_names_share_a_physical_future() {
        let mut early = scene();
        let mut late = scene();
        for (run, tick) in [(&mut early, 1), (&mut late, 101)] {
            schedule_electrical_input(run, tick, INPUT, true).unwrap();
            run.run_until_idle().unwrap();
        }
        assert_ne!(early.state_key(), late.state_key());
        assert_eq!(
            early.behavior_state().unwrap(),
            late.behavior_state().unwrap()
        );
        // Another complete cycle allocates new lifetime IDs, without changing
        // what a following input can do to the settled extended placement.
        late.input_now(INPUT, false).unwrap();
        late.run_until_idle().unwrap();
        late.input_now(INPUT, true).unwrap();
        late.run_until_idle().unwrap();
        assert_eq!(
            early.behavior_state().unwrap(),
            late.behavior_state().unwrap()
        );
    }

    #[test]
    fn zero_epoch_section_delays_and_progress_are_not_erased() {
        let mut run = scene();
        run.run_until_idle().unwrap();
        let state = run.behavior_state().unwrap();
        let mut changed =
            SynchronousWorldRuntime::<ElectricalPistonAdapter>::from_piston_behavior_state(&state)
                .unwrap();
        changed.state.time.game_tick = 1;
        assert_ne!(state, changed.piston_behavior_state().unwrap());
        changed.state.time.game_tick = 0;
        changed.state.time.section = TickSection::BlockEvents;
        assert_ne!(state, changed.piston_behavior_state().unwrap());

        run.input_now(INPUT, true).unwrap();
        run.step().unwrap();
        run.step().unwrap();
        let state = run.behavior_state().unwrap();
        assert!(state.state.carriers.contains_key(&OUT));
        for modify in [0, 1, 2, 3] {
            let mut changed =
                SynchronousWorldRuntime::<ElectricalPistonAdapter>::from_piston_behavior_state(
                    &state,
                )
                .unwrap();
            match modify {
                0 => {
                    changed
                        .state
                        .carriers
                        .get_mut(&OUT)
                        .unwrap()
                        .history
                        .progress = HalfProgress::Half
                }
                1 => {
                    changed
                        .state
                        .carriers
                        .get_mut(&OUT)
                        .unwrap()
                        .history
                        .last_progress = HalfProgress::Half
                }
                2 => changed
                    .state
                    .pending
                    .first_entry()
                    .unwrap()
                    .get_mut()
                    .make_contiguous()
                    .reverse(),
                _ => {
                    let (mut time, queue) = changed.state.pending.pop_first().unwrap();
                    time.game_tick += 1;
                    changed.state.pending.insert(time, queue);
                }
            }
            assert_ne!(
                state,
                changed.piston_behavior_state().unwrap(),
                "change {modify}"
            );
        }
    }

    #[test]
    fn same_tick_history_is_distinct_from_past_history() {
        let mut run = scene();
        schedule_electrical_input(&mut run, 9, INPUT, true).unwrap();
        while run.view().carrier(OUT).is_none() {
            run.step().unwrap();
        }
        let state = run.behavior_state().unwrap();
        let mut changed =
            SynchronousWorldRuntime::<ElectricalPistonAdapter>::from_piston_behavior_state(&state)
                .unwrap();
        changed
            .state
            .carriers
            .get_mut(&OUT)
            .unwrap()
            .history
            .saved_world_time = changed.state.time.game_tick;
        assert_ne!(state, changed.piston_behavior_state().unwrap());
    }

    #[test]
    fn resumed_representatives_keep_every_interrupted_root_and_microstep() {
        for off in [1, 2, 3, 8] {
            let mut original = scene();
            schedule_electrical_input(&mut original, 1, INPUT, true).unwrap();
            schedule_electrical_input(&mut original, off, INPUT, false).unwrap();
            let mut retired = 0;
            loop {
                let before = original.behavior_state().unwrap();
                let mut resumed = ElectricalPistonRuntime::from_behavior_state(&before).unwrap();
                loop {
                    let a = original.microstep().unwrap();
                    let b = resumed.microstep().unwrap();
                    assert_eq!(a.is_some(), b.is_some());
                    if let (Some(a), Some(b)) = (&a, &b) {
                        assert_eq!(a.result, b.result);
                        assert_eq!(a.invocation.kind, b.invocation.kind);
                        assert_eq!(a.invocation.time.section, b.invocation.time.section);
                        assert_eq!(a.delta, b.delta);
                        retired += usize::from(a.result == DeliveryResult::CarrierRetired);
                        assert_eq!(original.view().world(), resumed.view().world());
                    }
                    assert_eq!(original.at_input_boundary(), resumed.at_input_boundary());
                    if original.at_input_boundary() {
                        break;
                    }
                }
                assert_eq!(
                    original.behavior_state().unwrap(),
                    resumed.behavior_state().unwrap()
                );
                if original.pending_count() == 0 {
                    break;
                }
            }
            if off == 2 || off == 3 {
                assert!(retired > 0);
            }
        }
    }

    #[test]
    fn incomplete_roots_and_failed_prefixes_cannot_be_representatives() {
        let mut run = scene();
        run.microstep().unwrap();
        assert!(!run.at_input_boundary());
        assert_eq!(
            run.behavior_state().unwrap_err(),
            RuntimeError::InputInsideCallback
        );
        run.run_until_idle().unwrap();
        let state = run.behavior_state().unwrap();
        let mut failed =
            SynchronousWorldRuntime::<ElectricalPistonAdapter>::from_piston_behavior_state(&state)
                .unwrap();
        failed.status = TraceStatus::Failed {
            error: "unsupported physical step".into(),
        };
        assert!(matches!(
            failed.piston_behavior_state(),
            Err(RuntimeError::Failed(_))
        ));
    }

    #[test]
    fn clock_boundaries_preserve_inputs_before_later_roots_and_while_idle() {
        let mut run = scene();
        assert!(!run.advance_behavior_clock().unwrap());
        run.run_until_idle().unwrap();
        let zero = run.behavior_state().unwrap();
        assert!(run.advance_behavior_clock().unwrap());
        let positive = run.behavior_state().unwrap();
        assert_ne!(zero, positive);
        assert!(run.advance_behavior_clock().unwrap());
        assert_eq!(positive, run.behavior_state().unwrap());
        run.input_now(INPUT, true).unwrap();
        assert_eq!(
            run.advance_behavior_clock().unwrap_err(),
            RuntimeError::InputInsideCallback
        );
        run.step().unwrap();
        while !run.advance_behavior_clock().unwrap() {
            run.step().unwrap();
        }
        assert!(run.view().carrier(OUT).is_some());
        assert_eq!(run.view().time().section, TickSection::External);
        assert!(!run.advance_behavior_clock().unwrap());
        let pending = run.pending_count();
        run.input_now(INPUT, false).unwrap();
        run.step().unwrap();
        assert!(run.pending_count() >= pending);
        assert_eq!(run.view().time().section, TickSection::External);
    }
}
