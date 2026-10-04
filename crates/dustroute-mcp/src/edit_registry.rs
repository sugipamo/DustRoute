//! Durable edit history. Loading these records never restores executable plans.
use crate::assembly_registry::TargetServer;
use crate::observation_evidence::ObservationEvidence;
use crate::state::PlanStateStore;
use crate::storage::{Durability, replace};
use dustroute_translate::snapshot::MinecraftSnapshot;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EditState {
    NeedsInspection,
    Applied,
    Undone,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditRecord {
    pub schema: String,
    pub execution_profile: String,
    pub operation_id: Uuid,
    pub player: String,
    pub source_revision_id: Uuid,
    pub target: TargetServer,
    pub before: MinecraftSnapshot,
    pub after: MinecraftSnapshot,
    /// Missing on earlier historical records; never restored as a capability.
    #[serde(default)]
    pub edit_scope: Option<dustroute_library::world_edit::WorldEditScope>,
    #[serde(default)]
    pub job_stage: Option<crate::construction_jobs::JobStageBinding>,
    pub state: EditState,
    pub attempts: Vec<EditAttempt>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct EditAttempt {
    pub undo: bool,
    pub verified_steps: usize,
    pub total_steps: usize,
    pub started_at_unix_ms: u64,
    pub finished_at_unix_ms: Option<u64>,
    pub error: Option<String>,
    pub readbacks: Vec<ObservationEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<crate::failure::FailureReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<crate::failure::ExecutionProgress>,
}
pub(crate) const SCHEMA: &str = "dustroute.world-edit.v2";
const MAX_BYTES: u64 = 32 * 1024 * 1024;

pub(crate) struct EditRegistry {
    root: PathBuf,
    _lock: File,
}
impl EditRegistry {
    pub fn acquire(store: &PlanStateStore) -> Result<Self, String> {
        let root = store.edit_record_root();
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options
            .open(root.join("registry.lock"))
            .map_err(|e| e.to_string())?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| "world edit registry is busy".to_owned())?;
        Ok(Self { root, _lock: lock })
    }
    pub fn load(&self, id: Uuid, player: &str) -> Result<Option<EditRecord>, String> {
        let _measurement = crate::performance::span(crate::performance::Phase::StoreRead);
        let bytes = match crate::storage::read_record(
            &self.root.join(format!("{id}.store")),
            MAX_BYTES as usize,
        )
        .map_err(|e| e.to_string())?
        {
            Some(bytes) => bytes,
            None => return Ok(None),
        };
        if bytes.len() as u64 > MAX_BYTES {
            return Err("world edit record exceeds 32 MiB".into());
        }
        let record: EditRecord =
            dustroute_codec::storage::decode(SCHEMA, &bytes, MAX_BYTES as usize)
                .map_err(|e| e.to_string())?;
        if record.schema != SCHEMA || record.operation_id != id {
            return Err("invalid world edit record identity/schema".into());
        }
        if record.player != player {
            return Err("world edit belongs to another player".into());
        }
        Ok(Some(record))
    }
    pub fn save(&self, record: &EditRecord) -> Result<(), String> {
        if record.schema != SCHEMA {
            return Err("invalid world edit schema".into());
        }
        self.load(record.operation_id, &record.player)?;
        let bytes = {
            let _measurement = crate::performance::span(crate::performance::Phase::StoreEncode);
            dustroute_codec::storage::encode(SCHEMA, record, MAX_BYTES as usize)
                .map_err(|e| e.to_string())?
        };
        if bytes.len() as u64 > MAX_BYTES {
            return Err("world edit record exceeds 32 MiB".into());
        }
        replace(
            &self.root.join(format!("{}.store", record.operation_id)),
            &bytes,
            Durability::FileAndDirectory,
        )
        .map_err(|e| e.to_string())
    }
}
