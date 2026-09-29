//! Behavioral checks for a bounded physical execution profile.
//!
//! Geometry is fixed and dust resolves instantaneously. Profiles explicitly
//! select atomic torch steps or synchronous block-change effects. Neither is
//! an assertion of complete Vanilla world/scheduler conformance.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

use dustroute_library::assembly::{Assembly, AssemblyRevision};
use dustroute_library::behavior_type::{
    BehaviorInitialCondition, FiniteBurst, Periodic, PhysicalBehaviorContext,
    PhysicalBehaviorProfile,
};
use dustroute_library::blueprint::{
    AssemblyRevisionId, BlueprintCatalog, BlueprintRevision, BlueprintRevisionId, TypeContract,
    TypeRevision, TypeRevisionId,
};
use dustroute_minecraft::law::{ExecutableLaw, LawState};
use dustroute_minecraft::{
    BlockKind, HistoricalPlacementV1, ObservationClassification, Pos, World,
};
use serde::Serialize;

use crate::behavior_type::{
    BehaviorBudget, BehaviorModel, BehaviorTypeReport, verify_repeated_settling,
};
use crate::dust_law::DustLaw;
use crate::electrical::{
    DeviceOutputState, ElectricalTopology, InstantaneousElectricalState,
    solve_instantaneous_with_law, torch_support_is_powered,
};
use crate::finite_burst::{FiniteBurstTypeReport, verify_finite_burst};
use crate::periodic::{PeriodicTypeReport, verify_periodic};
use crate::world_laws::{WorldLaws, check_requirements};

pub mod abstract_history;

pub const PHYSICAL_BEHAVIOR_PROFILE: &str = "dustroute.dust-torch-synchronous-game-tick.v1";
pub const PHYSICAL_BEHAVIOR_EFFECTS_PROFILE: &str = "dustroute.dust-single-torch-block-effects.v1";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhysicalOutput {
    Signal { position: Pos },
    BlockPower { position: Pos },
}

impl PhysicalOutput {
    fn observe(self, electrical: &InstantaneousElectricalState) -> bool {
        match self {
            Self::Signal { position } => electrical.signal(position) > 0,
            Self::BlockPower { position } => electrical.power(position).powered(),
        }
    }
}

/// Coordinates are candidate-specific bindings, never part of the type. Inputs
/// operate actual levers; no invisible power driver is added to the assembly.
#[derive(Clone, Debug, Serialize)]
pub struct PhysicalBehaviorSelection {
    pub assembly: AssemblyRevisionId,
    pub behavior_type: TypeRevisionId,
    pub dust_law: BlueprintRevisionId,
    pub torch_law: BlueprintRevisionId,
    pub inputs: BTreeMap<String, Pos>,
    pub outputs: BTreeMap<String, PhysicalOutput>,
    /// Computation budget, not a circuit settling deadline.
    pub max_electrical_iterations: usize,
}

/// Private construction prevents a world snapshot from inventing hidden state.
/// Relative local histories/timers and all input levels participate in equality.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PhysicalBehaviorState {
    model: u64,
    inputs: Vec<bool>,
    torches: BTreeMap<Pos, LawState>,
}

#[derive(Debug)]
pub struct PhysicalBehaviorModel {
    selection: PhysicalBehaviorSelection,
    assembly: AssemblyRevision,
    definition: TypeRevision,
    execution: PhysicalExecutionModel,
}

/// Execution over an exact candidate value, including proposals not yet stored.
#[derive(Debug)]
pub(crate) struct PhysicalExecutionModel {
    identity: u64,
    profile: PhysicalBehaviorProfile,
    max_electrical_iterations: usize,
    torch_revision: BlueprintRevision,
    dust: DustLaw,
    torch: ExecutableLaw,
    world: HistoricalPlacementV1,
    topology: ElectricalTopology,
    inputs: Vec<Pos>,
    outputs: Vec<PhysicalOutput>,
}

/// Self-contained diagnostics only. No deserialization, adoption, or live-world
/// authority; every call to verify explores again from the declared initial state.
#[derive(Clone, Debug, Serialize)]
pub struct PhysicalBehaviorReport {
    pub profile: &'static str,
    pub placement_validation_profile: &'static str,
    pub initial_condition: &'static str,
    pub selection: PhysicalBehaviorSelection,
    pub assembly: AssemblyRevision,
    pub definition: TypeRevision,
    pub dust_revision: BlueprintRevision,
    pub torch_revision: BlueprintRevision,
    pub behavior: BehaviorTypeReport,
}

