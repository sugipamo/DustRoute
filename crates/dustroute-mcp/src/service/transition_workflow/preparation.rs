//! Rechecks preview, lifecycle and live state before any lever operation.
use super::{PreparedRun, TransitionSession};
use crate::TransitionSafety;
use crate::api::McpErrorCode;
use crate::failure::FailureCause;
use crate::operations::mutation::UnrecordedFailure;
use crate::operations::transition::TransitionRefusal;
use crate::service::world_from_snapshot_for_service;
use uuid::Uuid;

impl TransitionSession<'_> {
    pub async fn prepare_run(
        &self,
        operation_id: Uuid,
    ) -> Result<PreparedRun<'_>, TransitionRefusal> {
        let plan = self.plan(operation_id).await?;
        if self.policy.preview_required && !plan.lifecycle.is_previewed() {
            return Err(TransitionRefusal::coded(
                McpErrorCode::InvalidState,
                "show_operation is required first",
            ));
        }
        if plan.lifecycle.attempted() {
            return Err(UnrecordedFailure::message(
                "scenario was already executed; create and preview a new scenario",
            )
            .into());
        }
        if plan.safety.safety != TransitionSafety::Ready {
            return Err(TransitionRefusal::Safety(Box::new(plan.safety)));
        }
        let current = self
            .bridge
            .get_block(plan.lever, &plan.dimension)
            .await
            .map_err(FailureCause::from)?;
        let current_powered = current
            .state
            .properties
            .get("powered")
            .and_then(|value| value.parse::<bool>().ok());
        if current.state.name != "minecraft:lever" || current_powered != Some(plan.original_powered)
        {
            return Err(UnrecordedFailure::message(
                "lever state changed since proposal; create a new scenario",
            )
            .into());
        }
        let current_snapshot = self
            .bridge
            .scan_region(plan.bounds.min, plan.bounds.max, &plan.dimension)
            .await
            .map_err(FailureCause::from)?;
        if current_snapshot != plan.initial_snapshot {
            return Err(UnrecordedFailure::message(
                "transition region changed since preview; create a new scenario",
            )
            .into());
        }
        let world = match world_from_snapshot_for_service(&current_snapshot) {
            Ok(world) => world,
            Err(error) => return Err(UnrecordedFailure::message(error).into()),
        };
        let world = match dustroute_translate::world::ValidatedWorld::try_from(world) {
            Ok(world) => world,
            Err(error) => {
                return Err(TransitionRefusal::Validation(error));
            }
        };
        let mut analysis =
            dustroute_translate::world_reverse::analyze_world_region(&world, plan.bounds);
        analysis.scene.observation.dimension = plan.dimension.clone();
        Ok(PreparedRun {
            session: self,
            operation_id,
            plan,
            scene: analysis.scene,
        })
    }
}
