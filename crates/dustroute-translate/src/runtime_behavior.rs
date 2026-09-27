//! Complete physical state exploration for explicitly bound location behavior.
//! A behavior result alone is not a whole-realization/adoption certificate.
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use dustroute_library::assembly::Assembly;
use dustroute_library::blueprint::{
    BehaviorBinding, BlueprintCatalog, InstancePath, TypeContract, TypeRevision,
};
use dustroute_library::execution_context::{check_law_requirements, resolve_law_references};
use dustroute_library::location_observation::LocationPredicate;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_minecraft::time::piston_runtime::{ElectricalPistonRuntime, new_piston_runtime};
use dustroute_minecraft::time::runtime::{PistonBehaviorState, RuntimeView};
use dustroute_minecraft::{BlockKind, Pos};

use crate::behavior_type::{
    BehaviorBudget, BehaviorModel, BehaviorTypeReport, verify_repeated_settling,
};
use crate::location_behavior::{FixedObservation, LocationBehaviorBinding};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RuntimeBehaviorState {
    model: u64,
    physical: PistonBehaviorState,
}

pub struct RuntimeBehaviorModel {
    identity: u64,
    initial: RuntimeBehaviorState,
    definition: TypeRevision,
    context: RuntimeBehaviorContext,
    inputs: Vec<(Pos, bool)>,
    initial_inputs: Vec<bool>,
    outputs: Vec<FixedObservation>,
}

pub(crate) fn fresh_runtime(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
    context: &RuntimeBehaviorContext,
) -> Result<ElectricalPistonRuntime, String> {
    let view = assembly.inspect(catalog).map_err(|e| e.to_string())?;
    resolve_law_references(catalog, &context.execution_context())?;
    for occurrence in view.occurrences.values() {
        let source = catalog
            .revision(&occurrence.revision)
            .expect("indexed source");
        check_law_requirements(catalog, &context.execution_context(), &source.required_laws)?;
    }
    // Do not fill a bounding box through unknown holes or silently crop the
    // surrounding environment. Fragmented known space needs another adapter.
    let region = context.known_region;
    if !assembly
        .known_regions
        .iter()
        .any(|r| r.contains(region.min) && r.contains(region.max))
        || assembly
            .known_regions
            .iter()
            .any(|r| !region.contains(r.min) || !region.contains(r.max))
    {
        return Err("runtime exploration requires a declared rectangular known region covering the complete Assembly known space".into());
    }
    if context
        .input_levers
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .len()
        != context.input_levers.len()
    {
        return Err("duplicate runtime input lever".into());
    }
    let run = new_piston_runtime(view.proposed_world(), region, context.root_limits)
        .map_err(|e| e.to_string())?;
    for pos in &context.input_levers {
        let sample = run
            .view()
            .observe_location(*pos)
            .map_err(|e| e.to_string())?;
        if sample.block().kind != BlockKind::Lever || sample.block().powered.is_none() {
            return Err(
                "runtime input must be an actual known lever with an explicit powered state".into(),
            );
        }
    }
    Ok(run)
}

impl RuntimeBehaviorModel {
    /// Builds from the complete actual Assembly, never an immutable source's
    /// default blocks or a separately supplied running snapshot.
    pub fn from_fresh_assembly(
        catalog: &BlueprintCatalog,
        assembly: &Assembly,
        instance: &InstancePath,
        binding: &BehaviorBinding,
        context: &RuntimeBehaviorContext,
    ) -> Result<Self, String> {
        let definition = catalog
            .type_revision(binding.behavior_type())
            .ok_or("unknown behavior type")?
            .clone();
        let relation = match &definition.contract {
            TypeContract::RepeatedSettling { relation } => relation.clone(),
            TypeContract::PistonDoor { requirement } => requirement.relation(),
            _ => return Err("runtime exploration supports repeated-settling and completed-operation piston-door types".into()),
        };
        relation.validate().map_err(str::to_owned)?;
        let bound = LocationBehaviorBinding::resolve(catalog, assembly, instance, binding)?;
        if let TypeContract::PistonDoor { requirement } = &definition.contract {
            bound.validate_door(requirement)?;
        }
        let initial = fresh_runtime(catalog, assembly, context)?;
        let mut inputs = vec![];
        for name in &relation.inputs {
            let observation = &bound.inputs[name];
            let LocationPredicate::Powered {
                block_kind: BlockKind::Lever,
                powered,
            } = &observation.predicate
            else {
                return Err(
                    "controllable location input needs an explicit powered Lever predicate".into(),
                );
            };
            inputs.push((observation.position, *powered));
        }
        let selected: BTreeSet<_> = inputs.iter().map(|(pos, _)| *pos).collect();
        if selected.len() != inputs.len()
            || selected != context.input_levers.iter().copied().collect()
        {
            return Err("the complete declared input relation must cover exactly the context's distinct physical input levers".into());
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| "runtime model identity exhausted")?;
        let result = Self {
            identity,
            initial: RuntimeBehaviorState {
                model: identity,
                physical: initial.behavior_state().map_err(|e| e.to_string())?,
            },
            outputs: relation
                .outputs
                .iter()
                .map(|name| bound.outputs[name].clone())
                .collect(),
            initial_inputs: inputs
                .iter()
                .map(|(pos, positive)| {
                    initial
                        .view()
                        .observe_location(*pos)
                        .map_err(|e| e.to_string())?
                        .block()
                        .powered
                        .map(|powered| powered == *positive)
                        .ok_or("initial lever power is unknown".into())
                })
                .collect::<Result<_, String>>()?,
            inputs,
            definition,
            context: context.clone(),
        };
        result.outputs(&result.initial)?;
        Ok(result)
    }

