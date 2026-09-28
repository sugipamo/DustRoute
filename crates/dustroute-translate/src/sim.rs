use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::connectivity::{
    comparator_input_pos, comparator_output_pos, device_side_positions, observer_input_pos,
    repeater_input_pos, repeater_output_pos,
};
use crate::electrical::{
    DeviceOutputState, ElectricalSolveError, ElectricalTopology, InstantaneousElectricalState,
    PoweredBlockState, repeater_input_level, solve_instantaneous_with_topology,
    torch_support_is_powered,
};
use crate::torch_law::{self, TorchState};
use crate::world::{Block, BlockKind, Pos, World};
use crate::world_laws::{CompatibilityWorldLaws, builtin_compatibility_world_laws};
use dustroute_minecraft::execution_context::{WorldExecutionContext, WorldExecutionProfile};
use dustroute_minecraft::lamp_law::LampDeadline;
use dustroute_minecraft::observer_law::{ObserverChanges, ObserverDeadline, ObserverPulseAction};
use dustroute_minecraft::repeater_law::CompatibilityRepeaterState;
use dustroute_minecraft::time::{PhysicsEventPhase, PhysicsTime, SchedulerProfile};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputMutationError {
    Missing {
        position: Pos,
    },
    WrongKind {
        position: Pos,
        expected: &'static str,
        actual: BlockKind,
    },
    InvalidPressureLevel {
        position: Pos,
        level: u8,
    },
    Solver(ElectricalSolveError),
}

