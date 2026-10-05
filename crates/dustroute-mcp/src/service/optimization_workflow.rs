//! Publish candidates from immutable planning: save the draft before recording
//! operation history. Neither planning nor publication can write a world.
use super::{StoredCircuit, StoredRepairPlan, repair_workflow};
use crate::api::McpErrorCode;
use crate::operations::mutation::UnrecordedFailure;
use crate::operations::preview::optimization::WireObjective;
use crate::operations::preview::optimization::{
    MacroOptimizationPreview, WireOptimizationCandidate, WireOptimizationPreview,
};
use crate::state::PlanStateStore;
use crate::{McpPolicy, OperationKind, OperationRegistry};
use dustroute_app::DustRouteService;
use dustroute_optimize::{OptimizationContract, PhysicalOptimizationSearchBudget};

mod planning;
#[cfg(test)]
mod tests;

pub(super) struct OptimizationWorkflow<'a> {
    pub policy: &'a McpPolicy,
    pub state_store: &'a PlanStateStore,
    pub app: &'a DustRouteService,
    pub operations: &'a OperationRegistry,
}
impl OptimizationWorkflow<'_> {
    fn planner(&self) -> planning::OptimizationPlanner<'_> {
        planning::OptimizationPlanner {
            policy: self.policy,
            app: self.app,
        }
    }
    async fn store_repair_plan(
        &self,
        id: uuid::Uuid,
        plan: StoredRepairPlan,
    ) -> Result<(), String> {
        repair_workflow::RepairPlans(self.state_store).save(id, &plan)
    }
    pub(super) async fn propose_macro(
        &self,
        circuit_id: uuid::Uuid,
        circuit: StoredCircuit,
        component_id: String,
        contract: OptimizationContract,
    ) -> Result<MacroOptimizationPreview, UnrecordedFailure> {
        let planning::PreparedMacro { plan, preview } =
            self.planner()
                .prepare_macro(circuit_id, circuit, component_id, contract)?;
        self.store_repair_plan(preview.operation_id, plan)
            .await
            .map_err(|error| UnrecordedFailure::coded(McpErrorCode::Internal, error, false))?;
        self.operations
            .record_completed(
                preview.operation_id,
                OperationKind::OptimizationProposal,
                preview.candidate.clone().into(),
            )
            .await;
        Ok(preview)
    }

    pub(super) async fn propose_wire(
        &self,
        circuit_id: uuid::Uuid,
        circuit: StoredCircuit,
        focus: dustroute_translate::world_reverse::RegionBounds,
        objective: WireObjective,
        contract: OptimizationContract,
        search_budget: PhysicalOptimizationSearchBudget,
    ) -> Result<WireOptimizationPreview, UnrecordedFailure> {
        let planning::PreparedWire { plan, preview } = self.planner().prepare_wire(
            circuit_id,
            circuit,
            focus,
            objective,
            contract,
            search_budget,
        )?;
        self.store_repair_plan(preview.operation_id, plan)
            .await
            .map_err(|error| UnrecordedFailure::coded(McpErrorCode::Internal, error, false))?;
        let candidate = WireOptimizationCandidate::new(
            circuit_id,
            focus,
            preview.patch.clone(),
            preview.verification.semantic.clone(),
            contract,
            preview.contract_assessment.0.clone(),
        );
        self.operations
            .record_completed(
                preview.operation_id,
                OperationKind::OptimizationProposal,
                candidate.into(),
            )
            .await;
        Ok(preview)
    }
}
