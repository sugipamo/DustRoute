//! Verification-only abstraction of the fixed single-torch effects profile.
//! Exact histories remain in PhysicalBehaviorState for simulation and replay.
use std::cell::RefCell;

use dustroute_minecraft::law::abstract_history::{AbstractLawState, HistoryAbstractLaw};

use super::*;
use crate::abstract_behavior::{
    ABSTRACT_REPEATED_SETTLING_METHOD, AbstractBehaviorModel, AbstractBehaviorReport,
    HISTORY_ABSTRACTION_METHOD, verify_abstract_repeated_settling,
};
use crate::promotion::CheckStatus;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PhysicalAbstractState {
    model: u64,
    inputs: Vec<bool>,
    torch: Option<AbstractLawState>,
}

#[derive(Clone)]
struct ElectricalObservation {
    support: bool,
    outputs: Vec<bool>,
}

type ElectricalCache = BTreeMap<(Vec<bool>, Option<bool>), ElectricalObservation>;

pub struct PhysicalHistoryAbstraction<'a> {
    execution: &'a PhysicalExecutionModel,
    law: HistoryAbstractLaw<'a>,
    torch_position: Option<Pos>,
    // Optional contextual obligations: actual terminal levels must equal the
    // external controls in every explored state, including synchronous effects.
    input_ports: Vec<PhysicalOutput>,
    // Same immutable world, topology, laws and bindings for this whole lifetime.
    // All electrical dependencies (external inputs and lit) are in this key.
    electrical: RefCell<ElectricalCache>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PhysicalAbstractBehaviorReport {
    pub abstraction: &'static str,
    pub profile: &'static str,
    pub initial_condition: &'static str,
    pub selection: PhysicalBehaviorSelection,
    pub assembly: AssemblyRevision,
    pub definition: TypeRevision,
    pub dust_revision: BlueprintRevision,
    pub torch_revision: BlueprintRevision,
    pub behavior: AbstractBehaviorReport,
}

impl PhysicalBehaviorModel {
    pub fn history_abstraction(&self) -> Result<PhysicalHistoryAbstraction<'_>, String> {
        PhysicalHistoryAbstraction::new(&self.execution)
    }

    /// Explicit alternative proof route. Runtime execution, exact verification,
    /// periodic/finite-burst checks and saved revisions keep their semantics.
    pub fn verify_history_abstraction(
        &self,
        budget: BehaviorBudget,
    ) -> PhysicalAbstractBehaviorReport {
        let behavior = match self.history_abstraction() {
            Ok(model) => verify_abstract_repeated_settling(&self.definition, &model, budget),
            Err(detail) => AbstractBehaviorReport {
                type_revision: self.definition.id.clone(),
                proof_method: ABSTRACT_REPEATED_SETTLING_METHOD,
                status: CheckStatus::Undetermined,
                abstract_states: 0,
                evaluated_transitions: 0,
                detail,
            },
        };
        PhysicalAbstractBehaviorReport {
            abstraction: HISTORY_ABSTRACTION_METHOD,
            profile: self.execution.profile.as_str(),
            initial_condition: FRESH_INITIAL_CONDITION,
            selection: self.selection.clone(),
            assembly: self.assembly.clone(),
            definition: self.definition.clone(),
            dust_revision: self.execution.dust.revision().clone(),
            torch_revision: self.execution.torch_revision.clone(),
            behavior,
        }
    }
}

impl<'a> PhysicalHistoryAbstraction<'a> {
    pub(crate) fn new(execution: &'a PhysicalExecutionModel) -> Result<Self, String> {
        if execution.profile != PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1 {
            return Err("history abstraction currently supports the explicit single-torch block-effects profile only".into());
        }
        let torches: Vec<_> = execution
            .world
            .iter()
            .filter(|(_, b)| b.kind == BlockKind::RedstoneTorch)
            .map(|(p, _)| *p)
            .collect();
        if torches.len() > 1 {
            return Err("history abstraction requires at most one torch".into());
        }
        Ok(Self {
            execution,
            law: HistoryAbstractLaw::new(&execution.torch),
            torch_position: torches.first().copied(),
            input_ports: vec![],
            electrical: RefCell::new(BTreeMap::new()),
        })
    }