/// Periodic evidence for this exact realization and fixed environment only.
/// A new placement/connection requires fresh verification, not report reuse.
#[derive(Clone, Debug, Serialize)]
pub struct PhysicalPeriodicReport {
    pub profile: &'static str,
    pub placement_validation_profile: &'static str,
    pub initial_condition: &'static str,
    pub selection: PhysicalBehaviorSelection,
    pub assembly: AssemblyRevision,
    pub definition: TypeRevision,
    pub dust_revision: BlueprintRevision,
    pub torch_revision: BlueprintRevision,
    pub behavior: PeriodicTypeReport,
}

/// Finite activity/cessation evidence; does not certify external restartability.
#[derive(Clone, Debug, Serialize)]
pub struct PhysicalFiniteBurstReport {
    pub profile: &'static str,
    pub placement_validation_profile: &'static str,
    pub initial_condition: &'static str,
    pub selection: PhysicalBehaviorSelection,
    pub assembly: AssemblyRevision,
    pub definition: TypeRevision,
    pub dust_revision: BlueprintRevision,
    pub torch_revision: BlueprintRevision,
    pub behavior: FiniteBurstTypeReport,
}

const FRESH_INITIAL_CONDITION: &str =
    "fresh construction; declared lit state; empty history and timers before initial notification";

impl PhysicalBehaviorModel {
    /// Explicit fresh-construction assumption: declared torch lit states, empty
    /// local histories, no pending callbacks before initial power notification.
    /// This does not restore an observed running circuit's unknown hidden state.
    pub fn from_fresh_assembly(
        catalog: &BlueprintCatalog,
        selection: PhysicalBehaviorSelection,
    ) -> Result<Self, String> {
        Self::from_fresh_assembly_with_profile(
            catalog,
            selection,
            PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
        )
    }

    /// Select execution semantics explicitly. The original constructor retains
    /// its original profile; existing pins never acquire new behavior silently.
    pub fn from_fresh_assembly_with_profile(
        catalog: &BlueprintCatalog,
        selection: PhysicalBehaviorSelection,
        profile: PhysicalBehaviorProfile,
    ) -> Result<Self, String> {
        let assembly = catalog
            .assembly(&selection.assembly)
            .ok_or("unknown assembly revision")?
            .clone();
        let definition = catalog
            .type_revision(&selection.behavior_type)
            .ok_or("unknown behavior type revision")?
            .clone();
        let context = PhysicalBehaviorContext {
            profile,
            initial_condition: BehaviorInitialCondition::FreshConstruction,
            dust_law: selection.dust_law.clone(),
            torch_law: selection.torch_law.clone(),
            max_electrical_iterations: selection.max_electrical_iterations,
            input_drivers: vec![],
        };
        let execution = PhysicalExecutionModel::from_fresh_assembly(
            catalog,
            &assembly.assembly,
            &definition,
            &context,
            &selection.inputs,
            &selection.outputs,
        )?;
        Ok(Self {
            selection,
            assembly,
            definition,
            execution,
        })
    }

    pub fn verify(&self, budget: BehaviorBudget) -> PhysicalBehaviorReport {
        PhysicalBehaviorReport {
            profile: self.execution.profile.as_str(),
            placement_validation_profile: HistoricalPlacementV1::PROFILE,
            initial_condition: FRESH_INITIAL_CONDITION,
            selection: self.selection.clone(),
            assembly: self.assembly.clone(),
            definition: self.definition.clone(),
            dust_revision: self.execution.dust.revision().clone(),
            torch_revision: self.execution.torch_revision.clone(),
            behavior: verify_repeated_settling(&self.definition, self, budget),
        }
    }

    pub fn verify_periodic(&self, budget: BehaviorBudget) -> PhysicalPeriodicReport {
        PhysicalPeriodicReport {
            profile: self.execution.profile.as_str(),
            placement_validation_profile: HistoricalPlacementV1::PROFILE,
            initial_condition: FRESH_INITIAL_CONDITION,
            selection: self.selection.clone(),
            assembly: self.assembly.clone(),
            definition: self.definition.clone(),
            dust_revision: self.execution.dust.revision().clone(),
            torch_revision: self.execution.torch_revision.clone(),
            behavior: verify_periodic(&self.definition, self, budget),
        }
    }

