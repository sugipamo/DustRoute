//! Repeated-use type checking by closing the reachable execution-state graph.
//!
//! This proves a property of the supplied deterministic model, not of Vanilla.
//! Physical adapters must include all future-relevant state in their keys.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use dustroute_library::behavior_type::RepeatedSettling;
use dustroute_library::blueprint::{TypeContract, TypeRevision, TypeRevisionId};
use serde::{Deserialize, Serialize};

use crate::promotion::CheckStatus;

/// Pure, deterministic execution under one fixed law revision and scheduler.
/// The input and output vector orders are those of the selected type.
pub trait BehaviorModel {
    /// Equality must imply identical future behavior, including pending work,
    /// relative event order/delays, external inputs and device-local memory.
    /// A World-only hash or a sampled output vector is not sufficient.
    type State: Clone + Ord;

    fn initial_state(&self) -> Result<Self::State, String>;
    /// Changes external input levels without resetting internal state. Repeating
    /// the same assignment must be idempotent. Coordinates belong to the adapter.
    fn with_inputs(&self, state: &Self::State, inputs: &[bool]) -> Result<Self::State, String>;
    /// One atomic model step. Quiescence is an identity transition; finite
    /// internal cycles are allowed and need not be mistaken for quiescence.
    fn step(&self, state: &Self::State) -> Result<Self::State, String>;
    fn outputs(&self, state: &Self::State) -> Result<Vec<bool>, String>;
    /// Additional observations made inside one atomic input boundary. A
    /// synchronous world can change output more than once before accepting
    /// another input. Such recurring pulses must not disappear from a settling
    /// proof. Atomic models retain their original boundary-only behavior.
    fn step_with_observations(
        &self,
        state: &Self::State,
    ) -> Result<(Self::State, Vec<Vec<bool>>), String> {
        Ok((self.step(state)?, vec![]))
    }
}

/// Computation limits only; these never become circuit timing requirements.
#[derive(Clone, Copy, Debug)]
pub struct BehaviorBudget {
    pub max_states: usize,
    pub max_steps: usize,
    pub max_elapsed: Duration,
}

