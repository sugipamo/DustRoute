//! Device reactions for the existing bounded event model only.
//! The engine owns scheduling, causal order, budgets, commit and rejection.
use super::{EventOutcome, PhysicsEngineError, RedstoneRunnerMode};
use crate::time::{
    BlockEventKind, PhysicsEvent, PhysicsEventKind, PhysicsEventPhase, PhysicsEventQueue,
    QueuedEvent,
};
use crate::{
    Block, BlockKind, PistonAction, PistonMotionProfile, PistonPlan, Pos, RedstonePropagationError,
    Region, World, direct_piston_neighbors, external_world_delta, piston_input_powered_in_region,
    piston_state, plan_piston, plan_piston_in_region, redstone_input_delta, redstone_lamp_delta,
    redstone_position_known, redstone_repeater_delay_game_ticks, redstone_repeater_delta,
    redstone_repeater_input_powered, redstone_repeater_output_position, redstone_repeater_powered,
    redstone_update_positions, redstone_wire_delta, redstone_wire_update_positions,
};
use std::collections::BTreeSet;

pub(super) struct BoundedDevices {
    redstone_inputs: bool,
    propagation: bool,
    movement_game_ticks: u64,
    activation_game_ticks: u64,
    planning_region: Option<Region>,
}
impl BoundedDevices {
    pub(super) fn prepare(
        world: &World,
        queue: &PhysicsEventQueue,
        motion_profile: &PistonMotionProfile,
        planning_region: Option<Region>,
        mode: RedstoneRunnerMode,
    ) -> Result<Self, PhysicsEngineError> {
        let unsupported = world.iter().find_map(|(position, block)| {
            (!crate::execution_context::WorldExecutionProfile::BoundedRedstoneEventsV1
                .admits_block(block))
            .then_some((*position, block.kind))
        });
        if let Some((position, kind)) = unsupported {
            let error = PhysicsEngineError::from(RedstonePropagationError::UnsupportedComponent {
                position,
                kind,
                reason: "block kind is outside the bounded execution contract".into(),
            });
            return Err(error);
        }
        let redstone_inputs = !matches!(mode, RedstoneRunnerMode::PistonOnly);
        let propagation = matches!(mode, RedstoneRunnerMode::Propagation);
        if let Err(error) = motion_profile.validate() {
            let error = PhysicsEngineError::from(error);
            return Err(error);
        }
        let unsupported_event = queue
            .iter()
            .find(|event| {
                let piston_event = matches!(
                    event.kind,
                    PhysicsEventKind::BlockEvent {
                        event: BlockEventKind::PistonExtend | BlockEventKind::PistonRetract
                    } | PhysicsEventKind::PistonComplete { .. }
                );
                let redstone_input_event = redstone_inputs
                    && matches!(
                        event.kind,
                        PhysicsEventKind::RedstoneInput { .. }
                            | PhysicsEventKind::NeighborUpdate { .. }
                    )
                    || propagation
                        && matches!(
                            event.kind,
                            PhysicsEventKind::LeverPulseSequence { .. }
                                | PhysicsEventKind::WorldChange { .. }
                                | PhysicsEventKind::RepeaterTick { .. }
                        );
                !(piston_event || redstone_input_event)
            })
            .cloned();
        if let Some(event) = unsupported_event {
            let error = PhysicsEngineError::UnsupportedEvent {
                time: event.time,
                kind: event.kind.clone(),
            };
            return Err(error);
        }
        let movement_game_ticks = motion_profile.stable_completion_delay_game_ticks();
        // The activation interval is currently a modelled range. Selecting
        // its upper bound gives a deterministic conservative path while
        // retaining the range in the profile for later measured scheduling.
        let activation_game_ticks = motion_profile.initial_delay_max_game_ticks;
        Ok(Self {
            redstone_inputs,
            propagation,
            movement_game_ticks,
            activation_game_ticks,
            planning_region,
        })
    }

