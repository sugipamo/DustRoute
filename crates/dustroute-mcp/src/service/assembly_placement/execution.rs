//! Consume one reviewed attempt, verify model batches and persist partial progress.
use super::*;
use crate::operations::construction::{AssemblyConstructionDetails, AssemblyConstructionResult};

impl AssemblyService<'_> {
    pub(in crate::service) async fn mutate_assembly_construction(
        &self,
        id: uuid::Uuid,
        confirm: bool,
        undo: bool,
    ) -> AssemblyConstructionResult {
        let mut progress = ExecutionProgress::default();
        let result: Result<AssemblyConstructionResult, FailureCause> = async {
            if !confirm {
                return Err(FailureCause::new(
                    CauseKind::InvalidInput,
                    "confirm=true is required",
                ));
            }
            self.policy
                .authorize_mutation()
                .map_err(FailureCause::from)?;
            let queue = crate::performance::span(crate::performance::Phase::MutationQueue);
            let _guard = self.mutation_lock.lock().await;
            drop(queue);
            let plan = self.owned_assembly_plan(id, None).await?;
            if undo && plan.instance().is_some() {
                return Err(FailureCause::new(
                    CauseKind::InvalidState,
                    "use a new reviewed removal or placement for this instance operation",
                ));
            }
            if (undo && plan.state != PistonPlacementState::Applied)
                || (!undo
                    && (plan.state != PistonPlacementState::Planned
                        || !plan.previewed
                        || plan.expires_at <= Instant::now()))
            {
                return Err(FailureCause::new(
                    CauseKind::InvalidState,
                    "construction needs an unused preview; undo needs a verified application",
                ));
            }
            let removal = undo || plan.is_removal();
            let instance_id = plan.instance().map_or(id, |(instance_id, _)| instance_id);
            let registry = RegistryLock::acquire(self.state_store)?;
            let basis = self
                .construction_basis(&plan.player, plan.assembly_id.clone())
                .await?;
            if !plan.source_identity.matches(&basis) {
                return Err(FailureCause::new(
                    CauseKind::InvalidState,
                    "adopted source or context changed; create a new construction plan",
                ));
            }
            let transform = plan.transform;
            let reconstruct_from = plan.reconstruction().map(|r| r.baseline.clone());
            let remove_from = plan.operating_removal().map(|r| r.baseline.clone());
            progress.phase = FailurePhase::ModelProof;
            crate::performance::execution_progress(&progress);
            let capture = crate::performance::current();
            let queue = crate::performance::span(crate::performance::Phase::ModelQueue);
            let (proof, steps) = tokio::task::spawn_blocking(move || {
                capture.in_blocking(|| {
                    let _phase = crate::performance::span(crate::performance::Phase::ModelProof);
                    let proof = proof_from_basis(&basis, transform)?;
                    let steps = if let Some(baseline) = reconstruct_from {
                        proof.reconstruction_steps(&baseline)?
                    } else if let Some(baseline) = remove_from {
                        proof.operating_removal(&baseline)?.steps
                    } else {
                        proof.steps(removal).to_vec()
                    };
                    Ok::<_, String>((proof, steps))
                })
            })
            .await
            .map_err(|e| FailureCause::new(CauseKind::Unknown, e.to_string()))??;
            drop(queue);
            let bounds = proof.bounds();
            self.policy
                .authorize_dimension(&plan.dimension)
                .map_err(FailureCause::from)?;
            self.policy
                .validate_region(bounds)
                .map_err(FailureCause::from)?;
            self.policy
                .validate_placement_size(steps.len())
                .map_err(FailureCause::from)?;
            if proof.assembly() != plan.proof.assembly()
                || proof.context() != plan.proof.context()
                || steps != plan.steps(undo)
            {
                return Err(FailureCause::new(
                    CauseKind::InvalidState,
                    "fresh construction/removal differs from the preview; replan",
                ));
            }
            progress.phase = FailurePhase::BeforeReadback;
            crate::performance::execution_progress(&progress);
            let status = self.bridge.status().await.map_err(FailureCause::from)?;
            server_contract(&status, &plan.dimension)?;
            plan.target.check(&status)?;
            let mut record = if removal || plan.reconstruction().is_some() {
                let record = registry.get(instance_id, &plan.player)?;
                if (removal && record.state != InstanceState::Applied)
                    || record.state == InstanceState::Removed
                    || plan
                        .instance()
                        .is_some_and(|(_, revision)| record.revision != revision)
                {
                    return Err(FailureCause::new(
                        CauseKind::InvalidState,
                        "placed Assembly record changed or is not applied; reobserve and replan",
                    ));
                }
                if record.source_identity != plan.source_identity
                    || record.target != plan.target
                    || &record.assembly != proof.assembly()
                    || &record.context != proof.context()
                    || record.transform != plan.transform
                {
                    return Err(FailureCause::new(
                        CauseKind::InvalidState,
                        "placed Assembly pins differ from the removal plan",
                    ));
                }
                ValidatedAssemblyPlacement::matches(
                    &record.expected,
                    proof.settled(),
                    &status.version,
                )?;
                let observation = self.observe_instance(&record).await;
                if let Some(reconstruction) = plan.reconstruction() {
                    let actual = observation::stable_baseline(&observation)?;
                    ValidatedAssemblyPlacement::matches(
                        &actual,
                        &reconstruction.baseline,
                        &status.version,
                    )?;
                } else if let Some(operating) = plan.operating_removal() {
                    let actual = observation::stable_baseline(&observation)?;
                    ValidatedAssemblyPlacement::matches(
                        &actual,
                        &operating.baseline,
                        &status.version,
                    )?;
                } else if let Some(cause) = observation.refusal() {
                    return Err(cause);
                }
                record
            } else {
                PlacedAssembly {
                    schema: PlacedAssembly::schema(),
                    instance_id,
                    revision: 0,
                    player: plan.player.clone(),
                    assembly_id: plan.assembly_id.clone(),
                    source_identity: plan.source_identity.clone(),
                    transform: plan.transform,
                    target: plan.target.clone(),
                    assembly: proof.assembly().clone(),
                    context: proof.context().clone(),
                    expected: proof.settled().clone(),
                    state: InstanceState::NeedsInspection,
                    attempts: vec![],
                    last_observation: None,
                    updated_at_unix_ms: now_ms()?,
                }
            };
            progress.phase = FailurePhase::BeforeReadback;
            crate::performance::execution_progress(&progress);
            let baseline_readback = self
                .bridge
                .scan_region_fresh(bounds.min, bounds.max, &plan.dimension)
                .await
                .map_err(FailureCause::from)?
                .into_stationary_record()?;
            let baseline = baseline_readback.snapshot;
            if let Some(reconstruction) = plan.reconstruction() {
                ValidatedAssemblyPlacement::matches(
                    &baseline,
                    &reconstruction.baseline,
                    &status.version,
                )?;
            } else if let Some(operating) = plan.operating_removal() {
                ValidatedAssemblyPlacement::matches(
                    &baseline,
                    &operating.baseline,
                    &status.version,
                )?;
            } else {
                proof.validate_before(&baseline, &status.version, removal)?;
            }
            if !undo && plan.expires_at <= Instant::now() {
                return Err(FailureCause::new(
                    CauseKind::InvalidState,
                    "construction expired during validation",
                ));
            }
            progress.phase = FailurePhase::IntentSave;
            crate::performance::execution_progress(&progress);
            self.plans
                .table::<assembly_placement::StoredAssemblyPlacement>()
                .lock()
                .await
                .get_mut(&id)
                .ok_or("construction missing")?
                .state = PistonPlacementState::NeedsInspection;
            progress.operation_consumed = true;
            record.state = InstanceState::NeedsInspection;
            record.last_observation = None;
            record.attempts.push(Attempt {
                operation_id: id,
                removal,
                reconstruction: plan.reconstruction().cloned(),
                operating_removal: plan.operating_removal().cloned(),
                verified_steps: 0,
                total_steps: steps.len(),
                started_at_unix_ms: now_ms()?,
                finished_at_unix_ms: None,
                error: None,
                readbacks: vec![baseline_readback.readback],
                failure: None,
                progress: Some(progress.clone()),
            });
            registry.save(&mut record).map_err(|e| {
                progress.persistence = PersistenceOutcome::Uncertain;
                FailureCause::new(CauseKind::Persistence, e)
            })?;
            progress.persistence = PersistenceOutcome::IntentSaved;
            let mut completed = 0;
            let mut run = async {
                let execution_progress =
                    super::super::construction_executor::ConstructionExecutor {
                        bridge: self.bridge,
                        policy: self.policy,
                        target: &plan.target,
                    }
                    .execute(&baseline, &steps, |progress| {
                        let attempt = record
                            .attempts
                            .last_mut()
                            .ok_or("missing durable attempt")?;
                        match progress {
                            super::super::construction_executor::StageProgress::Readback(
                                receipt,
                            ) => attempt.readbacks.push(*receipt),
                            super::super::construction_executor::StageProgress::WriteIntent(
                                intent,
                            ) => attempt.progress = Some(intent),
                            super::super::construction_executor::StageProgress::Verified(count) => {
                                completed = count;
                                attempt.verified_steps = count;
                            }
                        }
                        registry
                            .save(&mut record)
                            .map_err(|e| FailureCause::new(CauseKind::Persistence, e))
                    })
                    .await?;
                let expected = steps.last().map_or_else(
                    || baseline.to_owned_snapshot(),
                    |step| step.expected.materialize(),
                );
                proof
                    .validate_after(&expected, &status.version, removal)
                    .map_err(|e| {
                        execution_progress
                            .cause(FailureCause::new(CauseKind::VerificationMismatch, e))
                    })?;
                Ok::<_, FailureReport>(execution_progress)
            }
            .await;
            progress = match &run {
                Ok(p) => p.clone(),
                Err(report) => (*report.progress).clone(),
            };
            record.state = if run.is_ok() {
                if removal {
                    InstanceState::Removed
                } else {
                    InstanceState::Applied
                }
            } else {
                InstanceState::NeedsInspection
            };
            let attempt = record
                .attempts
                .last_mut()
                .ok_or("missing durable attempt")?;
            attempt.finished_at_unix_ms = Some(now_ms()?);
            attempt.error = run.as_ref().err().map(ToString::to_string);
            attempt.failure = run.as_ref().err().cloned();
            attempt.progress = Some(progress.clone());
            if let Err(error) = registry.save(&mut record) {
                crate::failure::persistence_failed(&mut run, error);
            }
            if run.is_ok() {
                self.plans
                    .table::<assembly_placement::StoredAssemblyPlacement>()
                    .lock()
                    .await
                    .get_mut(&id)
                    .ok_or("construction missing")?
                    .state = if removal {
                    PistonPlacementState::Undone
                } else {
                    PistonPlacementState::Applied
                };
            }
            if let Ok(result) = &mut run {
                result.phase = FailurePhase::FinalSave;
                result.persistence = PersistenceOutcome::FinalSaved;
            }
            progress = match &run {
                Ok(p) => p.clone(),
                Err(report) => (*report.progress).clone(),
            };
            let response = AssemblyConstructionResult::completed(
                id,
                AssemblyConstructionDetails {
                    instance_id,
                    record_revision: record.revision,
                    undo: removal,
                    kind: plan.kind(),
                    verified_steps: completed,
                    total_steps: steps.len(),
                    reconstruction_conditions: plan
                        .reconstruction()
                        .map(|_| reconstruction::conditions()),
                    automatic_rollback: false,
                    bounds: bounds.into(),
                },
                run,
            );
            self.operations
                .record_completed(
                    id,
                    if removal {
                        OperationKind::PlacementUndo
                    } else {
                        OperationKind::PlacementApply
                    },
                    response.clone().into(),
                )
                .await;
            Ok(response)
        }
        .await;
        crate::performance::execution_progress(&progress);
        result.unwrap_or_else(|e| AssemblyConstructionResult::failed_attempt(id, progress.cause(e)))
    }
}
