//! Sound history overapproximation for verification, never runtime state.
//!
//! alpha(H) = (length(H), min(H)). Registers, inputs and callbacks stay exact.
//! Instructions can observe history counts/capacity but not individual ages.
//! Expiration covers every possible retained count while the youngest survives.
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{ExecutableLaw, Expr, Instruction, LawProgram, LawState};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct HistorySummary {
    pub count: usize,
    pub youngest_age: Option<u16>,
}

/// Distinct from LawState: summaries cannot be installed as concrete execution.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AbstractLawState {
    inputs: BTreeMap<String, u16>,
    registers: BTreeMap<String, u16>,
    histories: BTreeMap<String, HistorySummary>,
    pending: BTreeMap<String, u16>,
}

impl AbstractLawState {
    pub fn register(&self, name: &str) -> Option<u16> {
        self.registers.get(name).copied()
    }

    pub fn pending(&self, event: &str) -> Option<u16> {
        self.pending.get(event).copied()
    }

    pub fn history(&self, name: &str) -> Option<&HistorySummary> {
        self.histories.get(name)
    }

    fn project(state: &LawState) -> Self {
        Self {
            inputs: state.inputs.clone(),
            registers: state.registers.clone(),
            pending: state.pending.clone(),
            histories: state
                .histories
                .iter()
                .map(|(name, ages)| {
                    (
                        name.clone(),
                        HistorySummary {
                            count: ages.len(),
                            youngest_age: ages.iter().min().copied(),
                        },
                    )
                })
                .collect(),
        }
    }

    /// An instruction-only encoding, not a chosen time evolution. The shared
    /// instruction interpreter cannot distinguish ages with the same count.
    /// Never pass this to the concrete history-expiration operation.
    fn instruction_state(&self) -> LawState {
        LawState {
            inputs: self.inputs.clone(),
            registers: self.registers.clone(),
            pending: self.pending.clone(),
            histories: self
                .histories
                .iter()
                .map(|(name, summary)| {
                    (
                        name.clone(),
                        vec![summary.youngest_age.unwrap_or(0); summary.count],
                    )
                })
                .collect(),
        }
    }
}

/// Uses the selected program's shared instruction executor; no torch/gate names
/// or hard-coded threshold, window or recovery constants establish a proof.
pub struct HistoryAbstractLaw<'a> {
    law: &'a ExecutableLaw,
}

impl<'a> HistoryAbstractLaw<'a> {
    pub fn new(law: &'a ExecutableLaw) -> Self {
        // Exhaustive matches make extensions to the instruction language revisit
        // the abstraction boundary instead of silently observing encoded ages.
        assert_age_blind(&law.program);
        Self { law }
    }

    pub fn project(&self, concrete: &LawState) -> Result<AbstractLawState, String> {
        self.law.check_state(concrete)?;
        Ok(AbstractLawState::project(concrete))
    }

    pub fn event(
        &self,
        state: &AbstractLawState,
        event: &str,
        inputs: &BTreeMap<String, u16>,
    ) -> Result<AbstractLawState, String> {
        self.project(&self.law.event(&state.instruction_state(), event, inputs)?)
    }

    /// Callbacks see summaries, never fictitious concrete histories. A physical
    /// adapter must preserve the same register-effect ordering as exact execution.
    /// Any possible execution error aborts the whole abstract transition.
    pub fn successors<F>(
        &self,
        state: &AbstractLawState,
        max_successors: usize,
        on_change: &mut F,
    ) -> Result<Vec<AbstractLawState>, String>
    where
        F: FnMut(&Self, &mut AbstractLawState, &str) -> Result<(), String>,
    {
        self.law.check_state(&state.instruction_state())?;
        if max_successors == 0 {
            return Err("abstract successor budget exhausted".into());
        }
        let mut expired = vec![state.clone()];
        for (name, summary) in &state.histories {
            let window = self.law.program.histories[name].window;
            let youngest = summary
                .youngest_age
                .filter(|age| *age < window)
                .map(|age| age + 1);
            let counts = if youngest.is_some() {
                1..=summary.count
            } else {
                0..=0
            };
            let mut next = Vec::new();
            for previous in expired {
                for count in counts.clone() {
                    if next.len() >= max_successors {
                        return Err(
                            "abstract successor budget exhausted during history expiration".into(),
                        );
                    }
                    let mut candidate = previous.clone();
                    candidate.histories.insert(
                        name.clone(),
                        HistorySummary {
                            count,
                            youngest_age: youngest,
                        },
                    );
                    next.push(candidate);
                }
            }
            expired = next;
        }
        let mut results = BTreeSet::new();
        for candidate in expired {
            let next = self.law.advance_after_history_expiration(
                candidate.instruction_state(),
                &mut |_, local, register| {
                    let mut abstract_local = AbstractLawState::project(local);
                    on_change(self, &mut abstract_local, register)?;
                    *local = abstract_local.instruction_state();
                    Ok(())
                },
            )?;
            results.insert(self.project(&next)?);
        }
        Ok(results.into_iter().collect())
    }
}