    pub(super) fn handle(
        &self,
        event: &PhysicsEvent,
        world: &World,
    ) -> Result<EventOutcome, PhysicsEngineError> {
        match &event.kind {
            PhysicsEventKind::LeverPulseSequence {
                powered,
                on_sources,
                off_sources,
                pulse_width_game_ticks,
            } if self.propagation => self.lever_pulse(
                event,
                world,
                *powered,
                on_sources,
                off_sources,
                *pulse_width_game_ticks,
            ),
            PhysicsEventKind::WorldChange { after } if self.propagation => {
                self.world_change(event, world, after)
            }
            PhysicsEventKind::RedstoneInput { powered } if self.redstone_inputs => {
                self.redstone_input(event, world, *powered)
            }
            PhysicsEventKind::NeighborUpdate { .. } if self.redstone_inputs => {
                self.neighbor_update(event, world)
            }
            PhysicsEventKind::RepeaterTick { expected_powered } if self.propagation => {
                self.repeater_tick(event, world, *expected_powered)
            }
            PhysicsEventKind::BlockEvent {
                event: BlockEventKind::PistonExtend,
            } => self.piston_extend(event, world),
            PhysicsEventKind::BlockEvent {
                event: BlockEventKind::PistonRetract,
            } => self.piston_retract(event, world),
            PhysicsEventKind::PistonComplete { action, plan } => {
                self.piston_complete(world, *action, plan)
            }
            _ => Err(PhysicsEngineError::UnsupportedEvent {
                time: event.time,
                kind: event.kind.clone(),
            }),
        }
    }

    fn lever_pulse(
        &self,
        event: &PhysicsEvent,
        world: &World,
        powered: bool,
        on_sources: &[Pos],
        off_sources: &[Pos],
        pulse_width_game_ticks: u64,
    ) -> Result<EventOutcome, PhysicsEngineError> {
        if pulse_width_game_ticks == 0 {
            return Err(PhysicsEngineError::InvalidLeverPulseSequence {
                reason: "pulse width must be greater than zero game ticks".to_owned(),
            });
        }
        let Some(lever) = world.get(event.target) else {
            return Err(PhysicsEngineError::InvalidLeverPulseSequence {
                reason: format!("lever is missing at {:?}", event.target),
            });
        };
        if lever.kind != BlockKind::Lever {
            return Err(PhysicsEngineError::InvalidLeverPulseSequence {
                reason: format!(
                    "pulse target {:?} must be a Lever, found {:?}",
                    event.target, lever.kind
                ),
            });
        }
        let sources = if powered { on_sources } else { off_sources };
        if sources.is_empty() {
            return Err(PhysicsEngineError::InvalidLeverPulseSequence {
                reason: format!(
                    "{} edge has no pulse output source",
                    if powered { "on" } else { "off" }
                ),
            });
        }
        let mut unique_sources = BTreeSet::new();
        if sources
            .iter()
            .any(|source| *source == event.target || !unique_sources.insert(*source))
        {
            return Err(PhysicsEngineError::InvalidLeverPulseSequence {
                reason: "pulse output sources must be unique and distinct from the lever"
                    .to_owned(),
            });
        }

        // Validate the lever transition and every source edge before
        // returning the parent outcome.  A malformed source must not
        // leave the lever changed while the pulse children are
        // impossible to execute.
        let lever_delta = redstone_input_delta(world, event.target, powered)?;
        let mut staged = world.clone();
        if let Some(delta) = &lever_delta {
            delta.apply(&mut staged)?;
            let positions = redstone_update_positions(event.target, false);
            preflight_redstone_targets(&staged, self.planning_region, &positions)?;
        } else {
            let positions = redstone_update_positions(event.target, false);
            preflight_redstone_targets(&staged, self.planning_region, &positions)?;
        }

        for source in sources {
            let high = redstone_input_delta(&staged, *source, true)?;
            let mut high_world = staged.clone();
            if let Some(delta) = &high {
                delta.apply(&mut high_world)?;
            }
            let positions = redstone_update_positions(*source, false);
            preflight_redstone_targets(&high_world, self.planning_region, &positions)?;
            // The low edge is checked against the high-edge state so
            // both halves of the pulse have a known source contract.
            let _ = redstone_input_delta(&high_world, *source, false)?;
        }

        // A stable lever value is an intentional no-op.  In
        // particular, do not emit another high/low pulse merely
        // because the caller repeated the same input state.
        if lever_delta.is_none() {
            return Ok(EventOutcome::default());
        }

        let mut queued = queue_redstone_neighbors(event.target, false);
        for source in sources {
            queued.push(QueuedEvent {
                delay_ticks: 0,
                target: *source,
                phase: PhysicsEventPhase::External,
                kind: PhysicsEventKind::RedstoneInput { powered: true },
            });
            queued.push(QueuedEvent {
                delay_ticks: pulse_width_game_ticks,
                target: *source,
                phase: PhysicsEventPhase::External,
                kind: PhysicsEventKind::RedstoneInput { powered: false },
            });
        }
        Ok(EventOutcome {
            changes: Vec::new(),
            delta: lever_delta,
            queued,
        })
    }