    pub fn verify_finite_burst(&self, budget: BehaviorBudget) -> PhysicalFiniteBurstReport {
        PhysicalFiniteBurstReport {
            profile: self.execution.profile.as_str(),
            placement_validation_profile: HistoricalPlacementV1::PROFILE,
            initial_condition: FRESH_INITIAL_CONDITION,
            selection: self.selection.clone(),
            assembly: self.assembly.clone(),
            definition: self.definition.clone(),
            dust_revision: self.execution.dust.revision().clone(),
            torch_revision: self.execution.torch_revision.clone(),
            behavior: verify_finite_burst(&self.definition, self, budget),
        }
    }

    pub fn torch_state<'a>(
        &self,
        state: &'a PhysicalBehaviorState,
        position: Pos,
    ) -> Result<&'a LawState, String> {
        self.execution.torch_state(state, position)
    }

    pub fn electrical_state(
        &self,
        state: &PhysicalBehaviorState,
    ) -> Result<InstantaneousElectricalState, String> {
        self.execution.electrical_state(state)
    }

    /// Explicit external neighbor stimulus for finite diagnostic replay. This
    /// is never injected by autonomous recurrence verification. The bounded
    /// effects profile has at most one torch, so no recipient order is invented.
    pub fn notify_torch_neighbors(
        &self,
        state: &PhysicalBehaviorState,
    ) -> Result<PhysicalBehaviorState, String> {
        self.execution.check_state(state)?;
        self.execution.notify(state.clone())
    }
}

