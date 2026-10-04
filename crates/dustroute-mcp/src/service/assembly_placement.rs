//! Public Assembly planning, preview and persistent-instance orchestration.
mod diagnosis;
mod execution;
mod observation;
mod reconstruction;
#[cfg(test)]
mod typed_report_tests;
mod validation;

use super::*;
use crate::assembly_registry::{
    Attempt, InstanceState, PlacedAssembly, RegistryLock, TargetServer, now_ms,
};
use crate::operations::construction::AssemblyConstructionKind;
use crate::operations::mutation::Success;
use crate::operations::preview::{
    AssemblyConstructionPreview, AssemblyManagementReport, AssemblyPlanDetails, AssemblyPlanState,
    AssemblyPlanSteps, AssemblyPreview, AssemblyRemovalPreview, InstanceDetails, RemovalReference,
    ShownAssemblyPlan,
};
use crate::piston_assembly::ValidatedAssemblyPlacement;
use crate::source_identity::SourceIdentity;
use dustroute_library::blueprint::AssemblyRevisionId;
use dustroute_translate::assembly_transform::AssemblyTransform;
use rmcp::schemars;
use validation::proof_from_basis;
pub(super) use validation::server_contract;

#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct AssemblyPlacementTarget {
    /// Source coordinate that maps to target_anchor. No source definition is edited.
    source_anchor: CoordinateParam,
    target_anchor: CoordinateParam,
    #[serde(default)]
    rotation: dustroute_translate::cells::RotationY,
}

