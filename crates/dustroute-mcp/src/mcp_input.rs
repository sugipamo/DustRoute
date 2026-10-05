//! MCP input codec. Measurements travel with their exact, owned request;
//! application execution does not encode JSON or accept a caller's byte count.
use crate::blueprint_mcp::BlueprintWrite;
use serde::Serialize;

/// An input measurement, never persisted or used as adoption/placement evidence.
/// Private fields and a consuming accessor prevent replacing the measured body.
pub(crate) struct MeasuredBlueprintWrite {
    request: BlueprintWrite,
    bytes: Result<usize, String>,
}

impl MeasuredBlueprintWrite {
    pub(crate) fn measure(request: BlueprintWrite) -> Self {
        let bytes = json_bytes(&request);
        Self { request, bytes }
    }

    /// Called only at the existing acceptance gate, after catalog lock/load.
    pub(crate) fn into_checked(self, limit: usize) -> Result<BlueprintWrite, String> {
        if self.bytes? > limit {
            return Err("Blueprint request exceeds 4 MiB".into());
        }
        Ok(self.request)
    }
}

fn json_bytes(value: &impl Serialize) -> Result<usize, String> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint_mcp::{BlueprintRecords, Command, execute};
    use crate::state::PlanStateStore;

    struct FailingEncoding;
    impl Serialize for FailingEncoding {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("measurement failed"))
        }
    }

    #[test]
    fn encoding_failure_is_retained_until_the_catalog_acceptance_gate() {
        let input = || MeasuredBlueprintWrite {
            request: BlueprintWrite::Import {
                records: BlueprintRecords::default(),
            },
            bytes: json_bytes(&FailingEncoding),
        };
        assert_eq!(
            input().into_checked(4 * 1024 * 1024).err().unwrap(),
            "measurement failed"
        );
        let root = std::env::temp_dir().join(format!("dustroute-input-{}", uuid::Uuid::new_v4()));
        let store = PlanStateStore::new(root.clone(), 1);
        let directory = store.blueprint_root("Tester");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("catalog.store");
        std::fs::write(&path, b"broken archive").unwrap();
        let error = execute(&store, "Tester", Command::Write(input())).unwrap_err();
        assert!(error.starts_with("invalid current MCP Blueprint store:"));
        assert_eq!(std::fs::read(&path).unwrap(), b"broken archive");
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            execute(&store, "Tester", Command::Write(input())).unwrap_err(),
            "measurement failed"
        );
        assert!(!path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
