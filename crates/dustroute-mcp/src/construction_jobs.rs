//! Durable work intentions and progress. No executable model proof is saved.
use crate::assembly_registry::TargetServer;
use crate::state::PlanStateStore;
use crate::storage::{Durability, replace};
use dustroute_library::world_edit::WorldEditScope;
use dustroute_translate::piston_construction::{ElectricalWorkPlan, ElectricalWorkRegion};
use dustroute_translate::snapshot::MinecraftSnapshot;
use dustroute_translate::world::Region;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::PathBuf;
use uuid::Uuid;

pub(crate) const SCHEMA: &str = "dustroute.construction-job.v1";
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
    pub source: Value,
    pub target: TargetServer,
    pub before: MinecraftSnapshot,
    pub after: MinecraftSnapshot,
    pub scope: WorldEditScope,
    pub regions: Vec<ElectricalWorkRegion>,
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JobStageBinding {
    pub job_id: Uuid,
    pub region_index: usize,
    pub undo: bool,
}
impl JobRecord {
    pub fn work_plan(&self) -> Result<ElectricalWorkPlan, String> {
        let plan = ElectricalWorkPlan::new(
            &self.before,
            &self.after,
            self.regions
                .iter()
                .map(|r| r.region)
                .collect::<Vec<Region>>(),
            self.scope.clone(),
        )?;
        if plan.regions() != self.regions || self.completed_regions > self.regions.len() {
            return Err("saved job partition/progress differs from its intention".into());
        }
        Ok(plan)
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
        let file = File::open(self.root.join(format!("{id}.json"))).map_err(|e| e.to_string())?;
        let mut bytes = vec![];
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("construction job exceeds 16 MiB".into());
        }
        let record: JobRecord = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if record.schema != SCHEMA || record.id != id || record.player != player {
            return Err("construction job identity/schema/owner mismatch".into());
        }
        Ok(record)
    }
    pub fn save(&self, record: &JobRecord) -> Result<(), String> {
        if record.schema != SCHEMA || record.attempts.len() > 256 {
            return Err("construction job schema/history limit exceeded".into());
        }
        let path = self.root.join(format!("{}.json", record.id));
        if path.exists() {
            let old = self.load(record.id, &record.player)?;
            if old.forward_cancelled && !record.forward_cancelled {
                return Err("forward cancellation is permanent; create a new job".into());
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
        let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("construction job exceeds 16 MiB".into());
        }
        replace(&path, &bytes, Durability::FileAndDirectory).map_err(|e| e.to_string())
    }
}
