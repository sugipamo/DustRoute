//! Admission and background execution, including ownership handback or retention.
use super::*;

impl DustRouteMcp {
    pub(super) async fn start_survival(&self, id: uuid::Uuid, confirmed: bool) -> Value {
        if !confirmed {
            return failure(
                "confirmation_required",
                "review the generated preview and authorize its scope/materials",
            );
        }
        if let Err(e) = self.policy.authorize_mutation() {
            return failure("permission_denied", e);
        }
        let mut jobs = self.survival.entries.lock().await;
        let Some(entry) = jobs.get_mut(&id) else {
            return failure(
                "plan_not_live",
                "saved plans are diagnosis-only; generate a fresh plan",
            );
        };
        if let Err(e) = self.policy.authorize_player(&entry.owner) {
            return failure("permission_denied", e);
        }
        match entry.start_readiness() {
            StartReadiness::ExpiredOrCancelled => {
                entry.expire_preview();
                if let Err(e) = save(
                    &self
                        .state_store
                        .survival_job_root()
                        .join(id.to_string())
                        .join("status.json"),
                    &entry.status(),
                ) {
                    return failure("journal_io", e);
                }
                return failure("plan_expired_or_cancelled", "generate a fresh plan");
            }
            StartReadiness::AlreadyStarted => {
                return failure("job_already_started", "use get; no automatic replay");
            }
            StartReadiness::Ready => {}
        }
        let native = match self.bridge.survival_bridge() {
            Ok(n) => n,
            Err(e) => return failure("backend_unavailable", e),
        };
        let lease = match native.lease_survival() {
            Ok(l) => l,
            Err(e) => return failure("source_busy", e),
        };
        let (plan, parent) = entry.take_preview().expect("checked live preview");
        let source = entry.source.clone();
        let owner = entry.owner.clone();
        let cancel = entry.cancel.clone();
        let checkpoint = entry.checkpoint.clone();
        if let Err(e) = save(
            &self
                .state_store
                .survival_job_root()
                .join(id.to_string())
                .join("status.json"),
            &entry.status(),
        ) {
            let original = lease.source();
            lease.release(original);
            entry.publish(JobStatus::AdmissionRefused {
                failure: JobFailure::boundary(JobRefusalCode::JournalIo, &e),
                construction_dispatched: false,
            });
            return failure("journal_io", e);
        }
        drop(jobs);
        let service = self.clone();
        let worker = tokio::spawn(async move {
            service
                .run_survival(ExecutionInput {
                    id,
                    owner,
                    source,
                    plan,
                    lease,
                    cancel,
                    checkpoint,
                    parent,
                })
                .await;
        });
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(error) = worker.await {
                service
                    .set_survival_status(
                        id,
                        JobStatus::NeedsInspection {
                            reason: InspectionReason::Task {
                                failure: JobFailure::boundary(
                                    JobRefusalCode::ExecutionTaskFailed,
                                    error,
                                ),
                            },
                        },
                    )
                    .await;
            }
        });
        json!({"ok":true,"job_id":id,"state":"admitting","completed":false,"next_step":"action=get; admission can still refuse before any edits"})
    }

    async fn run_survival(&self, input: ExecutionInput) {
        let ExecutionInput {
            id,
            owner,
            source,
            plan,
            mut lease,
            cancel,
            checkpoint,
            parent,
        } = input;
        let bot = lease.source();
        let path = self.state_store.survival_job_root().join(id.to_string());
        let admission = async {
            let store = self.state_store.clone();
            let p = owner.clone();
            let original = source.clone();
            tokio::task::spawn_blocking(move || {
                let current =
                    crate::blueprint_mcp::construction_source(&store, &p, &original.record.id)?;
                if !original.matches(&current) {
                    return Err("adopted source changed since preview".into());
                }
                Ok(())
            })
            .await
            .map_err(|e| JobFailure::boundary(JobRefusalCode::AdmissionTaskFailed, e))?
            .map_err(|e: String| JobFailure::boundary(JobRefusalCode::SourceChanged, e))?;
            policy_scope(&self.policy, plan.scope(), &plan.source().dimension)
                .map_err(|e| JobFailure::boundary(JobRefusalCode::PermissionDenied, e))?;
            if parent.as_ref().is_some_and(|p| {
                p.checkpoint.endpoint
                    != crate::survival_execution::checkpoint::endpoint(&lease.reconnect())
            }) {
                return Err(JobFailure::boundary(
                    JobRefusalCode::CheckpointEndpointChanged,
                    "continuation requires the original endpoint and builder profile",
                ));
            }
            if cancel.load(Ordering::SeqCst) {
                return Err(JobFailure::boundary(
                    JobRefusalCode::CancelledBeforeStart,
                    "no construction dispatched",
                ));
            }
            let executor = SurvivalExecutor::create(
                bot.clone(),
                lease.reconnect(),
                plan,
                &path.join("execution"),
            )
            .await
            .map_err(JobFailure::from)?;
            let parent_lock = if let Some(parent) = &parent {
                Some(
                    crate::survival_execution::checkpoint::claim(
                        &self
                            .state_store
                            .survival_job_root()
                            .join(parent.id.to_string())
                            .join("execution"),
                        &parent.checkpoint,
                        id,
                    )
                    .map_err(JobFailure::from)?,
                )
            } else {
                None
            };
            Ok((executor, parent_lock))
        }
        .await;
        let (mut executor, _parent_lock) = match admission {
            Ok(e) => e,
            Err(e) => {
                lease.release(bot);
                self.set_survival_status(
                    id,
                    JobStatus::AdmissionRefused {
                        failure: e,
                        construction_dispatched: false,
                    },
                )
                .await;
                return;
            }
        };
        lease.begin_execution();
        loop {
            if cancel.load(Ordering::SeqCst) {
                let result = executor.cancel();
                self.set_survival_status(
                    id,
                    JobStatus::CancelledNeedsInspection {
                        error: result.err(),
                        completed_steps: executor.record().completed_steps,
                    },
                )
                .await;
                break;
            }
            if checkpoint.load(Ordering::SeqCst) && !executor.pending_mining() {
                match executor.checkpoint().await {
                    Ok(boundary) => {
                        let current = executor.client().clone();
                        // Release the old journal writer before publishing readiness.
                        drop(executor);
                        lease.release(current);
                        self.set_survival_status(
                            id,
                            JobStatus::Checkpointed {
                                completed_steps: boundary.completed_steps,
                                safe_idle: true,
                                checkpoint: Box::new(boundary),
                                next_step: "action=continue creates a fresh preview; start still requires confirmation".into(),
                            },
                        ).await;
                        return;
                    }
                    Err(error) => {
                        self.set_survival_status(
                            id,
                            JobStatus::NeedsInspection {
                                reason: InspectionReason::Execution {
                                    error,
                                    completed_steps: executor.record().completed_steps,
                                },
                            },
                        )
                        .await;
                        break;
                    }
                }
            }
            match executor.advance().await {
                Ok(ExecutionProgress::Completed) => {
                    lease.release(executor.client().clone());
                    self.set_survival_status(
                        id,
                        JobStatus::Completed {
                            completed_steps: Some(executor.record().completed_steps),
                            final_evidence: executor.record().events.last().cloned(),
                        },
                    )
                    .await;
                    return;
                }
                Ok(progress) => {
                    if !self
                        .set_survival_status(
                            id,
                            JobStatus::Running {
                                progress,
                                completed_steps: executor.record().completed_steps,
                            },
                        )
                        .await
                    {
                        let _ = executor.cancel();
                        break;
                    }
                }
                Err(error) => {
                    self.set_survival_status(
                        id,
                        JobStatus::NeedsInspection {
                            reason: InspectionReason::Execution {
                                error,
                                completed_steps: executor.record().completed_steps,
                            },
                        },
                    )
                    .await;
                    break;
                }
            }
        }
        if let Some(entry) = self.survival.entries.lock().await.get_mut(&id) {
            entry.retain(lease, executor);
        }
    }
    async fn set_survival_status(&self, id: uuid::Uuid, status: JobStatus) -> bool {
        let result = save(
            &self
                .state_store
                .survival_job_root()
                .join(id.to_string())
                .join("status.json"),
            &status,
        );
        let persisted = result.is_ok();
        let status = match result {
            Ok(()) => status,
            Err(e) => JobStatus::NeedsInspection {
                reason: InspectionReason::Persistence {
                    last_status: Box::new(status),
                    persistence_error: e,
                },
            },
        };
        if let Some(entry) = self.survival.entries.lock().await.get_mut(&id) {
            entry.publish(status);
        }
        persisted
    }
}
