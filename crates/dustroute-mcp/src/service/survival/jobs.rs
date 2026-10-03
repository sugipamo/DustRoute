//! One owner for process-local plans and quarantined execution resources.
//! Stored/displayed status is a projection, never an executable job constructor.
use super::*;

pub(super) struct Entry {
    pub owner: String,
    pub source: SourceIdentity,
    pub cancel: Arc<AtomicBool>,
    pub checkpoint: Arc<AtomicBool>,
    state: State,
}

enum State {
    Planned {
        plan: Box<HypotheticalConstructionPlan>,
        expires: Instant,
        parent: Option<Box<ContinuationParent>>,
    },
    Admitting,
    Running {
        progress: ExecutionProgress,
        completed_steps: usize,
    },
    CancelledBeforeStart,
    PlanExpiredOrCancelled,
    AdmissionRefused {
        failure: crate::survival_execution::diagnostic::DiagnosticPayload,
    },
    Checkpointed {
        completed_steps: usize,
        checkpoint: Box<crate::survival_execution::checkpoint::SafeCheckpoint>,
    },
    Completed {
        completed_steps: Option<usize>,
        final_evidence: Option<crate::survival_execution::ExecutionEvent>,
    },
    CancelledNeedsInspection {
        error: Option<crate::survival_execution::ExecutionError>,
        completed_steps: usize,
        retained: Option<Box<RetainedExecution>>,
    },
    NeedsInspection {
        reason: InspectionReason,
        retained: Option<Box<RetainedExecution>>,
    },
}

struct RetainedExecution {
    // Both handles stay alive together. Dropping this does not hand back a source.
    _lease: SurvivalLease,
    _executor: SurvivalExecutor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StartReadiness {
    Ready,
    ExpiredOrCancelled,
    AlreadyStarted,
}

impl State {
    fn start_readiness(&self, now: Instant, cancelled: bool) -> StartReadiness {
        match self {
            Self::Planned { expires, .. } if cancelled || now > *expires => {
                StartReadiness::ExpiredOrCancelled
            }
            Self::Planned { .. } => StartReadiness::Ready,
            Self::CancelledBeforeStart | Self::PlanExpiredOrCancelled => {
                StartReadiness::ExpiredOrCancelled
            }
            _ => StartReadiness::AlreadyStarted,
        }
    }
}

impl Entry {
    pub(super) fn planned(
        owner: String,
        source: SourceIdentity,
        plan: HypotheticalConstructionPlan,
        expires: Instant,
        parent: Option<ContinuationParent>,
    ) -> Self {
        Self {
            owner,
            source,
            cancel: Arc::new(AtomicBool::new(false)),
            checkpoint: Arc::new(AtomicBool::new(false)),
            state: State::Planned {
                plan: Box::new(plan),
                expires,
                parent: parent.map(Box::new),
            },
        }
    }

    pub(super) fn start_readiness(&self) -> StartReadiness {
        self.state
            .start_readiness(Instant::now(), self.cancel.load(Ordering::SeqCst))
    }

    pub(super) fn expire_preview(&mut self) {
        if self.start_readiness() == StartReadiness::ExpiredOrCancelled {
            self.state = State::PlanExpiredOrCancelled;
        }
    }

    pub(super) fn take_preview(
        &mut self,
    ) -> Option<(HypotheticalConstructionPlan, Option<ContinuationParent>)> {
        if !matches!(self.state, State::Planned { .. }) {
            return None;
        }
        let State::Planned { plan, parent, .. } =
            std::mem::replace(&mut self.state, State::Admitting)
        else {
            unreachable!("only a live preview can be consumed")
        };
        Some((*plan, parent.map(|value| *value)))
    }

    pub(super) fn request_cancel(&mut self) -> bool {
        self.cancel.store(true, Ordering::SeqCst);
        if matches!(self.state, State::Planned { .. }) {
            self.state = State::CancelledBeforeStart;
            true
        } else {
            false
        }
    }

    pub(super) fn allows_checkpoint(&self) -> bool {
        matches!(self.state, State::Admitting | State::Running { .. })
    }