    fn world_change(
        &self,
        event: &PhysicsEvent,
        world: &World,
        after: &Block,
    ) -> Result<EventOutcome, PhysicsEngineError> {
        redstone_position_known(self.planning_region, event.target)?;
        if !crate::execution_context::WorldExecutionProfile::BoundedRedstoneEventsV1
            .admits_block(after)
        {
            return Err(RedstonePropagationError::UnsupportedComponent {
                position: event.target,
                kind: after.kind,
                reason: "block kind is outside the bounded execution contract".into(),
            }
            .into());
        }
        let delta = external_world_delta(world, event.target, after);
        let queued = if let Some(delta) = &delta {
            let mut updated = world.clone();
            delta.apply(&mut updated)?;
            let positions = redstone_update_positions(event.target, true);
            preflight_redstone_targets(&updated, self.planning_region, &positions)?;
            queue_redstone_neighbors(event.target, true)
        } else {
            Vec::new()
        };
        Ok(EventOutcome {
            changes: Vec::new(),
            delta,
            queued,
        })
    }

    fn redstone_input(
        &self,
        event: &PhysicsEvent,
        world: &World,
        powered: bool,
    ) -> Result<EventOutcome, PhysicsEngineError> {
        if self.propagation {
            redstone_position_known(self.planning_region, event.target)?;
        }
        let neighbors = if self.propagation {
            Vec::new()
        } else {
            direct_piston_neighbors(world, self.planning_region, event.target)?
        };
        let delta = redstone_input_delta(world, event.target, powered)?;
        // Validate every affected piston against the post-edge source
        // state before returning the outcome. An incomplete side must
        // not leave a powered input mutation behind when its neighbor
        // cannot be evaluated; querying the staged world also lets a
        // previously absent observed `powered` property become known
        // from this explicit external edge.
        if let Some(delta) = &delta {
            let mut updated = world.clone();
            delta.apply(&mut updated)?;
            if self.propagation {
                let positions = redstone_update_positions(event.target, false);
                preflight_redstone_targets(&updated, self.planning_region, &positions)?;
            } else {
                for piston in &neighbors {
                    let _ =
                        piston_input_powered_in_region(&updated, self.planning_region, *piston)?;
                }
            }
        } else {
            if self.propagation {
                let positions = redstone_update_positions(event.target, false);
                preflight_redstone_targets(world, self.planning_region, &positions)?;
            } else {
                for piston in &neighbors {
                    let _ = piston_input_powered_in_region(world, self.planning_region, *piston)?;
                }
            }
        }
        let queued = if delta.is_some() {
            if self.propagation {
                queue_redstone_neighbors(event.target, false)
            } else {
                neighbors
                    .into_iter()
                    .map(|target| QueuedEvent {
                        delay_ticks: 0,
                        target,
                        phase: PhysicsEventPhase::NeighborUpdate,
                        kind: PhysicsEventKind::NeighborUpdate {
                            source: event.target,
                        },
                    })
                    .collect()
            }
        } else {
            Vec::new()
        };
        Ok(EventOutcome {
            changes: Vec::new(),
            delta,
            queued,
        })
    }

