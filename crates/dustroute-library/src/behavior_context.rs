//! Shared selection for review/adoption. Each variant keeps its own execution
//! and placement contract. The original JSON object has no added wrapper.
use serde::{Deserialize, Serialize};

use crate::behavior_type::PhysicalBehaviorContext;
use crate::blueprint::BlueprintRevisionId;
use crate::runtime_behavior::RuntimeBehaviorContext;
use dustroute_minecraft::execution_context::WorldExecutionContext;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum BehaviorReviewContext {
    FixedGeometry(PhysicalBehaviorContext),
    Runtime(RuntimeBehaviorContext),
}

impl From<PhysicalBehaviorContext> for BehaviorReviewContext {
    fn from(context: PhysicalBehaviorContext) -> Self {
        Self::FixedGeometry(context)
    }
}
impl From<RuntimeBehaviorContext> for BehaviorReviewContext {
    fn from(context: RuntimeBehaviorContext) -> Self {
        Self::Runtime(context)
    }
}
impl BehaviorReviewContext {
    pub fn execution_context(&self) -> WorldExecutionContext {
        match self {
            Self::FixedGeometry(context) => context.execution_context(),
            Self::Runtime(context) => context.execution_context(),
        }
    }
    pub fn is_runtime(&self) -> bool {
        matches!(self, Self::Runtime(_))
    }
    pub fn law_revisions(&self) -> Vec<BlueprintRevisionId> {
        self.execution_context()
            .laws
            .values()
            .map(|id| BlueprintRevisionId::new(id.clone()).expect("typed context law ID"))
            .collect()
    }
}
