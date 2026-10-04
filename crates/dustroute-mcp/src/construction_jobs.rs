//! Durable work intentions and progress. No executable model proof is saved.
use crate::assembly_registry::TargetServer;
use crate::placement_source::PlacementSource;
use crate::state::PlanStateStore;
use crate::storage::{Durability, replace};
use dustroute_library::world_edit::WorldEditScope;
use dustroute_translate::piston_construction::{
    ElectricalBoundary, ElectricalWorkPlan, ElectricalWorkRegion,
};
use dustroute_translate::snapshot::MinecraftSnapshot;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use uuid::Uuid;

pub(crate) const SCHEMA: &str = "dustroute.construction-job.v3";
const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobState {
    Ready,
    Completed,
    NeedsInspection,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JobRecord {
    pub schema: String,
    pub execution_profile: String,
    pub id: Uuid,
    pub player: String,
    pub source_revision_id: Uuid,
    pub source: PlacementSource,
    pub target: TargetServer,
    pub before: MinecraftSnapshot,
    pub after: MinecraftSnapshot,
    pub scope: WorldEditScope,
    pub regions: Vec<ElectricalWorkRegion>,
    pub boundaries: Vec<ElectricalBoundary>,
    pub completed_regions: usize,
    pub state: JobState,
    /// Cancelling forward work still permits fresh inverse cleanup plans.
    #[serde(default)]
    pub forward_cancelled: bool,
    pub active_operation_id: Option<Uuid>,
    pub attempts: Vec<JobAttempt>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct JobAttempt {
    pub operation_id: Uuid,
    pub region_index: usize,
    pub undo: bool,
    pub verified: bool,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<crate::failure::FailureReport>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JobStageBinding {
    pub job_id: Uuid,
    pub region_index: usize,
    pub undo: bool,
}
impl JobRecord {
    pub fn work_plan(&self) -> Result<ElectricalWorkPlan, String> {
        let plan = ElectricalWorkPlan::from_regions(
            &self.before,
            &self.after,
            &self.regions,
            self.scope.clone(),
        )?;
        if self.boundaries.len() != self.completed_regions
            || self.completed_regions > self.regions.len()
        {
            return Err("saved job partition/progress differs from its intention".into());
        }
        if self.completed_regions == self.regions.len()
            && self.snapshot_at(self.completed_regions)? != self.after
        {
            return Err("verified final boundary differs from immutable job target".into());
        }
        Ok(plan)
    }
    pub fn snapshot_at(&self, count: usize) -> Result<MinecraftSnapshot, String> {
        if count > self.completed_regions || self.boundaries.len() != self.completed_regions {
            return Err("invalid verified boundary count".into());
        }
        self.boundaries[..count]
            .iter()
            .try_fold(self.before.clone(), |state, delta| {
                delta.apply(&state, &self.scope)
            })
    }
    pub fn check_binding(&self, id: Uuid, binding: JobStageBinding) -> Result<(), String> {
        let expected = if binding.undo {
            self.completed_regions.checked_sub(1)
        } else {
            Some(self.completed_regions)
        };
        if binding.job_id != self.id
            || expected != Some(binding.region_index)
            || self.active_operation_id != Some(id)
            || (self.forward_cancelled && !binding.undo)
            || !(matches!(self.state, JobState::Ready | JobState::Completed)
                || (binding.undo && self.state == JobState::Cancelled))
        {
            return Err(
                "job stage was superseded, cancelled or needs inspection; read job history".into(),
            );
        }
        Ok(())
    }
}

pub(crate) struct JobRegistry {
    root: PathBuf,
    _lock: File,
}
impl JobRegistry {
    pub fn acquire(store: &PlanStateStore) -> Result<Self, String> {
        let root = store.construction_job_root();
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            let mut options = OpenOptions::new();
            options
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .mode(0o600);
            let lock = options
                .open(root.join("registry.lock"))
                .map_err(|e| e.to_string())?;
            fs2::FileExt::try_lock_exclusive(&lock)
                .map_err(|_| "construction job registry is busy")?;
            Ok(Self { root, _lock: lock })
        }
        #[cfg(not(unix))]
        {
            let lock = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(root.join("registry.lock"))
                .map_err(|e| e.to_string())?;
            fs2::FileExt::try_lock_exclusive(&lock)
                .map_err(|_| "construction job registry is busy")?;
            Ok(Self { root, _lock: lock })
        }
    }
    pub fn load(&self, id: Uuid, player: &str) -> Result<JobRecord, String> {
        let bytes = match crate::storage::read_record(
            &self.root.join(format!("{id}.store")),
            MAX_BYTES as usize,
        )
        .map_err(|e| e.to_string())?
        {
            Some(bytes) => bytes,
            None => return Err("construction job not found".into()),
        };
        if bytes.len() as u64 > MAX_BYTES {
            return Err("construction job exceeds 16 MiB".into());
        }
        let record: JobRecord =
            dustroute_codec::storage::decode(SCHEMA, &bytes, MAX_BYTES as usize)
                .map_err(|e| e.to_string())?;
        if record.schema != SCHEMA || record.id != id || record.player != player {
            return Err("construction job identity/schema/owner mismatch".into());
        }
        Ok(record)
    }
    pub fn save(&self, record: &JobRecord) -> Result<(), String> {
        if record.schema != SCHEMA
            || record.attempts.len() > 256
            || record.boundaries.len() != record.completed_regions
        {
            return Err("construction job schema/history limit exceeded".into());
        }
        let path = self.root.join(format!("{}.store", record.id));
        if crate::storage::record_exists(&path).map_err(|e| e.to_string())? {
            let old = self.load(record.id, &record.player)?;
            if old.forward_cancelled && !record.forward_cancelled {
                return Err("forward cancellation is permanent; create a new job".into());
            }
            let common = old.boundaries.len().min(record.boundaries.len());
            if old.boundaries[..common] != record.boundaries[..common]
                || old.completed_regions.abs_diff(record.completed_regions) > 1
            {
                return Err("verified boundary history must keep its existing prefix".into());
            }
            if old.execution_profile != record.execution_profile
                || old.source_revision_id != record.source_revision_id
                || old.source != record.source
                || old.target != record.target
                || old.before != record.before
                || old.after != record.after
                || old.scope != record.scope
                || old.regions != record.regions
            {
                return Err("construction job intention is immutable; create a new job".into());
            }
        }
        let bytes = dustroute_codec::storage::encode(SCHEMA, record, MAX_BYTES as usize)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("construction job exceeds 16 MiB".into());
        }
        replace(&path, &bytes, Durability::FileAndDirectory).map_err(|e| e.to_string())
    }
    /// Reserve the successful boundary before any world writes, so a growing
    /// checkpoint log cannot discover its storage limit after construction.
    pub fn ensure_completion_fits(
        record: &JobRecord,
        binding: JobStageBinding,
        proof: &dustroute_translate::piston_construction::ElectricalModification,
    ) -> Result<(), String> {
        let mut candidate = record.clone();
        if binding.undo {
            candidate.boundaries.truncate(binding.region_index);
        } else {
            candidate
                .boundaries
                .push(ElectricalBoundary::between(proof.before(), proof.after())?);
        }
        dustroute_codec::storage::encode(SCHEMA, &candidate, (MAX_BYTES - 65536) as usize)
            .map_err(|e| format!("construction boundary storage budget exhausted before writes; split the job: {e}"))?;
        Ok(())
    }
}
