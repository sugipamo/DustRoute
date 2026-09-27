//! Observable requirements, independent of a realization's geometry or name.
use std::collections::BTreeSet;

use crate::blueprint::BlueprintRevisionId;
use serde::{Deserialize, Serialize};

/// An explicit execution assumption, not a claim of live Minecraft evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PhysicalBehaviorContext {
    pub profile: PhysicalBehaviorProfile,
    pub initial_condition: BehaviorInitialCondition,
    pub dust_law: BlueprintRevisionId,
    pub torch_law: BlueprintRevisionId,
    pub max_electrical_iterations: usize,
    /// Actual Assembly coordinates. Grouping changes no geometry; relocating a
    /// candidate requires new bindings and fresh checks. Empty for old contexts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_drivers: Vec<PhysicalInputDriver>,
}

impl PhysicalBehaviorContext {
    /// Normalize the original persisted format without rewriting old records.
    /// Actual port/lever bindings remain in this context and are checked against
    /// the complete Assembly; the world contract carries no interpretation state.
    pub fn execution_context(
        &self,
    ) -> dustroute_minecraft::execution_context::WorldExecutionContext {
        use dustroute_minecraft::execution_context::{
            LawRole, WorldExecutionContext, WorldExecutionProfile,
        };
        let profile = match self.profile {
            PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1 => {
                WorldExecutionProfile::DustTorchSynchronousGameTickV1
            }
            PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1 => {
                WorldExecutionProfile::DustSingleTorchBlockEffectsV1
            }
        };
        let mut context = WorldExecutionContext::for_profile(profile);
        context
            .laws
            .insert(LawRole::DustStrength, self.dust_law.to_string());
        context
            .laws
            .insert(LawRole::Torch, self.torch_law.to_string());
        context.max_electrical_iterations = Some(self.max_electrical_iterations);
        context
    }
    /// World-selected laws, independent of how many interpretations overlap.
    pub fn law_revisions(&self) -> Vec<&BlueprintRevisionId> {
        [&self.dust_law, &self.torch_law]
            .into_iter()
            .chain(crate::builtin_laws::spatial_law_revisions())
            .collect()
    }

    /// Candidate ports and external driver bindings may move; physical laws and
    /// execution assumptions may not change to make an optimization pass.
    pub fn same_execution_assumptions(&self, other: &Self) -> bool {
        self.initial_condition == other.initial_condition
            && self.execution_context() == other.execution_context()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PhysicalInputDriver {
    pub port_position: dustroute_minecraft::Pos,
    pub port_kind: crate::blueprint::BlueprintPortKind,
    pub lever_position: dustroute_minecraft::Pos,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub enum PhysicalBehaviorProfile {
    #[serde(rename = "dustroute.dust-torch-synchronous-game-tick.v1")]
    DustTorchSynchronousGameTickV1,
    /// Fixed geometry with at most one torch; its block-state effects deliver
    /// feedback notifications before the rest of the local handler resumes.
    #[serde(rename = "dustroute.dust-single-torch-block-effects.v1")]
    DustSingleTorchBlockEffectsV1,
}

impl PhysicalBehaviorProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DustTorchSynchronousGameTickV1 => "dustroute.dust-torch-synchronous-game-tick.v1",
            Self::DustSingleTorchBlockEffectsV1 => "dustroute.dust-single-torch-block-effects.v1",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorInitialCondition {
    /// Declared lit state, empty histories and timers before initial notification.
    /// This never restores unknown hidden state from an observed running world.
    FreshConstruction,
}

/// An autonomous, single-output periodic behavior. From the declared initial
/// state, a finite startup is followed by a repeating Boolean waveform that
/// contains both levels. No numerical period, pulse width, phase or startup
/// deadline is required. A waveform may include bursts and recovery pauses.
/// There are no externally controlled inputs; the environment is part of the
/// realization's verification context, not an implicit arbitrary input source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Periodic {
    pub output: String,
}

impl Periodic {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.output.trim().is_empty() {
            return Err("a periodic type needs a nonempty output name");
        }
        Ok(())
    }
}

/// Autonomous finite output activity followed by permanent OFF. From the
/// declared initial state, at least two ON-to-OFF transitions must occur, then
/// the output must eventually remain OFF. An initially ON output counts when
/// it actually falls; no synthetic edge is inserted before the initial sample.
/// No exact pulse count, time bound, coordinates or burnout mechanism is fixed.
/// External restart/repeated-trigger behavior is a separate requirement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FiniteBurst {
    pub output: String,
}