impl Default for BehaviorBudget {
    fn default() -> Self {
        Self {
            max_states: 65_536,
            max_steps: 1_000_000,
            max_elapsed: Duration::from_secs(30),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum WitnessAction {
    SetInputs { inputs: Vec<bool> },
    Advance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BehaviorCounterexample {
    /// Replay from initial_state, then hold held_inputs. No reset is involved.
    pub prefix: Vec<WitnessAction>,
    pub held_inputs: Vec<bool>,
    pub expected_outputs: Vec<bool>,
    /// The repeating held-input cycle contains at least one incorrect output.
    pub cycle_outputs: Vec<Vec<bool>>,
}

/// Diagnostic data, never deserializable adoption or world-write authority.
#[derive(Clone, Debug, Serialize)]
pub struct BehaviorTypeReport {
    pub type_revision: TypeRevisionId,
    pub status: CheckStatus,
    pub reachable_states: usize,
    pub evaluated_steps: usize,
    /// Required for a pass. A concrete reachable counterexample can disprove
    /// the type before unrelated portions of the graph have been closed.
    pub graph_closed: bool,
    pub detail: String,
    pub counterexample: Option<BehaviorCounterexample>,
}

struct Reachable<S> {
    state: S,
    parent: Option<(usize, WitnessAction)>,
}

struct Edge {
    next: usize,
    outputs: Vec<Vec<bool>>,
}

struct Exploration<S> {
    states: Vec<Reachable<S>>,
    indices: BTreeMap<S, usize>,
    edges: Vec<Vec<Edge>>,
    steps: usize,
    start: Instant,
    budget: BehaviorBudget,
}

impl<S: Clone + Ord> Exploration<S> {
    fn check_budget(&self) -> Result<(), String> {
        if self.start.elapsed() >= self.budget.max_elapsed {
            return Err(
                "elapsed verification budget exhausted; no timing deadline was imposed".into(),
            );
        }
        Ok(())
    }

    fn intern(
        &mut self,
        state: S,
        parent: Option<(usize, WitnessAction)>,
    ) -> Result<usize, String> {
        if let Some(index) = self.indices.get(&state) {
            return Ok(*index);
        }
        if self.states.len() >= self.budget.max_states {
            return Err("reachable-state budget exhausted; reachable graph is not closed".into());
        }
        let index = self.states.len();
        self.indices.insert(state.clone(), index);
        self.states.push(Reachable { state, parent });
        Ok(index)
    }

    fn prefix(&self, mut index: usize) -> Vec<WitnessAction> {
        let mut result = vec![];
        while let Some((parent, action)) = &self.states[index].parent {
            result.push(action.clone());
            index = *parent;
        }
        result.reverse();
        result
    }

    fn explore<M: BehaviorModel<State = S>>(
        &mut self,
        model: &M,
        relation: &RepeatedSettling,
    ) -> Result<Option<BehaviorCounterexample>, String> {
        self.intern(model.initial_state()?, None)?;
        let mut cursor = 0;
        while cursor < self.states.len() {
            let original = self.states[cursor].state.clone();
            let mut edges = Vec::new();
            for row in &relation.rows {
                self.check_budget()?;
                if self.steps >= self.budget.max_steps {
                    return Err("model-step budget exhausted; reachable graph is not closed".into());
                }
                self.steps += 1;
                let driven = model.with_inputs(&original, &row.inputs)?;
                if model.with_inputs(&driven, &row.inputs)? != driven {
                    return Err("model input assignment is not idempotent".into());
                }
                let driven_index = self.intern(
                    driven.clone(),
                    Some((
                        cursor,
                        WitnessAction::SetInputs {
                            inputs: row.inputs.clone(),
                        },
                    )),
                )?;
                let mut outputs = vec![model.outputs(&driven)?];
                let (next, internal_outputs) = model.step_with_observations(&driven)?;
                outputs.extend(internal_outputs);
                if outputs
                    .iter()
                    .any(|sample| sample.len() != relation.outputs.len())
                {
                    return Err("model output width does not match the type".into());
                }
                let next = self.intern(next, Some((driven_index, WitnessAction::Advance)))?;
                edges.push(Edge { next, outputs });
            }
            self.edges.push(edges);
            cursor += 1;
            // A counterexample needs a complete reachable held-input cycle,
            // not closure of every unrelated branch. Check periodically so a
            // growing queue cannot hide an already established bad idle cycle.
            if cursor.is_power_of_two() || cursor % 256 == 0 {
                if let Some(witness) = self.check_cycles(relation)? {
                    return Ok(Some(witness));
                }
            }
        }
        self.check_cycles(relation)
    }

    /// A deterministic held-input trajectory eventually enters a cycle. Output
    /// settling holds exactly when every output on that cycle satisfies the row.
    /// Transient outputs and the time taken to reach the cycle are unrestricted.
    fn check_cycles(
        &self,
        relation: &RepeatedSettling,
    ) -> Result<Option<BehaviorCounterexample>, String> {
        for (input, row) in relation.rows.iter().enumerate() {
            let mut checked = vec![false; self.states.len()];
            let mut position = vec![None; self.states.len()];
            for start in 0..self.edges.len() {
                if checked[start] {
                    continue;
                }
                let mut path = vec![];
                let mut cursor = start;
                while cursor < self.edges.len() && !checked[cursor] && position[cursor].is_none() {
                    self.check_budget()?;
                    position[cursor] = Some(path.len());
                    path.push(cursor);
                    cursor = self.edges[cursor][input].next;
                }
                if let Some(begin) = position[cursor] {
                    let cycle: Vec<usize> = path[begin..].to_vec();
                    if cycle.iter().any(|index| {
                        self.edges[*index][input]
                            .outputs
                            .iter()
                            .any(|sample| sample != &row.outputs)
                    }) {
                        return Ok(Some(BehaviorCounterexample {
                            prefix: self.prefix(cursor),
                            held_inputs: row.inputs.clone(),
                            expected_outputs: row.outputs.clone(),
                            cycle_outputs: cycle
                                .iter()
                                .flat_map(|index| self.edges[*index][input].outputs.clone())
                                .collect(),
                        }));
                    }
                }
                for index in path {
                    checked[index] = true;
                    position[index] = None;
                }
            }
        }
        Ok(None)
    }
}

/// No finite sample count can stand in for closure: an exhausted exploration is
/// undetermined even when every sampled output happened to be correct.
pub fn verify_repeated_settling<M: BehaviorModel>(
    definition: &TypeRevision,
    model: &M,
    budget: BehaviorBudget,
) -> BehaviorTypeReport {
    let mut exploration = Exploration {
        states: vec![],
        indices: BTreeMap::new(),
        edges: vec![],
        steps: 0,
        start: Instant::now(),
        budget,
    };
    let result = (|| {
        let TypeContract::RepeatedSettling { relation } = &definition.contract else {
            return Err("the selected type is not a repeated-settling behavior contract".into());
        };
        relation.validate().map_err(str::to_owned)?;
        exploration.explore(model, relation)
    })();
    let (status, detail, counterexample) = match result {
        Ok(None) => (
            CheckStatus::Passed,
            "closed reachable model graph; every held-input cycle has the required outputs".into(),
            None,
        ),
        Ok(Some(witness)) => (
            CheckStatus::Failed,
            "a reachable held-input cycle prevents the required outputs from remaining stable"
                .into(),
            Some(witness),
        ),
        Err(reason) => (CheckStatus::Undetermined, reason, None),
    };
    BehaviorTypeReport {
        type_revision: definition.id.clone(),
        status,
        reachable_states: exploration.states.len(),
        evaluated_steps: exploration.steps,
        graph_closed: !exploration.states.is_empty()
            && exploration.edges.len() == exploration.states.len(),
        detail,
        counterexample,
    }
}
