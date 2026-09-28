//! Piston movement with independently ticking carriers and synchronous
//! notifications and electrical updates in one mixed-facing world.
mod adapter;
mod command;
mod devices;
mod electrical;
mod geometry;
mod movement;
mod notifications;

use std::collections::VecDeque;

use super::runtime::*;
use crate::piston_motion_law::PistonBlockEvent;
use crate::{Block, Facing, Pos, Region, World};

pub(crate) use adapter::ElectricalPistonAdapter;
pub use movement::MotionPlan;

pub const ELECTRICAL_PROFILE: &str = "dustroute.piston-electrical-callbacks.java-1-21-11.v9";

#[cfg(test)]
mod device_capability_tests;
/// All six body directions share one electrical world and synchronous queue.
/// Live placement requires fresh target review and verified observations.
pub struct ElectricalPistonRuntime(SynchronousWorldRuntime<ElectricalPistonAdapter>);

impl ElectricalPistonRuntime {
    /// Simulate one command insertion into known air. Construction is an
    /// explicit, settled sequence; it is not an explorer input or live-write
    /// permission. Callers must independently verify the completed world.
    pub fn install_now(&mut self, position: Pos, block: Block) -> Result<u64, RuntimeError> {
        if self.pending_count() != 0 || !self.at_input_boundary() {
            return Err(RuntimeError::Invalid(
                "construction requires an idle runtime".into(),
            ));
        }
        self.0.input_now(call(
            position,
            PistonEvent::ElectricalInstall {
                block: Box::new(block),
            },
        ))
    }