impl FiniteBurst {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.output.trim().is_empty() {
            return Err("a finite-burst type needs a nonempty output name");
        }
        Ok(())
    }
}

/// A total Boolean input/output relation with repeated-use semantics.
///
/// Starting from the declared initial physical state, inputs may change between
/// atomic model steps, including during settling. Whenever inputs are then
/// held constant, all named outputs must eventually remain at the corresponding
/// row values. The same physical state is retained across input changes.
///
/// Coordinates, delay bounds, transient pulses and internal decomposition are
/// deliberately absent. Internal activity is allowed when outputs settle.
/// Verification budgets are not deadlines on the behavior of the circuit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepeatedSettling {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// Each input vector must occur exactly once. Row order is insignificant.
    pub rows: Vec<BooleanRow>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BooleanRow {
    pub inputs: Vec<bool>,
    pub outputs: Vec<bool>,
}

/// Ordinary door operation. Inputs change only at completed operation states;
/// this does not inherit RepeatedSettling's interruption-tolerance semantics.
/// The ordered 3x3 observations are bound to a fixed physical aperture.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PistonDoor {
    pub closed_input: String,
    pub aperture: [[DoorCell; 3]; 3],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DoorCell {
    pub air: String,
    pub solid: String,
}

impl PistonDoor {
    pub fn three_by_three() -> Self {
        Self {
            closed_input: "closed_command".into(),
            aperture: std::array::from_fn(|row| {
                std::array::from_fn(|column| DoorCell {
                    air: format!("aperture_{row}_{column}_air"),
                    solid: format!("aperture_{row}_{column}_solid"),
                })
            }),
        }
    }

    /// Only the port names and truth table are shared. The verifier must use
    /// completed-operation semantics, never unrestricted input exploration.
    pub fn relation(&self) -> RepeatedSettling {
        RepeatedSettling {
            inputs: vec![self.closed_input.clone()],
            outputs: self
                .aperture
                .iter()
                .flatten()
                .flat_map(|cell| [cell.air.clone(), cell.solid.clone()])
                .collect(),
            rows: [false, true]
                .into_iter()
                .map(|closed| BooleanRow {
                    inputs: vec![closed],
                    outputs: (0..9).flat_map(|_| [!closed, closed]).collect(),
                })
                .collect(),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        self.relation().validate()
    }
}

impl RepeatedSettling {
    /// Checks the finite relation's data, never a physical implementation.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.inputs.is_empty() || self.outputs.is_empty() {
            return Err("a repeated-settling type needs inputs and outputs");
        }
        let count = 1_usize
            .checked_shl(
                self.inputs
                    .len()
                    .try_into()
                    .map_err(|_| "too many inputs")?,
            )
            .ok_or("input relation cannot be represented on this platform")?;
        if self.rows.len() != count {
            return Err("the relation must cover every Boolean input vector");
        }
        let mut names = BTreeSet::new();
        for name in self.inputs.iter().chain(&self.outputs) {
            if name.trim().is_empty() || !names.insert(name) {
                return Err("behavior port names must be nonempty and unique");
            }
        }
        let mut vectors = BTreeSet::new();
        for row in &self.rows {
            if row.inputs.len() != self.inputs.len() || row.outputs.len() != self.outputs.len() {
                return Err("behavior row width does not match its ports");
            }
            if !vectors.insert(&row.inputs) {
                return Err("duplicate behavior input vector");
            }
        }
        Ok(())
    }
}
