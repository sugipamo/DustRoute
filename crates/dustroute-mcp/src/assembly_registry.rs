//! Durable facts about world operations, deliberately separate from executable
//! capabilities and TTL-bound plans. A file lock spans each mutation attempt.
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use dustroute_library::assembly::Assembly;
use dustroute_library::blueprint::AssemblyRevisionId;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_translate::{assembly_transform::AssemblyTransform, snapshot::MinecraftSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{bridge::BotStatus, state::PlanStateStore};

const SCHEMA: &str = "dustroute.placed-assembly.v5";
const MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct TargetServer {
    pub host: String,
    pub port: u16,
    pub version: String,
    pub dimension: String,
    pub enabled_features: Vec<String>,
}
impl TargetServer {
    pub fn observed(status: &BotStatus, dimension: &str) -> Result<Self, String> {
        Ok(Self {
            host: status.host.clone(),
            port: status.port,
            version: status.version.clone(),
            dimension: dimension.into(),
            enabled_features: status
                .enabled_features
                .clone()
                .ok_or("server settings unavailable")?,
        })
    }
    pub fn check(&self, status: &BotStatus) -> Result<(), String> {
        if !status.connected
            || status.dimension.as_deref() != Some(&self.dimension)
            || Self::observed(status, &self.dimension)? != *self
        {
            return Err(
                "recorded server endpoint, dimension or settings differ from the connected target"
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InstanceState {
    NeedsInspection,
    Applied,
    Removed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Attempt {
    pub operation_id: Uuid,
    pub removal: bool,
    /// Historical reconstruction request, never an executable capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconstruction: Option<ReconstructionAttempt>,
    /// Historical reference and steps; fresh replay and observation are required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operating_removal: Option<OperatingRemoval>,
    pub verified_steps: usize,
    pub total_steps: usize,
    pub started_at_unix_ms: u64,
    pub finished_at_unix_ms: Option<u64>,
    pub error: Option<String>,
    /// Historical readback receipts only; fresh observations are always required.
    #[serde(default)]
    pub readbacks: Vec<crate::observation_evidence::ObservationEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<crate::failure::FailureReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<crate::failure::ExecutionProgress>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ReconstructionAttempt {
    pub baseline: MinecraftSnapshot,
    #[serde(
        deserialize_with = "dustroute_translate::piston_construction::expected_state::deserialize_steps"
    )]
    pub steps: Vec<dustroute_translate::piston_construction::ElectricalConstructionStep>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct OperatingRemoval {
    pub baseline: MinecraftSnapshot,
    #[serde(
        deserialize_with = "dustroute_translate::piston_construction::expected_state::deserialize_steps"
    )]
    pub steps: Vec<dustroute_translate::piston_construction::ElectricalConstructionStep>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlacedAssembly {
    pub schema: String,
    pub instance_id: Uuid,
    pub revision: u64,
    pub player: String,
    pub assembly_id: AssemblyRevisionId,
    pub source_identity: crate::source_identity::SourceIdentity,
    pub transform: AssemblyTransform,
    pub target: TargetServer,
    pub assembly: Assembly,
    pub context: RuntimeBehaviorContext,
    pub expected: MinecraftSnapshot,
    pub state: InstanceState,
    pub attempts: Vec<Attempt>,
    /// Archived presentation only. Fresh operation baselines come from the
    /// typed live observation path, never by decoding this saved report.
    pub last_observation: Option<crate::recorded_instance::RecordedInstanceReport>,
    pub updated_at_unix_ms: u64,
}

impl PlacedAssembly {
    pub fn schema() -> String {
        SCHEMA.into()
    }
    pub fn summary(&self) -> Value {
        serde_json::json!({"instance_id":self.instance_id,"record_revision":self.revision,
            "assembly_revision_id":self.assembly_id,"target":self.target,"transform":self.transform,
            "bounds":{"min":self.expected.min,"max":self.expected.max},"state":self.state,
            "attempts":self.attempts,"last_observation":self.last_observation,
            "updated_at_unix_ms":self.updated_at_unix_ms,"saved_record_is_validation_proof":false})
    }
}

pub(crate) fn now_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "timestamp overflow".into())
}

pub(crate) struct RegistryLock {
    root: PathBuf,
    _lock: File,
}
impl RegistryLock {
    pub fn acquire(store: &PlanStateStore) -> Result<Self, String> {
        let root = store.assembly_instance_root();
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let lock = options(false)
            .open(root.join("registry.lock"))
            .map_err(|e| e.to_string())?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| "placed Assembly registry is busy; retry the same request".to_owned())?;
        Ok(Self { root, _lock: lock })
    }

    fn load(&self, id: Uuid) -> Result<Option<PlacedAssembly>, String> {
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
            return Err("placed Assembly record exceeds 32 MiB".into());
        }
        let record: PlacedAssembly =
            dustroute_codec::storage::decode(SCHEMA, &bytes, MAX_BYTES as usize)
                .map_err(|e| e.to_string())?;
        if record.schema != SCHEMA
            || record.instance_id != id
            || record.revision == 0
            || record.attempts.iter().any(|a| {
                a.operating_removal.is_some() && (!a.removal || a.reconstruction.is_some())
            })
        {
            return Err("placed Assembly schema or identity mismatch".into());
        }
        Ok(Some(record))
    }

    pub fn get(&self, id: Uuid, player: &str) -> Result<PlacedAssembly, String> {
        let record = self.load(id)?.ok_or("placed Assembly not found")?;
        if record.player != player {
            return Err("placed Assembly belongs to another player".into());
        }
        Ok(record)
    }

    pub fn list(&self, player: &str) -> Result<Vec<Value>, String> {
        let mut ids = std::collections::BTreeSet::new();
        for entry in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if !matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("store" | "json")
            ) {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or("invalid placed Assembly registry filename")?;
            ids.insert(id);
        }
        let mut result = Vec::new();
        for id in ids {
            let record = self
                .load(id)?
                .ok_or("placed Assembly disappeared while locked")?;
            if record.player == player {
                result.push(record);
            }
        }
        result.sort_by_key(|r| r.instance_id.to_string());
        Ok(result.iter().map(PlacedAssembly::summary).collect())
    }

    /// Compare-and-save also rejects stale values from callers in this process.
    /// The intent and progress are fsynced before subsequent world writes.
    pub fn save(&self, record: &mut PlacedAssembly) -> Result<(), String> {
        let previous = self.load(record.instance_id)?;
        if previous.as_ref().map_or(0, |r| r.revision) != record.revision
            || previous.as_ref().is_some_and(|r| r.player != record.player)
            || record.schema != SCHEMA
        {
            return Err("placed Assembly record changed; reobserve and replan".into());
        }
        let mut next = record.clone();
        next.schema = SCHEMA.into();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or("record version exhausted")?;
        next.updated_at_unix_ms = now_ms()?;
        let bytes = {
            let _measurement = crate::performance::span(crate::performance::Phase::StoreEncode);
            dustroute_codec::storage::encode(SCHEMA, &next, MAX_BYTES as usize)
                .map_err(|e| e.to_string())?
        };
        if bytes.len() as u64 > MAX_BYTES {
            return Err("placed Assembly record exceeds 32 MiB".into());
        }
        let destination = self.root.join(format!("{}.store", record.instance_id));
        crate::storage::replace(
            &destination,
            &bytes,
            crate::storage::Durability::FileAndDirectory,
        )
        .map_err(|error| {
            format!("placed Assembly persistence failed; inspect before retry: {error}")
        })?;
        *record = next;
        Ok(())
    }
}

