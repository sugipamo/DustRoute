use std::collections::BTreeMap;

use dustroute_physical::ComponentId;
use serde::{Deserialize, Serialize};

use crate::{BehaviorTrace, TraceTimeUnit};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PulsePolarity {
    High,
    Low,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PulseObservation {
    pub component: ComponentId,
    pub polarity: PulsePolarity,
    pub time_unit: TraceTimeUnit,
    pub start_tick: u64,
    pub end_tick: u64,
    pub width_ticks: u64,
    pub surrounding_steady_value: bool,
    /// Indices of the baseline, pulse-start, and pulse-end events in the
    /// source trace. This retains the exact causal interval used by the
    /// classifier without inventing an upstream cause.
    pub evidence_event_indices: [usize; 3],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SignalIntent {
    /// Only settled input/output behavior is specified. A transient is
    /// reported, but cannot be called a design violation.
    SteadyStateOnly,
    /// The signal must remain at this value throughout the observed interval.
    Stable { powered: bool },
    /// A pulse with this polarity and width range is an intended function.
    IntentionalPulse {
        polarity: PulsePolarity,
        time_unit: TraceTimeUnit,
        minimum_width_ticks: u64,
        maximum_width_ticks: u64,
    },
    /// Pulses up to the stated width are tolerated, but are not necessarily
    /// the purpose of the circuit.
    MaximumPulseWidth {
        polarity: PulsePolarity,
        time_unit: TraceTimeUnit,
        maximum_width_ticks: u64,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransientVerdict {
    PulseObserved,
    TransientDeviation,
    HazardCandidate,
    HazardConfirmed,
    IntentionalPulse,
}

impl TransientVerdict {
    const fn as_str(self) -> &'static str {
        match self {
            Self::PulseObserved => "pulse_observed",
            Self::TransientDeviation => "transient_deviation",
            Self::HazardCandidate => "hazard_candidate",
            Self::HazardConfirmed => "hazard_confirmed",
            Self::IntentionalPulse => "intentional_pulse",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransientFinding {
    pub pulse: PulseObservation,
    pub verdict: TransientVerdict,
    pub rationale: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransientAssessment {
    pub findings: Vec<TransientFinding>,
    pub counts: BTreeMap<String, usize>,
    pub contracts_applied: usize,
}

#[must_use]
pub fn observe_pulses(trace: &BehaviorTrace) -> Vec<PulseObservation> {
    let mut by_component = BTreeMap::<ComponentId, Vec<(usize, _)>>::new();
    for (index, event) in trace.events.iter().enumerate() {
        by_component
            .entry(event.component)
            .or_default()
            .push((index, event));
    }
    let mut pulses = Vec::new();
    for (component, events) in by_component {
        for window in events.windows(3) {
            let [
                (before_index, before),
                (start_index, start),
                (end_index, end),
            ] = window
            else {
                continue;
            };
            if before.powered != end.powered
                || start.powered == before.powered
                || end.tick <= start.tick
            {
                continue;
            }
            pulses.push(PulseObservation {
                component,
                polarity: if start.powered {
                    PulsePolarity::High
                } else {
                    PulsePolarity::Low
                },
                time_unit: trace.time_unit,
                start_tick: start.tick,
                end_tick: end.tick,
                width_ticks: end.tick - start.tick,
                surrounding_steady_value: before.powered,
                evidence_event_indices: [*before_index, *start_index, *end_index],
            });
        }
    }
    pulses
}

#[must_use]
pub fn assess_transients(
    trace: &BehaviorTrace,
    contracts: &BTreeMap<ComponentId, SignalIntent>,
) -> TransientAssessment {
    let findings = observe_pulses(trace)
        .into_iter()
        .map(|pulse| {
            let intent = contracts.get(&pulse.component);
            classify(pulse, intent)
        })
        .collect::<Vec<_>>();
    let mut counts = BTreeMap::new();
    for finding in &findings {
        *counts
            .entry(finding.verdict.as_str().to_owned())
            .or_default() += 1;
    }
    TransientAssessment {
        findings,
        counts,
        contracts_applied: contracts.len(),
    }
}

fn classify(pulse: PulseObservation, intent: Option<&SignalIntent>) -> TransientFinding {
    let (verdict, rationale) = match intent {
        None => (
            TransientVerdict::HazardCandidate,
            "a transient deviation was observed, but no design intent is registered".to_owned(),
        ),
        Some(SignalIntent::SteadyStateOnly) => (
            TransientVerdict::TransientDeviation,
            "the pulse differs from its surrounding steady value; the contract specifies settled behavior only".to_owned(),
        ),
        Some(SignalIntent::Stable { powered }) => (
            TransientVerdict::HazardConfirmed,
            format!(
                "the signal must remain {powered}, but a {:?} pulse was observed",
                pulse.polarity
            ),
        ),
        Some(SignalIntent::IntentionalPulse {
            polarity,
            time_unit,
            minimum_width_ticks,
            maximum_width_ticks,
        }) if *polarity == pulse.polarity
            && *time_unit == pulse.time_unit
            && (*minimum_width_ticks..=*maximum_width_ticks).contains(&pulse.width_ticks) =>
        {
            (
                TransientVerdict::IntentionalPulse,
                "the observed polarity and width match the registered pulse intent".to_owned(),
            )
        }
        Some(SignalIntent::IntentionalPulse { .. }) => (
            TransientVerdict::HazardConfirmed,
            "the observed pulse does not match the registered polarity or width".to_owned(),
        ),
        Some(SignalIntent::MaximumPulseWidth {
            polarity,
            time_unit,
            maximum_width_ticks,
        }) if *polarity == pulse.polarity
            && *time_unit == pulse.time_unit
            && pulse.width_ticks <= *maximum_width_ticks =>
        {
            (
                TransientVerdict::PulseObserved,
                "the observed pulse is within the registered tolerance".to_owned(),
            )
        }
        Some(SignalIntent::MaximumPulseWidth { .. }) => (
            TransientVerdict::HazardConfirmed,
            "the observed pulse exceeds the registered polarity or width tolerance".to_owned(),
        ),
    };
    TransientFinding {
        pulse,
        verdict,
        rationale,
    }
}

#[cfg(test)]
mod tests {
    use crate::BehaviorEvent;

    use super::*;

    fn pulse_trace() -> BehaviorTrace {
        BehaviorTrace {
            label: "one tick glitch".to_owned(),
            time_unit: TraceTimeUnit::RedstoneTick,
            events: vec![
                BehaviorEvent {
                    tick: 0,
                    sub_tick_order: 0,
                    game_tick: None,
                    phase: crate::TransitionPhase::Unknown,
                    event_kind: crate::EventKind::StateTransition,
                    cause: crate::EventCause::Unknown,
                    source: crate::EventSource::Unknown,
                    cause_sequence: None,
                    component: ComponentId(7),
                    powered: true,
                },
                BehaviorEvent {
                    tick: 3,
                    sub_tick_order: 0,
                    game_tick: None,
                    phase: crate::TransitionPhase::Unknown,
                    event_kind: crate::EventKind::StateTransition,
                    cause: crate::EventCause::Unknown,
                    source: crate::EventSource::Unknown,
                    cause_sequence: None,
                    component: ComponentId(7),
                    powered: false,
                },
                BehaviorEvent {
                    tick: 4,
                    sub_tick_order: 0,
                    game_tick: None,
                    phase: crate::TransitionPhase::Unknown,
                    event_kind: crate::EventKind::StateTransition,
                    cause: crate::EventCause::Unknown,
                    source: crate::EventSource::Unknown,
                    cause_sequence: None,
                    component: ComponentId(7),
                    powered: true,
                },
            ],
            stable: true,
            status: crate::TraceStatus::Complete,
        }
    }

    #[test]
    fn extracts_a_measured_pulse_without_assigning_intent() {
        let pulses = observe_pulses(&pulse_trace());
        assert_eq!(pulses.len(), 1);
        assert_eq!(pulses[0].polarity, PulsePolarity::Low);
        assert_eq!(pulses[0].width_ticks, 1);
        assert_eq!(pulses[0].evidence_event_indices, [0, 1, 2]);
    }

    #[test]
    fn missing_intent_is_only_a_candidate() {
        let assessment = assess_transients(&pulse_trace(), &BTreeMap::new());
        assert_eq!(
            assessment.findings[0].verdict,
            TransientVerdict::HazardCandidate
        );
    }

    #[test]
    fn contracts_distinguish_a_violation_from_an_intentional_pulse() {
        let stable = BTreeMap::from([(ComponentId(7), SignalIntent::Stable { powered: true })]);
        assert_eq!(
            assess_transients(&pulse_trace(), &stable).findings[0].verdict,
            TransientVerdict::HazardConfirmed
        );

        let intentional = BTreeMap::from([(
            ComponentId(7),
            SignalIntent::IntentionalPulse {
                polarity: PulsePolarity::Low,
                time_unit: TraceTimeUnit::RedstoneTick,
                minimum_width_ticks: 1,
                maximum_width_ticks: 2,
            },
        )]);
        assert_eq!(
            assess_transients(&pulse_trace(), &intentional).findings[0].verdict,
            TransientVerdict::IntentionalPulse
        );
    }
}