    fn neighbor_update(
        &self,
        event: &PhysicsEvent,
        world: &World,
    ) -> Result<EventOutcome, PhysicsEngineError> {
        if self.propagation {
            match world.kind_at(event.target) {
                BlockKind::RedstoneWire => {
                    let delta = redstone_wire_delta(world, event.target, self.planning_region)?;
                    let queued = delta
                        .as_ref()
                        .map(|_| {
                            queue_redstone_wire_neighbors_in_phase(
                                world,
                                event.target,
                                false,
                                event.time.phase,
                            )
                        })
                        .unwrap_or_default();
                    return Ok(EventOutcome {
                        changes: Vec::new(),
                        delta,
                        queued,
                    });
                }
                BlockKind::RedstoneLamp => {
                    let delta = redstone_lamp_delta(world, event.target, self.planning_region)?;
                    return Ok(EventOutcome {
                        changes: Vec::new(),
                        delta,
                        queued: Vec::new(),
                    });
                }
                BlockKind::Repeater => {
                    let expected_powered =
                        redstone_repeater_input_powered(world, event.target, self.planning_region)?;
                    let current_powered = redstone_repeater_powered(world, event.target)?;
                    if !crate::execution_context::bounded_world_laws()
                        .repeater
                        .needs_update(current_powered, expected_powered)
                    {
                        return Ok(EventOutcome::default());
                    }
                    let delay_game_ticks = redstone_repeater_delay_game_ticks(world, event.target)?;
                    let output = redstone_repeater_output_position(
                        world,
                        event.target,
                        self.planning_region,
                    )?;
                    preflight_redstone_targets(world, self.planning_region, &[output])?;
                    return Ok(EventOutcome {
                        changes: Vec::new(),
                        delta: None,
                        queued: vec![QueuedEvent {
                            delay_ticks: delay_game_ticks,
                            target: event.target,
                            phase: PhysicsEventPhase::ScheduledTick,
                            kind: PhysicsEventKind::RepeaterTick { expected_powered },
                        }],
                    });
                }
                _ => {}
            }
        }
        let Some(piston) = world.get(event.target) else {
            return Ok(EventOutcome::default());
        };
        if piston.kind != BlockKind::Piston {
            return Ok(EventOutcome::default());
        }
        let powered = piston_input_powered_in_region(world, self.planning_region, event.target)?;
        let action = crate::execution_context::bounded_world_laws()
            .piston
            .requested_action(piston_state(piston), powered);
        let queued = action
            .map(|action| QueuedEvent {
                delay_ticks: self.activation_game_ticks,
                target: event.target,
                phase: PhysicsEventPhase::BlockEvent,
                kind: PhysicsEventKind::BlockEvent {
                    event: match action {
                        PistonAction::Extend => BlockEventKind::PistonExtend,
                        PistonAction::Retract => BlockEventKind::PistonRetract,
                    },
                },
            })
            .into_iter()
            .collect();
        Ok(EventOutcome {
            changes: Vec::new(),
            delta: None,
            queued,
        })
    }

