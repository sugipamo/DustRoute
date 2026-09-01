use crate::{
    ContextualVerificationState, MacroSteadyStateReport, MacroStructuralReport,
    MacroTransitionReport,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalContractMode {
    ExactTruthTable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimingContractMode {
    ExactTrace,
    BoundedDelay,
    SettledValueOnly,
    PreserveOrder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimingContract {
    pub mode: TimingContractMode,
    pub maximum_added_redstone_ticks: usize,
    pub settle_deadline_redstone_ticks: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PulseContract {
    pub allow_new_pulses: bool,
    pub allow_removed_pulses: bool,
    pub maximum_width_delta_redstone_ticks: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalogContract {
    pub preserve_strength: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundaryContract {
    pub preserve_blocks: bool,
    pub preserve_facing: bool,
    pub preserve_driver_positions: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MutationContract {
    pub focus_only: bool,
    pub allow_temporary_expansion: bool,
    pub maximum_changed_blocks: usize,
    pub automatic_apply: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptimizationContract {
    pub logical: LogicalContractMode,
    pub timing: TimingContract,
    pub pulse: PulseContract,
    pub analog: AnalogContract,
    pub boundary: BoundaryContract,
    pub mutation: MutationContract,
}

impl Default for OptimizationContract {
    fn default() -> Self {
        Self {
            logical: LogicalContractMode::ExactTruthTable,
            timing: TimingContract {
                mode: TimingContractMode::BoundedDelay,
                maximum_added_redstone_ticks: 5,
                settle_deadline_redstone_ticks: 20,
            },
            pulse: PulseContract {
                allow_new_pulses: false,
                allow_removed_pulses: false,
                maximum_width_delta_redstone_ticks: 0,
            },
            analog: AnalogContract {
                preserve_strength: false,
            },
            boundary: BoundaryContract {
                preserve_blocks: true,
                preserve_facing: true,
                preserve_driver_positions: true,
            },
            mutation: MutationContract {
                focus_only: true,
                allow_temporary_expansion: true,
                maximum_changed_blocks: 500,
                automatic_apply: false,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractCheckState {
    Passed,
    Failed,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractCheck {
    pub state: ContractCheckState,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptimizationContractAssessment {
    pub logical: ContractCheck,
    pub timing: ContractCheck,
    pub pulse: ContractCheck,
    pub analog: ContractCheck,
    pub boundary: ContractCheck,
    pub mutation: ContractCheck,
}

impl OptimizationContractAssessment {
    #[must_use]
    pub fn satisfied(&self) -> bool {
        [
            &self.logical,
            &self.timing,
            &self.pulse,
            &self.analog,
            &self.boundary,
            &self.mutation,
        ]
        .into_iter()
        .all(|check| check.state == ContractCheckState::Passed)
    }
}

#[must_use]
pub fn assess_macro_contract(
    contract: OptimizationContract,
    structural: &MacroStructuralReport,
    steady: Option<&MacroSteadyStateReport>,
    transitions: Option<&MacroTransitionReport>,
    changed_blocks: usize,
    analog_strength_verified: bool,
) -> OptimizationContractAssessment {
    let logical = match steady {
        Some(report) if report.state == ContextualVerificationState::Passed => passed(),
        Some(report) => failed(format!(
            "{} truth-table assignment(s) differ",
            report.differing_assignments.len()
        )),
        None => unavailable("steady-state truth table was not verified"),
    };
    let timing = assess_timing(contract.timing, transitions);
    let pulse = assess_pulses(contract.pulse, transitions);
    let analog = if !contract.analog.preserve_strength || analog_strength_verified {
        passed()
    } else {
        unavailable("analog strength preservation was requested but not verified")
    };
    let boundary = if structural.valid() {
        passed()
    } else {
        failed("structural or boundary validation failed")
    };
    let mutation = if changed_blocks <= contract.mutation.maximum_changed_blocks {
        passed()
    } else {
        failed(format!(
            "{changed_blocks} changed blocks exceeds contract maximum {}",
            contract.mutation.maximum_changed_blocks
        ))
    };
    OptimizationContractAssessment {
        logical,
        timing,
        pulse,
        analog,
        boundary,
        mutation,
    }
}

fn assess_timing(
    contract: TimingContract,
    transitions: Option<&MacroTransitionReport>,
) -> ContractCheck {
    let Some(report) = transitions else {
        return unavailable("transition traces were not verified");
    };
    if report.state == ContextualVerificationState::Pending {
        return unavailable(
            report
                .reason
                .clone()
                .unwrap_or_else(|| "transition traces were unavailable".to_owned()),
        );
    }
    let valid = match contract.mode {
        TimingContractMode::ExactTrace => report.differing_cases == 0,
        TimingContractMode::SettledValueOnly => report.cases.iter().all(final_values_match),
        TimingContractMode::BoundedDelay => report.cases.iter().all(|case| {
            final_values_match(case)
                && settle_tick(&case.candidate_outputs)
                    <= settle_tick(&case.original_outputs)
                        .saturating_add(contract.maximum_added_redstone_ticks)
                && settle_tick(&case.candidate_outputs) <= contract.settle_deadline_redstone_ticks
        }),
        TimingContractMode::PreserveOrder => report.cases.iter().all(|case| {
            transitions_of(&case.original_outputs) == transitions_of(&case.candidate_outputs)
        }),
    };
    if valid {
        passed()
    } else {
        failed("transition timing violates the selected timing mode")
    }
}

fn assess_pulses(
    contract: PulseContract,
    transitions: Option<&MacroTransitionReport>,
) -> ContractCheck {
    let Some(report) = transitions else {
        return unavailable("pulse traces were not verified");
    };
    if report.state == ContextualVerificationState::Pending {
        return unavailable(
            report
                .reason
                .clone()
                .unwrap_or_else(|| "pulse traces were unavailable".to_owned()),
        );
    }
    for case in &report.cases {
        let original = transient_widths(&case.original_outputs);
        let candidate = transient_widths(&case.candidate_outputs);
        if !contract.allow_new_pulses && candidate.len() > original.len() {
            return failed("candidate introduces a new transient pulse");
        }
        if !contract.allow_removed_pulses && candidate.len() < original.len() {
            return failed("candidate removes an existing transient pulse");
        }
        if original
            .iter()
            .zip(&candidate)
            .any(|(original, candidate)| {
                original.abs_diff(*candidate) > contract.maximum_width_delta_redstone_ticks
            })
        {
            return failed("transient pulse width exceeds the allowed delta");
        }
    }
    passed()
}

fn final_values_match(case: &crate::MacroTransitionCase) -> bool {
    case.original_outputs.last() == case.candidate_outputs.last()
}

fn settle_tick(trace: &[Vec<bool>]) -> usize {
    let Some(final_value) = trace.last() else {
        return 0;
    };
    trace
        .iter()
        .rposition(|value| value != final_value)
        .map_or(0, |index| index + 1)
}

fn transitions_of(trace: &[Vec<bool>]) -> Vec<Vec<bool>> {
    trace
        .windows(2)
        .filter(|pair| pair[0] != pair[1])
        .map(|pair| pair[1].clone())
        .collect()
}

fn transient_widths(trace: &[Vec<bool>]) -> Vec<usize> {
    let Some(first) = trace.first() else {
        return Vec::new();
    };
    let mut runs = vec![(first, 1_usize)];
    for value in &trace[1..] {
        if runs.last().is_some_and(|(previous, _)| *previous == value) {
            runs.last_mut().expect("run exists").1 += 1;
        } else {
            runs.push((value, 1));
        }
    }
    if runs.len() <= 2 {
        Vec::new()
    } else {
        runs[1..runs.len() - 1]
            .iter()
            .map(|(_, width)| *width)
            .collect()
    }
}

fn passed() -> ContractCheck {
    ContractCheck {
        state: ContractCheckState::Passed,
        reasons: Vec::new(),
    }
}

fn failed(reason: impl Into<String>) -> ContractCheck {
    ContractCheck {
        state: ContractCheckState::Failed,
        reasons: vec![reason.into()],
    }
}

fn unavailable(reason: impl Into<String>) -> ContractCheck {
    ContractCheck {
        state: ContractCheckState::Unavailable,
        reasons: vec![reason.into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MacroTransitionCase, MacroTransitionReport};

    #[test]
    fn default_contract_rejects_a_new_pulse_even_when_final_values_match() {
        let report = MacroTransitionReport {
            state: ContextualVerificationState::Failed,
            cases: vec![MacroTransitionCase {
                from: vec![false],
                to: vec![false],
                original_outputs: vec![vec![false]; 4],
                candidate_outputs: vec![vec![false], vec![true], vec![false], vec![false]],
                equivalent: false,
                first_difference_tick: Some(1),
            }],
            differing_cases: 1,
            reason: None,
        };
        let assessment = assess_pulses(OptimizationContract::default().pulse, Some(&report));
        assert_eq!(assessment.state, ContractCheckState::Failed);
    }

    #[test]
    fn pulse_detection_handles_a_transition_that_temporarily_reverts() {
        let report = MacroTransitionReport {
            state: ContextualVerificationState::Failed,
            cases: vec![MacroTransitionCase {
                from: vec![false],
                to: vec![true],
                original_outputs: vec![vec![false], vec![true], vec![true], vec![true]],
                candidate_outputs: vec![vec![false], vec![true], vec![false], vec![true]],
                equivalent: false,
                first_difference_tick: Some(2),
            }],
            differing_cases: 1,
            reason: None,
        };
        let assessment = assess_pulses(OptimizationContract::default().pulse, Some(&report));
        assert_eq!(assessment.state, ContractCheckState::Failed);
    }

    #[test]
    fn bounded_delay_accepts_a_shift_within_the_budget_but_exact_trace_rejects_it() {
        let report = MacroTransitionReport {
            state: ContextualVerificationState::Failed,
            cases: vec![MacroTransitionCase {
                from: vec![false],
                to: vec![true],
                original_outputs: vec![vec![false], vec![true], vec![true], vec![true]],
                candidate_outputs: vec![vec![false], vec![false], vec![true], vec![true]],
                equivalent: false,
                first_difference_tick: Some(1),
            }],
            differing_cases: 1,
            reason: None,
        };
        let bounded = assess_timing(
            TimingContract {
                mode: TimingContractMode::BoundedDelay,
                maximum_added_redstone_ticks: 1,
                settle_deadline_redstone_ticks: 4,
            },
            Some(&report),
        );
        let exact = assess_timing(
            TimingContract {
                mode: TimingContractMode::ExactTrace,
                maximum_added_redstone_ticks: 0,
                settle_deadline_redstone_ticks: 4,
            },
            Some(&report),
        );
        assert_eq!(bounded.state, ContractCheckState::Passed);
        assert_eq!(exact.state, ContractCheckState::Failed);
    }

    #[test]
    fn mutation_contract_rejects_an_oversized_patch() {
        let structural = MacroStructuralReport {
            candidate_collisions: Vec::new(),
            route_collisions: Vec::new(),
            route_cross_net_contacts: Vec::new(),
            candidate_support_issues: Vec::new(),
            required_route_supports: Vec::new(),
            blocked_route_supports: Vec::new(),
        };
        let assessment = assess_macro_contract(
            OptimizationContract {
                mutation: MutationContract {
                    maximum_changed_blocks: 2,
                    ..OptimizationContract::default().mutation
                },
                ..OptimizationContract::default()
            },
            &structural,
            None,
            None,
            3,
            false,
        );
        assert_eq!(assessment.mutation.state, ContractCheckState::Failed);
    }

    #[test]
    fn pending_transition_measurement_is_unavailable_not_passed() {
        let report = MacroTransitionReport {
            state: ContextualVerificationState::Pending,
            cases: Vec::new(),
            differing_cases: 0,
            reason: Some("too many inputs".to_owned()),
        };
        let assessment = assess_timing(OptimizationContract::default().timing, Some(&report));
        assert_eq!(assessment.state, ContractCheckState::Unavailable);
        assert_eq!(assessment.reasons, ["too many inputs"]);
    }
}