impl PhysicalExecutionModel {
    pub(crate) fn from_fresh_assembly(
        catalog: &BlueprintCatalog,
        assembly: &Assembly,
        definition: &TypeRevision,
        context: &PhysicalBehaviorContext,
        input_bindings: &BTreeMap<String, Pos>,
        output_bindings: &BTreeMap<String, PhysicalOutput>,
    ) -> Result<Self, String> {
        match (context.profile, context.initial_condition) {
            (
                PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1
                | PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
                BehaviorInitialCondition::FreshConstruction,
            ) => {}
        }
        let (input_names, output_names): (&[String], &[String]) = match &definition.contract {
            TypeContract::RepeatedSettling { relation } => {
                relation.validate().map_err(str::to_owned)?;
                (&relation.inputs, &relation.outputs)
            }
            TypeContract::Periodic {
                requirement: Periodic { output },
            }
            | TypeContract::FiniteBurst {
                requirement: FiniteBurst { output },
            } => {
                if output.trim().is_empty() {
                    return Err("an autonomous type needs a nonempty output name".into());
                }
                let view = assembly.inspect(catalog).map_err(|e| e.to_string())?;
                for boundary in &assembly.boundaries {
                    if view
                        .resolved_port(&boundary.port)
                        .map_err(|e| e.to_string())?
                        .0
                        .direction
                        == dustroute_library::PortDirection::Input
                    {
                        return Err(
                            "autonomous verification does not support external input boundaries"
                                .into(),
                        );
                    }
                }
                (&[], std::slice::from_ref(output))
            }
            _ => return Err("selected type does not describe supported behavior".into()),
        };
        if input_bindings.keys().collect::<BTreeSet<_>>()
            != input_names.iter().collect::<BTreeSet<_>>()
            || output_bindings.keys().collect::<BTreeSet<_>>()
                != output_names.iter().collect::<BTreeSet<_>>()
        {
            return Err("physical bindings must match every named type input and output".into());
        }
        let inputs: Vec<_> = input_names.iter().map(|n| input_bindings[n]).collect();
        let outputs: Vec<_> = output_names.iter().map(|n| output_bindings[n]).collect();
        if inputs.iter().collect::<BTreeSet<_>>().len() != inputs.len() {
            return Err("independent inputs cannot drive the same lever".into());
        }
        let WorldLaws {
            dust,
            torch_revision,
            torch,
        } = WorldLaws::resolve(catalog, context)?;
        let view = assembly.inspect(catalog).map_err(|e| e.to_string())?;
        for occurrence in view.occurrences.values() {
            check_requirements(
                catalog,
                context,
                &catalog
                    .revision(&occurrence.revision)
                    .expect("indexed source")
                    .required_laws,
            )?;
        }
        let world = view.proposed_world();
        if context.profile == PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1
            && world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::RedstoneTorch)
                .count()
                > 1
        {
            return Err("block-effects profile supports at most one torch; inter-torch callback order is not modeled".into());
        }
        // The fixed topology queries at most this halo. Unknown cells cannot be
        // read as air. Check arithmetic and offsets before generic world checks.
        for (position, block) in world.iter() {
            if !context.execution_context().profile.admits_block(block)
                || block.observation_classification == ObservationClassification::Coarse
                || block.requires_live_observation()
            {
                return Err(format!(
                    "unsupported physical behavior block at {position:?}: {:?}",
                    block.kind
                ));
            }
            if block.support_offset.is_some_and(|p| {
                !matches!(
                    (p.x, p.y, p.z),
                    (1, 0, 0) | (-1, 0, 0) | (0, 1, 0) | (0, -1, 0) | (0, 0, 1) | (0, 0, -1)
                )
            }) {
                return Err(format!("non-adjacent support at {position:?}"));
            }
            for dx in -2..=2 {
                for dy in -2..=2 {
                    for dz in -2..=2 {
                        let neighbor = Pos::new(
                            position.x.checked_add(dx).ok_or("coordinate overflow")?,
                            position.y.checked_add(dy).ok_or("coordinate overflow")?,
                            position.z.checked_add(dz).ok_or("coordinate overflow")?,
                        );
                        if view.block_at(neighbor).is_none() {
                            return Err(format!("unknown physical neighborhood at {neighbor:?}"));
                        }
                    }
                }
            }
        }
        // Both supported execution profiles are immutable v1 semantics. Current
        // promotion/adoption independently requires the stronger placement gate.
        let world =
            HistoricalPlacementV1::try_from(world).map_err(|e| format!("{e}: {:?}", e.issues))?;
        for position in &inputs {
            if world.kind_at(*position) != BlockKind::Lever {
                return Err(format!(
                    "input binding requires a physical lever at {position:?}"
                ));
            }
        }
        static NEXT_MODEL: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT_MODEL
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| "model identity exhausted")?;
        let topology = ElectricalTopology::from_world(&world);
        let model = Self {
            identity,
            profile: context.profile,
            max_electrical_iterations: context.max_electrical_iterations,
            torch_revision,
            dust,
            torch,
            world,
            topology,
            inputs,
            outputs,
        };
        for output in &model.outputs {
            model.validate_observation(*output)?;
        }
        Ok(model)
    }

    fn validate_observation(&self, output: PhysicalOutput) -> Result<(), String> {
        let valid = match output {
            PhysicalOutput::Signal { position } => matches!(
                self.world.kind_at(position),
                BlockKind::RedstoneWire
                    | BlockKind::RedstoneTorch
                    | BlockKind::Lever
                    | BlockKind::RedstoneBlock
            ),
            PhysicalOutput::BlockPower { position } => {
                self.world.get(position).is_some_and(|block| {
                    let traits = block.redstone_traits();
                    traits.conducts_weak_power || traits.conducts_strong_power
                })
            }
        };
        valid
            .then_some(())
            .ok_or_else(|| format!("signal binding is not observable in this profile: {output:?}"))
    }

    fn check_state(&self, state: &PhysicalBehaviorState) -> Result<(), String> {
        if state.model != self.identity {
            Err("execution state belongs to another physical model".into())
        } else {
            Ok(())
        }
    }

    fn driven_world(&self, state: &PhysicalBehaviorState) -> World {
        self.world_for_inputs(&state.inputs)
    }

    fn world_for_inputs(&self, inputs: &[bool]) -> World {
        let mut world = self.world.clone().into_world();
        for (position, input) in self.inputs.iter().zip(inputs) {
            let block = world.get_mut(*position).expect("validated input");
            block.powered = Some(*input);
            block.power_level = Some(if *input { 15 } else { 0 });
        }
        world
    }

    fn resolve(
        &self,
        state: &PhysicalBehaviorState,
        world: &World,
    ) -> Result<InstantaneousElectricalState, String> {
        let devices = DeviceOutputState {
            torch_lit: state
                .torches
                .iter()
                .map(|(p, s)| (*p, s.register("lit") == Some(1)))
                .collect(),
            ..Default::default()
        };
        self.resolve_devices(world, &devices)
    }

    fn resolve_devices(
        &self,
        world: &World,
        devices: &DeviceOutputState,
    ) -> Result<InstantaneousElectricalState, String> {
        solve_instantaneous_with_law(
            world,
            devices,
            self.max_electrical_iterations,
            &self.topology,
            &self.dust,
        )
        .map_err(|e| e.to_string())
    }

    fn observed_outputs(&self, electrical: &InstantaneousElectricalState) -> Vec<bool> {
        self.outputs
            .iter()
            .map(|output| output.observe(electrical))
            .collect()
    }

    fn notify(&self, mut state: PhysicalBehaviorState) -> Result<PhysicalBehaviorState, String> {
        let world = self.driven_world(&state);
        let electrical = self.resolve(&state, &world)?;
        for (position, local) in &mut state.torches {
            *local = self.torch.event(
                local,
                "neighbor_update",
                &BTreeMap::from([(
                    "powered".into(),
                    u16::from(torch_support_is_powered(&world, *position, &electrical)),
                )]),
            )?;
        }
        Ok(state)
    }

    /// Read-only trace diagnostics. These observations cannot reconstruct a state.
    pub fn torch_state<'a>(
        &self,
        state: &'a PhysicalBehaviorState,
        position: Pos,
    ) -> Result<&'a LawState, String> {
        self.check_state(state)?;
        state
            .torches
            .get(&position)
            .ok_or_else(|| "no torch at position".into())
    }

    pub fn electrical_state(
        &self,
        state: &PhysicalBehaviorState,
    ) -> Result<InstantaneousElectricalState, String> {
        self.check_state(state)?;
        self.resolve(state, &self.driven_world(state))
    }
}