    fn repeater_tick(
        &self,
        event: &PhysicsEvent,
        world: &World,
        expected_powered: bool,
    ) -> Result<EventOutcome, PhysicsEngineError> {
        let current_input =
            redstone_repeater_input_powered(world, event.target, self.planning_region)?;
        // The input edge that created this scheduled tick may have
        // been reversed before the delay elapsed. Retain the event
        // as evidence, but do not apply an obsolete output pulse.
        let Some(next_powered) = crate::execution_context::bounded_world_laws()
            .repeater
            .scheduled_output(current_input, expected_powered)
        else {
            return Ok(EventOutcome::default());
        };
        let output = redstone_repeater_output_position(world, event.target, self.planning_region)?;
        preflight_redstone_targets(world, self.planning_region, &[output])?;
        let delta =
            redstone_repeater_delta(world, event.target, next_powered, self.planning_region)?;
        let queued = delta
            .as_ref()
            .map(|_| {
                vec![QueuedEvent {
                    delay_ticks: 0,
                    target: output,
                    // Neighbor updates emitted by a scheduled tick
                    // remain in the same tick's scheduled phase. This
                    // preserves causal order under the deterministic
                    // phase model while allowing the front wire/lamp
                    // or piston to react immediately after the tick.
                    phase: PhysicsEventPhase::ScheduledTick,
                    kind: PhysicsEventKind::NeighborUpdate {
                        source: event.target,
                    },
                }]
            })
            .unwrap_or_default();
        Ok(EventOutcome {
            changes: Vec::new(),
            delta,
            queued,
        })
    }

    fn piston_extend(
        &self,
        event: &PhysicsEvent,
        world: &World,
    ) -> Result<EventOutcome, PhysicsEngineError> {
        if self.redstone_inputs
            && world.get(event.target).is_some_and(|piston| {
                piston.kind == BlockKind::Piston
                    && !crate::execution_context::bounded_world_laws()
                        .piston
                        .permits(piston_state(piston), PistonAction::Extend)
            })
        {
            // Multiple source edges may deliver the same powered
            // neighbor update before the first Block Event runs.
            // Vanilla retains those events as evidence, but the
            // already-moving piston must not turn the duplicate into
            // a failed run.  Interruption/reversal remains outside
            // this subset and is represented as a no-op here.
            return Ok(EventOutcome::default());
        }
        let plan = match self.planning_region {
            Some(region) => {
                plan_piston_in_region(world, region, event.target, PistonAction::Extend)?
            }
            None => plan_piston(world, event.target, PistonAction::Extend)?,
        };
        let start_delta = plan.start_delta();
        let mut started = world.clone();
        start_delta.apply(&mut started)?;
        let completion = plan.completion_plan(&started)?;
        Ok(EventOutcome {
            changes: Vec::new(),
            delta: Some(start_delta),
            queued: vec![QueuedEvent {
                delay_ticks: self.movement_game_ticks,
                target: event.target,
                phase: PhysicsEventPhase::BlockEntity,
                kind: PhysicsEventKind::PistonComplete {
                    action: PistonAction::Extend,
                    plan: Box::new(completion),
                },
            }],
        })
    }

    fn piston_retract(
        &self,
        event: &PhysicsEvent,
        world: &World,
    ) -> Result<EventOutcome, PhysicsEngineError> {
        if self.redstone_inputs
            && world.get(event.target).is_some_and(|piston| {
                piston.kind == BlockKind::Piston
                    && !crate::execution_context::bounded_world_laws()
                        .piston
                        .permits(piston_state(piston), PistonAction::Retract)
            })
        {
            // See the extension branch above: a duplicate retract or
            // a reverse edge during motion is retained as a no-op
            // event, while the actual completion remains atomic.
            return Ok(EventOutcome::default());
        }
        let plan = match self.planning_region {
            Some(region) => {
                plan_piston_in_region(world, region, event.target, PistonAction::Retract)?
            }
            None => plan_piston(world, event.target, PistonAction::Retract)?,
        };
        let start_delta = plan.start_delta();
        let mut started = world.clone();
        start_delta.apply(&mut started)?;
        let completion = plan.completion_plan(&started)?;
        Ok(EventOutcome {
            changes: Vec::new(),
            delta: Some(start_delta),
            queued: vec![QueuedEvent {
                delay_ticks: self.movement_game_ticks,
                target: event.target,
                phase: PhysicsEventPhase::BlockEntity,
                kind: PhysicsEventKind::PistonComplete {
                    action: PistonAction::Retract,
                    plan: Box::new(completion),
                },
            }],
        })
    }