    pub fn definition(&self) -> &TypeRevision {
        &self.definition
    }
    pub fn context(&self) -> &RuntimeBehaviorContext {
        &self.context
    }
    pub fn verify(&self, budget: BehaviorBudget) -> BehaviorTypeReport {
        self.verify_observed(self, budget)
    }
    pub(crate) fn verify_observed<M: BehaviorModel>(
        &self,
        model: &M,
        budget: BehaviorBudget,
    ) -> BehaviorTypeReport {
        match &self.definition.contract {
            TypeContract::PistonDoor { .. } => crate::piston_door_type::verify_piston_door(
                &self.definition,
                model,
                self.initial_inputs[0],
                budget,
            ),
            _ => verify_repeated_settling(&self.definition, model, budget),
        }
    }

    fn restore(&self, state: &RuntimeBehaviorState) -> Result<ElectricalPistonRuntime, String> {
        if state.model != self.identity {
            return Err("state belongs to a different runtime behavior model".into());
        }
        ElectricalPistonRuntime::from_behavior_state(&state.physical).map_err(|e| e.to_string())
    }
    fn capture(&self, run: &ElectricalPistonRuntime) -> Result<RuntimeBehaviorState, String> {
        Ok(RuntimeBehaviorState {
            model: self.identity,
            physical: run.behavior_state().map_err(|e| e.to_string())?,
        })
    }
    fn sample(&self, run: &ElectricalPistonRuntime) -> Result<Vec<bool>, String> {
        self.outputs
            .iter()
            .map(|o| {
                o.predicate.evaluate(
                    &run.view()
                        .observe_location(o.position)
                        .map_err(|e| e.to_string())?,
                )
            })
            .collect()
    }
    // The observer sees every committed intermediate state, including input
    // callbacks. Its diagnostics cannot change or reject physical execution.
    fn root(
        &self,
        run: &mut ElectricalPistonRuntime,
        observe: &mut impl FnMut(RuntimeView<'_>),
    ) -> Result<Vec<Vec<bool>>, String> {
        let mut samples = vec![];
        loop {
            let progressed = run.microstep().map_err(|e| e.to_string())?.is_some();
            observe(run.view());
            let value = self.sample(run)?;
            if samples.last() != Some(&value) {
                samples.push(value);
            }
            if !progressed || run.at_input_boundary() {
                break;
            }
        }
        Ok(samples)
    }
    pub(crate) fn drive_observed(
        &self,
        state: &RuntimeBehaviorState,
        inputs: &[bool],
        observe: &mut impl FnMut(RuntimeView<'_>),
    ) -> Result<RuntimeBehaviorState, String> {
        if inputs.len() != self.inputs.len() {
            return Err("runtime input width mismatch".into());
        }
        let mut run = self.restore(state)?;
        observe(run.view());
        for ((pos, positive), value) in self.inputs.iter().zip(inputs) {
            let block = run
                .view()
                .observe_location(*pos)
                .map_err(|e| e.to_string())?;
            if block.block().kind != BlockKind::Lever {
                return Err("runtime input lever was displaced or replaced".into());
            }
            let powered = *value == *positive;
            if block.block().powered != Some(powered) {
                run.input_now(*pos, powered).map_err(|e| e.to_string())?;
                self.root(&mut run, observe)?;
                // Each input is itself a complete synchronous root.
                run = self.restore(&RuntimeBehaviorState {
                    model: self.identity,
                    physical: run.behavior_state().map_err(|e| e.to_string())?,
                })?;
            }
        }
        self.capture(&run)
    }
    pub(crate) fn advance_observed(
        &self,
        state: &RuntimeBehaviorState,
        observe: &mut impl FnMut(RuntimeView<'_>),
    ) -> Result<(RuntimeBehaviorState, Vec<Vec<bool>>), String> {
        let mut run = self.restore(state)?;
        observe(run.view());
        let samples = if run.advance_behavior_clock().map_err(|e| e.to_string())? {
            observe(run.view());
            vec![self.sample(&run)?]
        } else {
            self.root(&mut run, observe)?
        };
        Ok((self.capture(&run)?, samples))
    }
}

impl BehaviorModel for RuntimeBehaviorModel {
    type State = RuntimeBehaviorState;
    fn initial_state(&self) -> Result<Self::State, String> {
        Ok(self.initial.clone())
    }
    fn with_inputs(&self, state: &Self::State, inputs: &[bool]) -> Result<Self::State, String> {
        self.drive_observed(state, inputs, &mut |_| {})
    }
    fn step(&self, state: &Self::State) -> Result<Self::State, String> {
        Ok(self.advance_observed(state, &mut |_| {})?.0)
    }
    fn outputs(&self, state: &Self::State) -> Result<Vec<bool>, String> {
        self.sample(&self.restore(state)?)
    }
    fn step_with_observations(
        &self,
        state: &Self::State,
    ) -> Result<(Self::State, Vec<Vec<bool>>), String> {
        self.advance_observed(state, &mut |_| {})
    }
}