fn options(exclusive: bool) -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if exclusive {
        options.create_new(true);
    } else {
        options.create(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_translate::{cells::RotationY, world::Pos, world::Region};

    #[test]
    fn durable_registry_rejects_competing_writers_stale_values_wrong_owners_and_corrupt_records() {
        let root = std::env::temp_dir().join(format!("dustroute-instance-test-{}", Uuid::new_v4()));
        let store = PlanStateStore::new(root.clone(), 1);
        let registry = RegistryLock::acquire(&store).unwrap();
        assert!(RegistryLock::acquire(&store).is_err());
        let pos = Pos::default();
        let mut record = PlacedAssembly {
            schema: PlacedAssembly::schema(),
            instance_id: Uuid::new_v4(),
            revision: 0,
            player: "Tester".into(),
            assembly_id: AssemblyRevisionId::new("test.assembly.v1").unwrap(),
            source_identity: crate::source_identity::SourceIdentity {
                record: dustroute_library::assembly::AssemblyRevision {
                    id: AssemblyRevisionId::new("test.source.v1").unwrap(),
                    parents: vec![],
                    assembly: Assembly {
                        name: "source".into(),
                        instances: vec![],
                        blocks: vec![],
                        known_regions: vec![Region::new(pos, pos)],
                        connections: vec![],
                        boundaries: vec![],
                    },
                },
                adopted_by: dustroute_library::blueprint::BlueprintUpdateId::new("test.adoption")
                    .unwrap(),
                context: RuntimeBehaviorContext::fresh_pistons(Region::new(pos, pos), vec![]),
                catalog: Default::default(),
            },
            transform: AssemblyTransform {
                source_anchor: pos,
                target_anchor: pos,
                rotation: RotationY::R0,
            },
            target: TargetServer {
                host: "localhost".into(),
                port: 25565,
                version: "1.21.11".into(),
                dimension: "minecraft:overworld".into(),
                enabled_features: vec!["minecraft:vanilla".into()],
            },
            assembly: Assembly {
                name: "data only".into(),
                instances: vec![],
                blocks: vec![],
                known_regions: vec![Region::new(pos, pos)],
                connections: vec![],
                boundaries: vec![],
            },
            context: RuntimeBehaviorContext::fresh_pistons(Region::new(pos, pos), vec![]),
            expected: MinecraftSnapshot {
                min: pos,
                max: pos,
                blocks: vec![],
            },
            state: InstanceState::NeedsInspection,
            attempts: vec![],
            last_observation: None,
            updated_at_unix_ms: 0,
        };
        registry.save(&mut record).unwrap();
        let mut stale = record.clone();
        registry.save(&mut record).unwrap();
        assert!(registry.save(&mut stale).is_err());
        assert!(registry.get(record.instance_id, "Other").is_err());
        assert!(registry.list("Other").unwrap().is_empty());
        let path = store
            .assembly_instance_root()
            .join(format!("{}.store", record.instance_id));
        let mut old = registry.get(record.instance_id, "Tester").unwrap();
        old.updated_at_unix_ms = 1;
        let encode = |record: &PlacedAssembly| {
            dustroute_codec::storage::encode(SCHEMA, record, MAX_BYTES as usize).unwrap()
        };
        fs::write(&path, encode(&old)).unwrap();
        let retired = path.with_extension("json");
        fs::write(&retired, b"{retired JSON}").unwrap();
        assert_eq!(registry.list("Tester").unwrap().len(), 1);
        assert_eq!(fs::read(&retired).unwrap(), b"{retired JSON}");
        fs::remove_file(&retired).unwrap();
        drop(registry);
        let reopened = RegistryLock::acquire(&PlanStateStore::new(root.clone(), 1)).unwrap();
        assert_eq!(
            reopened.get(record.instance_id, "Tester").unwrap().state,
            InstanceState::NeedsInspection
        );
        assert_eq!(
            reopened.list("Tester").unwrap().len(),
            1,
            "plan TTL must never erase placed records"
        );
        let mut history = reopened.get(record.instance_id, "Tester").unwrap();
        history.attempts.push(Attempt {
            operation_id: Uuid::new_v4(),
            removal: true,
            reconstruction: None,
            operating_removal: Some(OperatingRemoval {
                baseline: history.expected.clone(),
                steps: vec![],
            }),
            verified_steps: 0,
            total_steps: 0,
            started_at_unix_ms: 1,
            finished_at_unix_ms: Some(2),
            error: None,
            readbacks: vec![],
            failure: None,
            progress: None,
        });
        fs::write(&path, encode(&history)).unwrap();
        assert!(
            reopened.get(record.instance_id, "Tester").unwrap().attempts[0]
                .operating_removal
                .is_some()
        );
        for schema in [
            "dustroute.placed-assembly.v1",
            "dustroute.placed-assembly.v2",
            "dustroute.placed-assembly.v3",
            "dustroute.placed-assembly.v4",
        ] {
            history.schema = schema.into();
            fs::write(&path, encode(&history)).unwrap();
            assert!(reopened.get(record.instance_id, "Tester").is_err());
            let mut without_removal = history.clone();
            without_removal.attempts[0].operating_removal = None;
            fs::write(&path, encode(&without_removal)).unwrap();
            assert!(reopened.get(record.instance_id, "Tester").is_err());
        }
        history.schema = SCHEMA.into();
        history.attempts[0].removal = false;
        fs::write(&path, encode(&history)).unwrap();
        assert!(reopened.get(record.instance_id, "Tester").is_err());
        fs::write(&path, b"{truncated").unwrap();
        assert!(reopened.get(record.instance_id, "Tester").is_err());
        assert!(reopened.save(&mut record).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{truncated");
        old.schema = "unknown.future.schema".into();
        fs::write(&path, encode(&old)).unwrap();
        assert!(reopened.get(record.instance_id, "Tester").is_err());
        fs::remove_file(&path).unwrap();
        let retired = path.with_extension("json");
        fs::write(&retired, b"{retired JSON}").unwrap();
        assert!(reopened.save(&mut record).is_err());
        assert!(reopened.list("Tester").is_err());
        assert!(!path.exists());
        assert_eq!(fs::read(&retired).unwrap(), b"{retired JSON}");
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }
}