    /// Explicit teardown step for a construction simulation, at an idle root.
    pub fn remove_now(&mut self, position: Pos) -> Result<u64, RuntimeError> {
        if self.pending_count() != 0 || !self.at_input_boundary() {
            return Err(RuntimeError::Invalid(
                "teardown requires an idle runtime".into(),
            ));
        }
        self.0
            .input_now(call(position, PistonEvent::ElectricalRemove))
    }
    pub fn behavior_state(&self) -> Result<PistonBehaviorState, RuntimeError> {
        self.0.piston_behavior_state()
    }
    /// Resume only a state captured by this exact runtime and adapter revision.
    pub fn from_behavior_state(state: &PistonBehaviorState) -> Result<Self, RuntimeError> {
        SynchronousWorldRuntime::from_piston_behavior_state(state).map(Self)
    }
    pub fn advance_behavior_clock(&mut self) -> Result<bool, RuntimeError> {
        self.0.piston_behavior_clock()
    }
    pub fn from_checkpoint(
        checkpoint: &RuntimeCheckpoint<PistonEvent>,
    ) -> Result<Self, RuntimeError> {
        SynchronousWorldRuntime::from_checkpoint(checkpoint).map(Self)
    }
    pub fn checkpoint(&self) -> RuntimeCheckpoint<PistonEvent> {
        self.0.checkpoint()
    }
    pub fn state_key(&self) -> RuntimeStateKey<PistonEvent> {
        self.0.state_key()
    }
    pub fn view(&self) -> RuntimeView<'_> {
        self.0.view()
    }
    pub fn trace(&self) -> &[RuntimeRecord<PistonEvent>] {
        self.0.trace()
    }
    pub fn status(&self) -> &crate::time::TraceStatus {
        self.0.status()
    }
    pub fn pending_count(&self) -> usize {
        self.0.pending_count()
    }
    pub fn at_input_boundary(&self) -> bool {
        self.0.at_input_boundary()
    }
    pub fn microstep(&mut self) -> Result<Option<RuntimeRecord<PistonEvent>>, RuntimeError> {
        self.0.microstep()
    }
    pub fn step(&mut self) -> Result<bool, RuntimeError> {
        self.0.step()
    }
    pub fn run_until_idle(&mut self) -> Result<(), RuntimeError> {
        self.0.run_until_idle()
    }
    pub fn input_now(&mut self, source: Pos, powered: bool) -> Result<u64, RuntimeError> {
        self.0
            .input_now(call(source, PistonEvent::Input { powered }))
    }
    /// Explicit interaction with a registered device; currently stone buttons.
    /// Use is distinct from assigning a powered state and schedules its own release.
    pub fn use_now(&mut self, position: Pos) -> Result<u64, RuntimeError> {
        self.0.input_now(devices::event(
            position,
            crate::device_program::Callback::Use,
            None,
        ))
    }
    pub fn execution_context(&self) -> crate::execution_context::WorldExecutionContext {
        crate::execution_context::WorldExecutionContext::for_profile(
            crate::execution_context::WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V9,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct NeighborJob {
    target: Pos,
    source: Pos,
    shape: bool,
}

/// Includes suspended method stages and notification batches. No future work
/// is held in a mutable adapter cache or attached to a Blueprint Revision.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum PistonEvent {
    Motion {
        plan: Box<MotionPlan>,
    },
    Initialize,
    Input {
        powered: bool,
    },
    Notify {
        jobs: VecDeque<NeighborJob>,
    },
    Block {
        event: PistonBlockEvent,
        facing: Facing,
    },
    Arm {
        positions: Vec<Pos>,
        after: Option<Box<PistonEvent>>,
    },
    ExtendBody {
        body: Box<Block>,
    },
    RetractBody {
        body: Box<Block>,
        event: PistonBlockEvent,
        restored_facing: Facing,
    },
    RetractPayload {
        body: Box<Block>,
        event: PistonBlockEvent,
    },
    ForceFinish {
        carrier: CarrierId,
    },
    CarrierTick,
    Device {
        callback: crate::device_program::Callback,
        source: Option<Pos>,
        captured: Option<Box<Block>>,
    },
    DeviceContinue {
        run: Box<crate::device_program::DeviceRun>,
    },
    DeviceAfterArrival {
        block: Box<Block>,
    },
    ElectricalInstall {
        block: Box<Block>,
    },
    ElectricalPreprocess {
        block: Box<Block>,
        side: usize,
    },
    ElectricalWrite {
        block: Box<Block>,
    },
    ElectricalAdded,
    ElectricalAfterAdded {
        block: Box<Block>,
    },
    ElectricalRemove,
    ElectricalRemoveShapes {
        block: Box<Block>,
    },
    ElectricalCommandNeighbors,
    ElectricalRemoved {
        block: Box<Block>,
    },
    ElectricalRemovedWireUpdate {
        block: Box<Block>,
    },
}

/// Current pinned electrical constructor, also used for recorded observation replay.
pub fn new_piston_runtime(
    world: World,
    region: Region,
    limits: RuntimeLimits,
) -> Result<ElectricalPistonRuntime, RuntimeError> {
    crate::execution_context::WorldExecutionContext::for_profile(
        crate::execution_context::WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V9,
    )
    .validate()
    .map_err(RuntimeError::Invalid)?;
    let mut runtime = SynchronousWorldRuntime::new(world, region, limits)?;
    runtime.enqueue(QueueRequest::External {
        game_tick: 0,
        call: RuntimeCall {
            target: region.min,
            payload: PistonEvent::Initialize,
        },
    })?;
    Ok(ElectricalPistonRuntime(runtime))
}

pub fn schedule_electrical_input(
    runtime: &mut ElectricalPistonRuntime,
    game_tick: u64,
    source: Pos,
    powered: bool,
) -> Result<(), RuntimeError> {
    runtime.0.enqueue(QueueRequest::External {
        game_tick,
        call: RuntimeCall {
            target: source,
            payload: PistonEvent::Input { powered },
        },
    })?;
    Ok(())
}

/// Replay a server-thread interaction after this world tick has completed.
/// Unlike the before-tick input helper, this retains the carrier's same-world-
/// time relationship while deferring requested block events to the next tick.
pub fn schedule_electrical_input_after_tick(
    runtime: &mut ElectricalPistonRuntime,
    game_tick: u64,
    source: Pos,
    powered: bool,
) -> Result<(), RuntimeError> {
    runtime.0.enqueue(QueueRequest::AfterWorldTick {
        game_tick,
        call: call(source, PistonEvent::Input { powered }),
    })?;
    Ok(())
}

fn call(target: Pos, payload: PistonEvent) -> RuntimeCall<PistonEvent> {
    RuntimeCall { target, payload }
}
fn unsupported(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Handler(message.into())
}

fn next_tick(tick: u64, delay: u64) -> Result<u64, RuntimeError> {
    tick.checked_add(delay).ok_or(RuntimeError::ClockOverflow)
}

/// Schedule an explicit use after a world tick, retaining real release timing.
pub fn schedule_device_use_after_tick(
    runtime: &mut ElectricalPistonRuntime,
    game_tick: u64,
    position: Pos,
) -> Result<(), RuntimeError> {
    runtime.0.enqueue(QueueRequest::AfterWorldTick {
        game_tick,
        call: devices::event(position, crate::device_program::Callback::Use, None),
    })?;
    Ok(())
}