    pub(crate) fn with_input_ports(
        execution: &'a PhysicalExecutionModel,
        input_ports: Vec<PhysicalOutput>,
    ) -> Result<Self, String> {
        if input_ports.len() != execution.inputs.len() {
            return Err("physical input terminal width does not match controls".into());
        }
        for port in &input_ports {
            execution.validate_observation(*port)?;
        }
        let mut model = Self::new(execution)?;
        model.input_ports = input_ports;
        Ok(model)
    }

    pub fn project_state(
        &self,
        concrete: &PhysicalBehaviorState,
    ) -> Result<PhysicalAbstractState, String> {
        self.execution.check_state(concrete)?;
        Ok(PhysicalAbstractState {
            model: concrete.model,
            inputs: concrete.inputs.clone(),
            torch: self
                .torch_position
                .map(|p| self.law.project(&concrete.torches[&p]))
                .transpose()?,
        })
    }

    fn check_state(&self, state: &PhysicalAbstractState) -> Result<(), String> {
        if state.model != self.execution.identity {
            Err("abstract execution state belongs to another physical model".into())
        } else {
            Ok(())
        }
    }

    fn electrical(&self, state: &PhysicalAbstractState) -> Result<ElectricalObservation, String> {
        self.check_state(state)?;
        let lit = state
            .torch
            .as_ref()
            .map(|local| local.register("lit") == Some(1));
        let key = (state.inputs.clone(), lit);
        if let Some(result) = self.electrical.borrow().get(&key) {
            return Ok(result.clone());
        }
        let world = self.execution.world_for_inputs(&state.inputs);
        let devices = DeviceOutputState {
            torch_lit: self.torch_position.zip(lit).into_iter().collect(),
            ..Default::default()
        };
        let solved = self.execution.resolve_devices(&world, &devices)?;
        for (index, port) in self.input_ports.iter().enumerate() {
            if port.observe(&solved) != state.inputs[index] {
                return Err(format!(
                    "input driver does not establish the requested Boolean value at terminal {port:?}"
                ));
            }
        }
        let result = ElectricalObservation {
            support: self
                .torch_position
                .is_some_and(|p| torch_support_is_powered(&world, p, &solved)),
            outputs: self.execution.observed_outputs(&solved),
        };
        self.electrical.borrow_mut().insert(key, result.clone());
        Ok(result)
    }

    fn notify(&self, mut state: PhysicalAbstractState) -> Result<PhysicalAbstractState, String> {
        let power = self.electrical(&state)?.support;
        if let Some(local) = &mut state.torch {
            *local = self.law.event(
                local,
                "neighbor_update",
                &BTreeMap::from([("powered".into(), u16::from(power))]),
            )?;
        }
        Ok(state)
    }
}

impl AbstractBehaviorModel for PhysicalHistoryAbstraction<'_> {
    type State = PhysicalAbstractState;
    fn initial_state(&self) -> Result<Self::State, String> {
        // Same fresh-construction and initial notification as concrete execution.
        self.project_state(&self.execution.initial_state()?)
    }

    fn with_inputs(&self, state: &Self::State, inputs: &[bool]) -> Result<Self::State, String> {
        self.check_state(state)?;
        if inputs.len() != self.execution.inputs.len() {
            return Err("abstract physical input width does not match bindings".into());
        }
        if inputs == state.inputs {
            return Ok(state.clone());
        }
        let mut driven = state.clone();
        driven.inputs = inputs.to_vec();
        self.notify(driven)
    }

    fn successors(&self, state: &Self::State, limit: usize) -> Result<Vec<Self::State>, String> {
        self.check_state(state)?;
        if limit == 0 {
            return Err("abstract successor budget exhausted".into());
        }
        let Some(local) = &state.torch else {
            return Ok(vec![state.clone()]);
        };
        let successors = self
            .law
            .successors(local, limit, &mut |_, tentative, register| {
                if register == "lit" {
                    let mut changed = state.clone();
                    changed.torch = Some(tentative.clone());
                    *tentative = self.notify(changed)?.torch.expect("one torch");
                }
                Ok(())
            })?;
        Ok(successors
            .into_iter()
            .map(|torch| PhysicalAbstractState {
                model: state.model,
                inputs: state.inputs.clone(),
                torch: Some(torch),
            })
            .collect())
    }

    fn outputs(&self, state: &Self::State) -> Result<Vec<bool>, String> {
        Ok(self.electrical(state)?.outputs)
    }
}
