//! Live progress and persisted history. Saved records never grant execution authority.
use super::*;

impl DustRouteMcp {
    pub(super) async fn get_survival(&self, id: uuid::Uuid, include_record: bool) -> Reply {
        // Progress polling must not repeatedly deserialize the entire plan and
        // journal or hold the registry mutex across filesystem I/O.
        let live = self
            .survival
            .entries
            .lock()
            .await
            .get(&id)
            .map(|e| (e.owner.clone(), e.status()));
        if let Some((owner, status)) = &live {
            if let Err(e) = self.policy.authorize_player(owner) {
                return failure(ServiceCode::PermissionDenied, e);
            }
            if !include_record {
                return Reply::Live(Box::new(replies::LiveDisplay::new(
                    id,
                    owner.clone(),
                    status.clone(),
                )));
            }
        }
        let available_live_status = live
            .as_ref()
            .map(|(owner, status)| replies::LiveDisplay::new(id, owner.clone(), status.clone()));
        let service = self.clone();
        let result = match tokio::task::spawn_blocking(move || {
            service.read_survival_record(id, include_record, live)
        })
        .await
        {
            Ok(result) => result,
            Err(e) => failure(ServiceCode::JournalUnavailable, e),
        };
        result.with_live_status(available_live_status)
    }

    fn read_survival_record(
        &self,
        id: uuid::Uuid,
        include_record: bool,
        live: Option<(String, JobStatus)>,
    ) -> Reply {
        let path = self.state_store.survival_job_root().join(id.to_string());
        let manifest: JobManifest = match load(&path.join("manifest.store")) {
            Ok(m) => m,
            Err(e) => return failure(ServiceCode::JobUnavailable, e),
        };
        if let Err(error) = manifest.validate_identity(id) {
            return failure(ServiceCode::InvalidRecord, error);
        }
        let owner = &manifest.owner;
        if let Err(e) = self.policy.authorize_player(owner) {
            return failure(ServiceCode::PermissionDenied, e);
        }
        if live
            .as_ref()
            .is_some_and(|(live_owner, _)| live_owner != owner)
        {
            return failure(
                ServiceCode::InvalidRecord,
                "saved owner differs from live job",
            );
        }
        let historical_status =
            if crate::storage::record_exists(&path.join("status.store")).unwrap_or(true) {
                match load::<JobStatus>(&path.join("status.store")) {
                    Ok(v) => Some(v),
                    Err(e) => return failure(ServiceCode::InvalidRecord, e),
                }
            } else {
                None
            };
        let directory = path.join("execution");
        let diagnosis =
            if crate::storage::record_exists(&directory.join("record.store")).unwrap_or(true) {
                match crate::survival_execution::diagnose(&directory) {
                    Ok(d) => Some(d),
                    Err(e) => return failure(ServiceCode::JournalUnavailable, e),
                }
            } else {
                None
            };
        let recorded_continuation = diagnosis.as_ref().map(|d| d.record.continuation.clone());
        let continuation_job_id = if include_record
            && crate::storage::record_exists(&directory.join("continuation-claim.store"))
                .unwrap_or(true)
        {
            match load::<crate::survival_execution::diagnostic::CheckpointClaim>(
                &directory.join("continuation-claim.store"),
            ) {
                Ok(claim) => Some(claim.new_job),
                Err(e) => return failure(ServiceCode::InvalidContinuationClaim, e),
            }
        } else {
            None
        };
        let present = live.is_some();
        let status = live.map(|(_, status)| status).or(historical_status);
        let completed_steps = status
            .as_ref()
            .and_then(JobStatus::completed_steps)
            .or_else(|| diagnosis.as_ref().map(|d| d.record.completed_steps));
        let next_action = if continuation_job_id.is_some() {
            super::guidance::NextAction::InspectLinkedJob
        } else {
            super::guidance::NextAction::for_status(status.as_ref(), !present)
        };
        Reply::Record(Box::new(replies::RecordDisplay {
            ok: Success,
            schema_version: JobSchema::V1,
            job_id: id,
            owner: manifest.owner.clone(),
            process_local_job_present: present,
            historical_only: !present,
            execution_authority_restored: DiagnosticOnly,
            completed_steps,
            status,
            recorded_continuation,
            next_action,
            next_step: next_action.description(),
            manifest: include_record.then_some(manifest),
            diagnosis: if include_record {
                Some(diagnosis)
            } else {
                None
            },
            continuation_job_id,
        }))
    }
}
