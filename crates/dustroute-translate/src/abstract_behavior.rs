//! Universal repeated-settling checks over an overapproximate transition graph.
//! An abstract obstruction is not a concrete counterexample. It stays unknown.
use std::collections::BTreeMap;
use std::time::Instant;

use dustroute_library::blueprint::{TypeContract, TypeRevision, TypeRevisionId};
use serde::Serialize;

use crate::behavior_type::BehaviorBudget;
use crate::promotion::CheckStatus;

pub const ABSTRACT_REPEATED_SETTLING_METHOD: &str =
    "universal-overapproximate-repeated-settling.v1";

pub const HISTORY_ABSTRACTION_METHOD: &str = "history-count-youngest-age-overapproximation.v1";

/// Implementations must cover the projection of EVERY concrete successor,
/// output and error from each represented state. There must be no dead ends:
/// quiescence has an identity successor. Inputs do not advance model time.
pub trait AbstractBehaviorModel {
    type State: Clone + Ord;
    fn initial_state(&self) -> Result<Self::State, String>;
    fn with_inputs(&self, state: &Self::State, inputs: &[bool]) -> Result<Self::State, String>;
    fn successors(&self, state: &Self::State, limit: usize) -> Result<Vec<Self::State>, String>;
    /// Outputs must be identical across every represented concrete state.
    fn outputs(&self, state: &Self::State) -> Result<Vec<bool>, String>;
}

/// Diagnostics only; cannot be loaded as validation/adoption authority.
#[derive(Clone, Debug, Serialize)]
pub struct AbstractBehaviorReport {
    pub type_revision: TypeRevisionId,
    pub proof_method: &'static str,
    pub status: CheckStatus,
    pub abstract_states: usize,
    pub evaluated_transitions: usize,
    pub detail: String,
}

struct Edge {
    outputs: Vec<bool>,
    next: Vec<usize>,
}

struct Graph<S> {
    states: Vec<S>,
    indices: BTreeMap<S, usize>,
    edges: Vec<Vec<Edge>>,
    transitions: usize,
    started: Instant,
    budget: BehaviorBudget,
}

impl<S: Clone + Ord> Graph<S> {
    fn check_time(&self) -> Result<(), String> {
        if self.started.elapsed() >= self.budget.max_elapsed {
            Err("abstract verification time budget exhausted; no settling deadline imposed".into())
        } else {
            Ok(())
        }
    }

    fn intern(&mut self, state: S) -> Result<usize, String> {
        if let Some(&index) = self.indices.get(&state) {
            return Ok(index);
        }
        if self.states.len() >= self.budget.max_states {
            return Err("abstract state budget exhausted; graph is not closed".into());
        }
        let index = self.states.len();
        self.indices.insert(state.clone(), index);
        self.states.push(state);
        Ok(index)
    }

    /// Iterative Kosaraju traversal avoids recursion on long timer paths.
    /// Every node in a cyclic SCC can recur forever; each must have the required
    /// outputs. Checking terminal SCCs alone would miss optional infinite loops.
    fn check_cycles(&self, input: usize, expected: &[bool]) -> Result<(), String> {
        let size = self.states.len();
        let mut reverse = vec![vec![]; size];
        for (source, rows) in self.edges.iter().enumerate() {
            self.check_time()?;
            for &next in &rows[input].next {
                reverse[next].push(source);
            }
        }
        let mut visited = vec![false; size];
        let mut order = vec![];
        for start in 0..size {
            if visited[start] {
                continue;
            }
            visited[start] = true;
            let mut stack = vec![(start, 0)];
            while let Some((node, cursor)) = stack.last_mut() {
                self.check_time()?;
                let successors = &self.edges[*node][input].next;
                if let Some(&next) = successors.get(*cursor) {
                    *cursor += 1;
                    if !visited[next] {
                        visited[next] = true;
                        stack.push((next, 0));
                    }
                } else {
                    order.push(*node);
                    stack.pop();
                }
            }
        }
        let mut assigned = vec![false; size];
        for start in order.into_iter().rev() {
            if assigned[start] {
                continue;
            }
            assigned[start] = true;
            let mut stack = vec![start];
            let mut count = 0;
            let mut wrong = false;
            let self_loop = self.edges[start][input].next.contains(&start);
            while let Some(node) = stack.pop() {
                self.check_time()?;
                count += 1;
                wrong |= self.edges[node][input].outputs != expected;
                for &previous in &reverse[node] {
                    if !assigned[previous] {
                        assigned[previous] = true;
                        stack.push(previous);
                    }
                }
            }
            if wrong && (count > 1 || self_loop) {
                return Err(format!(
                    "possible nonsettling abstract cycle for input row {input}; no concrete counterexample established"
                ));
            }
        }
        Ok(())
    }
}

pub fn verify_abstract_repeated_settling<M: AbstractBehaviorModel>(
    definition: &TypeRevision,
    model: &M,
    budget: BehaviorBudget,
) -> AbstractBehaviorReport {
    let mut graph = Graph {
        states: vec![],
        indices: BTreeMap::new(),
        edges: vec![],
        transitions: 0,
        started: Instant::now(),
        budget,
    };
    let result = (|| -> Result<(), String> {
        let TypeContract::RepeatedSettling { relation } = &definition.contract else {
            return Err("selected type is not a repeated-settling contract".into());
        };
        relation.validate().map_err(str::to_owned)?;
        graph.check_time()?;
        graph.intern(model.initial_state()?)?;
        let mut cursor = 0;
        while cursor < graph.states.len() {
            let original = graph.states[cursor].clone();
            let mut edges = vec![];
            for row in &relation.rows {
                graph.check_time()?;
                if graph.transitions >= budget.max_steps {
                    return Err("abstract transition budget exhausted; graph is not closed".into());
                }
                let driven = model.with_inputs(&original, &row.inputs)?;
                if model.with_inputs(&driven, &row.inputs)? != driven {
                    return Err("abstract input assignment is not idempotent".into());
                }
                // Also explores sequences of input assignments without a tick.
                graph.intern(driven.clone())?;
                let outputs = model.outputs(&driven)?;
                if outputs.len() != relation.outputs.len() {
                    return Err("abstract output width does not match the type".into());
                }
                let limit = budget.max_states.min(budget.max_steps - graph.transitions);
                let successors = model.successors(&driven, limit)?;
                if successors.is_empty() {
                    return Err("abstract transition has an uncovered dead end".into());
                }
                let mut next = vec![];
                for state in successors {
                    graph.check_time()?;
                    if graph.transitions >= budget.max_steps {
                        return Err(
                            "abstract transition budget exhausted; graph is not closed".into()
                        );
                    }
                    graph.transitions += 1;
                    next.push(graph.intern(state)?);
                }
                next.sort_unstable();
                next.dedup();
                edges.push(Edge { outputs, next });
            }
            graph.edges.push(edges);
            cursor += 1;
        }
        for (input, row) in relation.rows.iter().enumerate() {
            graph.check_cycles(input, &row.outputs)?;
        }
        graph.check_time()
    })();
    AbstractBehaviorReport {
        type_revision: definition.id.clone(), proof_method: ABSTRACT_REPEATED_SETTLING_METHOD,
        status: if result.is_ok() { CheckStatus::Passed } else { CheckStatus::Undetermined },
        abstract_states: graph.states.len(), evaluated_transitions: graph.transitions,
        detail: result.err().unwrap_or_else(|| "closed overapproximate graph; every held-input cycle satisfies the original output relation".into()),
    }
}
