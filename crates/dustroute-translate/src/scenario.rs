use std::collections::{BTreeMap, BTreeSet};

use dustroute_ir::{EventCause, EventKind, EventSource};
use serde::{Deserialize, Serialize};

use crate::{BlockKind, MinecraftSnapshot, Pos, RedstoneTickSimulator, world_from_snapshot};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioCapability {
    SteadyPower,
    DirectionalDust,
    RepeaterTiming,
    RepeaterLocking,
    TorchTiming,
    ComparatorAnalog,
    ObserverPulse,
    LampObservation,
    ExactWithinTickOrder,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScenarioAction {
    /// Legacy boolean input action.  It is retained for fixture compatibility
    /// but is validated against a real Lever, Button, or PressurePlate.
    SetPowered {
        redstone_tick: u64,
        position: Pos,
        powered: bool,
    },
    SetLeverState {
        redstone_tick: u64,
        position: Pos,
        powered: bool,
    },
    PressButton {
        redstone_tick: u64,
        position: Pos,
    },
    ReleaseButton {
        redstone_tick: u64,
        position: Pos,
    },
    SetPressurePlateLevel {
        redstone_tick: u64,
        position: Pos,
        level: u8,
    },
    SetExternalPower {
        redstone_tick: u64,
        position: Pos,
        powered: bool,
    },
}

impl ScenarioAction {
    const fn tick(&self) -> u64 {
        match self {
            Self::SetPowered { redstone_tick, .. }
            | Self::SetLeverState { redstone_tick, .. }
            | Self::PressButton { redstone_tick, .. }
            | Self::ReleaseButton { redstone_tick, .. }
            | Self::SetPressurePlateLevel { redstone_tick, .. }
            | Self::SetExternalPower { redstone_tick, .. } => *redstone_tick,
        }
    }

    fn apply(&self, simulator: &mut RedstoneTickSimulator) -> Result<(), String> {
        match self {
            Self::SetPowered {
                position, powered, ..
            } => simulator
                .set_powered(*position, *powered)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Self::SetLeverState {
                position, powered, ..
            } => simulator
                .set_lever_state(*position, *powered)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Self::PressButton { position, .. } => simulator
                .set_button_state(*position, true)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Self::ReleaseButton { position, .. } => simulator
                .set_button_state(*position, false)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Self::SetPressurePlateLevel {
                position, level, ..
            } => simulator
                .set_pressure_plate_level(*position, *level)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Self::SetExternalPower {
                position, powered, ..
            } => simulator
                .set_external_powered(*position, *powered)
                .map(|_| ())
                .map_err(|error| error.to_string()),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenarioExpectation {
    #[serde(default)]
    pub final_strengths: BTreeMap<Pos, u8>,
    #[serde(default)]
    pub final_powered: BTreeMap<Pos, bool>,
    #[serde(default)]
    pub pulses: Vec<ScenarioPulseExpectation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenarioPulseExpectation {
    pub position: Pos,
    pub powered: bool,
    pub minimum_width_redstone_ticks: u64,
    pub maximum_width_redstone_ticks: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Scenario {
    pub label: String,
    pub initial: MinecraftSnapshot,
    #[serde(default)]
    pub actions: Vec<ScenarioAction>,
    #[serde(default)]
    pub observe: BTreeSet<Pos>,
    pub duration_redstone_ticks: u64,
    #[serde(default)]
    pub required_capabilities: Vec<ScenarioCapability>,
    #[serde(default)]
    pub expectation: ScenarioExpectation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioSafety {
    Simulated,
    LiveObservationRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenarioEvent {
    pub redstone_tick: u64,
    /// Ordering within the corresponding game/redstone tick. Simulator
    /// traces use the observation iteration order; live traces preserve the
    /// Mineflayer packet order.
    #[serde(default)]
    pub sub_tick_order: u64,
    /// Coarse event classification retained independently from electrical
    /// state so a pulse edge is not mistaken for a steady signal.
    #[serde(default)]
    pub event_kind: EventKind,
    /// Causal evidence available at the scenario boundary. `unknown` is
    /// valid for a live packet trace whose server scheduler is opaque.
    #[serde(default)]
    pub cause: EventCause,
    #[serde(default)]
    pub source: EventSource,
    /// Optional parent event sequence. It is unset until a scheduler-aware
    /// event producer can provide a trustworthy causal link.
    #[serde(default)]
    pub cause_sequence: Option<u64>,
    pub sequence: u64,
    pub position: Pos,
    pub strength: u8,
    pub powered: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenarioTrace {
    pub duration_redstone_ticks: u64,
    pub events: Vec<ScenarioEvent>,
    pub final_strengths: BTreeMap<Pos, u8>,
    pub final_powered: BTreeMap<Pos, bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScenarioDifference {
    FinalStrength {
        position: Pos,
        expected: u8,
        actual: u8,
    },
    FinalPowered {
        position: Pos,
        expected: bool,
        actual: bool,
    },
    EventCount {
        expected: usize,
        actual: usize,
    },
    EventTick {
        position: Pos,
        expected: u64,
        actual: u64,
    },
    EventOrder {
        index: usize,
        expected_position: Pos,
        actual_position: Pos,
    },
    EventSubTickOrder {
        index: usize,
        position: Pos,
        expected: u64,
        actual: u64,
    },
    Event {
        index: usize,
        expected: ScenarioEvent,
        actual: ScenarioEvent,
    },
    MissingPulse {
        position: Pos,
        powered: bool,
    },
    PulseWidth {
        position: Pos,
        powered: bool,
        minimum: u64,
        maximum: u64,
        actual: u64,
    },
    UnsupportedPhysics {
        blocks: Vec<Pos>,
    },
    CapabilityUnavailable {
        capability: ScenarioCapability,
        blocks: Vec<Pos>,
        reason: String,
    },
    TorchBurnoutCandidate {
        blocks: Vec<Pos>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenarioRun {
    pub label: String,
    pub safety: ScenarioSafety,
    pub trace: ScenarioTrace,
    pub differences: Vec<ScenarioDifference>,
}

pub fn run_scenario(scenario: &Scenario) -> Result<ScenarioRun, String> {
    let world = world_from_snapshot(&scenario.initial).map_err(|error| error.to_string())?;
    let mut unsupported: Vec<_> = world
        .iter()
        .filter(|(_, block)| block.kind == BlockKind::Piston || block.requires_live_observation())
        .map(|(pos, _)| *pos)
        .collect();
    if scenario
        .required_capabilities
        .contains(&ScenarioCapability::ExactWithinTickOrder)
    {
        unsupported.extend(scenario.observe.iter().copied());
    }
    unsupported.sort();
    unsupported.dedup();
    if !unsupported.is_empty() {
        return Ok(ScenarioRun {
            label: scenario.label.clone(),
            safety: ScenarioSafety::LiveObservationRequired,
            trace: ScenarioTrace::default(),
            differences: vec![ScenarioDifference::UnsupportedPhysics {
                blocks: unsupported,
            }],
        });
    }
    if let Some((capability, blocks, reason)) = unavailable_capability(&world, scenario) {
        return Ok(ScenarioRun {
            label: scenario.label.clone(),
            safety: ScenarioSafety::LiveObservationRequired,
            trace: ScenarioTrace::default(),
            differences: vec![ScenarioDifference::CapabilityUnavailable {
                capability,
                blocks,
                reason,
            }],
        });
    }
    let block_kinds = world
        .iter()
        .map(|(position, block)| (*position, block.kind))
        .collect::<BTreeMap<_, _>>();
    let mut simulator = RedstoneTickSimulator::new(world).map_err(|error| error.to_string())?;
    let mut trace = ScenarioTrace::default();
    let mut previous = BTreeMap::new();
    let mut sequence = 0;
    for tick in 0..=scenario.duration_redstone_ticks {
        let mut sub_tick_order = 0;
        for action in scenario
            .actions
            .iter()
            .filter(|action| action.tick() == tick)
        {
            action.apply(&mut simulator)?;
        }
        let state = if tick == 0 {
            simulator.snapshot()
        } else {
            simulator
                .advance_tick()
                .map_err(|error| error.to_string())?
        };
        for position in &scenario.observe {
            let value = (state.strength(*position), state.powered(*position));
            if previous.get(position).copied() != Some(value) {
                let previous_value = previous.get(position).copied();
                let (event_kind, cause, source) = if tick == 0 && previous_value.is_none() {
                    (
                        EventKind::StateTransition,
                        EventCause::InitialSnapshot,
                        EventSource::InitialSnapshot,
                    )
                } else {
                    let (event_kind, cause) = simulated_event_metadata(
                        block_kinds.get(position).copied(),
                        previous_value,
                        value,
                    );
                    (event_kind, cause, EventSource::Simulator)
                };
                trace.events.push(ScenarioEvent {
                    redstone_tick: tick,
                    sub_tick_order,
                    event_kind,
                    cause,
                    source,
                    cause_sequence: None,
                    sequence,
                    position: *position,
                    strength: value.0,
                    powered: value.1,
                });
                sequence += 1;
                sub_tick_order += 1;
                previous.insert(*position, value);
            }
            trace.final_strengths.insert(*position, value.0);
            trace.final_powered.insert(*position, value.1);
        }
    }
    trace.duration_redstone_ticks = scenario.duration_redstone_ticks;
    let mut differences = compare_expectation(&scenario.expectation, &trace);
    let burnout: Vec<_> = simulator
        .snapshot()
        .torch_burnout_candidates
        .into_iter()
        .collect();
    if !burnout.is_empty() {
        differences.push(ScenarioDifference::TorchBurnoutCandidate { blocks: burnout });
    }
    Ok(ScenarioRun {
        label: scenario.label.clone(),
        safety: if differences.iter().any(|difference| {
            matches!(difference, ScenarioDifference::TorchBurnoutCandidate { .. })
        }) {
            ScenarioSafety::LiveObservationRequired
        } else {
            ScenarioSafety::Simulated
        },
        trace,
        differences,
    })
}

fn simulated_event_metadata(
    block_kind: Option<BlockKind>,
    previous: Option<(u8, bool)>,
    value: (u8, bool),
) -> (EventKind, EventCause) {
    if matches!(
        block_kind,
        Some(BlockKind::Lever | BlockKind::Button | BlockKind::PressurePlate)
    ) {
        return (EventKind::ExternalAction, EventCause::ExternalInput);
    }
    if block_kind == Some(BlockKind::Observer) {
        let event_kind = if value.1 && previous.is_none_or(|(_, powered)| !powered) {
            EventKind::PulseStart
        } else if !value.1 && previous.is_some_and(|(_, powered)| powered) {
            EventKind::PulseEnd
        } else {
            EventKind::StateTransition
        };
        return (event_kind, EventCause::ObserverFrontStateChange);
    }
    if block_kind == Some(BlockKind::Repeater) {
        return (EventKind::SignalPropagation, EventCause::RepeaterDelay);
    }
    (
        EventKind::SignalPropagation,
        EventCause::SimulatorPropagation,
    )
}

fn unavailable_capability(
    world: &crate::World,
    scenario: &Scenario,
) -> Option<(ScenarioCapability, Vec<Pos>, String)> {
    for capability in &scenario.required_capabilities {
        let mut blocks = Vec::new();
        let missing_observer = *capability == ScenarioCapability::ObserverPulse
            && !world
                .iter()
                .any(|(_, block)| block.kind == BlockKind::Observer);
        let reason = match capability {
            ScenarioCapability::SteadyPower => {
                for (pos, block) in world
                    .iter()
                    .filter(|(_, block)| block.kind.is_redstone_related())
                {
                    if block.kind == BlockKind::Observer
                        || block.capabilities().steady_state
                            == dustroute_minecraft::CapabilityLevel::Unsupported
                    {
                        blocks.push(*pos);
                    }
                }
                "steady-state semantics are not available for every observed redstone block"
            }
            ScenarioCapability::DirectionalDust => {
                for (pos, block) in world
                    .iter()
                    .filter(|(_, block)| block.kind.is_redstone_related())
                {
                    if block.capabilities().connectivity
                        == dustroute_minecraft::CapabilityLevel::Unsupported
                    {
                        blocks.push(*pos);
                    }
                }
                "directional connectivity is not available for every observed redstone block"
            }
            ScenarioCapability::RepeaterTiming => {
                for (pos, block) in world
                    .iter()
                    .filter(|(_, block)| block.kind == BlockKind::Repeater)
                {
                    if block.facing.is_none() || block.delay.is_none() {
                        blocks.push(*pos);
                    }
                }
                "repeater direction and delay must be observed before timing simulation"
            }
            ScenarioCapability::RepeaterLocking => {
                for (pos, block) in world
                    .iter()
                    .filter(|(_, block)| block.kind == BlockKind::Repeater)
                {
                    if block.facing.is_none() {
                        blocks.push(*pos);
                    }
                }
                "repeater direction must be observed before lock simulation"
            }
            ScenarioCapability::TorchTiming => {
                for (pos, block) in world
                    .iter()
                    .filter(|(_, block)| block.kind == BlockKind::RedstoneTorch)
                {
                    if block.support_offset.is_none() {
                        blocks.push(*pos);
                    }
                }
                "torch support direction must be observed before timing simulation"
            }
            ScenarioCapability::ComparatorAnalog => {
                for (pos, block) in world
                    .iter()
                    .filter(|(_, block)| block.kind == BlockKind::Comparator)
                {
                    if block.facing.is_none() {
                        blocks.push(*pos);
                    }
                }
                "comparator direction must be observed before analog simulation"
            }
            ScenarioCapability::ObserverPulse => {
                for (pos, block) in world
                    .iter()
                    .filter(|(_, block)| block.kind == BlockKind::Observer)
                {
                    if block.facing.is_none() {
                        blocks.push(*pos);
                    }
                }
                "observer facing must be observed before pulse simulation"
            }
            ScenarioCapability::LampObservation => {
                for (pos, _block) in world
                    .iter()
                    .filter(|(_, block)| block.kind == BlockKind::RedstoneLamp)
                {
                    if !scenario.observe.contains(pos) {
                        blocks.push(*pos);
                    }
                }
                "every observed lamp must be included in the scenario observation set"
            }
            ScenarioCapability::ExactWithinTickOrder => {
                blocks.extend(scenario.observe.iter().copied());
                "exact server-side tick ordering requires live observation"
            }
        };
        if *capability == ScenarioCapability::ExactWithinTickOrder
            || missing_observer
            || !blocks.is_empty()
        {
            return Some((*capability, blocks, reason.to_owned()));
        }
    }
    None
}

fn compare_expectation(
    expected: &ScenarioExpectation,
    actual: &ScenarioTrace,
) -> Vec<ScenarioDifference> {
    let mut differences = Vec::new();
    for (position, expected) in &expected.final_strengths {
        let actual = actual.final_strengths.get(position).copied().unwrap_or(0);
        if actual != *expected {
            differences.push(ScenarioDifference::FinalStrength {
                position: *position,
                expected: *expected,
                actual,
            });
        }
    }
    for (position, expected) in &expected.final_powered {
        let actual = actual.final_powered.get(position).copied().unwrap_or(false);
        if actual != *expected {
            differences.push(ScenarioDifference::FinalPowered {
                position: *position,
                expected: *expected,
                actual,
            });
        }
    }
    for expected in &expected.pulses {
        let widths = pulse_widths(actual, expected.position, expected.powered);
        if widths.is_empty() {
            differences.push(ScenarioDifference::MissingPulse {
                position: expected.position,
                powered: expected.powered,
            });
        } else if let Some(actual) = widths.into_iter().find(|width| {
            *width < expected.minimum_width_redstone_ticks
                || *width > expected.maximum_width_redstone_ticks
        }) {
            differences.push(ScenarioDifference::PulseWidth {
                position: expected.position,
                powered: expected.powered,
                minimum: expected.minimum_width_redstone_ticks,
                maximum: expected.maximum_width_redstone_ticks,
                actual,
            });
        }
    }
    differences
}

fn pulse_widths(trace: &ScenarioTrace, position: Pos, powered: bool) -> Vec<u64> {
    let events: Vec<_> = trace
        .events
        .iter()
        .filter(|event| event.position == position)
        .collect();
    let mut starts = Vec::new();
    let mut active = None;
    for event in events {
        if event.powered == powered && active.is_none() {
            active = Some(event.redstone_tick);
        } else if event.powered != powered {
            if let Some(start) = active.take() {
                starts.push(event.redstone_tick.saturating_sub(start));
            }
        }
    }
    if let Some(start) = active {
        starts.push(trace.duration_redstone_ticks.saturating_sub(start));
    }
    starts
}

#[must_use]
pub fn compare_scenario_traces(
    expected: &ScenarioTrace,
    actual: &ScenarioTrace,
) -> Vec<ScenarioDifference> {
    let mut differences = compare_expectation(
        &ScenarioExpectation {
            final_strengths: expected.final_strengths.clone(),
            final_powered: expected.final_powered.clone(),
            pulses: Vec::new(),
        },
        actual,
    );
    if expected.events.len() != actual.events.len() {
        differences.push(ScenarioDifference::EventCount {
            expected: expected.events.len(),
            actual: actual.events.len(),
        });
    }
    let mut expected_unordered = expected
        .events
        .iter()
        .map(|event| {
            (
                event.redstone_tick,
                event.position,
                event.strength,
                event.powered,
            )
        })
        .collect::<Vec<_>>();
    let mut actual_unordered = actual
        .events
        .iter()
        .map(|event| {
            (
                event.redstone_tick,
                event.position,
                event.strength,
                event.powered,
            )
        })
        .collect::<Vec<_>>();
    expected_unordered.sort();
    actual_unordered.sort();
    let only_order_differs = expected_unordered == actual_unordered;
    for (index, (expected, actual)) in expected.events.iter().zip(&actual.events).enumerate() {
        // Event provenance describes how the trace was produced and is not a
        // behavioral mismatch: a live packet necessarily says
        // `packet_observation`, while the simulator says
        // `simulator_propagation`. Compare the observable state and ordering
        // separately so provenance never invalidates an otherwise equivalent
        // live/simulated trace.
        let observable_match = expected.redstone_tick == actual.redstone_tick
            && expected.sub_tick_order == actual.sub_tick_order
            && expected.position == actual.position
            && expected.strength == actual.strength
            && expected.powered == actual.powered;
        if !observable_match {
            if expected.redstone_tick == actual.redstone_tick
                && expected.position == actual.position
                && expected.strength == actual.strength
                && expected.powered == actual.powered
                && expected.sub_tick_order != actual.sub_tick_order
            {
                differences.push(ScenarioDifference::EventSubTickOrder {
                    index,
                    position: expected.position,
                    expected: expected.sub_tick_order,
                    actual: actual.sub_tick_order,
                });
            } else if only_order_differs {
                differences.push(ScenarioDifference::EventOrder {
                    index,
                    expected_position: expected.position,
                    actual_position: actual.position,
                });
            } else if expected.position == actual.position
                && expected.strength == actual.strength
                && expected.powered == actual.powered
                && expected.redstone_tick != actual.redstone_tick
            {
                differences.push(ScenarioDifference::EventTick {
                    position: expected.position,
                    expected: expected.redstone_tick,
                    actual: actual.redstone_tick,
                });
            } else if expected.redstone_tick == actual.redstone_tick
                && expected.strength == actual.strength
                && expected.powered == actual.powered
                && expected.position != actual.position
            {
                differences.push(ScenarioDifference::EventOrder {
                    index,
                    expected_position: expected.position,
                    actual_position: actual.position,
                });
            } else {
                differences.push(ScenarioDifference::Event {
                    index,
                    expected: expected.clone(),
                    actual: actual.clone(),
                });
            }
        }
    }
    differences
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MinecraftSnapshotBlock;

    #[test]
    fn runs_a_shared_repeater_scenario_and_compares_traces() {
        let input = Pos::new(0, 1, 0);
        let output = Pos::new(2, 1, 0);
        let snapshot = MinecraftSnapshot {
            min: Pos::new(0, 0, 0),
            max: Pos::new(2, 1, 0),
            blocks: vec![
                MinecraftSnapshotBlock {
                    pos: Pos::new(0, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: BTreeMap::new(),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(1, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: BTreeMap::new(),
                },
                MinecraftSnapshotBlock {
                    pos: input,
                    name: "minecraft:lever".into(),
                    properties: BTreeMap::from([
                        ("face".into(), "floor".into()),
                        ("facing".into(), "west".into()),
                        ("powered".into(), "false".into()),
                    ]),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(1, 1, 0),
                    name: "minecraft:repeater".into(),
                    properties: BTreeMap::from([
                        ("facing".into(), "west".into()),
                        ("delay".into(), "1".into()),
                        ("powered".into(), "false".into()),
                    ]),
                },
                MinecraftSnapshotBlock {
                    pos: output,
                    name: "minecraft:redstone_wire".into(),
                    properties: BTreeMap::from([
                        ("west".into(), "side".into()),
                        ("east".into(), "side".into()),
                        ("power".into(), "0".into()),
                    ]),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(2, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: BTreeMap::new(),
                },
            ],
        };
        let scenario = Scenario {
            label: "one tick repeater".into(),
            initial: snapshot,
            actions: vec![ScenarioAction::SetPowered {
                redstone_tick: 0,
                position: input,
                powered: true,
            }],
            observe: BTreeSet::from([output]),
            duration_redstone_ticks: 1,
            required_capabilities: vec![ScenarioCapability::RepeaterTiming],
            expectation: ScenarioExpectation {
                final_strengths: BTreeMap::from([(output, 15)]),
                final_powered: BTreeMap::from([(output, true)]),
                pulses: Vec::new(),
            },
        };
        let run = run_scenario(&scenario).unwrap();
        assert_eq!(run.safety, ScenarioSafety::Simulated);
        assert!(run.differences.is_empty(), "{run:?}");
        assert!(run.trace.events.iter().any(|event| {
            event.event_kind == EventKind::SignalPropagation
                && event.cause == EventCause::SimulatorPropagation
                && event.source == EventSource::Simulator
        }));
        assert!(compare_scenario_traces(&run.trace, &run.trace).is_empty());
    }

    #[test]
    fn evaluates_pulse_width_contracts() {
        let input = Pos::new(0, 1, 0);
        let scenario = Scenario {
            label: "one tick input pulse".into(),
            initial: MinecraftSnapshot {
                min: Pos::new(0, 0, 0),
                max: input,
                blocks: vec![
                    MinecraftSnapshotBlock {
                        pos: Pos::new(0, 0, 0),
                        name: "minecraft:stone".into(),
                        properties: BTreeMap::new(),
                    },
                    MinecraftSnapshotBlock {
                        pos: input,
                        name: "minecraft:lever".into(),
                        properties: BTreeMap::from([
                            ("face".into(), "floor".into()),
                            ("facing".into(), "north".into()),
                            ("powered".into(), "false".into()),
                        ]),
                    },
                ],
            },
            actions: vec![
                ScenarioAction::SetPowered {
                    redstone_tick: 0,
                    position: input,
                    powered: true,
                },
                ScenarioAction::SetPowered {
                    redstone_tick: 1,
                    position: input,
                    powered: false,
                },
            ],
            observe: BTreeSet::from([input]),
            duration_redstone_ticks: 2,
            required_capabilities: vec![ScenarioCapability::SteadyPower],
            expectation: ScenarioExpectation {
                final_strengths: BTreeMap::from([(input, 0)]),
                final_powered: BTreeMap::from([(input, false)]),
                pulses: vec![ScenarioPulseExpectation {
                    position: input,
                    powered: true,
                    minimum_width_redstone_ticks: 1,
                    maximum_width_redstone_ticks: 1,
                }],
            },
        };
        assert!(run_scenario(&scenario).unwrap().differences.is_empty());
    }

    #[test]
    fn simulates_an_observer_pulse_from_a_block_state_transition() {
        let input = Pos::new(0, 1, 0);
        let observer = Pos::new(1, 1, 0);
        let output = Pos::new(2, 1, 0);
        let scenario = Scenario {
            label: "observer pulse".into(),
            initial: MinecraftSnapshot {
                min: Pos::new(0, 0, 0),
                max: output,
                blocks: vec![
                    MinecraftSnapshotBlock {
                        pos: Pos::new(0, 0, 0),
                        name: "minecraft:stone".into(),
                        properties: BTreeMap::new(),
                    },
                    MinecraftSnapshotBlock {
                        pos: Pos::new(2, 0, 0),
                        name: "minecraft:stone".into(),
                        properties: BTreeMap::new(),
                    },
                    MinecraftSnapshotBlock {
                        pos: input,
                        name: "minecraft:lever".into(),
                        properties: BTreeMap::from([
                            ("face".into(), "floor".into()),
                            ("facing".into(), "east".into()),
                            ("powered".into(), "false".into()),
                        ]),
                    },
                    MinecraftSnapshotBlock {
                        pos: observer,
                        name: "minecraft:observer".into(),
                        properties: BTreeMap::from([
                            ("facing".into(), "west".into()),
                            ("powered".into(), "false".into()),
                        ]),
                    },
                    MinecraftSnapshotBlock {
                        pos: output,
                        name: "minecraft:redstone_wire".into(),
                        properties: BTreeMap::new(),
                    },
                ],
            },
            actions: vec![ScenarioAction::SetLeverState {
                redstone_tick: 0,
                position: input,
                powered: true,
            }],
            observe: BTreeSet::from([output]),
            duration_redstone_ticks: 2,
            required_capabilities: vec![ScenarioCapability::ObserverPulse],
            expectation: ScenarioExpectation {
                final_strengths: BTreeMap::from([(output, 0)]),
                final_powered: BTreeMap::from([(output, false)]),
                pulses: vec![ScenarioPulseExpectation {
                    position: output,
                    powered: true,
                    minimum_width_redstone_ticks: 1,
                    maximum_width_redstone_ticks: 1,
                }],
            },
        };
        let run = run_scenario(&scenario).unwrap();
        assert_eq!(run.safety, ScenarioSafety::Simulated);
        assert!(run.differences.is_empty(), "{run:?}");
    }

    #[test]
    fn typed_button_actions_drive_only_a_button() {
        let button = Pos::new(0, 1, 0);
        let scenario = Scenario {
            label: "button input".into(),
            initial: MinecraftSnapshot {
                min: Pos::new(0, 0, 0),
                max: button,
                blocks: vec![
                    MinecraftSnapshotBlock {
                        pos: Pos::new(0, 0, 0),
                        name: "minecraft:stone".into(),
                        properties: BTreeMap::new(),
                    },
                    MinecraftSnapshotBlock {
                        pos: button,
                        name: "minecraft:stone_button".into(),
                        properties: BTreeMap::from([
                            ("face".into(), "floor".into()),
                            ("facing".into(), "north".into()),
                            ("powered".into(), "false".into()),
                        ]),
                    },
                ],
            },
            actions: vec![
                ScenarioAction::PressButton {
                    redstone_tick: 0,
                    position: button,
                },
                ScenarioAction::ReleaseButton {
                    redstone_tick: 1,
                    position: button,
                },
            ],
            observe: BTreeSet::from([button]),
            duration_redstone_ticks: 2,
            required_capabilities: Vec::new(),
            expectation: ScenarioExpectation::default(),
        };
        let run = run_scenario(&scenario).unwrap();
        assert_eq!(run.safety, ScenarioSafety::Simulated);
        assert!(run.differences.is_empty(), "{run:?}");
    }

    #[test]
    fn required_repeater_timing_is_gated_when_state_is_missing() {
        let repeater = Pos::new(0, 1, 0);
        let scenario = Scenario {
            label: "incomplete repeater timing".into(),
            initial: MinecraftSnapshot {
                min: Pos::new(0, 0, 0),
                max: repeater,
                blocks: vec![MinecraftSnapshotBlock {
                    pos: repeater,
                    name: "minecraft:repeater".into(),
                    properties: BTreeMap::new(),
                }],
            },
            actions: Vec::new(),
            observe: BTreeSet::from([repeater]),
            duration_redstone_ticks: 1,
            required_capabilities: vec![ScenarioCapability::RepeaterTiming],
            expectation: ScenarioExpectation::default(),
        };
        let run = run_scenario(&scenario).unwrap();
        assert_eq!(run.safety, ScenarioSafety::LiveObservationRequired);
        assert!(matches!(
            run.differences.as_slice(),
            [ScenarioDifference::CapabilityUnavailable {
                capability: ScenarioCapability::RepeaterTiming,
                blocks,
                ..
            }] if blocks == &[repeater]
        ));
    }

    #[test]
    fn target_snapshot_is_live_observation_only() {
        let scenario = Scenario {
            label: "target requires live observation".into(),
            initial: MinecraftSnapshot {
                min: Pos::new(0, 0, 0),
                max: Pos::new(0, 0, 0),
                blocks: vec![MinecraftSnapshotBlock {
                    pos: Pos::new(0, 0, 0),
                    name: "minecraft:target".into(),
                    properties: BTreeMap::new(),
                }],
            },
            actions: Vec::new(),
            observe: BTreeSet::new(),
            duration_redstone_ticks: 1,
            required_capabilities: Vec::new(),
            expectation: ScenarioExpectation::default(),
        };
        let run = run_scenario(&scenario).unwrap();
        assert_eq!(run.safety, ScenarioSafety::LiveObservationRequired);
        assert!(matches!(
            run.differences.as_slice(),
            [ScenarioDifference::UnsupportedPhysics { blocks }] if blocks == &[Pos::new(0, 0, 0)]
        ));
    }

    #[test]
    fn reports_same_tick_order_as_evidence_without_changing_state_mismatch() {
        let first = Pos::new(0, 1, 0);
        let second = Pos::new(1, 1, 0);
        let expected = ScenarioTrace {
            duration_redstone_ticks: 1,
            events: vec![
                ScenarioEvent {
                    redstone_tick: 1,
                    sub_tick_order: 0,
                    event_kind: EventKind::SignalPropagation,
                    cause: EventCause::SimulatorPropagation,
                    source: EventSource::Simulator,
                    cause_sequence: None,
                    sequence: 0,
                    position: first,
                    strength: 15,
                    powered: true,
                },
                ScenarioEvent {
                    redstone_tick: 1,
                    sub_tick_order: 1,
                    event_kind: EventKind::SignalPropagation,
                    cause: EventCause::SimulatorPropagation,
                    source: EventSource::Simulator,
                    cause_sequence: None,
                    sequence: 1,
                    position: second,
                    strength: 15,
                    powered: true,
                },
            ],
            final_strengths: BTreeMap::from([(first, 15), (second, 15)]),
            final_powered: BTreeMap::from([(first, true), (second, true)]),
        };
        let mut actual = expected.clone();
        actual.events[0].sub_tick_order = 1;
        actual.events[1].sub_tick_order = 0;
        let differences = compare_scenario_traces(&expected, &actual);
        assert_eq!(differences.len(), 2);
        assert!(
            differences.iter().all(|difference| matches!(
                difference,
                ScenarioDifference::EventSubTickOrder { .. }
            ))
        );
    }

    #[test]
    fn provenance_differences_do_not_break_behavioral_equivalence() {
        let position = Pos::new(0, 1, 0);
        let expected = ScenarioTrace {
            events: vec![ScenarioEvent {
                redstone_tick: 1,
                sub_tick_order: 0,
                event_kind: EventKind::SignalPropagation,
                cause: EventCause::SimulatorPropagation,
                source: EventSource::Simulator,
                cause_sequence: None,
                sequence: 0,
                position,
                strength: 15,
                powered: true,
            }],
            final_strengths: BTreeMap::from([(position, 15)]),
            final_powered: BTreeMap::from([(position, true)]),
            duration_redstone_ticks: 1,
        };
        let mut actual = expected.clone();
        actual.events[0].cause = EventCause::PacketObservation;
        actual.events[0].source = EventSource::LiveMineflayer;
        actual.events[0].cause_sequence = Some(7);
        assert!(compare_scenario_traces(&expected, &actual).is_empty());
    }

    #[test]
    fn legacy_scenario_event_json_defaults_to_coarse_order() {
        let event: ScenarioEvent = serde_json::from_str(
            r#"{"redstone_tick":3,"sequence":4,"position":{"x":0,"y":1,"z":0},"strength":15,"powered":true}"#,
        )
        .unwrap();
        assert_eq!(event.sub_tick_order, 0);
    }

    #[test]
    fn legacy_set_powered_rejects_a_wire_anchor() {
        let wire = Pos::new(0, 1, 0);
        let scenario = Scenario {
            label: "invalid wire driver".into(),
            initial: MinecraftSnapshot {
                min: Pos::new(0, 0, 0),
                max: wire,
                blocks: vec![
                    MinecraftSnapshotBlock {
                        pos: Pos::new(0, 0, 0),
                        name: "minecraft:stone".into(),
                        properties: BTreeMap::new(),
                    },
                    MinecraftSnapshotBlock {
                        pos: wire,
                        name: "minecraft:redstone_wire".into(),
                        properties: BTreeMap::new(),
                    },
                ],
            },
            actions: vec![ScenarioAction::SetPowered {
                redstone_tick: 0,
                position: wire,
                powered: true,
            }],
            observe: BTreeSet::new(),
            duration_redstone_ticks: 0,
            required_capabilities: Vec::new(),
            expectation: ScenarioExpectation::default(),
        };
        let error = run_scenario(&scenario).expect_err("wire mutation must be rejected");
        assert!(
            error.contains("must be lever, button, or pressure_plate"),
            "{error}"
        );
    }
}
