use dustroute_translate::vanilla_instrumentation::parse_and_validate_instrumentation;

#[test]
fn offline_piston_input_fixture_is_valid_and_fail_closed_about_scheduler_order() {
    let artifact = parse_and_validate_instrumentation(include_str!(
        "fixtures/vanilla_1_21_11_offline_piston_input.json"
    ))
    .expect("reviewed offline instrumentation fixture must satisfy the contract");

    assert!(artifact.completeness.input_timing);
    assert_eq!(artifact.input.activation_game_tick, 0);
    assert_eq!(artifact.input.first_redstone_change_game_tick, Some(0));
    assert_eq!(artifact.input.first_packet_update_game_tick, Some(0));
    assert!(!artifact.completeness.ordered_ticks);
    assert!(artifact.ordered_ticks.is_empty());
    assert!(artifact.completeness.piston_state);
    assert!(artifact.piston_states.iter().any(|state| state.state_kind
        == dustroute_translate::vanilla_instrumentation::PistonStateKind::Stable));
    assert_eq!(artifact.neighbor_updates.len(), 6);
    assert!(!artifact.completeness.neighbor_updates);
    assert_eq!(artifact.neighbor_updates[0].sub_tick_order, 0);
    assert_eq!(artifact.neighbor_updates[1].position.x, 653);
}

#[test]
fn bounded_capture_fixture_keeps_signed_preroll_and_partial_streams() {
    let artifact = parse_and_validate_instrumentation(include_str!(
        "fixtures/vanilla_1_21_11_bounded_capture.json"
    ))
    .expect("bounded capture contract fixture must satisfy the validator");

    assert_eq!(artifact.ordered_ticks[0].trigger_game_tick, -2);
    assert_eq!(artifact.state_events[0].game_tick, -2);
    assert_eq!(artifact.capture.as_ref().unwrap().sequence_gap_count, 3);
    assert!(!artifact.capture.as_ref().unwrap().sequence_contiguous);
}