    fn piston_complete(
        &self,
        world: &World,
        action: PistonAction,
        plan: &PistonPlan,
    ) -> Result<EventOutcome, PhysicsEngineError> {
        if action != plan.action {
            return Err(PhysicsEngineError::InvalidOutcome);
        }
        Ok(EventOutcome {
            changes: Vec::new(),
            delta: Some(plan.completion_plan(world)?.world_delta().clone()),
            queued: Vec::new(),
        })
    }
}

fn queue_redstone_neighbors(source: Pos, include_self: bool) -> Vec<QueuedEvent> {
    queue_redstone_neighbors_in_phase(source, include_self, PhysicsEventPhase::NeighborUpdate)
}

fn queue_redstone_wire_neighbors_in_phase(
    world: &World,
    source: Pos,
    include_self: bool,
    phase: PhysicsEventPhase,
) -> Vec<QueuedEvent> {
    let direct = redstone_update_positions(source, include_self)
        .into_iter()
        .collect::<BTreeSet<_>>();
    redstone_wire_update_positions(source, include_self)
        .into_iter()
        .filter(|target| {
            direct.contains(target) || world.kind_at(*target) == BlockKind::RedstoneWire
        })
        .map(|target| QueuedEvent {
            delay_ticks: 0,
            target,
            phase,
            kind: PhysicsEventKind::NeighborUpdate { source },
        })
        .collect()
}

fn queue_redstone_neighbors_in_phase(
    source: Pos,
    include_self: bool,
    phase: PhysicsEventPhase,
) -> Vec<QueuedEvent> {
    redstone_update_positions(source, include_self)
        .into_iter()
        .map(|target| QueuedEvent {
            delay_ticks: 0,
            target,
            phase,
            kind: PhysicsEventKind::NeighborUpdate { source },
        })
        .collect()
}

fn preflight_redstone_targets(
    world: &World,
    known_region: Option<Region>,
    positions: &[Pos],
) -> Result<(), PhysicsEngineError> {
    for position in positions {
        redstone_position_known(known_region, *position)?;
        match world.kind_at(*position) {
            BlockKind::RedstoneWire => {
                let _ = redstone_wire_delta(world, *position, known_region)?;
                // A changed wire can reach an adjacent wire through either a
                // direct or bounded vertical offset-neighbor relation. Validate
                // those existing targets before accepting the upstream delta,
                // so malformed observed shapes remain fail-closed rather than
                // leaving a partially propagated source edge behind.
                for neighbor in redstone_wire_update_positions(*position, false) {
                    match world.kind_at(neighbor) {
                        BlockKind::RedstoneWire => {
                            let _ = redstone_wire_delta(world, neighbor, known_region)?;
                        }
                        BlockKind::Repeater => {
                            preflight_repeater_target(world, known_region, neighbor)?;
                        }
                        _ => {}
                    }
                }
            }
            BlockKind::RedstoneLamp => {
                let _ = redstone_lamp_delta(world, *position, known_region)?;
            }
            BlockKind::Piston => {
                let _ = piston_input_powered_in_region(world, known_region, *position)?;
            }
            BlockKind::Repeater => {
                preflight_repeater_target(world, known_region, *position)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn preflight_repeater_target(
    world: &World,
    known_region: Option<Region>,
    position: Pos,
) -> Result<(), PhysicsEngineError> {
    let _ = redstone_repeater_input_powered(world, position, known_region)?;
    let _ = redstone_repeater_powered(world, position)?;
    let _ = redstone_repeater_delay_game_ticks(world, position)?;
    let output = redstone_repeater_output_position(world, position, known_region)?;
    redstone_position_known(known_region, output)?;
    Ok(())
}