fn assert_age_blind(program: &LawProgram) {
    fn expression(expr: &Expr) {
        match expr {
            Expr::Constant { .. }
            | Expr::Input { .. }
            | Expr::Register { .. }
            | Expr::Count { .. } => {}
            Expr::Not { value } => expression(value),
            Expr::Equal { left, right }
            | Expr::AtLeast { left, right }
            | Expr::And { left, right }
            | Expr::Maximum { left, right }
            | Expr::SaturatingSubtract { left, right } => {
                expression(left);
                expression(right);
            }
        }
    }
    fn instructions(body: &[Instruction]) {
        for instruction in body {
            match instruction {
                Instruction::Set { value, .. } => expression(value),
                Instruction::Remember { .. } | Instruction::ScheduleIfAbsent { .. } => {}
                Instruction::If {
                    condition,
                    then,
                    otherwise,
                } => {
                    expression(condition);
                    instructions(then);
                    instructions(otherwise);
                }
            }
        }
    }
    for body in program.handlers.values() {
        instructions(body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::law::{History, Register};

    fn program() -> LawProgram {
        LawProgram {
            inputs: BTreeMap::from([("input".into(), 1)]),
            registers: BTreeMap::from([
                (
                    "output".into(),
                    Register {
                        initial: 1,
                        maximum: 3,
                    },
                ),
                (
                    "diagnostic".into(),
                    Register {
                        initial: 0,
                        maximum: 1,
                    },
                ),
            ]),
            histories: BTreeMap::from([(
                "events".into(),
                History {
                    window: 3,
                    capacity: 3,
                },
            )]),
            handlers: BTreeMap::from([
                (
                    "remember".into(),
                    vec![Instruction::Remember {
                        history: "events".into(),
                    }],
                ),
                (
                    "tick".into(),
                    vec![
                        Instruction::Set {
                            register: "output".into(),
                            value: Expr::Count {
                                history: "events".into(),
                            },
                        },
                        Instruction::If {
                            condition: Expr::Register {
                                name: "diagnostic".into(),
                            },
                            then: vec![Instruction::Remember {
                                history: "events".into(),
                            }],
                            otherwise: vec![],
                        },
                        Instruction::ScheduleIfAbsent {
                            event: "tick".into(),
                            after: 6,
                        },
                    ],
                ),
                (
                    "feedback".into(),
                    vec![Instruction::ScheduleIfAbsent {
                        event: "tick".into(),
                        after: 2,
                    }],
                ),
            ]),
        }
    }

    fn histories(max: usize) -> Vec<Vec<u16>> {
        let mut result = vec![vec![]];
        for length in 1..=max {
            for code in 0..4_usize.pow(length as u32) {
                result.push(
                    (0..length)
                        .map(|shift| ((code >> (shift * 2)) & 3) as u16)
                        .collect(),
                );
            }
        }
        result
    }

    #[test]
    fn every_small_concrete_successor_is_covered_including_duplicates_errors_and_effects() {
        let law = program().compile().unwrap();
        let abstraction = HistoryAbstractLaw::new(&law);
        let mut successes = 0;
        let mut errors = 0;
        for history in histories(3) {
            for diagnostic in 0..=1 {
                for pending in [None, Some(1), Some(2), Some(6)] {
                    let mut state = law.initial_state();
                    state.histories.insert("events".into(), history.clone());
                    state.registers.insert("diagnostic".into(), diagnostic);
                    if let Some(delay) = pending {
                        state.pending.insert("tick".into(), delay);
                    }
                    let projected = abstraction.project(&state).unwrap();
                    let original = state.clone();
                    let exact =
                        law.advance_with_register_effects(&state, &mut |law, state, name| {
                            if name == "output" {
                                *state = law.event(
                                    state,
                                    "feedback",
                                    &BTreeMap::from([("input".into(), 1)]),
                                )?;
                            }
                            Ok(())
                        });
                    let approximate =
                        abstraction.successors(&projected, 64, &mut |law, state, name| {
                            if name == "output" {
                                *state = law.event(
                                    state,
                                    "feedback",
                                    &BTreeMap::from([("input".into(), 1)]),
                                )?;
                            }
                            Ok(())
                        });
                    match (exact, approximate) {
                        (Ok(next), Ok(next_set)) => {
                            assert!(next_set.contains(&abstraction.project(&next).unwrap()));
                            successes += 1;
                        }
                        (Err(_), Ok(_)) => panic!("a concrete error was erased"),
                        (_, Err(_)) => errors += 1, // A possible error cannot be ignored.
                    }
                    for input in 0..=1 {
                        let inputs = BTreeMap::from([("input".into(), input)]);
                        let exact = law.event(&state, "remember", &inputs);
                        let approximate = abstraction.event(&projected, "remember", &inputs);
                        assert_eq!(exact.is_err(), approximate.is_err());
                        if let Ok(next) = exact {
                            assert_eq!(abstraction.project(&next).unwrap(), approximate.unwrap());
                        }
                    }
                    assert_eq!(state, original);
                }
            }
        }
        assert!(successes > 500 && errors > 0);
    }

    #[test]
    fn a_summary_has_all_expiration_branches_not_a_representative_trajectory() {
        let law = program().compile().unwrap();
        let abstraction = HistoryAbstractLaw::new(&law);
        let mut first = law.initial_state();
        first.histories.insert("events".into(), vec![3, 0]);
        let mut second = first.clone();
        second.histories.insert("events".into(), vec![2, 0]);
        assert_eq!(
            abstraction.project(&first).unwrap(),
            abstraction.project(&second).unwrap()
        );
        let successors = abstraction
            .successors(&abstraction.project(&first).unwrap(), 2, &mut |_, _, _| {
                Ok(())
            })
            .unwrap();
        assert_eq!(successors.len(), 2);
        for concrete in [first.clone(), second] {
            assert!(
                successors.contains(
                    &abstraction
                        .project(&law.advance(&concrete).unwrap())
                        .unwrap()
                )
            );
        }
        assert!(
            abstraction
                .successors(&abstraction.project(&first).unwrap(), 1, &mut |_, _, _| Ok(
                    ()
                ))
                .is_err()
        );
        let mut changed_register = first.clone();
        changed_register.registers.insert("diagnostic".into(), 1);
        assert_ne!(
            abstraction.project(&first).unwrap(),
            abstraction.project(&changed_register).unwrap()
        );
    }

    #[test]
    fn all_histories_branch_and_expiration_handles_zero_and_maximum_windows() {
        let mut definition = program();
        definition
            .histories
            .insert("second".into(), definition.histories["events"].clone());
        let law = definition.compile().unwrap();
        let abstraction = HistoryAbstractLaw::new(&law);
        let mut state = law.initial_state();
        for ages in state.histories.values_mut() {
            *ages = vec![3, 0];
        }
        let projected = abstraction.project(&state).unwrap();
        assert_eq!(
            abstraction
                .successors(&projected, 4, &mut |_, _, _| Ok(()))
                .unwrap()
                .len(),
            4
        );
        assert!(
            abstraction
                .successors(&projected, 3, &mut |_, _, _| Ok(()))
                .is_err()
        );
        for window in [0, u16::MAX] {
            let mut definition = program();
            definition.histories.get_mut("events").unwrap().window = window;
            let law = definition.compile().unwrap();
            let abstraction = HistoryAbstractLaw::new(&law);
            let mut state = law.initial_state();
            state
                .histories
                .insert("events".into(), vec![window, window]);
            let next = abstraction
                .successors(&abstraction.project(&state).unwrap(), 1, &mut |_, _, _| {
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                next[0].history("events").unwrap(),
                &HistorySummary {
                    count: 0,
                    youngest_age: None
                }
            );
        }
    }
}
