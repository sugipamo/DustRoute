//! One shared live executor for full construction and differential edits.
//! Callers persist intent and consume the capability before entering here.
use crate::assembly_registry::TargetServer;
use crate::bridge_protocol::CommandWrite;
use crate::observation_evidence::ObservationEvidence;
use crate::piston_assembly::ValidatedAssemblyPlacement;
use crate::{BotBridge, McpPolicy};
use dustroute_translate::piston_construction::{ElectricalConstructionStep, construction_batches};
use dustroute_translate::snapshot::MinecraftSnapshot;
use dustroute_translate::world_reverse::RegionBounds;

pub(super) enum StageProgress {
    Readback(Box<ObservationEvidence>),
    Verified(usize),
}

pub(super) fn batch_summary(steps: &[ElectricalConstructionStep]) -> serde_json::Value {
    serde_json::json!(
        construction_batches(steps)
            .map(|batch| serde_json::json!({
                "first_step":batch.first_step(),"last_step":batch.last_step(),
                "changed_blocks":batch.steps().len(),"wait_ticks":batch.wait_ticks(),
                "full_region_readback_before_and_after":true,
            }))
            .collect::<Vec<_>>()
    )
}

pub(super) struct ConstructionExecutor<'a> {
    pub bridge: &'a BotBridge,
    pub policy: &'a McpPolicy,
    pub target: &'a TargetServer,
}
impl ConstructionExecutor<'_> {
    pub async fn execute(
        &self,
        baseline: &MinecraftSnapshot,
        steps: &[ElectricalConstructionStep],
        mut checkpoint: impl FnMut(StageProgress) -> Result<(), String>,
    ) -> Result<(), String> {
        let bounds = RegionBounds::new(baseline.min, baseline.max);
        self.policy
            .authorize_mutation()
            .map_err(|e| e.to_string())?;
        self.policy
            .authorize_dimension(&self.target.dimension)
            .map_err(|e| e.to_string())?;
        self.policy
            .validate_region(bounds)
            .map_err(|e| e.to_string())?;
        self.policy
            .validate_placement_size(steps.len())
            .map_err(|e| e.to_string())?;
        let mut expected = baseline;
        for batch in construction_batches(steps) {
            let result: Result<(), String> = async {
                let writes = batch
                    .steps()
                    .iter()
                    .map(|step| {
                        if !bounds.contains(step.position)
                            || step.expected.min != bounds.min
                            || step.expected.max != bounds.max
                        {
                            return Err("construction step escapes the reviewed region".into());
                        }
                        Ok(CommandWrite {
                            pos: step.position,
                            state: step.state.parse()?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let status = self.bridge.status().await.map_err(|e| e.to_string())?;
                super::assembly_placement::server_contract(&status, &self.target.dimension)?;
                self.target.check(&status)?;
                let before = self
                    .bridge
                    .scan_region_fresh(bounds.min, bounds.max, &self.target.dimension)
                    .await
                    .map_err(|e| e.to_string())?
                    .into_stationary_record()?;
                ValidatedAssemblyPlacement::matches(&before.snapshot, expected, &status.version)?;
                {
                    let _measurement =
                        crate::performance::span(crate::performance::Phase::Checkpoint);
                    checkpoint(StageProgress::Readback(Box::new(before.readback)))?;
                }
                self.bridge
                    .write_blocks(&writes, &self.target.dimension)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut remaining = batch.wait_ticks();
                while remaining > 0 {
                    let ticks = remaining.min(200) as u16;
                    self.bridge
                        .wait_ticks(ticks, &self.target.dimension)
                        .await
                        .map_err(|e| e.to_string())?;
                    remaining -= u64::from(ticks);
                }
                let after = self
                    .bridge
                    .scan_region_fresh(bounds.min, bounds.max, &self.target.dimension)
                    .await
                    .map_err(|e| e.to_string())?
                    .into_stationary_record()?;
                {
                    let _measurement =
                        crate::performance::span(crate::performance::Phase::Checkpoint);
                    checkpoint(StageProgress::Readback(Box::new(after.readback)))?;
                }
                let status = self.bridge.status().await.map_err(|e| e.to_string())?;
                self.target.check(&status)?;
                ValidatedAssemblyPlacement::matches(
                    &after.snapshot,
                    batch.expected(),
                    &status.version,
                )?;
                {
                    let _measurement =
                        crate::performance::span(crate::performance::Phase::Checkpoint);
                    checkpoint(StageProgress::Verified(batch.last_step()))?;
                }
                Ok(())
            }
            .await;
            result.map_err(|error| {
                format!(
                    "construction batch steps {}..{}: {error}",
                    batch.first_step(),
                    batch.last_step()
                )
            })?;
            expected = batch.expected();
        }
        Ok(())
    }
}