impl BehaviorModel for PhysicalExecutionModel {
    type State = PhysicalBehaviorState;

    fn initial_state(&self) -> Result<Self::State, String> {
        let mut torches = BTreeMap::new();
        for (position, block) in self
            .world
            .iter()
            .filter(|(_, b)| b.kind == BlockKind::RedstoneTorch)
        {
            torches.insert(
                *position,
                self.torch.initialize_register(
                    &self.torch.initial_state(),
                    "lit",
                    u16::from(block.powered.unwrap_or(true)),
                )?,
            );
        }
        self.notify(PhysicalBehaviorState {
            model: self.identity,
            inputs: self
                .inputs
                .iter()
                .map(|p| {
                    self.world
                        .get(*p)
                        .expect("validated input")
                        .powered
                        .unwrap_or(false)
                })
                .collect(),
            torches,
        })
    }

    fn with_inputs(&self, state: &Self::State, inputs: &[bool]) -> Result<Self::State, String> {
        self.check_state(state)?;
        if inputs.len() != self.inputs.len() {
            return Err("physical input width does not match type bindings".into());
        }
        if inputs == state.inputs {
            return Ok(state.clone());
        }
        let mut next = state.clone();
        next.inputs = inputs.to_vec();
        self.notify(next)
    }

    fn step(&self, state: &Self::State) -> Result<Self::State, String> {
        self.check_state(state)?;
        let mut next = state.clone();
        match self.profile {
            PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1 => {
                for local in next.torches.values_mut() {
                    *local = self.torch.advance(local)?;
                }
                self.notify(next)
            }
            PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1 => {
                if let Some((&position, local)) = state.torches.first_key_value() {
                    let advanced = self.torch.advance_with_register_effects(
                        local,
                        &mut |_, tentative, register| {
                            if register == "lit" {
                                next.torches.insert(position, tentative.clone());
                                // Deliver feedback at the block-change boundary,
                                // before history/recovery instructions continue.
                                next = self.notify(next.clone())?;
                                *tentative = next.torches[&position].clone();
                            }
                            Ok(())
                        },
                    )?;
                    next.torches.insert(position, advanced);
                }
                // Passing time alone is not a neighbor event. In particular,
                // history expiry must not invent a new recovery callback.
                Ok(next)
            }
        }
    }

    fn outputs(&self, state: &Self::State) -> Result<Vec<bool>, String> {
        let electrical = self.electrical_state(state)?;
        Ok(self.observed_outputs(&electrical))
    }
}

impl BehaviorModel for PhysicalBehaviorModel {
    type State = PhysicalBehaviorState;
    fn initial_state(&self) -> Result<Self::State, String> {
        self.execution.initial_state()
    }
    fn with_inputs(&self, state: &Self::State, inputs: &[bool]) -> Result<Self::State, String> {
        self.execution.with_inputs(state, inputs)
    }
    fn step(&self, state: &Self::State) -> Result<Self::State, String> {
        self.execution.step(state)
    }
    fn outputs(&self, state: &Self::State) -> Result<Vec<bool>, String> {
        self.execution.outputs(state)
    }
}
