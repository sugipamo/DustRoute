use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DEFAULT_TTL_SECONDS: u64 = 60 * 60;

/// Only these expendable records share TTL/rename storage. Catalogs retain
/// their separate locks and file sync; instances retain file + directory sync.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PlanRecordKind {
    Repairs,
    CircuitRevisions,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanEnvelope<T> {
    pub saved_at_unix_seconds: u64,
    pub value: T,
}

impl PlanRecordKind {
    pub(crate) const fn schema(self) -> &'static str {
        match self {
            Self::Repairs => "dustroute.repair-plan-store.v1",
            Self::CircuitRevisions => "dustroute.circuit-revision-store.v1",
        }
    }
    const fn max_bytes(self) -> usize {
        match self {
            Self::Repairs => 16 * 1024 * 1024,
            Self::CircuitRevisions => 4 * 1024 * 1024 + 4096,
        }
    }
    const fn directory(self) -> &'static str {
        match self {
            Self::Repairs => "repairs",
            Self::CircuitRevisions => "circuit_revisions",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PlanStateStore {
    root: PathBuf,
    ttl_seconds: u64,
}

impl PlanStateStore {
    #[cfg(feature = "voxrig")]
    pub(crate) fn survival_job_root(&self) -> PathBuf {
        self.root.join("survival-jobs")
    }
    pub(crate) fn construction_job_root(&self) -> PathBuf {
        self.root.join("construction-jobs")
    }
    pub(crate) fn assembly_instance_root(&self) -> PathBuf {
        self.root.join("assembly-instances")
    }
    pub(crate) fn edit_record_root(&self) -> PathBuf {
        self.root.join("world-edits")
    }

    pub(crate) fn blueprint_root(&self, player: &str) -> PathBuf {
        let identity = storage_identity("player", player);
        self.root.join("blueprints").join(identity)
    }
    pub(crate) fn from_environment(scope: &str) -> Self {
        let scope = storage_identity("scope", scope);
        let base = std::env::var_os("DUSTROUTE_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("dustroute-mcp-state"));
        let ttl_seconds = std::env::var("DUSTROUTE_PLAN_TTL_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_TTL_SECONDS);
        Self::new(base.join("storage-v2").join(scope), ttl_seconds)
    }

    pub(crate) fn new(root: PathBuf, ttl_seconds: u64) -> Self {
        Self { root, ttl_seconds }
    }

    pub(crate) fn save<T: Serialize>(
        &self,
        kind: PlanRecordKind,
        id: uuid::Uuid,
        value: &T,
    ) -> Result<(), String> {
        let directory = self.root.join(kind.directory());
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        restrict_directory(&self.root)?;
        restrict_directory(&directory)?;
        let bytes = {
            let _measurement = crate::performance::span(crate::performance::Phase::StoreEncode);
            let envelope = PlanEnvelope {
                saved_at_unix_seconds: unix_seconds()?,
                value,
            };
            dustroute_codec::storage::encode(kind.schema(), &envelope, kind.max_bytes())
                .map_err(|e| e.to_string())?
        };
        let destination = directory.join(format!("{id}.store"));
        crate::storage::check_destination(&destination).map_err(|e| e.to_string())?;
        crate::storage::replace(
            &destination,
            &bytes,
            crate::storage::Durability::ReplaceOnly,
        )
        .map_err(|error| error.to_string())
    }

    pub(crate) fn load<T: DeserializeOwned>(
        &self,
        kind: PlanRecordKind,
        id: uuid::Uuid,
    ) -> Result<Option<T>, String> {
        let _measurement = crate::performance::span(crate::performance::Phase::StoreRead);
        let path = self.root.join(kind.directory()).join(format!("{id}.store"));
        let bytes = match crate::storage::read_record(&path, kind.max_bytes())
            .map_err(|e| e.to_string())?
        {
            Some(bytes) => bytes,
            None => return Ok(None),
        };
        let metadata: PlanEnvelope<serde::de::IgnoredAny> =
            dustroute_codec::storage::decode(kind.schema(), &bytes, kind.max_bytes())
                .map_err(|e| e.to_string())?;
        let saved_at = metadata.saved_at_unix_seconds;
        if unix_seconds()?.saturating_sub(saved_at) > self.ttl_seconds {
            let _ = fs::remove_file(path);
            return Ok(None);
        }
        let envelope: PlanEnvelope<T> =
            dustroute_codec::storage::decode(kind.schema(), &bytes, kind.max_bytes())
                .map_err(|e| e.to_string())?;
        Ok(Some(envelope.value))
    }
}

fn storage_identity(namespace: &str, value: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"dustroute.storage-identity.v1\0");
    for part in [namespace, value] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| error.to_string())
}

#[cfg(unix)]
fn restrict_directory(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn restrict_directory(_path: &std::path::Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct ExamplePlan {
        value: String,
    }

    #[test]
    fn shares_an_atomic_plan_between_store_instances() {
        let root =
            std::env::temp_dir().join(format!("dustroute-state-test-{}", uuid::Uuid::new_v4()));
        let writer = PlanStateStore::new(root.clone(), 60);
        let reader = PlanStateStore::new(root.clone(), 60);
        let id = uuid::Uuid::new_v4();
        writer
            .save(
                PlanRecordKind::Repairs,
                id,
                &ExamplePlan {
                    value: "previewable".to_owned(),
                },
            )
            .unwrap();
        assert_eq!(
            reader.load(PlanRecordKind::Repairs, id).unwrap(),
            Some(ExamplePlan {
                value: "previewable".to_owned()
            })
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn expired_plans_are_removed_before_payload_type_decode_and_reads_do_not_extend_ttl() {
        let root = std::env::temp_dir().join(format!("dustroute-ttl-{}", uuid::Uuid::new_v4()));
        let store = PlanStateStore::new(root.clone(), 60);
        let id = uuid::Uuid::new_v4();
        store
            .save(
                PlanRecordKind::Repairs,
                id,
                &ExamplePlan {
                    value: "valid".into(),
                },
            )
            .unwrap();
        let path = root.join("repairs").join(format!("{id}.store"));
        let before = fs::read(&path).unwrap();
        store
            .load::<ExamplePlan>(PlanRecordKind::Repairs, id)
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        let bytes = dustroute_codec::storage::encode(
            PlanRecordKind::Repairs.schema(),
            &PlanEnvelope {
                saved_at_unix_seconds: 0,
                value: 12_u64,
            },
            4096,
        )
        .unwrap();
        fs::write(&path, bytes).unwrap();
        assert!(
            store
                .load::<ExamplePlan>(PlanRecordKind::Repairs, id)
                .unwrap()
                .is_none()
        );
        assert!(!path.exists());
        let old = path.with_extension("json");
        fs::write(&old, b"retired JSON").unwrap();
        assert!(
            store
                .load::<ExamplePlan>(PlanRecordKind::Repairs, id)
                .is_err()
        );
        assert!(
            store
                .save(
                    PlanRecordKind::Repairs,
                    id,
                    &ExamplePlan {
                        value: "new".into()
                    }
                )
                .is_err()
        );
        assert_eq!(fs::read(&old).unwrap(), b"retired JSON");
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn storage_identity_is_framed_versioned_and_separates_namespaces() {
        assert_eq!(
            storage_identity("player", "Tester"),
            storage_identity("player", "Tester")
        );
        assert_ne!(
            storage_identity("player", "Tester"),
            storage_identity("scope", "Tester")
        );
        assert_ne!(storage_identity("ab", "c"), storage_identity("a", "bc"));
        assert_eq!(storage_identity("player", "Tester").len(), 64);
    }
}
