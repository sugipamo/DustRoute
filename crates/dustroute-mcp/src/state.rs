use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde::de::DeserializeOwned;

const DEFAULT_TTL_SECONDS: u64 = 60 * 60;

#[derive(Clone, Debug)]
pub(crate) struct PlanStateStore {
    root: PathBuf,
    ttl_seconds: u64,
}

impl PlanStateStore {
    pub(crate) fn from_environment(scope: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        scope.hash(&mut hasher);
        let scope = format!("{:016x}", hasher.finish());
        let base = std::env::var_os("DUSTROUTE_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("dustroute-mcp-state"));
        let ttl_seconds = std::env::var("DUSTROUTE_PLAN_TTL_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_TTL_SECONDS);
        Self::new(base.join(scope), ttl_seconds)
    }

    fn new(root: PathBuf, ttl_seconds: u64) -> Self {
        Self { root, ttl_seconds }
    }

    pub(crate) fn save<T: Serialize>(
        &self,
        kind: &str,
        id: uuid::Uuid,
        value: &T,
    ) -> Result<(), String> {
        let directory = self.root.join(kind);
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        restrict_directory(&self.root)?;
        restrict_directory(&directory)?;
        let envelope = serde_json::json!({
            "saved_at_unix_seconds": unix_seconds()?,
            "value": value,
        });
        let bytes = serde_json::to_vec(&envelope).map_err(|error| error.to_string())?;
        let destination = directory.join(format!("{id}.json"));
        let temporary = directory.join(format!(".{id}.{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        restrict_file(&temporary)?;
        fs::rename(&temporary, destination).map_err(|error| error.to_string())
    }

    pub(crate) fn load<T: DeserializeOwned>(
        &self,
        kind: &str,
        id: uuid::Uuid,
    ) -> Result<Option<T>, String> {
        let path = self.root.join(kind).join(format!("{id}.json"));
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let envelope: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let saved_at = envelope["saved_at_unix_seconds"]
            .as_u64()
            .ok_or_else(|| "stored plan has no valid timestamp".to_owned())?;
        if unix_seconds()?.saturating_sub(saved_at) > self.ttl_seconds {
            let _ = fs::remove_file(path);
            return Ok(None);
        }
        serde_json::from_value(envelope["value"].clone())
            .map(Some)
            .map_err(|error| error.to_string())
    }
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

#[cfg(unix)]
fn restrict_file(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn restrict_file(_path: &std::path::Path) -> Result<(), String> {
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
                "repairs",
                id,
                &ExamplePlan {
                    value: "previewable".to_owned(),
                },
            )
            .unwrap();
        assert_eq!(
            reader.load("repairs", id).unwrap(),
            Some(ExamplePlan {
                value: "previewable".to_owned()
            })
        );
        fs::remove_dir_all(root).unwrap();
    }
}