impl std::fmt::Display for InputMutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { position } => write!(f, "no input block at {position:?}"),
            Self::WrongKind {
                position,
                expected,
                actual,
            } => write!(
                f,
                "input at {position:?} must be {expected}, found {actual:?}"
            ),
            Self::InvalidPressureLevel { position, level } => write!(
                f,
                "pressure plate level {level} at {position:?} is outside 0..=15"
            ),
            Self::Solver(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for InputMutationError {}

impl From<ElectricalSolveError> for InputMutationError {
    fn from(value: ElectricalSolveError) -> Self {
        Self::Solver(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickState {
    pub tick: u64,
    pub strengths: BTreeMap<Pos, u8>,
    pub block_power: BTreeMap<Pos, PoweredBlockState>,
    pub repeater_powered: BTreeMap<Pos, bool>,
    pub torch_lit: BTreeMap<Pos, bool>,
    pub comparator_output: BTreeMap<Pos, u8>,
    pub observer_powered: BTreeMap<Pos, bool>,
    pub lamp_lit: BTreeMap<Pos, bool>,
    /// Historical modeled burnout crossings. Retained under the existing public
    /// name so callers continue to request live evidence; not a current-lit bit.
    pub torch_burnout_candidates: BTreeSet<Pos>,
    pub instantaneous_iterations: usize,
}

/// State visible to an Observer at its front face. Block identity and
/// simulator-visible electrical/device state are both retained so a pulse is
/// scheduled for block-state changes as well as signal transitions.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ObserverObservedState {
    block: Option<Block>,
    signal: u8,
    power: PoweredBlockState,
    repeater_powered: bool,
    torch_lit: bool,
    comparator_output: u8,
    observer_powered: bool,
    lamp_lit: bool,
}

impl TickState {
    #[must_use]
    pub fn strength(&self, pos: Pos) -> u8 {
        self.strengths.get(&pos).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn power(&self, pos: Pos) -> PoweredBlockState {
        self.block_power.get(&pos).copied().unwrap_or_default()
    }

    #[must_use]
    pub fn powered(&self, pos: Pos) -> bool {
        self.lamp_lit.get(&pos).copied().unwrap_or(false)
            || self.repeater_powered.get(&pos).copied().unwrap_or(false)
            || self.torch_lit.get(&pos).copied().unwrap_or(false)
            || self.comparator_output.get(&pos).copied().unwrap_or(0) > 0
            || self.observer_powered.get(&pos).copied().unwrap_or(false)
            || self.strength(pos) > 0
            || self.power(pos).powered()
    }
}

/// The canonical unit emitted by the translate simulator.
///
/// A simulator step consumes one scheduler event and exposes the result as an
/// ordered before/after edge. A step can be a `NoOp`: time advanced or a
/// boundary was prepared while no observable electrical/device state changed.
/// Keeping that distinction explicit prevents callers from manufacturing a
/// transition merely because a tick elapsed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationTransition {
    /// Scheduler coordinate of the event that produced this edge.
    pub time: PhysicsTime,
    /// Block/scheduler event that was consumed. The boundary and resolve
    /// variants are intentionally visible so callers can distinguish a
    /// device update from a time-only compatibility step.
    pub event_kind: SimulationEventKind,
    pub from: TickState,
    pub to: TickState,
    pub kind: SimulationTransitionKind,
}

impl SimulationTransition {
    fn new(
        time: PhysicsTime,
        event_kind: SimulationEventKind,
        from: TickState,
        to: TickState,
    ) -> Self {
        let changed = from.strengths != to.strengths
            || from.block_power != to.block_power
            || from.repeater_powered != to.repeater_powered
            || from.torch_lit != to.torch_lit
            || from.comparator_output != to.comparator_output
            || from.observer_powered != to.observer_powered
            || from.lamp_lit != to.lamp_lit
            || from.torch_burnout_candidates != to.torch_burnout_candidates;
        Self {
            time,
            event_kind,
            from,
            to,
            kind: if changed {
                SimulationTransitionKind::Changed
            } else {
                SimulationTransitionKind::NoOp
            },
        }
    }

    #[must_use]
    pub const fn is_noop(&self) -> bool {
        matches!(self.kind, SimulationTransitionKind::NoOp)
    }

    #[must_use]
    pub const fn elapsed_ticks(&self) -> u64 {
        self.to.tick.saturating_sub(self.from.tick)
    }

    #[must_use]
    pub fn before_powered(&self, pos: Pos) -> bool {
        self.from.powered(pos)
    }

    #[must_use]
    pub fn after_powered(&self, pos: Pos) -> bool {
        self.to.powered(pos)
    }
}

/// Whether a compatibility scheduler step changed an observable state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationTransitionKind {
    Changed,
    NoOp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationEventKind {
    /// Advances the compatibility redstone-tick boundary and prepares the
    /// block-specific events for that boundary.
    CompatibilityBoundary,
    /// Applies the delayed repeater outputs prepared at the boundary.
    RepeaterUpdate,
    /// Applies the delayed comparator outputs prepared at the boundary.
    ComparatorUpdate,
    /// Applies torch output changes and updates the burnout observation.
    TorchUpdate,
    /// Turns an Observer off at the end of its pulse.
    ObserverPulseEnd,
    /// Turns an Observer on for a newly detected front-face change.
    ObserverPulseStart,
    /// Resolves the instantaneous electrical state after device updates.
    SignalResolve,
    /// Applies lamp on/off behavior after signal resolution.
    LampUpdate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SimulationEvent {
    time: PhysicsTime,
    kind: SimulationEventKind,
}

/// State prepared by one compatibility boundary and consumed by the
/// block-specific events scheduled at that boundary. Computing all delayed
/// outputs from the same pre-boundary electrical state preserves the existing
/// synchronous kernel while exposing each commit as its own event.
#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingBoundary {
    game_tick: u64,
    before_observers: BTreeMap<Pos, ObserverObservedState>,
    next_repeater_states: BTreeMap<Pos, CompatibilityRepeaterState>,
    next_repeaters: BTreeMap<Pos, bool>,
    comparator_requested: BTreeMap<Pos, u8>,
    next_comparators: BTreeMap<Pos, u8>,
    next_torches: BTreeMap<Pos, bool>,
    next_torch_states: BTreeMap<Pos, TorchState>,
    pending_observers: BTreeSet<Pos>,
    observers_to_off: BTreeSet<Pos>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SimulationEventQueue {
    events: BTreeMap<u64, BTreeMap<PhysicsEventPhase, VecDeque<SimulationEvent>>>,
    next_sub_tick_order: BTreeMap<u64, u64>,
}

impl SimulationEventQueue {
    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    fn schedule(&mut self, game_tick: u64, phase: PhysicsEventPhase, kind: SimulationEventKind) {
        self.events
            .entry(game_tick)
            .or_default()
            .entry(phase)
            .or_default()
            .push_back(SimulationEvent {
                time: PhysicsTime {
                    game_tick,
                    phase,
                    sub_tick_order: 0,
                },
                kind,
            });
    }

    fn pop(&mut self, profile: &SchedulerProfile) -> Option<SimulationEvent> {
        let game_tick = *self.events.first_key_value()?.0;
        let phases = self.events.get_mut(&game_tick).expect("game tick exists");
        let phase = *phases
            .iter()
            .min_by_key(|(phase, _)| profile.phase_rank(**phase))?
            .0;
        let queue = phases.get_mut(&phase).expect("phase exists");
        let mut event = queue.pop_front().expect("phase queue is non-empty");
        if queue.is_empty() {
            phases.remove(&phase);
        }
        if phases.is_empty() {
            self.events.remove(&game_tick);
        }
        // Compatibility events are monotonic. Once an event from a newer
        // game tick is selected, counters for older ticks can never receive
        // another child event. Dropping them keeps long simulations bounded
        // without changing the order of events that remain eligible.
        self.next_sub_tick_order
            .retain(|queued_tick, _| *queued_tick >= game_tick);
        let sub_tick_order = self.next_sub_tick_order.entry(game_tick).or_default();
        event.time.sub_tick_order = *sub_tick_order;
        *sub_tick_order = sub_tick_order.saturating_add(1);
        Some(event)
    }
}

#[derive(Clone)]
pub struct RedstoneTickSimulator {
    laws: &'static CompatibilityWorldLaws,
    world: World,
    topology: ElectricalTopology,
    tick: u64,
    scheduler_profile: SchedulerProfile,
    current_time: PhysicsTime,
    event_queue: SimulationEventQueue,
    pending_boundary: Option<PendingBoundary>,
    repeater_powered: BTreeMap<Pos, bool>,
    repeater_states: BTreeMap<Pos, CompatibilityRepeaterState>,
    torch_lit: BTreeMap<Pos, bool>,
    torch_states: BTreeMap<Pos, TorchState>,
    torch_burnout_candidates: BTreeSet<Pos>,
    comparator_output: BTreeMap<Pos, u8>,
    comparator_queues: BTreeMap<Pos, VecDeque<u8>>,
    observer_powered: BTreeMap<Pos, bool>,
    observer_pending: BTreeSet<Pos>,
    observer_off_deadline: BTreeMap<Pos, u64>,
    observer_observations: BTreeMap<Pos, ObserverObservedState>,
    lamp_lit: BTreeMap<Pos, bool>,
    lamp_off_deadline: BTreeMap<Pos, u64>,
    instantaneous: InstantaneousElectricalState,
}

impl RedstoneTickSimulator {
    pub fn new(world: World) -> Result<Self, ElectricalSolveError> {
        crate::electrical::validate_compatibility_kinds(&world)?;
        let laws = builtin_compatibility_world_laws();
        let topology = ElectricalTopology::from_world(&world);
        let devices = DeviceOutputState::initially_lit(&world);
        let repeater_states = world
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::Repeater)
            .map(|(pos, block)| {
                let delay = block.delay.unwrap_or(1).clamp(1, 4);
                let powered = devices.repeater_powered.get(pos).copied().unwrap_or(false);
                (
                    *pos,
                    laws.repeater
                        .initial(delay, powered)
                        .expect("pinned repeater law accepts the retained initialization policy"),
                )
            })
            .collect();
        let instantaneous = solve_instantaneous_with_topology(&world, &devices, 128, &topology)?;
        let comparator_queues = devices
            .comparator_output
            .keys()
            .map(|pos| {
                (
                    *pos,
                    VecDeque::from([devices.comparator_output.get(pos).copied().unwrap_or(0)]),
                )
            })
            .collect();
        let lamp_lit = world
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::RedstoneLamp)
            .map(|(pos, _)| {
                (
                    *pos,
                    laws.lamp
                        .initially_lit(instantaneous.power(*pos).powered())
                        .expect("pinned lamp initialization"),
                )
            })
            .collect();
        let mut simulator = Self {
            laws,
            world,
            topology,
            tick: 0,
            scheduler_profile: SchedulerProfile::default(),
            current_time: PhysicsTime::default(),
            event_queue: SimulationEventQueue::default(),
            pending_boundary: None,
            repeater_powered: devices.repeater_powered,
            repeater_states,
            torch_states: devices
                .torch_lit
                .iter()
                .map(|(pos, lit)| (*pos, torch_law::initial(*lit)))
                .collect(),
            torch_lit: devices.torch_lit,
            torch_burnout_candidates: BTreeSet::new(),
            comparator_output: devices.comparator_output,
            comparator_queues,
            observer_powered: devices.observer_powered,
            observer_pending: BTreeSet::new(),
            observer_off_deadline: BTreeMap::new(),
            observer_observations: BTreeMap::new(),
            lamp_lit,
            lamp_off_deadline: BTreeMap::new(),
            instantaneous,
        };
        simulator.observer_observations = simulator.observer_states();
        simulator.notify_torches();
        Ok(simulator)
    }

    /// The compatibility world's complete law and execution assumptions.
    /// Device output caches, timers and observations remain per simulator.
    #[must_use]
    pub fn execution_context(&self) -> WorldExecutionContext {
        let mut context = self.laws.context.clone();
        context.scheduler = Some(self.scheduler_profile);
        context
    }

    /// Constructs this existing model after validating its entire contract.
    /// Other models and invented initial histories cannot be substituted.
    pub fn new_in_context(world: World, context: &WorldExecutionContext) -> Result<Self, String> {
        if context.profile != WorldExecutionProfile::RedstoneCompatibilityBoundaryV1 {
            return Err("compatibility simulator requires its compatibility world profile".into());
        }
        context.validate_world_kinds(&world)?;
        let mut simulator = Self::new(world).map_err(|error| error.to_string())?;
        simulator.scheduler_profile = context.scheduler.expect("validated scheduler");
        Ok(simulator)
    }

    fn devices(&self) -> DeviceOutputState {
        DeviceOutputState {
            repeater_powered: self.repeater_powered.clone(),
            torch_lit: self.torch_lit.clone(),
            comparator_output: self.comparator_output.clone(),
            observer_powered: self.observer_powered.clone(),
        }
    }

    /// Returns the ordering policy used by the compatibility event queue.
    #[must_use]
    pub const fn scheduler_profile(&self) -> SchedulerProfile {
        self.scheduler_profile
    }

    /// Overrides the event-ordering policy for a controlled fixture. Block
    /// delays remain owned by their device models and are not changed here.
    #[must_use]
    pub fn with_scheduler_profile(mut self, profile: SchedulerProfile) -> Self {
        self.scheduler_profile = profile;
        self
    }

    /// Logical scheduler coordinate of the last processed event.
    #[must_use]
    pub const fn time(&self) -> PhysicsTime {
        self.current_time
    }

    /// Number of queued compatibility events. The normal stepping API
    /// creates the next clock event lazily, so this is usually zero between
    /// calls and does not make a stable circuit appear non-quiescent.
    #[must_use]
    pub fn pending_scheduler_events(&self) -> usize {
        self.event_queue
            .events
            .values()
            .flat_map(BTreeMap::values)
            .map(VecDeque::len)
            .sum()
    }

    pub fn settle_instantaneous(&mut self) -> Result<TickState, ElectricalSolveError> {
        self.instantaneous =
            solve_instantaneous_with_topology(&self.world, &self.devices(), 128, &self.topology)?;
        self.notify_torches();
        Ok(self.snapshot())
    }

    fn notify_torches(&mut self) {
        for (pos, state) in &mut self.torch_states {
            let powered = torch_support_is_powered(&self.world, *pos, &self.instantaneous);
            *state = torch_law::notify(state, powered);
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> TickState {
        TickState {
            tick: self.tick,
            strengths: self.instantaneous.signal_levels.clone(),
            block_power: self.instantaneous.block_power.clone(),
            repeater_powered: self.repeater_powered.clone(),
            torch_lit: self.torch_lit.clone(),
            comparator_output: self.comparator_output.clone(),
            observer_powered: self.observer_powered.clone(),
            lamp_lit: self.lamp_lit.clone(),
            torch_burnout_candidates: self.torch_burnout_candidates.clone(),
            instantaneous_iterations: self.instantaneous.iterations,
        }
    }

    /// Returns whether a scheduled device or delayed output still has work
    /// queued for a future tick.  The queue is initialized with the current
    /// state, so merely having a repeater/comparator queue is not considered
    /// pending work; only a queued value that differs from the current output
    /// keeps the simulator non-quiescent.
    #[must_use]
    pub fn has_pending_events(&self) -> bool {
        !self.event_queue.is_empty()
            || self.pending_boundary.is_some()
            || self
                .torch_states
                .values()
                .any(|state| state.pending_game_ticks().is_some())
            || self.repeater_states.iter().any(|(pos, state)| {
                !self.repeater_powered.contains_key(pos) || state.has_pending_output()
            })
            || self.comparator_queues.iter().any(|(pos, queue)| {
                queue
                    .iter()
                    .any(|value| self.comparator_output.get(pos).copied() != Some(*value))
            })
            || !self.observer_pending.is_empty()
            || self
                .observer_off_deadline
                .values()
                .any(|deadline| *deadline > self.tick)
            || self
                .lamp_off_deadline
                .values()
                .any(|deadline| *deadline > self.tick)
    }

    /// Executes one canonical scheduler event. A compatibility boundary is
    /// followed by block-specific events at the same game tick; callers can
    /// therefore observe delayed device updates without making the legacy
    /// redstone tick the primary execution unit.
    pub fn step_event(&mut self) -> Result<SimulationTransition, ElectricalSolveError> {
        debug_assert!(self.scheduler_profile.validate().is_ok());
        if self.event_queue.is_empty() {
            debug_assert!(self.pending_boundary.is_none());
            let next_game_tick = self
                .scheduler_profile
                .game_tick_after_delay(self.current_time.game_tick, 2);
            self.event_queue.schedule(
                next_game_tick,
                PhysicsEventPhase::ScheduledTick,
                SimulationEventKind::CompatibilityBoundary,
            );
        }
        let event = self
            .event_queue
            .pop(&self.scheduler_profile)
            .expect("next compatibility event is queued");
        self.current_time = event.time;
        let before = self.snapshot();
        match event.kind {
            SimulationEventKind::CompatibilityBoundary => {
                self.prepare_boundary(event.time.game_tick);
            }
            SimulationEventKind::RepeaterUpdate => self.apply_repeater_update(),
            SimulationEventKind::ComparatorUpdate => self.apply_comparator_update(),
            SimulationEventKind::TorchUpdate => self.apply_torch_update(),
            SimulationEventKind::ObserverPulseEnd => self.apply_observer_pulse_end(),
            SimulationEventKind::ObserverPulseStart => self.apply_observer_pulse_start(),
            SimulationEventKind::SignalResolve => {
                self.instantaneous = solve_instantaneous_with_topology(
                    &self.world,
                    &self.devices(),
                    128,
                    &self.topology,
                )?;
                self.notify_torches();
            }
            SimulationEventKind::LampUpdate => self.apply_lamp_update(),
        }
        self.finish_boundary_if_drained();
        Ok(SimulationTransition::new(
            event.time,
            event.kind,
            before,
            self.snapshot(),
        ))
    }

    /// Calculates one synchronous compatibility boundary without mutating
    /// delayed device output queues. Each prepared value is committed by its
    /// corresponding block event, preserving the old result while exposing a
    /// real event boundary to transition consumers.
    fn prepare_boundary(&mut self, game_tick: u64) {
        debug_assert!(self.pending_boundary.is_none());
        self.tick = self.tick.saturating_add(1);
        let before_observers = self.observer_observations.clone();

        let mut next_repeater_states = BTreeMap::new();
        let mut next_repeaters = BTreeMap::new();
        for (pos, block) in self.world.iter() {
            if block.kind != BlockKind::Repeater {
                continue;
            }
            let requested = repeater_input_pos(&self.world, *pos).is_some_and(|input| {
                repeater_input_level(&self.world, input, &self.instantaneous) > 0
            });
            let locked = device_side_positions(&self.world, *pos).is_some_and(|sides| {
                sides.into_iter().any(|side| {
                    self.repeater_powered.get(&side).copied().unwrap_or(false)
                        && repeater_output_pos(&self.world, side) == Some(*pos)
                        || self.comparator_output.get(&side).copied().unwrap_or(0) > 0
                            && comparator_output_pos(&self.world, side) == Some(*pos)
                })
            });
            let next = self
                .laws
                .repeater
                .boundary(
                    self.repeater_states
                        .get(pos)
                        .expect("repeater state initialized"),
                    requested,
                    locked,
                )
                .expect("pinned repeater law accepts sampled Boolean inputs");
            next_repeaters.insert(*pos, next.powered());
            next_repeater_states.insert(*pos, next);
        }

        // The compatibility boundary spans two game ticks. The pinned program
        // executes both, retaining its history and pending recovery event.
        let next_torch_states: BTreeMap<_, _> = self
            .torch_states
            .iter()
            .map(|(pos, state)| (*pos, torch_law::advance(&torch_law::advance(state))))
            .collect();
        let next_torches = next_torch_states
            .iter()
            .map(|(pos, state)| (*pos, torch_law::lit(state)))
            .collect();

        let mut comparator_requested = BTreeMap::new();
        let mut next_comparators = BTreeMap::new();
        for (pos, block) in self.world.iter() {
            if block.kind != BlockKind::Comparator {
                continue;
            }
            let rear = comparator_input_pos(&self.world, *pos)
                .map(|input| electrical_level_at(input, &self.instantaneous))
                .unwrap_or(0);
            let sides = device_side_positions(&self.world, *pos)
                .map(|sides| sides.map(|side| electrical_level_at(side, &self.instantaneous)))
                .unwrap_or([0, 0]);
            let subtract =
                block.observed_properties.get("mode").map(String::as_str) == Some("subtract");
            let requested = self
                .laws
                .comparator
                .evaluate(rear, sides, subtract)
                .expect("pinned comparator law accepts the existing u8 input levels");
            comparator_requested.insert(*pos, requested);
            let mut queue = self
                .comparator_queues
                .get(pos)
                .cloned()
                .expect("comparator queue initialized");
            queue.pop_front();
            queue.push_back(requested);
            next_comparators.insert(*pos, queue.front().copied().unwrap_or(requested));
        }

        let pending_observers = std::mem::take(&mut self.observer_pending);
        let observers_to_off = self
            .observer_off_deadline
            .iter()
            .filter_map(|(pos, deadline)| (*deadline <= self.tick).then_some(*pos))
            .collect();
        self.pending_boundary = Some(PendingBoundary {
            game_tick,
            before_observers,
            next_repeater_states,
            next_repeaters,
            comparator_requested,
            next_comparators,
            next_torches,
            next_torch_states,
            pending_observers,
            observers_to_off,
        });

        if !self
            .pending_boundary
            .as_ref()
            .is_some_and(|pending| pending.next_repeaters.is_empty())
        {
            self.schedule_pending_event(
                game_tick,
                PhysicsEventPhase::ScheduledTick,
                SimulationEventKind::RepeaterUpdate,
            );
        }
        if !self
            .pending_boundary
            .as_ref()
            .is_some_and(|pending| pending.next_comparators.is_empty())
        {
            self.schedule_pending_event(
                game_tick,
                PhysicsEventPhase::ScheduledTick,
                SimulationEventKind::ComparatorUpdate,
            );
        }
        if !self
            .pending_boundary
            .as_ref()
            .is_some_and(|pending| pending.next_torches.is_empty())
        {
            self.schedule_pending_event(
                game_tick,
                PhysicsEventPhase::ScheduledTick,
                SimulationEventKind::TorchUpdate,
            );
        }
        if !self
            .pending_boundary
            .as_ref()
            .is_some_and(|pending| pending.observers_to_off.is_empty())
        {
            self.schedule_pending_event(
                game_tick,
                PhysicsEventPhase::BlockEvent,
                SimulationEventKind::ObserverPulseEnd,
            );
        }
        if !self
            .pending_boundary
            .as_ref()
            .is_some_and(|pending| pending.pending_observers.is_empty())
        {
            self.schedule_pending_event(
                game_tick,
                PhysicsEventPhase::BlockEvent,
                SimulationEventKind::ObserverPulseStart,
            );
        }
        self.schedule_pending_event(
            game_tick,
            PhysicsEventPhase::Observation,
            SimulationEventKind::SignalResolve,
        );
        self.schedule_pending_event(
            game_tick,
            PhysicsEventPhase::Observation,
            SimulationEventKind::LampUpdate,
        );
    }

    fn schedule_pending_event(
        &mut self,
        game_tick: u64,
        phase: PhysicsEventPhase,
        kind: SimulationEventKind,
    ) {
        self.event_queue.schedule(game_tick, phase, kind);
    }

    fn apply_repeater_update(&mut self) {
        let Some(pending) = self.pending_boundary.as_ref() else {
            return;
        };
        // Observation mutations historically retain per-position queues. Keep
        // that lifecycle; this migration does not invent device restart state.
        self.repeater_states
            .extend(pending.next_repeater_states.clone());
        self.repeater_powered = pending.next_repeaters.clone();
    }

    fn apply_comparator_update(&mut self) {
        let Some(pending) = self.pending_boundary.as_ref() else {
            return;
        };
        for (pos, requested) in &pending.comparator_requested {
            let queue = self
                .comparator_queues
                .get_mut(pos)
                .expect("comparator queue initialized");
            queue.pop_front();
            queue.push_back(*requested);
        }
        self.comparator_output = pending.next_comparators.clone();
    }

    fn apply_torch_update(&mut self) {
        let Some(next_states) = self
            .pending_boundary
            .as_ref()
            .map(|pending| pending.next_torch_states.clone())
        else {
            return;
        };
        for (pos, state) in &next_states {
            if torch_law::burnout_seen(state) {
                // Retain the conservative public diagnostic; modeled evidence
                // does not silently upgrade a scenario to live-world proof.
                self.torch_burnout_candidates.insert(*pos);
            }
            self.torch_lit.insert(*pos, torch_law::lit(state));
        }
        self.torch_states = next_states;
    }

    fn apply_observer_pulse_end(&mut self) {
        let Some(positions) = self
            .pending_boundary
            .as_ref()
            .map(|pending| pending.observers_to_off.clone())
        else {
            return;
        };
        for pos in positions {
            let due = self
                .observer_off_deadline
                .get(&pos)
                .is_some_and(|deadline| *deadline <= self.tick);
            if let Some(action) = self
                .laws
                .observer
                .end(self.world.kind_at(pos) == BlockKind::Observer, due)
                .expect("pinned observer end law")
            {
                self.apply_observer_action(pos, action);
            }
        }
    }

    fn apply_observer_pulse_start(&mut self) {
        let Some(positions) = self
            .pending_boundary
            .as_ref()
            .map(|pending| pending.pending_observers.clone())
        else {
            return;
        };
        for pos in positions {
            if let Some(action) = self
                .laws
                .observer
                .start(self.world.kind_at(pos) == BlockKind::Observer)
                .expect("pinned observer start law")
            {
                self.apply_observer_action(pos, action);
            }
        }
    }

    fn apply_observer_action(&mut self, pos: Pos, action: ObserverPulseAction) {
        self.observer_powered.insert(pos, action.powered);
        match action.deadline {
            ObserverDeadline::Keep => {}
            ObserverDeadline::SetAfter(delay) => {
                self.observer_off_deadline
                    .insert(pos, self.tick.saturating_add(u64::from(delay)));
            }
            ObserverDeadline::Clear => {
                self.observer_off_deadline.remove(&pos);
            }
        }
    }

    fn apply_lamp_update(&mut self) {
        for (pos, block) in self.world.iter() {
            if block.kind != BlockKind::RedstoneLamp {
                continue;
            }
            let action = self.laws.lamp.boundary(
                self.instantaneous.power(*pos).powered(),
                self.lamp_lit.get(pos).copied().unwrap_or(false),
                self.lamp_off_deadline
                    .get(pos)
                    .map(|deadline| *deadline <= self.tick),
                self.tick == u64::MAX,
            );
            if let Some(lit) = action.lit {
                self.lamp_lit.insert(*pos, lit);
            }
            match action.deadline {
                LampDeadline::Keep => {}
                LampDeadline::SetAfter(delay) => {
                    self.lamp_off_deadline
                        .insert(*pos, self.tick.saturating_add(u64::from(delay)));
                }
                LampDeadline::Clear => {
                    self.lamp_off_deadline.remove(pos);
                }
            }
        }
        self.finish_boundary_if_drained();
    }

    fn finish_boundary_if_drained(&mut self) {
        if !self.event_queue.is_empty() {
            return;
        }
        if let Some(pending) = self.pending_boundary.take() {
            debug_assert_eq!(pending.game_tick, self.current_time.game_tick);
            self.record_observer_changes(&pending.before_observers);
        }
    }

    /// Drains the current compatibility boundary by repeatedly using
    /// [`Self::step_event`]. The returned list is used by the transition trace
    /// projection, while the public tick API only returns its final state.
    pub(crate) fn advance_tick_events(
        &mut self,
    ) -> Result<Vec<SimulationTransition>, ElectricalSolveError> {
        let target_tick = if self.pending_boundary.is_some() {
            self.tick
        } else {
            self.tick.saturating_add(1)
        };
        let mut steps = Vec::new();
        loop {
            let step = self.step_event()?;
            steps.push(step);
            if self.pending_boundary.is_none()
                && self.event_queue.is_empty()
                && self.tick >= target_tick
            {
                break;
            }
        }
        Ok(steps)
    }

    /// Transition-first alias for one scheduler event. It never drains
    /// multiple events to reach the next state-changing edge; a successful
    /// no-op event is returned explicitly in the `SimulationTransition`.
    pub fn step_transition(&mut self) -> Result<SimulationTransition, ElectricalSolveError> {
        self.step_event()
    }

    /// Compatibility projection of one complete historical redstone tick.
    pub fn advance_tick(&mut self) -> Result<TickState, ElectricalSolveError> {
        Ok(self
            .advance_tick_events()?
            .last()
            .map(|step| step.to.clone())
            .unwrap_or_else(|| self.snapshot()))
    }

    /// Applies several typed external-input changes as one world mutation
    /// batch. Observers compare their front-face state once around the whole
    /// batch, and the electrical fixed point is solved only once; this keeps
    /// exhaustive truth-table rows from paying one settle pass per input.
    pub fn set_input_states(
        &mut self,
        inputs: &[(Pos, bool)],
    ) -> Result<TickState, InputMutationError> {
        let before_observers = self.observer_observations.clone();
        for (pos, powered) in inputs {
            let Some(block) = self.world.get(*pos).cloned() else {
                // An absent position is the representation of an inferred
                // open-boundary driver. A false value leaves it absent.
                if *powered {
                    self.world.set(*pos, Block::new(BlockKind::RedstoneBlock));
                }
                continue;
            };
            match block.kind {
                BlockKind::Lever | BlockKind::Button => {
                    let mut changed = block;
                    changed.powered = Some(*powered);
                    self.world.set(*pos, changed);
                }
                BlockKind::PressurePlate => {
                    let mut changed = block;
                    changed.powered = Some(*powered);
                    changed.power_level = Some(if *powered { 15 } else { 0 });
                    self.world.set(*pos, changed);
                }
                BlockKind::Air | BlockKind::RedstoneBlock => {
                    if *powered {
                        self.world.set(*pos, Block::new(BlockKind::RedstoneBlock));
                    } else if self.world.kind_at(*pos) == BlockKind::RedstoneBlock {
                        self.world.remove(*pos);
                    }
                }
                actual => {
                    return Err(InputMutationError::WrongKind {
                        position: *pos,
                        expected: "lever, button, pressure_plate, air, or redstone_block",
                        actual,
                    });
                }
            }
        }
        crate::wire::update_wire_shapes(&mut self.world);
        self.topology = ElectricalTopology::from_world(&self.world);
        let state = self
            .settle_instantaneous()
            .map_err(InputMutationError::from)?;
        self.record_observer_changes(&before_observers);
        Ok(state)
    }

    pub fn set_powered(
        &mut self,
        pos: Pos,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        let before_observers = self.observer_observations.clone();
        let Some(block) = self.world.get(pos).cloned() else {
            return Err(InputMutationError::Missing { position: pos });
        };
        if !matches!(
            block.kind,
            BlockKind::Lever | BlockKind::Button | BlockKind::PressurePlate
        ) {
            return Err(InputMutationError::WrongKind {
                position: pos,
                expected: "lever, button, or pressure_plate",
                actual: block.kind,
            });
        }
        let mut changed = block;
        changed.powered = Some(powered);
        if changed.kind == BlockKind::PressurePlate {
            changed.power_level = Some(if powered { 15 } else { 0 });
        }
        self.world.set(pos, changed);
        let state = self
            .settle_instantaneous()
            .map_err(InputMutationError::from)?;
        self.record_observer_changes(&before_observers);
        Ok(state)
    }

    pub fn set_lever_state(
        &mut self,
        pos: Pos,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        self.set_stateful_powered(pos, BlockKind::Lever, powered)
    }

    pub fn set_button_state(
        &mut self,
        pos: Pos,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        self.set_stateful_powered(pos, BlockKind::Button, powered)
    }

    /// Sets a pressure plate's analog redstone level.  Entity occupancy is an
    /// external concern; the simulator accepts the observed level explicitly.
    pub fn set_pressure_plate_level(
        &mut self,
        pos: Pos,
        level: u8,
    ) -> Result<TickState, InputMutationError> {
        let before_observers = self.observer_observations.clone();
        if level > 15 {
            return Err(InputMutationError::InvalidPressureLevel {
                position: pos,
                level,
            });
        }
        let Some(block) = self.world.get(pos).cloned() else {
            return Err(InputMutationError::Missing { position: pos });
        };
        if block.kind != BlockKind::PressurePlate {
            return Err(InputMutationError::WrongKind {
                position: pos,
                expected: "pressure_plate",
                actual: block.kind,
            });
        }
        let mut changed = block;
        changed.power_level = Some(level);
        changed.powered = Some(level > 0);
        self.world.set(pos, changed);
        let state = self
            .settle_instantaneous()
            .map_err(InputMutationError::from)?;
        self.record_observer_changes(&before_observers);
        Ok(state)
    }

    fn set_stateful_powered(
        &mut self,
        pos: Pos,
        expected: BlockKind,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        let before_observers = self.observer_observations.clone();
        let Some(block) = self.world.get(pos).cloned() else {
            return Err(InputMutationError::Missing { position: pos });
        };
        if block.kind != expected {
            return Err(InputMutationError::WrongKind {
                position: pos,
                expected: match expected {
                    BlockKind::Lever => "lever",
                    BlockKind::Button => "button",
                    _ => "stateful input",
                },
                actual: block.kind,
            });
        }
        let mut changed = block;
        changed.powered = Some(powered);
        self.world.set(pos, changed);
        let state = self
            .settle_instantaneous()
            .map_err(InputMutationError::from)?;
        self.record_observer_changes(&before_observers);
        Ok(state)
    }

    /// Drives an inferred open-boundary input without making the temporary
    /// source part of the circuit's persistent physical representation.
    pub fn set_external_powered(
        &mut self,
        pos: Pos,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        let before_observers = self.observer_observations.clone();
        let current = self.world.kind_at(pos);
        if !matches!(current, BlockKind::Air | BlockKind::RedstoneBlock) {
            return Err(InputMutationError::WrongKind {
                position: pos,
                expected: "air or redstone_block for an external driver",
                actual: current,
            });
        }
        if powered {
            self.world
                .set(pos, crate::world::Block::new(BlockKind::RedstoneBlock));
        } else if self.world.kind_at(pos) == BlockKind::RedstoneBlock {
            self.world.remove(pos);
        }
        crate::wire::update_wire_shapes(&mut self.world);
        self.topology = ElectricalTopology::from_world(&self.world);
        let state = self
            .settle_instantaneous()
            .map_err(InputMutationError::from)?;
        self.record_observer_changes(&before_observers);
        Ok(state)
    }

    /// Applies an observed block-state mutation and schedules any Observer
    /// whose front face saw the mutation. This is the bridge for future live
    /// world events; unlike input helpers it accepts arbitrary block kinds.
    pub fn set_block_state(
        &mut self,
        pos: Pos,
        block: Block,
    ) -> Result<TickState, InputMutationError> {
        let before_observers = self.observer_observations.clone();
        self.world.set(pos, block);
        crate::wire::update_wire_shapes(&mut self.world);
        self.topology = ElectricalTopology::from_world(&self.world);
        if self.world.kind_at(pos) == BlockKind::Observer {
            let powered = self
                .world
                .get(pos)
                .and_then(|block| block.powered)
                .unwrap_or(false);
            self.observer_powered.insert(pos, powered);
        } else {
            self.observer_powered.remove(&pos);
        }
        self.observer_powered
            .retain(|observer, _| self.world.kind_at(*observer) == BlockKind::Observer);
        self.observer_pending
            .retain(|observer| self.world.kind_at(*observer) == BlockKind::Observer);
        self.observer_off_deadline
            .retain(|observer, _| self.world.kind_at(*observer) == BlockKind::Observer);
        let state = self
            .settle_instantaneous()
            .map_err(InputMutationError::from)?;
        self.record_observer_changes(&before_observers);
        Ok(state)
    }

    fn observer_states(&self) -> BTreeMap<Pos, ObserverObservedState> {
        self.world
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::Observer)
            .map(|(pos, _)| {
                let observed = observer_input_pos(&self.world, *pos);
                let block = observed.and_then(|target| self.world.get(target).cloned());
                let target = observed.unwrap_or(*pos);
                (
                    *pos,
                    ObserverObservedState {
                        block,
                        signal: self.instantaneous.signal(target),
                        power: self.instantaneous.power(target),
                        repeater_powered: self
                            .repeater_powered
                            .get(&target)
                            .copied()
                            .unwrap_or(false),
                        torch_lit: self.torch_lit.get(&target).copied().unwrap_or(false),
                        comparator_output: self
                            .comparator_output
                            .get(&target)
                            .copied()
                            .unwrap_or(0),
                        observer_powered: self
                            .observer_powered
                            .get(&target)
                            .copied()
                            .unwrap_or(false),
                        lamp_lit: self.lamp_lit.get(&target).copied().unwrap_or(false),
                    },
                )
            })
            .collect()
    }

    fn record_observer_changes(&mut self, before: &BTreeMap<Pos, ObserverObservedState>) {
        let after = self.observer_states();
        for (pos, state) in &after {
            let previous = before.get(pos);
            let changes = previous
                .map(|previous| ObserverChanges {
                    block_record: previous.block != state.block,
                    signal: previous.signal != state.signal,
                    weak: previous.power.weak != state.power.weak,
                    strong: previous.power.strong != state.power.strong,
                    repeater: previous.repeater_powered != state.repeater_powered,
                    torch: previous.torch_lit != state.torch_lit,
                    comparator: previous.comparator_output != state.comparator_output,
                    observer: previous.observer_powered != state.observer_powered,
                    lamp: previous.lamp_lit != state.lamp_lit,
                })
                .unwrap_or_default();
            if self
                .laws
                .observer
                .should_notify(previous.is_some(), changes)
                .expect("pinned observer change law")
            {
                self.observer_pending.insert(*pos);
            }
        }
        self.observer_observations = after;
    }

    pub fn settle_ticks(&mut self, count: usize) -> Result<TickState, ElectricalSolveError> {
        let mut state = self.snapshot();
        for _ in 0..count {
            state = self.advance_tick()?;
        }
        Ok(state)
    }
}

fn electrical_level_at(pos: Pos, state: &InstantaneousElectricalState) -> u8 {
    state.signal(pos).max(state.power(pos).level())
}

#[cfg(test)]
mod tests {
    use crate::wire::update_wire_shapes;
    use crate::world::{Block, Facing};

    use super::*;

    #[test]
    fn transition_step_matches_legacy_tick_projection_and_retains_noop() {
        let world = World::new();
        let mut transition_simulator = RedstoneTickSimulator::new(world.clone()).unwrap();
        let mut legacy_simulator = RedstoneTickSimulator::new(world).unwrap();
        let before = transition_simulator.snapshot();

        let transition = transition_simulator.step_transition().unwrap();
        let legacy_after = legacy_simulator.advance_tick().unwrap();

        assert_eq!(transition.from, before);
        assert_eq!(transition.to, legacy_after);
        assert_eq!(transition.time.game_tick, 2);
        assert_eq!(transition.time.phase, PhysicsEventPhase::ScheduledTick);
        assert_eq!(transition.elapsed_ticks(), 1);
        assert!(transition.is_noop());
        assert_eq!(transition.kind, SimulationTransitionKind::NoOp);
    }

    #[test]
    fn transition_step_reports_device_change_without_changing_legacy_result() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.powered = Some(true);
        lever.support_offset = Some(Pos::new(1, 0, 0));
        let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
        torch.facing = Some(Facing::East);
        torch.support_offset = Some(Pos::new(-1, 0, 0));

        let mut transition_simulator = RedstoneTickSimulator::new(world.clone()).unwrap();
        let mut legacy_simulator = RedstoneTickSimulator::new(world).unwrap();
        let boundary = transition_simulator.step_transition().unwrap();
        let remaining = transition_simulator.advance_tick_events().unwrap();
        let legacy_after = legacy_simulator.advance_tick().unwrap();

        assert_eq!(
            boundary.event_kind,
            SimulationEventKind::CompatibilityBoundary
        );
        assert!(boundary.is_noop());
        assert_eq!(remaining.last().map(|step| &step.to), Some(&legacy_after));
        assert!(remaining.iter().any(|step| {
            step.event_kind == SimulationEventKind::TorchUpdate
                && step.kind == SimulationTransitionKind::Changed
                && step.from.torch_lit.get(&Pos::new(1, 0, 0)) == Some(&true)
                && step.to.torch_lit.get(&Pos::new(1, 0, 0)) == Some(&false)
        }));
    }

    #[test]
    fn compatibility_clock_does_not_accumulate_completed_tick_counters() {
        let mut simulator = RedstoneTickSimulator::new(World::new()).unwrap();
        for _ in 0..128 {
            simulator.advance_tick().unwrap();
        }
        assert!(simulator.event_queue.next_sub_tick_order.len() <= 1);
        assert_eq!(simulator.time().game_tick, 256);
    }

    #[test]
    fn scheduler_events_expose_block_specific_boundaries() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let observer = world.place(BlockKind::Observer, Pos::new(1, 0, 0));
        observer.facing = Some(Facing::East);
        let repeater = world.place(BlockKind::Repeater, Pos::new(3, 0, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(1);
        let comparator = world.place(BlockKind::Comparator, Pos::new(5, 0, 0));
        comparator.facing = Some(Facing::East);
        world.place(BlockKind::RedstoneTorch, Pos::new(7, 0, 0));
        world.set(Pos::new(9, 0, 0), Block::new(BlockKind::RedstoneLamp));

        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        let mut changed = Block::new(BlockKind::Transparent);
        changed.observed_name = Some("minecraft:glass".to_owned());
        simulator
            .set_block_state(Pos::new(0, 0, 0), changed)
            .unwrap();

        let steps = simulator.advance_tick_events().unwrap();
        let kinds = steps.iter().map(|step| step.event_kind).collect::<Vec<_>>();
        for expected in [
            SimulationEventKind::CompatibilityBoundary,
            SimulationEventKind::RepeaterUpdate,
            SimulationEventKind::ComparatorUpdate,
            SimulationEventKind::TorchUpdate,
            SimulationEventKind::ObserverPulseStart,
            SimulationEventKind::SignalResolve,
            SimulationEventKind::LampUpdate,
        ] {
            assert!(
                kinds.contains(&expected),
                "missing {expected:?} in {kinds:?}"
            );
        }
        assert!(steps.iter().all(|step| step.time.game_tick == 2));
    }

    #[test]
    fn torch_changes_only_on_tick() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.powered = Some(true);
        lever.support_offset = Some(Pos::new(1, 0, 0));
        let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
        torch.facing = Some(Facing::East);
        torch.support_offset = Some(Pos::new(-1, 0, 0));
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert_eq!(simulator.snapshot().strength(Pos::new(1, 0, 0)), 15);
        simulator.settle_instantaneous().unwrap();
        assert_eq!(simulator.snapshot().strength(Pos::new(1, 0, 0)), 15);
        assert_eq!(
            simulator
                .advance_tick()
                .unwrap()
                .strength(Pos::new(1, 0, 0)),
            0
        );
    }

    #[test]
    fn observer_adapter_preserves_every_observed_field_and_missing_baselines() {
        let observer = Pos::new(1, 1, 0);
        let mut world = World::new();
        world.place(BlockKind::Observer, observer).facing = Some(Facing::East);
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::Solid));
        let mut sim = RedstoneTickSimulator::new(world).unwrap();
        let actual = sim.observer_states();
        for field in 0..9 {
            let mut before = actual.clone();
            let previous = before.get_mut(&observer).unwrap();
            match field {
                0 => previous.block = None,
                1 => previous.signal = 1,
                2 => previous.power.weak = 1,
                3 => previous.power.strong = 1,
                4 => previous.repeater_powered = !previous.repeater_powered,
                5 => previous.torch_lit = !previous.torch_lit,
                6 => previous.comparator_output = 1,
                7 => previous.observer_powered = !previous.observer_powered,
                8 => previous.lamp_lit = !previous.lamp_lit,
                _ => unreachable!(),
            }
            sim.observer_pending.clear();
            sim.record_observer_changes(&before);
            assert!(sim.observer_pending.contains(&observer), "field {field}");
        }
        for before in [&actual, &BTreeMap::new()] {
            sim.observer_pending.clear();
            sim.record_observer_changes(before);
            assert!(sim.observer_pending.is_empty());
        }
    }

    #[test]
    fn observer_emits_a_one_redstone_tick_pulse_after_observed_input_changes() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(2, 0, 0), Block::new(BlockKind::Solid));
        let lever = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
        lever.powered = Some(false);
        let observer = world.place(BlockKind::Observer, Pos::new(1, 1, 0));
        observer.facing = Some(Facing::East);
        observer.powered = Some(false);
        world.place(BlockKind::RedstoneWire, Pos::new(2, 1, 0));
        update_wire_shapes(&mut world);

        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert!(!simulator.snapshot().observer_powered[&Pos::new(1, 1, 0)]);
        simulator.set_lever_state(Pos::new(0, 1, 0), true).unwrap();
        assert!(!simulator.snapshot().observer_powered[&Pos::new(1, 1, 0)]);

        let high = simulator.advance_tick().unwrap();
        assert!(high.observer_powered[&Pos::new(1, 1, 0)]);
        assert_eq!(high.strength(Pos::new(2, 1, 0)), 15);

        let low = simulator.advance_tick().unwrap();
        assert!(!low.observer_powered[&Pos::new(1, 1, 0)]);
        assert_eq!(low.strength(Pos::new(2, 1, 0)), 0);
    }

    #[test]
    fn observer_detects_an_arbitrary_block_state_mutation() {
        let mut world = World::new();
        world.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
        let observer = world.place(BlockKind::Observer, Pos::new(1, 1, 0));
        observer.facing = Some(Facing::East);
        world.place(BlockKind::RedstoneWire, Pos::new(2, 1, 0));
        update_wire_shapes(&mut world);
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();

        let mut changed = Block::new(BlockKind::Transparent);
        changed.observed_name = Some("minecraft:glass".to_owned());
        simulator
            .set_block_state(Pos::new(0, 1, 0), changed)
            .unwrap();
        let state = simulator.advance_tick().unwrap();
        assert!(state.observer_powered[&Pos::new(1, 1, 0)]);
    }

    #[test]
    fn repeater_refreshes_signal_after_delay() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(3, 0, 0),
            Block::new(BlockKind::Solid),
        );
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::RedstoneBlock));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(2, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(1);
        world.place(BlockKind::RedstoneWire, Pos::new(3, 1, 0));
        update_wire_shapes(&mut world);
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert_eq!(simulator.snapshot().strength(Pos::new(3, 1, 0)), 0);
        assert_eq!(
            simulator
                .advance_tick()
                .unwrap()
                .strength(Pos::new(3, 1, 0)),
            15
        );
    }

    #[test]
    fn powered_side_repeater_locks_main_repeater_low() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, -2),
            Pos::new(2, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let input = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
        input.powered = Some(false);
        input.support_offset = Some(Pos::new(0, -1, 0));
        let main = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
        main.facing = Some(Facing::East);
        main.delay = Some(1);
        let lock = world.place(BlockKind::Repeater, Pos::new(1, 1, -1));
        lock.facing = Some(Facing::South);
        lock.delay = Some(1);
        world.set(Pos::new(1, 1, -2), Block::new(BlockKind::RedstoneBlock));

        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        simulator.advance_tick().unwrap();
        assert!(simulator.snapshot().repeater_powered[&Pos::new(1, 1, -1)]);
        simulator.set_powered(Pos::new(0, 1, 0), true).unwrap();
        simulator.advance_tick().unwrap();
        assert!(!simulator.snapshot().repeater_powered[&Pos::new(1, 1, 0)]);
    }

    #[test]
    fn imported_powered_repeater_keeps_its_initial_delay_state() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(2, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let source = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
        source.powered = Some(true);
        source.support_offset = Some(Pos::new(0, -1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(2);
        repeater.powered = Some(true);

        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert!(simulator.snapshot().repeater_powered[&Pos::new(1, 1, 0)]);
        assert!(simulator.advance_tick().unwrap().repeater_powered[&Pos::new(1, 1, 0)]);
    }

    #[test]
    fn one_tick_input_pulse_survives_a_two_tick_repeater_delay() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(2, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let input = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
        input.powered = Some(false);
        input.support_offset = Some(Pos::new(0, -1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(2);
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        simulator.set_powered(Pos::new(0, 1, 0), true).unwrap();
        simulator.advance_tick().unwrap();
        simulator.set_powered(Pos::new(0, 1, 0), false).unwrap();
        assert!(simulator.advance_tick().unwrap().repeater_powered[&Pos::new(1, 1, 0)]);
        assert!(!simulator.advance_tick().unwrap().repeater_powered[&Pos::new(1, 1, 0)]);
    }

    fn comparator_world(mode: &str) -> World {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, -3),
            Pos::new(3, 0, 0),
            Block::new(BlockKind::Solid),
        );
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::RedstoneBlock));
        world.set(Pos::new(2, 1, -3), Block::new(BlockKind::RedstoneBlock));
        world.place(BlockKind::RedstoneWire, Pos::new(2, 1, -2));
        world.place(BlockKind::RedstoneWire, Pos::new(2, 1, -1));
        let comparator = world.place(BlockKind::Comparator, Pos::new(2, 1, 0));
        comparator.facing = Some(Facing::East);
        comparator
            .observed_properties
            .insert("mode".to_owned(), mode.to_owned());
        world.place(BlockKind::RedstoneWire, Pos::new(3, 1, 0));
        update_wire_shapes(&mut world);
        world
    }

    #[test]
    fn comparator_compare_and_subtract_preserve_analog_strength() {
        let mut compare = RedstoneTickSimulator::new(comparator_world("compare")).unwrap();
        assert_eq!(
            compare.advance_tick().unwrap().strength(Pos::new(3, 1, 0)),
            15
        );

        let mut subtract = RedstoneTickSimulator::new(comparator_world("subtract")).unwrap();
        assert_eq!(
            subtract.advance_tick().unwrap().strength(Pos::new(3, 1, 0)),
            1
        );
    }

    #[test]
    fn lamp_turns_on_from_wire_and_uses_delayed_off() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(2, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let input = world.place(BlockKind::Button, Pos::new(0, 1, 0));
        input.powered = Some(true);
        input.support_offset = Some(Pos::new(0, -1, 0));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        world.set(Pos::new(2, 1, 0), Block::new(BlockKind::RedstoneLamp));
        update_wire_shapes(&mut world);
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert!(simulator.snapshot().lamp_lit[&Pos::new(2, 1, 0)]);
        simulator.set_powered(Pos::new(0, 1, 0), false).unwrap();
        assert!(simulator.advance_tick().unwrap().lamp_lit[&Pos::new(2, 1, 0)]);
        assert!(simulator.advance_tick().unwrap().lamp_lit[&Pos::new(2, 1, 0)]);
        assert!(!simulator.advance_tick().unwrap().lamp_lit[&Pos::new(2, 1, 0)]);
    }

    #[test]
    fn lamp_world_adapter_retains_saturating_expiry_at_the_last_tick() {
        let pos = Pos::new(0, 0, 0);
        let mut world = World::new();
        world.place(BlockKind::RedstoneLamp, pos);
        let mut sim = RedstoneTickSimulator::new(world).unwrap();
        sim.lamp_lit.insert(pos, true);
        sim.tick = u64::MAX - 1;
        sim.apply_lamp_update();
        assert!(sim.lamp_lit[&pos]);
        assert_eq!(sim.lamp_off_deadline[&pos], u64::MAX);
        sim.tick = u64::MAX;
        sim.apply_lamp_update();
        assert!(!sim.lamp_lit[&pos]);
        assert!(!sim.lamp_off_deadline.contains_key(&pos));
        // A newly requested deadline at MAX saturates and expires immediately.
        sim.lamp_lit.insert(pos, true);
        sim.apply_lamp_update();
        assert!(!sim.lamp_lit[&pos]);
        assert!(!sim.lamp_off_deadline.contains_key(&pos));
    }

    #[test]
    fn input_mutations_reject_missing_or_non_input_blocks() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert!(matches!(
            simulator.set_powered(Pos::new(0, 1, 0), true),
            Err(InputMutationError::WrongKind { .. })
        ));
        assert!(matches!(
            simulator.set_lever_state(Pos::new(9, 9, 9), true),
            Err(InputMutationError::Missing { .. })
        ));
    }

    #[test]
    fn weighted_pressure_plate_level_is_changed_with_its_boolean_state() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::PressurePlate, Pos::new(0, 1, 0));
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        simulator
            .set_pressure_plate_level(Pos::new(0, 1, 0), 7)
            .unwrap();
        assert_eq!(simulator.snapshot().power(Pos::new(0, 0, 0)).level(), 7);
        assert!(simulator.snapshot().powered(Pos::new(0, 0, 0)));
    }

    #[test]
    fn rapid_torch_toggling_is_reported_as_a_burnout_candidate() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.powered = Some(false);
        lever.support_offset = Some(Pos::new(1, 0, 0));
        let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
        torch.support_offset = Some(Pos::new(-1, 0, 0));
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        for tick in 0..16 {
            simulator
                .set_powered(Pos::new(-1, 0, 0), tick % 2 == 0)
                .unwrap();
            simulator.advance_tick().unwrap();
            if tick < 14 {
                assert!(simulator.snapshot().torch_burnout_candidates.is_empty());
            }
        }
        assert!(
            simulator
                .snapshot()
                .torch_burnout_candidates
                .contains(&Pos::new(1, 0, 0))
        );
        assert!(!simulator.snapshot().torch_lit[&Pos::new(1, 0, 0)]);
        assert!(simulator.has_pending_events());
        while simulator.snapshot().tick < 94 {
            simulator.advance_tick().unwrap();
        }
        assert!(!simulator.snapshot().torch_lit[&Pos::new(1, 0, 0)]);
        simulator.advance_tick().unwrap();
        assert!(simulator.snapshot().torch_lit[&Pos::new(1, 0, 0)]);
        assert!(!simulator.has_pending_events());
    }
}
