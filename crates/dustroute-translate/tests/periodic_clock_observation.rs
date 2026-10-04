//! Independent wire captures enter at this test boundary. Comparison itself
//! consumes Rust records and retains finite evidence and legacy counterexamples.
use dustroute_library::behavior_type::PhysicalBehaviorProfile;
use dustroute_translate::periodic_clock_observation::{
    self as comparison, ClockCapture, TorchEdge,
};
use dustroute_translate::promotion::CheckStatus;
use serde_json::{Value, json};

const AUTONOMOUS: &str = include_str!("fixtures/periodic_clock_1_21_11.json");
const RECOVERY: &str = include_str!("fixtures/periodic_clock_recovery_1_21_11.json");
const QUEUE: &str = include_str!("fixtures/periodic_clock_queue_1_21_11.json");
fn capture(input: &str) -> ClockCapture {
    serde_json::from_str(input).unwrap()
}

#[test]
fn legacy_autonomous_feedback_divergence_is_preserved() {
    let report = comparison::compare_with_profile(
        &capture(AUTONOMOUS),
        PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
    )
    .unwrap();
    assert_eq!(report.samples_compared, 641);
    assert!(!report.all_sampled_block_states_match);
    assert_eq!(report.difference_count, 48);
    assert_eq!(report.differences.first().unwrap().game_tick, 190);
    assert_eq!(report.differences.last().unwrap().game_tick, 599);
    assert!(report.differences.iter().all(|d| {
        d.model.torch_lit
            && d.model.dust_power == 15
            && !d.observed.torch_lit
            && d.observed.dust_power == 0
    }));
    assert_eq!(report.observed_torch_edges.len(), 15);
    assert_eq!(
        report.observed_torch_edges.last().unwrap(),
        &TorchEdge {
            game_tick: 30,
            lit: false
        }
    );
    // The model proves recurrence yet fails the independent live comparison.
    assert_eq!(report.model_proof.behavior.status, CheckStatus::Passed);
    assert_eq!(
        report
            .finite_burst_model_proof
            .as_ref()
            .unwrap()
            .behavior
            .status,
        CheckStatus::Failed
    );
    assert_eq!(
        report
            .model_proof
            .behavior
            .cycle
            .as_ref()
            .unwrap()
            .state_period_steps,
        190
    );
    assert!(!report.live_infinite_recurrence_proven);
    assert!(!report.internal_callback_order_verified);
}

#[test]
fn effects_profile_matches_all_autonomous_samples_and_rejects_the_clock_type() {
    let report = comparison::compare(&capture(AUTONOMOUS)).unwrap();
    assert_eq!(
        report.profile,
        "dustroute.dust-single-torch-block-effects.v1"
    );
    assert_eq!(report.samples_compared, 641);
    assert!(report.all_sampled_block_states_match);
    assert_eq!(report.difference_count, 0);
    assert_eq!(report.modeled_torch_edges, report.observed_torch_edges);
    assert!(report.autonomous_observation);
    assert_eq!(report.model_proof.profile, report.profile);
    assert_eq!(report.model_proof.behavior.status, CheckStatus::Failed);
    let burst = report.finite_burst_model_proof.as_ref().unwrap();
    assert_eq!(burst.profile, report.profile);
    assert_eq!(burst.behavior.status, CheckStatus::Passed);
    assert_eq!(burst.behavior.cessation.as_ref().unwrap().falling_edges, 8);
    assert_eq!(burst.behavior.cessation.as_ref().unwrap().off_from_step, 30);
    assert!(!report.restartability_verified);
    assert!(!report.live_infinite_recurrence_proven);
    assert!(!report.internal_callback_order_verified);
}

#[test]
fn diagnostic_neighbor_notification_is_not_autonomous_evidence() {
    let mut capture = capture(RECOVERY);
    assert_eq!(capture.external_inputs[0].game_tick, 220);
    assert!(capture.samples[30..222].iter().all(|s| !s.torch_lit));
    assert!(capture.samples[222].torch_lit);
    assert_eq!(capture.samples[222].dust_power, 15);
    assert!(comparison::compare(&capture).is_err());
    let replay = comparison::compare_recovery(&capture).unwrap();
    assert_eq!(replay.samples_compared, 261);
    assert!(replay.all_sampled_block_states_match);
    assert_eq!(replay.difference_count, 0);
    assert_eq!(replay.modeled_torch_edges, replay.observed_torch_edges);
    assert!(!replay.autonomous_observation);
    assert!(replay.finite_burst_model_proof.is_none());
    assert!(!replay.restartability_verified);
    assert_eq!(replay.model_proof.behavior.status, CheckStatus::Failed);
    assert!(comparison::compare_recovery(&self::capture(AUTONOMOUS)).is_err());
    capture.external_inputs[0].game_tick = 200;
    assert!(comparison::compare_recovery(&capture).is_err());
}

#[test]
fn saved_queue_diagnostics_distinguish_current_and_stale_snapshots() {
    let replay = comparison::compare(&capture(QUEUE)).unwrap();
    assert_eq!(replay.samples_compared, 33);
    assert!(replay.all_sampled_block_states_match);
    // Queue metadata remains independent evidence; comparison cannot restore it
    // into hidden runtime state or infer callbacks from stale disk snapshots.
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
        let checked = serde_json::from_value::<ClockCapture>(changed)
            .map_err(|e| e.to_string())
            .and_then(|c| comparison::compare(&c));
        assert!(checked.is_err(), "{pointer}");
    }
    let mut missing_sample = capture(AUTONOMOUS);
    missing_sample.samples.pop();
    assert!(comparison::compare(&missing_sample).is_err());
}

#[test]
fn extra_placement_or_stimulus_fields_do_not_weaken_fixed_scope() {
    let original: Value = serde_json::from_str(RECOVERY).unwrap();
    for pointer in [
        "/placement/0",
        "/placement/0/position",
        "/external_inputs/0",
    ] {
        let mut changed = original.clone();
        changed
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(
            serde_json::from_value::<ClockCapture>(changed).is_err(),
            "{pointer}"
        );
    }
}
