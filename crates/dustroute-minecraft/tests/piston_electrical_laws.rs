use dustroute_minecraft::piston_electrical_law::*;

#[test]
fn expanded_payload_admits_moving_sources_and_blocks_heads_without_rewriting_legacy_rules() {
    use dustroute_minecraft::piston_law::{builtin_piston_laws, electrical_payload_laws};
    use dustroute_minecraft::{Block, BlockKind};
    let source = Block::new(BlockKind::RedstoneBlock);
    assert_eq!(builtin_piston_laws().payload_rejection(&source).unwrap(), 1);
    assert_eq!(
        electrical_payload_laws()
            .payload_rejection(&source)
            .unwrap(),
        0
    );
    let head = Block::new(BlockKind::PistonHead);
    assert_eq!(builtin_piston_laws().payload_rejection(&head).unwrap(), 1);
    assert_eq!(
        electrical_payload_laws().payload_rejection(&head).unwrap(),
        4
    );
    assert_eq!(
        electrical_payload_laws()
            .payload_rejection(&Block::new(BlockKind::RedstoneWire))
            .unwrap(),
        1
    );
}

fn emission(
    source: SignalSource,
    query: QueryDirection,
    aligned: bool,
    connected: bool,
    enabled: bool,
    level: u8,
) -> Emission {
    builtin_laws()
        .emission(EmissionFacts {
            source,
            level,
            query,
            direction_match: aligned,
            wire_connected: connected,
            wires_enabled: enabled,
        })
        .unwrap()
}

#[test]
fn java_sources_distinguish_weak_strong_and_query_direction() {
    for level in 0..=15 {
        for query in [
            QueryDirection::Down,
            QueryDirection::Up,
            QueryDirection::Horizontal,
        ] {
            for aligned in [false, true] {
                for connected in [false, true] {
                    for enabled in [false, true] {
                        assert_eq!(
                            emission(
                                SignalSource::Passive,
                                query,
                                aligned,
                                connected,
                                enabled,
                                level
                            ),
                            Emission { weak: 0, strong: 0 }
                        );
                        assert_eq!(
                            emission(
                                SignalSource::Lever,
                                query,
                                aligned,
                                connected,
                                enabled,
                                level
                            ),
                            Emission {
                                weak: level,
                                strong: if aligned { level } else { 0 }
                            }
                        );
                        assert_eq!(
                            emission(
                                SignalSource::RedstoneBlock,
                                query,
                                aligned,
                                connected,
                                enabled,
                                level
                            ),
                            Emission {
                                weak: 15,
                                strong: 0
                            }
                        );
                        let signal = if aligned { level } else { 0 };
                        assert_eq!(
                            emission(
                                SignalSource::Repeater,
                                query,
                                aligned,
                                connected,
                                enabled,
                                level
                            ),
                            Emission {
                                weak: signal,
                                strong: signal
                            }
                        );
                        let wire = match query {
                            QueryDirection::Down => 0,
                            QueryDirection::Up => {
                                if enabled {
                                    level
                                } else {
                                    0
                                }
                            }
                            QueryDirection::Horizontal => {
                                if enabled && connected {
                                    level
                                } else {
                                    0
                                }
                            }
                        };
                        assert_eq!(
                            emission(
                                SignalSource::Wire,
                                query,
                                aligned,
                                connected,
                                enabled,
                                level
                            ),
                            Emission {
                                weak: wire,
                                strong: wire
                            }
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn conductors_relay_strong_input_without_recursing_through_other_conductors() {
    let law = builtin_laws();
    for weak in 0..=15 {
        for strong in 0..=15 {
            assert_eq!(law.emitted(weak, strong, true), Some(weak.max(strong)));
            assert_eq!(law.emitted(weak, strong, false), Some(weak));
        }
    }
    assert_eq!(law.emitted(16, 0, true), None);
    assert_eq!(law.emitted(0, 16, true), None);
}

#[test]
fn front_exclusion_applies_only_to_direct_queries() {
    let law = builtin_laws();
    assert!(!law.accepts(PistonPowerQuery::Adjacent, true, true));
    assert!(law.accepts(PistonPowerQuery::Adjacent, false, true));
    for query in [PistonPowerQuery::OwnPosition, PistonPowerQuery::Quasi] {
        for front in [false, true] {
            assert!(law.accepts(query, front, true));
            assert!(!law.accepts(query, front, false));
        }
    }
}

#[test]
fn a_short_repeater_input_produces_a_pulse_and_locks_suppress_delivered_ticks() {
    let law = builtin_laws();
    for delay in 1..=4 {
        let facts = RepeaterFacts {
            powered: false,
            input: false,
            locked: false,
            target_misaligned: false,
            delay,
        };
        // A scheduled ON remains an ON even if its original input has fallen.
        let short = law.repeater(facts).unwrap();
        assert!(!short.request);
        assert!(short.apply && short.next_powered && short.schedule_off);
        assert_eq!(short.delay_game_ticks, u64::from(delay) * 2);
        let locked = law
            .repeater(RepeaterFacts {
                locked: true,
                ..facts
            })
            .unwrap();
        assert!(!locked.apply && !locked.request && !locked.schedule_off);
        let off = law
            .repeater(RepeaterFacts {
                powered: true,
                ..facts
            })
            .unwrap();
        assert!(off.request && off.apply);
        assert!(!off.next_powered && !off.schedule_off);
        assert_eq!(off.priority, 1);
        let on = law
            .repeater(RepeaterFacts {
                input: true,
                ..facts
            })
            .unwrap();
        assert!(on.request && on.apply && on.next_powered);
        assert!(!on.schedule_off);
        assert_eq!(on.priority, 2);
        assert_eq!(
            law.repeater(RepeaterFacts {
                target_misaligned: true,
                ..facts
            })
            .unwrap()
            .priority,
            0
        );
        let unchanged = law
            .repeater(RepeaterFacts {
                powered: true,
                input: true,
                ..facts
            })
            .unwrap();
        assert!(!unchanged.apply && !unchanged.request && unchanged.next_powered);
    }
    for delay in [0, 5, 255] {
        assert!(
            law.repeater(RepeaterFacts {
                powered: false,
                input: true,
                locked: false,
                target_misaligned: false,
                delay
            })
            .is_none()
        );
    }
}

#[test]
fn new_programs_round_trip_without_rebinding_legacy_rules() {
    let programs = builtin_programs();
    let json = serde_json::to_string(programs).unwrap();
    let decoded = serde_json::from_str(&json).unwrap();
    ElectricalLaws::compile(&decoded).unwrap();
    assert_eq!(programs, &decoded);
    let old = dustroute_minecraft::repeater_law::builtin_bounded_law();
    assert_eq!(old.scheduled_output(false, true), None);
    let new = builtin_laws()
        .repeater(RepeaterFacts {
            powered: false,
            input: false,
            locked: false,
            target_misaligned: false,
            delay: 1,
        })
        .unwrap();
    assert!(new.next_powered && new.schedule_off);
}
