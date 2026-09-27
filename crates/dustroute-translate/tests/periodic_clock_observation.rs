//! The effects profile must match the original independent observations. Retain
//! the old profile's counterexample too: existing pins must not change silently.
#[path = "../examples/compare_periodic_clock.rs"]
mod comparison;

use dustroute_library::behavior_type::PhysicalBehaviorProfile;
use serde_json::{Value, json};

const AUTONOMOUS: &str = include_str!("fixtures/periodic_clock_1_21_11.json");
const RECOVERY: &str = include_str!("fixtures/periodic_clock_recovery_1_21_11.json");
const QUEUE: &str = include_str!("fixtures/periodic_clock_queue_1_21_11.json");

#[test]
fn legacy_autonomous_feedback_divergence_is_preserved() {
    let report = comparison::compare_with_profile(
        AUTONOMOUS,
        PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
    )
    .unwrap();
    assert_eq!(report["samples_compared"], 641);
    assert_eq!(report["all_sampled_block_states_match"], false);
    assert_eq!(report["difference_count"], 48);
    let differences = report["differences"].as_array().unwrap();
    assert_eq!(differences.first().unwrap()["game_tick"], 190);
    assert_eq!(differences.last().unwrap()["game_tick"], 599);
    assert!(differences.iter().all(|d| {
        d["model"]["torch_lit"] == true
            && d["model"]["dust_power"] == 15
            && d["observed"]["torch_lit"] == false
            && d["observed"]["dust_power"] == 0
    }));
    let observed = report["observed_torch_edges"].as_array().unwrap();
    assert_eq!(observed.len(), 15);
    assert_eq!(
        observed.last().unwrap(),
        &json!({"game_tick":30,"lit":false})
    );
    // Complete-state recurrence holds in this model, while the model fails live comparison.
    assert_eq!(report["model_proof"]["behavior"]["status"], "passed");
    assert_eq!(
        report["finite_burst_model_proof"]["behavior"]["status"],
        "failed"
    );
    assert_eq!(
        report["model_proof"]["behavior"]["cycle"]["state_period_steps"],
        190
    );
    assert_eq!(report["live_infinite_recurrence_proven"], false);
    assert_eq!(report["internal_callback_order_verified"], false);
}

#[test]
fn effects_profile_matches_all_autonomous_samples_and_rejects_the_clock_type() {
    let report = comparison::compare(AUTONOMOUS).unwrap();
    assert_eq!(
        report["profile"],
        "dustroute.dust-single-torch-block-effects.v1"
    );
    assert_eq!(report["samples_compared"], 641);
    assert_eq!(report["all_sampled_block_states_match"], true);
    assert_eq!(report["difference_count"], 0);
    assert_eq!(
        report["modeled_torch_edges"],
        report["observed_torch_edges"]
    );
    assert_eq!(report["autonomous_observation"], true);
    assert_eq!(report["model_proof"]["profile"], report["profile"]);
    assert_eq!(report["model_proof"]["behavior"]["status"], "failed");
    let burst = &report["finite_burst_model_proof"];
    assert_eq!(burst["profile"], report["profile"]);
    assert_eq!(burst["behavior"]["status"], "passed");
    assert_eq!(burst["behavior"]["cessation"]["falling_edges"], 8);
    assert_eq!(burst["behavior"]["cessation"]["off_from_step"], 30);
    assert_eq!(report["restartability_verified"], false);
    assert_eq!(report["live_infinite_recurrence_proven"], false);
    assert_eq!(report["internal_callback_order_verified"], false);
}

#[test]
fn diagnostic_neighbor_notification_is_not_autonomous_evidence() {
    let capture: Value = serde_json::from_str(RECOVERY).unwrap();
    assert_eq!(capture["external_inputs"][0]["game_tick"], 220);
    let samples = capture["samples"].as_array().unwrap();
    assert!(samples[30..222].iter().all(|s| s["torch_lit"] == false));
    assert_eq!(samples[222]["torch_lit"], true);
    assert_eq!(samples[222]["dust_power"], 15);
    assert!(comparison::compare(RECOVERY).is_err());
    let replay = comparison::compare_recovery(RECOVERY).unwrap();
    assert_eq!(replay["samples_compared"], 261);
    assert_eq!(replay["all_sampled_block_states_match"], true);
    assert_eq!(replay["difference_count"], 0);
    assert_eq!(
        replay["modeled_torch_edges"],
        replay["observed_torch_edges"]
    );
    assert_eq!(replay["autonomous_observation"], false);
    assert!(replay["finite_burst_model_proof"].is_null());
    assert_eq!(replay["restartability_verified"], false);
    assert_eq!(replay["model_proof"]["behavior"]["status"], "failed");
    assert!(comparison::compare_recovery(AUTONOMOUS).is_err());
    let mut unexpected_stimulus = capture;
    unexpected_stimulus["external_inputs"][0]["game_tick"] = json!(200);
    assert!(comparison::compare_recovery(&unexpected_stimulus.to_string()).is_err());
}

#[test]
fn saved_queue_diagnostics_distinguish_current_and_stale_snapshots() {
    let replay = comparison::compare(QUEUE).unwrap();
    assert_eq!(replay["samples_compared"], 33);
    assert_eq!(replay["all_sampled_block_states_match"], true);
    let capture: Value = serde_json::from_str(QUEUE).unwrap();
    let checkpoints = capture["queue_checkpoints"].as_array().unwrap();
    let burnout = checkpoints.iter().find(|c| c["game_tick"] == 30).unwrap();
    assert_eq!(burnout["current_queue_verified"], true);
    assert_eq!(burnout["saved_chunk_age_game_ticks"], 0);
    assert_eq!(
        burnout["ticks"],
        json!([{"block":"minecraft:redstone_wall_torch","stored_delay":2,"priority":0}])
    );
    let later = checkpoints.iter().find(|c| c["game_tick"] == 32).unwrap();
    assert_eq!(later["current_queue_verified"], false);
    assert_eq!(later["saved_chunk_age_game_ticks"], 2);
    assert_eq!(later["region_sha256"], burnout["region_sha256"]);
    // Its repeated delay cannot be interpreted as a new callback at tick 34.
}

#[test]
fn comparison_rejects_incomplete_or_changed_capture_scope() {
    let original: Value = serde_json::from_str(AUTONOMOUS).unwrap();
    for (pointer, value) in [
        ("/complete", json!(false)),
        ("/cleanup/blocks_removed", json!(false)),
        ("/external_inputs", json!([{"game_tick":220}])),
        ("/samples/5/game_tick", json!(6)),
        ("/samples/0/torch_lit", json!(false)),
        ("/samples/4/dust_power", json!(16)),
        ("/placement/0/state", json!("minecraft:glass")),
        ("/known_region/max/x", json!(2)),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(
            comparison::compare(&changed.to_string()).is_err(),
            "{pointer}"
        );
    }
    let mut missing_sample = original;
    missing_sample["samples"].as_array_mut().unwrap().pop();
    assert!(comparison::compare(&missing_sample.to_string()).is_err());
}
