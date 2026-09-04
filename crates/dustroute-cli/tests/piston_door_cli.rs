use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../dustroute-translate/tests/fixtures/3x3_piston_shuttle_fanout.json"
);

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dustroute-cli"))
}

fn report(output: std::process::Output) -> Value {
    assert!(
        output.stderr.is_empty(),
        "CLI wrote unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("CLI stdout must be JSON")
}

#[test]
fn cycle_command_executes_fixture_and_returns_machine_readable_traces() {
    let output = cli()
        .args(["run-piston-door", FIXTURE, "cycle"])
        .output()
        .expect("run-piston-door should start");
    assert!(output.status.success());
    let report = report(output);
    assert_eq!(report["ok"], true);
    assert_eq!(report["status"], "complete");
    assert_eq!(
        report["report_schema"],
        "dustroute.piston-door-execution-report.v1"
    );
    assert_eq!(report["mode"], "cycle");
    assert_eq!(
        report["scenario_id"],
        "lever_controlled_one_to_three_to_nine_piston_shuttle"
    );
    assert_eq!(report["initial_state"]["block_count"], 268);
    assert_eq!(report["final_state"]["block_count"], 268);
    assert_eq!(report["trace_status"]["kind"], "complete");
    assert!(
        report["transition_trace"]["records"]
            .as_array()
            .is_some_and(|records| !records.is_empty())
    );
    assert!(
        report["event_trace"]["records"]
            .as_array()
            .is_some_and(|records| !records.is_empty())
    );
}

#[test]
fn open_command_accepts_translated_layouts() {
    let output = cli()
        .args([
            "run-piston-door",
            FIXTURE,
            "open",
            "--translate",
            "37,11,29",
        ])
        .output()
        .expect("translated run-piston-door should start");
    assert!(output.status.success());
    let report = report(output);
    assert_eq!(report["ok"], true);
    assert_eq!(report["mode"], "open");
    assert_eq!(
        report["translation"],
        serde_json::json!({ "x": 37, "y": 11, "z": 29 })
    );
    assert_eq!(report["trace_status"]["kind"], "complete");
    let translated_open_cell = report["final_state"]["blocks"]
        .as_array()
        .expect("final blocks must be an array")
        .iter()
        .any(|record| {
            record["position"] == serde_json::json!({"x": 37, "y": 11, "z": 30})
                && record["block"]["kind"] == "Solid"
        });
    assert!(translated_open_cell, "translated open door cell is missing");
}

#[test]
fn open_command_accepts_json_from_stdin() {
    let mut child = cli()
        .args(["run-piston-door", "-", "open"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("stdin run-piston-door should start");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(include_bytes!(
            "../../dustroute-translate/tests/fixtures/3x3_piston_shuttle_fanout.json"
        ))
        .expect("write scenario JSON to stdin");
    let output = child.wait_with_output().expect("wait for stdin command");
    assert!(output.status.success());
    let report = report(output);
    assert_eq!(report["ok"], true);
    assert_eq!(report["mode"], "open");
    assert_eq!(report["trace_status"]["kind"], "complete");
}

#[test]
fn malformed_scenario_returns_structured_fail_closed_json() {
    let path = std::env::temp_dir().join(format!(
        "dustroute-invalid-piston-door-{}.json",
        std::process::id()
    ));
    let invalid =
        include_str!("../../dustroute-translate/tests/fixtures/3x3_piston_shuttle_fanout.json")
            .replace(
                "\"pulse_width_game_ticks\": 80",
                "\"pulse_width_game_ticks\": 0",
            );
    fs::write(&path, invalid).expect("write invalid scenario fixture");
    let path_string = path.to_string_lossy().into_owned();
    let output = cli()
        .args(["run-piston-door", &path_string])
        .output()
        .expect("invalid run-piston-door should start");
    fs::remove_file(&path).expect("remove temporary invalid fixture");
    assert!(!output.status.success());
    let report = report(output);
    assert_eq!(report["ok"], false);
    assert_eq!(report["status"], "failed");
    assert_eq!(report["error"]["code"], "invalid_scenario");
    assert_eq!(report["error"]["stage"], "parse");
    assert!(
        report["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("pulse width"))
    );
}

#[test]
fn invalid_mode_or_option_returns_structured_argument_error() {
    let output = cli()
        .args(["run-piston-door", FIXTURE, "bogus"])
        .output()
        .expect("invalid command should start");
    assert!(!output.status.success());
    let report = report(output);
    assert_eq!(report["ok"], false);
    assert_eq!(report["error"]["code"], "invalid_arguments");
    assert_eq!(report["error"]["stage"], "arguments");
}
