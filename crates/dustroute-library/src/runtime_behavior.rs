//! Assumptions for the moving-world proof path. Existing fixed-geometry
//! contexts and their saved meanings are intentionally separate.
use dustroute_minecraft::execution_context::{WorldExecutionContext, WorldExecutionProfile};
pub use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{Pos, Region};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub enum RuntimeBehaviorProfile {
    #[serde(rename = "dustroute.piston-electrical-root-exploration.v16")]
    UnifiedPistonElectricalRootExplorationV16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBehaviorContext {
    pub profile: RuntimeBehaviorProfile,
    pub initial_condition: crate::behavior_type::BehaviorInitialCondition,
    pub known_region: Region,
    /// Actual levers the environment may change. Other levers retain their
    /// declared initial state. No invisible driver is added to the world.
    pub input_levers: Vec<Pos>,
    /// Computation limits renewed at each complete root, not circuit deadlines.
    pub root_limits: RuntimeLimits,
}

impl RuntimeBehaviorContext {
    /// Standard context for newly constructed piston mechanisms, regardless of
    /// body direction. Saved contexts continue to require an explicit profile.
    pub fn fresh_pistons(known_region: Region, input_levers: Vec<Pos>) -> Self {
        Self {
            profile: RuntimeBehaviorProfile::UnifiedPistonElectricalRootExplorationV16,
            initial_condition: crate::behavior_type::BehaviorInitialCondition::FreshConstruction,
            known_region,
            input_levers,
            root_limits: RuntimeLimits::default(),
        }
    }

    pub fn execution_context(&self) -> WorldExecutionContext {
        WorldExecutionContext::for_profile(
            WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V16,
        )
    }
}
