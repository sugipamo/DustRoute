//! Live progress and persisted history. Saved records never grant execution authority.
use super::*;

impl DustRouteMcp {
    pub(super) async fn get_survival(&self, id: uuid::Uuid, include_record: bool) -> Value {
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
                return failure("permission_denied", e);
            }
            if !include_record {
                return json!({"ok":true,"schema_version":"dustroute.survival-job.v1","job_id":id,"owner":owner,
                    "process_local_job_present":true,"historical_only":false,"execution_authority_restored":false,
                    "completed_steps":status.completed_steps(),"status":status,
                    "next_step":"inspect status; stopped jobs never automatically replay"});
            }
        }
        let service = self.clone();
        match tokio::task::spawn_blocking(move || {
            service.read_survival_record(id, include_record, live)
        })
        .await
        {
            Ok(result) => result,
            Err(e) => failure("journal_unavailable", e),
        }
    }

    fn read_survival_record(
        &self,
        id: uuid::Uuid,
        include_record: bool,
        live: Option<(String, JobStatus)>,
    ) -> Value {
        let path = self.state_store.survival_job_root().join(id.to_string());
        let manifest: JobManifest = match load(&path.join("manifest.json")) {
            Ok(m) => m,
            Err(e) => return failure("job_unavailable", e),
        };
        if let Err(error) = manifest.validate_identity(id) {
            return failure("invalid_record", error);
        }
        let owner = &manifest.owner;
        if let Err(e) = self.policy.authorize_player(owner) {
            return failure("permission_denied", e);
        }
        if live
            .as_ref()
            .is_some_and(|(live_owner, _)| live_owner != owner)
        {
            return failure("invalid_record", "saved owner differs from live job");
        }
        let historical_status = if path.join("status.json").exists() {
            match load::<JobStatus>(&path.join("status.json")) {
                Ok(v) => Some(v),
                Err(e) => return failure("invalid_record", e),
            }
        } else {
            None
        };
        let directory = path.join("execution");
        let diagnosis = if directory.join("record.json").exists() {
            match crate::survival_execution::diagnose(&directory) {
                Ok(d) => Some(d),
                Err(e) => return failure("journal_unavailable", e),
            }
        } else {
            None
        };
        let record = diagnosis.as_ref().map(|d| &d.record);
        let mut response = json!({"ok":true,"schema_version":"dustroute.survival-job.v1","job_id":id,"owner":owner,
            "process_local_job_present":live.is_some(),"status":live.as_ref().map(|(_,s)|s).or(historical_status.as_ref()),
            "historical_only":live.is_none(),"execution_authority_restored":false,
            "completed_steps":record.map(|r|r.completed_steps),"recorded_continuation":record.map(|r|&r.continuation),
            "next_step":if live.is_none(){"historical diagnosis only; reobserve and resolve outstanding operations before any new plan"}else{"inspect status; stopped jobs never automatically replay"}});
        if include_record {
            response["manifest"] = json!(manifest);
            response["diagnosis"] = json!(diagnosis);
            if directory.join("continuation-claim.json").exists() {
                match load::<crate::survival_execution::diagnostic::CheckpointClaim>(
                    &directory.join("continuation-claim.json"),
                ) {
                    Ok(claim) => response["continuation_job_id"] = json!(claim.new_job),
                    Err(e) => return failure("invalid_continuation_claim", e),
                }
            }
        }
        response
    }
}