impl AssemblyPlacementTarget {
    fn transform(&self) -> AssemblyTransform {
        let p = |c: CoordinateParam| Pos::new(c.x, c.y, c.z);
        AssemblyTransform {
            source_anchor: p(self.source_anchor),
            target_anchor: p(self.target_anchor),
            rotation: self.rotation,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct StoredAssemblyPlacement {
    player: String,
    dimension: String,
    assembly_id: AssemblyRevisionId,
    source_identity: SourceIdentity,
    proof: ValidatedAssemblyPlacement,
    previewed: bool,
    state: PistonPlacementState,
    expires_at: Instant,
    transform: AssemblyTransform,
    target: TargetServer,
    action: AssemblyPlacementAction,
}

#[derive(Clone, Debug)]
enum AssemblyPlacementAction {
    Construct,
    Remove {
        instance_id: uuid::Uuid,
        revision: u64,
        operating: Option<Box<crate::assembly_registry::OperatingRemoval>>,
    },
    Reconstruct {
        instance_id: uuid::Uuid,
        revision: u64,
        plan: Box<crate::assembly_registry::ReconstructionAttempt>,
    },
}

impl StoredAssemblyPlacement {
    fn report_steps(&self) -> AssemblyPlanSteps {
        AssemblyPlanSteps::new(
            &self.proof,
            self.steps(false),
            self.steps(true),
            self.operating_removal(),
            self.reconstruction(),
        )
    }

    fn instance(&self) -> Option<(uuid::Uuid, u64)> {
        match self.action {
            AssemblyPlacementAction::Construct => None,
            AssemblyPlacementAction::Remove {
                instance_id,
                revision,
                ..
            }
            | AssemblyPlacementAction::Reconstruct {
                instance_id,
                revision,
                ..
            } => Some((instance_id, revision)),
        }
    }
    fn reconstruction(&self) -> Option<&crate::assembly_registry::ReconstructionAttempt> {
        match &self.action {
            AssemblyPlacementAction::Reconstruct { plan, .. } => Some(plan),
            _ => None,
        }
    }
    fn is_removal(&self) -> bool {
        matches!(self.action, AssemblyPlacementAction::Remove { .. })
    }
    fn operating_removal(&self) -> Option<&crate::assembly_registry::OperatingRemoval> {
        match &self.action {
            AssemblyPlacementAction::Remove { operating, .. } => operating.as_deref(),
            _ => None,
        }
    }
    fn kind(&self) -> AssemblyConstructionKind {
        match self.action {
            AssemblyPlacementAction::Construct => AssemblyConstructionKind::Construct,
            AssemblyPlacementAction::Remove { .. } => AssemblyConstructionKind::Remove,
            AssemblyPlacementAction::Reconstruct { .. } => AssemblyConstructionKind::Reconstruct,
        }
    }
    fn steps(
        &self,
        undo: bool,
    ) -> &[dustroute_translate::piston_construction::ElectricalConstructionStep] {
        self.reconstruction().map_or_else(
            || {
                self.operating_removal().map_or_else(
                    || self.proof.steps(undo || self.is_removal()),
                    |r| r.steps.as_slice(),
                )
            },
            |r| r.steps.as_slice(),
        )
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ManageAssemblyParams {
    action: AssemblyManagementAction,
    /// UUID returned as instance_id by construction. Required except for list.
    instance_id: Option<String>,
    /// Explicitly review removal from the settled reference for observed inputs.
    /// Only valid with plan_removal. Does not infer live runtime history.
    #[serde(default)]
    removal_reference: RemovalReference,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AssemblyManagementAction {
    List,
    Get,
    Observe,
    /// Compare the live layout with the design, independently of repair eligibility.
    Diagnose,
    PlanRemoval,
    /// Tear down the observed supported layout and rebuild its declared initial state.
    PlanReconstruction,
}

/// Durable Assembly workflow; no MCP router, selection map or circuit cache.
pub(super) struct AssemblyService<'a> {
    pub bridge: &'a BotBridge,
    pub policy: &'a McpPolicy,
    pub state_store: &'a PlanStateStore,
    pub plans: &'a OperationPlans,
    pub operations: &'a OperationRegistry,
    pub mutation_lock: &'a Mutex<()>,
    pub player_scope: player_scope::PlayerScope<'a>,
}

impl AssemblyService<'_> {
    async fn rebuild_instance_proof(
        &self,
        record: &PlacedAssembly,
    ) -> Result<ValidatedAssemblyPlacement, String> {
        let basis = self
            .construction_basis(&record.player, record.assembly_id.clone())
            .await?;
        if !record.source_identity.matches(&basis) {
            return Err("saved source/adoption/context differs from the current catalog; no automatic revision replacement".into());
        }
        let transform = record.transform;
        let proof = tokio::task::spawn_blocking(move || proof_from_basis(&basis, transform))
            .await
            .map_err(|e| e.to_string())??;
        if proof.assembly() != &record.assembly || proof.context() != &record.context {
            return Err(
                "saved target Assembly/context does not match freshly transformed source".into(),
            );
        }
        ValidatedAssemblyPlacement::matches(
            &record.expected,
            proof.settled(),
            &record.target.version,
        )?;
        Ok(proof)
    }

    pub(super) async fn manage_placed_assembly(
        &self,
        params: ManageAssemblyParams,
    ) -> Result<AssemblyManagementReport, FailureCause> {
        let player = self.player_scope.resolve(None)?;
        if let Some(error) = self.player_scope.authorize(&player).err() {
            return Err(error);
        }
        let registry = RegistryLock::acquire(self.state_store)?;
        let reconstruct = matches!(params.action, AssemblyManagementAction::PlanReconstruction);
        let diagnose = matches!(params.action, AssemblyManagementAction::Diagnose);
        let operating = params.removal_reference == RemovalReference::ObservedInputs;
        if operating && !matches!(params.action, AssemblyManagementAction::PlanRemoval) {
            return Err("removal_reference=observed_inputs requires plan_removal".into());
        }
        let (observe, removal) = match params.action {
            AssemblyManagementAction::List => {
                if params.instance_id.is_some() {
                    return Err("list does not take an instance_id".into());
                }
                return Ok(AssemblyManagementReport::List {
                    ok: Success,
                    instances: registry.list(&player)?,
                    fresh_observation: false,
                });
            }
            AssemblyManagementAction::Get => (false, false),
            AssemblyManagementAction::Observe => (true, false),
            AssemblyManagementAction::Diagnose => (true, false),
            AssemblyManagementAction::PlanRemoval => (true, true),
            AssemblyManagementAction::PlanReconstruction => (true, false),
        };
        let instance_id = params.instance_id.ok_or("instance_id required")?;
        let id = uuid::Uuid::parse_str(&instance_id).map_err(|e| e.to_string())?;
        let mut record = registry.get(id, &player)?;
        if !observe {
            return Ok(AssemblyManagementReport::Get(Box::new(
                InstanceDetails::new(record),
            )));
        }
        let proof = self.rebuild_instance_proof(&record).await;
        let observation = self.observe_instance(&record).await;
        let eligible = record.state == InstanceState::Applied
            && proof.is_ok()
            && observation.matches_reference();
        let report = crate::recorded_instance::RecordedInstanceReport {
            observation: observation.recorded(),
            revalidation: match &proof {
                Ok(proof) => crate::recorded_instance::RecordedRevalidation::Passed {
                    fresh_target_review: Box::new(proof.review().recorded()),
                },
                Err(error) => crate::recorded_instance::RecordedRevalidation::Failed {
                    reason: error.clone(),
                },
            },
            removal_eligible: eligible,
            diagnosis: if diagnose || reconstruct {
                Some(self.diagnose_instance(&record, &proof, &observation).await)
            } else {
                None
            },
        };
        record.last_observation = Some(report.clone());
        registry.save(&mut record)?;
        if reconstruct {
            let diagnosis = report.diagnosis.clone().map(Box::new);
            let attempt = async {
                self.plan_assembly_reconstruction(record, proof?, observation, report)
                    .await
            }
            .await;
            return Ok(match attempt {
                Ok(plan) => AssemblyManagementReport::Reconstruction {
                    plan: Box::new(plan),
                    diagnosis,
                },
                Err(error) => AssemblyManagementReport::ReconstructionRefused {
                    ok: false,
                    error,
                    diagnosis,
                },
            });
        }
        if diagnose {
            return Ok(AssemblyManagementReport::Diagnosed {
                ok: Success,
                instance: Box::new(record.summary()),
                diagnosis: report.diagnosis.clone().map(Box::new),
                observation: Box::new(report),
            });
        }
        if !removal || (!operating && !eligible) {
            return Ok(AssemblyManagementReport::Observed {
                ok: !removal,
                instance: Box::new(record.summary()),
                observation: Box::new(report),
                error: if removal {
                    Some(
                        "conditional removal requires an applied record, matching observation and fresh passing review",
                    )
                } else {
                    None
                },
            });
        }
        let proof = proof?;
        if record.state != InstanceState::Applied {
            return Err("removal requires an applied instance".into());
        }
        let operating_plan = if operating {
            let baseline = observation::stable_baseline(&observation)?;
            let model = proof.clone();
            Some(Box::new(
                tokio::task::spawn_blocking(move || model.operating_removal(&baseline))
                    .await
                    .map_err(|e| e.to_string())??,
            ))
        } else {
            None
        };
        let steps = operating_plan
            .as_ref()
            .map_or_else(|| proof.steps(true), |p| p.steps.as_slice());
        self.policy
            .validate_placement_size(steps.len())
            .map_err(|e| e.to_string())?;
        let operation_id = uuid::Uuid::new_v4();
        let response = AssemblyRemovalPreview::new(
            operation_id,
            &record,
            params.removal_reference,
            operating_plan.as_deref(),
            &proof,
            report,
            self.policy.read_only,
        );
        let mut plans = self
            .plans
            .table::<assembly_placement::StoredAssemblyPlacement>()
            .lock()
            .await;
        plans.retain(|_, p| {
            p.state != PistonPlacementState::Planned || p.expires_at > Instant::now()
        });
        if plans.len() >= 256 {
            return Err("too many retained Assembly construction plans".into());
        }
        plans.insert(
            operation_id,
            StoredAssemblyPlacement {
                player,
                dimension: record.target.dimension.clone(),
                assembly_id: record.assembly_id,
                source_identity: record.source_identity,
                proof,
                previewed: false,
                state: PistonPlacementState::Planned,
                expires_at: Instant::now() + Duration::from_secs(300),
                transform: record.transform,
                target: record.target,
                action: AssemblyPlacementAction::Remove {
                    instance_id: id,
                    revision: record.revision,
                    operating: operating_plan,
                },
            },
        );
        drop(plans);
        self.operations
            .record_completed(
                operation_id,
                OperationKind::PlacementPreview,
                AssemblyPreview::Removal(Box::new(response.clone())).into(),
            )
            .await;
        Ok(AssemblyManagementReport::Removal(Box::new(response)))
    }

    async fn construction_basis(
        &self,
        player: &str,
        id: AssemblyRevisionId,
    ) -> Result<SourceIdentity, String> {
        let store = self.state_store.clone();
        let player = player.to_owned();
        tokio::task::spawn_blocking(move || {
            crate::blueprint_mcp::construction_source(&store, &player, &id)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    pub(super) async fn plan_assembly_construction(
        &self,
        params: PreviewPlacementParams,
    ) -> Result<AssemblyConstructionPreview, FailureCause> {
        if !params.circuit.is_empty()
            || params.revision_id.is_some()
            || params.optimize.unwrap_or(false)
        {
            return Err("assembly_target requires only an adopted assembly_revision_id".into());
        }
        let id = params
            .assembly_revision_id
            .ok_or("assembly_revision_id required")?;
        let target = params.assembly_target.ok_or("assembly_target required")?;
        let player = self.player_scope.resolve(params.player.as_deref())?;
        if let Some(error) = self.player_scope.authorize(&player).err() {
            return Err(error);
        }
        let basis = self.construction_basis(&player, id.clone()).await?;
        let SourceIdentity {
            record,
            context,
            catalog,
            ..
        } = basis.clone();
        let count = record.assembly.blocks.len();
        if count == 0 || count > 256 || params.max_blocks.is_some_and(|n| count > n) {
            return Err("custom construction budget is 1 through 256 block records, also bounded by max_blocks".into());
        }
        self.policy
            .validate_placement_size(count)
            .map_err(|e| e.to_string())?;
        // The assisted player's dimension is observed, not supplied by the request.
        let observation = self
            .bridge
            .observe_player(&player, 64.0)
            .await
            .map_err(|e| e.to_string())?;
        let dimension = observation.dimension;
        self.policy
            .authorize_dimension(&dimension)
            .map_err(|e| e.to_string())?;
        let region = target.transform().region(context.known_region)?;
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(region.min, region.max);
        self.policy
            .validate_region(bounds)
            .map_err(|e| e.to_string())?;
        let status = self.bridge.status().await.map_err(|e| e.to_string())?;
        server_contract(&status, &dimension)?;
        let baseline = self
            .bridge
            .scan_region(bounds.min, bounds.max, &dimension)
            .await
            .map_err(|e| e.to_string())?;
        let transform = target.transform();
        let target_server = TargetServer::observed(&status, &dimension)?;
        let proof = tokio::task::spawn_blocking(move || {
            ValidatedAssemblyPlacement::new(
                &catalog,
                &record.assembly,
                &context,
                transform,
                &baseline,
                &status.version,
            )
        })
        .await
        .map_err(|e| e.to_string())??;
        self.policy
            .validate_placement_size(proof.steps(true).len().max(proof.steps(false).len()))
            .map_err(|e| e.to_string())?;
        let operation_id = uuid::Uuid::new_v4();
        let response = AssemblyConstructionPreview::new(
            operation_id,
            id.clone(),
            &basis,
            dimension.clone(),
            self.policy.read_only,
            &proof,
        );
        let mut plans = self
            .plans
            .table::<assembly_placement::StoredAssemblyPlacement>()
            .lock()
            .await;
        plans.retain(|_, p| {
            p.state != PistonPlacementState::Planned || p.expires_at > Instant::now()
        });
        if plans.len() >= 256 {
            return Err("too many retained Assembly construction plans".into());
        }
        plans.insert(
            operation_id,
            StoredAssemblyPlacement {
                player,
                dimension,
                assembly_id: id,
                source_identity: basis,
                proof,
                previewed: false,
                state: PistonPlacementState::Planned,
                expires_at: Instant::now() + Duration::from_secs(300),
                transform,
                target: target_server,
                action: AssemblyPlacementAction::Construct,
            },
        );
        drop(plans);
        self.operations
            .record_completed(
                operation_id,
                OperationKind::PlacementPreview,
                AssemblyPreview::Construction(Box::new(response.clone())).into(),
            )
            .await;
        Ok(response)
    }

    async fn owned_assembly_plan(
        &self,
        id: uuid::Uuid,
        player: Option<&str>,
    ) -> Result<StoredAssemblyPlacement, FailureCause> {
        let player = self.player_scope.resolve(player)?;
        if let Some(error) = self.player_scope.authorize(&player).err() {
            return Err(error);
        }
        let plan = self
            .plans
            .table::<assembly_placement::StoredAssemblyPlacement>()
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| FailureCause::new(CauseKind::NotFound, "construction plan not found"))?;
        if plan.player != player {
            return Err(FailureCause::new(
                CauseKind::PermissionDenied,
                "construction belongs to another player",
            ));
        }
        Ok(plan)
    }

    pub(super) async fn get_assembly_construction(
        &self,
        id: uuid::Uuid,
    ) -> Result<AssemblyPlanDetails, FailureCause> {
        let plan = self.owned_assembly_plan(id, None).await?;
        Ok(AssemblyPlanDetails {
            ok: Success,
            operation_id: id,
            kind: plan.kind(),
            assembly_revision_id: plan.assembly_id.clone(),
            bounds: plan.proof.bounds(),
            proposed_assembly: plan.proof.assembly().clone(),
            steps: plan.report_steps(),
            previewed: plan.previewed,
            state: match plan.state {
                PistonPlacementState::Planned => AssemblyPlanState::Planned,
                PistonPlacementState::Applied => AssemblyPlanState::Applied,
                PistonPlacementState::Undone => AssemblyPlanState::Undone,
                PistonPlacementState::NeedsInspection => AssemblyPlanState::NeedsInspection,
            },
            stored_history_is_validation_proof: false,
        })
    }

    pub(super) async fn show_assembly_construction(
        &self,
        id: uuid::Uuid,
        player: Option<&str>,
    ) -> Result<ShownAssemblyPlan, FailureCause> {
        let plan = self.owned_assembly_plan(id, player).await?;
        if plan.state != PistonPlacementState::Planned || plan.expires_at <= Instant::now() {
            return Err("construction preview is expired or consumed".into());
        }
        let bounds = plan.proof.bounds();
        if let Some((instance_id, revision)) = plan.instance() {
            let registry = RegistryLock::acquire(self.state_store)?;
            let record = registry.get(instance_id, &plan.player)?;
            if record.revision != revision
                || (plan.is_removal() && record.state != InstanceState::Applied)
            {
                return Err("placed Assembly record changed; reobserve and replan removal".into());
            }
        }
        self.policy
            .authorize_dimension(&plan.dimension)
            .map_err(FailureCause::from)?;
        self.policy
            .validate_region(bounds)
            .map_err(FailureCause::from)?;
        let preview = self
            .bridge
            .preview_region(&plan.player, bounds.min, bounds.max, &plan.dimension)
            .await
            .map_err(FailureCause::from)?;
        self.plans
            .table::<assembly_placement::StoredAssemblyPlacement>()
            .lock()
            .await
            .get_mut(&id)
            .ok_or("construction missing")?
            .previewed = true;
        Ok(ShownAssemblyPlan {
            ok: Success,
            operation_id: id,
            preview,
            bounds,
            kind: plan.kind(),
            steps: plan.report_steps(),
        })
    }
}
