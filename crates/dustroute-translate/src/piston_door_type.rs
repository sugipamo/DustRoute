//! Completed-operation verification in the same deterministic physical world.
//! Completion is membership in a fully examined held-input recurrent region
//! with permanently correct aperture observations, including internal samples.
//! Every phase of that region permits the next command. No world reset, clock
//! deadline, world-only key, or simulation input filter is introduced.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;

use dustroute_library::blueprint::{TypeContract, TypeRevision};

use crate::behavior_type::{
    BehaviorBudget, BehaviorCounterexample, BehaviorModel, BehaviorTypeReport, WitnessAction,
};
use crate::promotion::CheckStatus;

struct State<S> {
    value: S,
    parent: Option<(usize, WitnessAction)>,
}
struct Edge {
    next: usize,
    outputs: Vec<Vec<bool>>,
}
struct Explorer<S> {
    states: Vec<State<S>>,
    indices: BTreeMap<S, usize>,
    edges: BTreeMap<(usize, bool), Edge>,
    completed: BTreeSet<usize>,
    steps: usize,
    budget: BehaviorBudget,
    start: Instant,
}
impl<S: Clone + Ord> Explorer<S> {
    fn check(&self) -> Result<(), String> {
        if self.start.elapsed() >= self.budget.max_elapsed {
            return Err("completed-operation verification elapsed budget exhausted; no circuit deadline imposed".into());
        }
        Ok(())
    }
    fn intern(
        &mut self,
        value: S,
        parent: Option<(usize, WitnessAction)>,
    ) -> Result<usize, String> {
        self.check()?;
        if let Some(index) = self.indices.get(&value) {
            return Ok(*index);
        }
        if self.states.len() >= self.budget.max_states {
            return Err("completed-operation reachable-state budget exhausted".into());
        }
        let index = self.states.len();
        self.indices.insert(value.clone(), index);
        self.states.push(State { value, parent });
        Ok(index)
    }
    fn prefix(&self, mut index: usize) -> Vec<WitnessAction> {
        let mut result = Vec::new();
        while let Some((parent, action)) = &self.states[index].parent {
            result.push(action.clone());
            index = *parent;
        }
        result.reverse();
        result
    }
    fn edge<M: BehaviorModel<State = S>>(
        &mut self,
        model: &M,
        index: usize,
        closed: bool,
    ) -> Result<usize, String> {
        self.check()?;
        if let Some(edge) = self.edges.get(&(index, closed)) {
            return Ok(edge.next);
        }
        if self.steps >= self.budget.max_steps {
            return Err("completed-operation model-step budget exhausted".into());
        }
        self.steps += 1;
        let state = &self.states[index].value;
        if model.with_inputs(state, &[closed])? != *state {
            return Err("held operation input is not idempotent".into());
        }
        let mut outputs = vec![model.outputs(state)?];
        let (next, inside) = model.step_with_observations(state)?;
        outputs.extend(inside);
        if outputs.iter().any(|o| o.len() != 18) {
            return Err("door observation width mismatch".into());
        }
        let next = self.intern(next, Some((index, WitnessAction::Advance)))?;
        self.edges.insert((index, closed), Edge { next, outputs });
        Ok(next)
    }
    fn trajectory<M: BehaviorModel<State = S>>(
        &mut self,
        model: &M,
        mut index: usize,
        closed: bool,
    ) -> Result<(Vec<usize>, usize), String> {
        let mut path = Vec::new();
        let mut seen = BTreeMap::new();
        loop {
            self.check()?;
            if let Some(begin) = seen.get(&index) {
                return Ok((path, *begin));
            }
            seen.insert(index, path.len());
            path.push(index);
            index = self.edge(model, index, closed)?;
        }
    }
    fn wrong(&self, path: &[usize], closed: bool) -> bool {
        let expected: Vec<_> = (0..9).flat_map(|_| [!closed, closed]).collect();
        path.iter().any(|index| {
            self.edges[&(*index, closed)]
                .outputs
                .iter()
                .any(|o| o != &expected)
        })
    }
    fn witness(&self, cycle: &[usize], closed: bool) -> BehaviorCounterexample {
        BehaviorCounterexample {
            prefix: self.prefix(cycle[0]),
            held_inputs: vec![closed],
            expected_outputs: (0..9).flat_map(|_| [!closed, closed]).collect(),
            cycle_outputs: cycle
                .iter()
                .flat_map(|i| self.edges[&(*i, closed)].outputs.clone())
                .collect(),
        }
    }
}

/// Every permitted operation trajectory and every completed cycle phase is
/// explored to closure. Activity in unrelated devices need not stop: a cycle
/// is allowed when the aperture stays correct. Unknown/budget-limited futures
/// never become completion certificates or passes.
pub fn verify_piston_door<M: BehaviorModel>(
    definition: &TypeRevision,
    model: &M,
    initial_closed: bool,
    budget: BehaviorBudget,
) -> BehaviorTypeReport {
    let mut e = Explorer {
        states: Vec::new(),
        indices: BTreeMap::new(),
        edges: BTreeMap::new(),
        completed: BTreeSet::new(),
        steps: 0,
        budget,
        start: Instant::now(),
    };
    let result = (|| -> Result<(CheckStatus, Option<BehaviorCounterexample>, String), String> {
        let TypeContract::PistonDoor { requirement } = &definition.contract else {
            return Err("expected completed-operation piston-door contract".into());
        };
        requirement.validate().map_err(str::to_owned)?;
        let first = e.intern(model.initial_state()?, None)?;
        let (initial, begin) = e.trajectory(model, first, initial_closed)?;
        if e.wrong(&initial, initial_closed) {
            let witness = e
                .wrong(&initial[begin..], initial_closed)
                .then(|| e.witness(&initial[begin..], initial_closed));
            return Ok((
                CheckStatus::Failed,
                witness,
                "declared initial aperture is incorrect or changes during initial completion"
                    .into(),
            ));
        }
        let mut ready = VecDeque::new();
        for &index in &initial[begin..] {
            if e.completed.insert(index) {
                ready.push_back(index);
            }
        }
        while let Some(index) = ready.pop_front() {
            for closed in [false, true] {
                e.check()?;
                let driven = model.with_inputs(&e.states[index].value, &[closed])?;
                let driven = e.intern(
                    driven,
                    Some((
                        index,
                        WitnessAction::SetInputs {
                            inputs: vec![closed],
                        },
                    )),
                )?;
                let (path, begin) = e.trajectory(model, driven, closed)?;
                let cycle = &path[begin..];
                if e.wrong(cycle, closed) {
                    return Ok((CheckStatus::Failed,Some(e.witness(cycle,closed)),"an allowed operation reaches a recurrent region with incorrect aperture observations".into()));
                }
                for &index in cycle {
                    if e.completed.insert(index) {
                        ready.push_back(index);
                    }
                }
            }
        }
        Ok((
            CheckStatus::Passed,
            None,
            format!(
                "closed completed-operation graph; {} complete physical cycle phases permit new commands; held-input intermediate aperture observations remain correct; interruption tolerance is not required",
                e.completed.len()
            ),
        ))
    })();
    let (status, counterexample, detail) = match result {
        Ok(r) => r,
        Err(reason) => (CheckStatus::Undetermined, None, reason),
    };
    BehaviorTypeReport {
        type_revision: definition.id.clone(),
        status,
        reachable_states: e.states.len(),
        evaluated_steps: e.steps,
        graph_closed: status == CheckStatus::Passed,
        detail,
        counterexample,
    }
}
