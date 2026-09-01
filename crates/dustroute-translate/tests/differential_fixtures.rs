use std::fs;
use std::path::PathBuf;

use dustroute_translate::{PhysicalTrace, TraceSource};

#[test]
fn promoted_minecraft_differential_traces_are_valid_and_have_metadata() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/differential");
    for entry in fs::read_dir(&directory).expect("differential fixture directory") {
        let path = entry.expect("fixture entry").path();
        if !path.to_string_lossy().ends_with(".trace.json") {
            continue;
        }
        let trace: PhysicalTrace =
            serde_json::from_str(&fs::read_to_string(&path).expect("read promoted trace"))
                .expect("promoted trace schema");
        assert_eq!(trace.source, TraceSource::MinecraftJava, "{path:?}");
        assert!(!trace.observations.is_empty(), "{path:?}");
        let metadata = PathBuf::from(path.to_string_lossy().replace(".trace.json", ".meta.json"));
        let metadata: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&metadata).expect("read promoted trace metadata"),
        )
        .expect("promoted metadata schema");
        assert_eq!(
            metadata["schema_version"],
            "dustroute.differential-fixture.v1"
        );
        assert!(
            metadata["reason"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty())
        );
    }
}