    pub(super) fn status(&self) -> JobStatus {
        match &self.state {
            State::Planned { .. } => JobStatus::Planned,
            State::Admitting => JobStatus::Admitting,
            State::Running {
                progress,
                completed_steps,
            } => JobStatus::Running {
                progress: progress.clone(),
                completed_steps: *completed_steps,
            },
            State::CancelledBeforeStart => JobStatus::CancelledBeforeStart {
                world_writes: false,
            },
            State::PlanExpiredOrCancelled => JobStatus::PlanExpiredOrCancelled {
                construction_dispatched: false,
            },
            State::AdmissionRefused { failure } => JobStatus::AdmissionRefused {
                failure: failure.clone(),
                construction_dispatched: false,
            },
            State::Checkpointed {
                completed_steps,
                checkpoint,
            } => JobStatus::Checkpointed {
                completed_steps: *completed_steps,
                safe_idle: true,
                checkpoint: checkpoint.clone(),
                next_step:
                    "action=continue creates a fresh preview; start still requires confirmation"
                        .into(),
            },
            State::Completed {
                completed_steps,
                final_evidence,
            } => JobStatus::Completed {
                completed_steps: *completed_steps,
                final_evidence: final_evidence.clone(),
            },
            State::CancelledNeedsInspection {
                error,
                completed_steps,
                ..
            } => JobStatus::CancelledNeedsInspection {
                error: error.clone(),
                completed_steps: *completed_steps,
            },
            State::NeedsInspection { reason, .. } => JobStatus::NeedsInspection {
                reason: reason.clone(),
            },
        }
    }

    pub(super) fn publish(&mut self, status: JobStatus) {
        // A native preview can only enter through `planned`, never from a DTO.
        let mut next = match status {
            JobStatus::Planned => return,
            JobStatus::Admitting => State::Admitting,
            JobStatus::Running {
                progress,
                completed_steps,
            } => State::Running {
                progress,
                completed_steps,
            },
            JobStatus::CancelledBeforeStart { .. } => State::CancelledBeforeStart,
            JobStatus::PlanExpiredOrCancelled { .. } => State::PlanExpiredOrCancelled,
            JobStatus::AdmissionRefused { failure, .. } => State::AdmissionRefused { failure },
            JobStatus::Checkpointed {
                completed_steps,
                checkpoint,
                ..
            } => State::Checkpointed {
                completed_steps,
                checkpoint,
            },
            JobStatus::Completed {
                completed_steps,
                final_evidence,
            } => State::Completed {
                completed_steps,
                final_evidence,
            },
            JobStatus::CancelledNeedsInspection {
                error,
                completed_steps,
            } => State::CancelledNeedsInspection {
                error,
                completed_steps,
                retained: None,
            },
            JobStatus::NeedsInspection { reason } => State::NeedsInspection {
                reason,
                retained: None,
            },
        };
        // Publishing another diagnostic must never drop quarantined resources.
        if let (
            State::CancelledNeedsInspection { retained, .. }
            | State::NeedsInspection { retained, .. },
            State::CancelledNeedsInspection { retained: old, .. }
            | State::NeedsInspection { retained: old, .. },
        ) = (&mut next, &mut self.state)
        {
            *retained = old.take();
        }
        self.state = next;
    }

    pub(super) fn retain(&mut self, lease: SurvivalLease, executor: SurvivalExecutor) {
        let completed_steps = executor.record().completed_steps;
        let retained = Box::new(RetainedExecution {
            _lease: lease,
            _executor: executor,
        });
        match &mut self.state {
            State::CancelledNeedsInspection { retained: slot, .. }
            | State::NeedsInspection { retained: slot, .. } => *slot = Some(retained),
            _ => {
                self.state = State::NeedsInspection {
                    reason: InspectionReason::Execution {
                        error: crate::survival_execution::ExecutionError {
                            code: SurvivalErrorCode::ExecutionNeedsInspection,
                            detail: "stopped execution resources require inspection".into(),
                        },
                        completed_steps,
                    },
                    retained: Some(retained),
                };
            }
        }
    }

    #[cfg(test)]
    pub(super) fn fork_preview(&self, expires: Option<Instant>) -> Self {
        let State::Planned {
            plan,
            expires: original,
            parent,
        } = &self.state
        else {
            panic!("test fixture must fork a checked live preview")
        };
        Self::planned(
            self.owner.clone(),
            self.source.clone(),
            (**plan).clone(),
            expires.unwrap_or(*original),
            parent.as_deref().cloned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_start_cannot_expire_or_reclassify_dispatched_work() {
        for state in [
            State::Admitting,
            State::Running {
                progress: ExecutionProgress::StepCompleted { completed: 7 },
                completed_steps: 7,
            },
            State::Completed {
                completed_steps: Some(7),
                final_evidence: None,
            },
        ] {
            for cancelled in [false, true] {
                assert_eq!(
                    state.start_readiness(Instant::now(), cancelled),
                    StartReadiness::AlreadyStarted,
                );
            }
        }
    }
}
