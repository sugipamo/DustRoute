//! Independent wire-fixture decoding; production normalization takes typed states.
use dustroute_translate::observed_properties::ObservedProperty;
use dustroute_translate::transition_conformance::{ObservedBlockState, ObservedTransitionEvent};
use dustroute_translate::vanilla_instrumentation::{
    InstrumentedBlockState, VanillaInstrumentationArtifact,
};
use serde_json::{Value, json};

#[test]
fn scalar_values_keep_their_type_and_full_integer_width() {
    let wire = json!({"name":"minecraft:repeater","properties":{
        "text":"false", "powered":false, "minimum":i64::MIN, "maximum":u64::MAX, "zero":0
    }});
    let state: InstrumentedBlockState = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(
        state.properties["text"],
        ObservedProperty::Text("false".into())
    );
    assert_eq!(
        state.properties["powered"],
        ObservedProperty::Boolean(false)
    );
    assert_eq!(
        state.properties["minimum"].to_string(),
        i64::MIN.to_string()
    );
    assert_eq!(
        state.properties["maximum"].to_string(),
        u64::MAX.to_string()
    );
    assert_eq!(state.properties["zero"].to_string(), "0");
    assert_eq!(serde_json::to_value(state).unwrap(), wire);
}

#[test]
fn malformed_collections_never_become_empty_properties() {
    for malformed in [
        Value::Null,
        json!([]),
        json!(false),
        json!(3),
        json!("empty"),
    ] {
        let wire = json!({"name":"minecraft:stone","properties":malformed});
        assert!(serde_json::from_value::<ObservedBlockState>(wire.clone()).is_err());
        assert!(serde_json::from_value::<InstrumentedBlockState>(wire).is_err());
    }
    // Preserve the existing distinction: instrumentation omitted properties
    // defaults to empty, whereas packet fixtures require the properties field.
    let absent = json!({"name":"minecraft:air"});
    assert!(serde_json::from_value::<ObservedBlockState>(absent.clone()).is_err());
    assert!(
        serde_json::from_value::<InstrumentedBlockState>(absent)
            .unwrap()
            .properties
            .is_empty()
    );
}

#[test]
fn event_rejection_reports_the_property_kind_slot_and_owner_position() {
    for (invalid, kind) in [
        (Value::Null, "Null"),
        (json!([]), "Sequence"),
        (json!({}), "Object"),
        (json!(1.5), "NonIntegerNumber"),
        (json!(1.0), "NonIntegerNumber"),
    ] {
        let wire = json!({
            "sequence":1,"kind":"block_update","relative_game_tick":0,"sub_tick_order":0,
            "scheduler_phase":null,"changed":true,
            "before":{"name":"minecraft:lever","properties":{}},
            "after":{"name":"minecraft:lever","properties":{"powered":invalid}},
            "position":{"x":12,"y":80,"z":-7}
        });
        let error = serde_json::from_value::<ObservedTransitionEvent>(wire)
            .unwrap_err()
            .to_string();
        for fragment in ["after", "powered", kind, "x: 12", "y: 80", "z: -7"] {
            assert!(error.contains(fragment), "missing {fragment} in {error}");
        }
    }
}

#[test]
fn every_instrumentation_state_slot_rejects_invalid_values_before_validation() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/vanilla_1_21_11_offline_piston_input.json"
    ))
    .unwrap();
    for pointer in [
        "/state_events/0/before",
        "/state_events/0/after",
        "/piston_states/0/body",
        "/piston_states/0/head",
        "/piston_states/0/moving_block",
        "/neighbor_updates/0/target",
    ] {
        let mut wire = fixture.clone();
        let state = wire
            .pointer_mut(pointer)
            .expect("fixture has the state slot");
        if state.is_null() {
            *state = json!({"name":"minecraft:stone"});
        }
        state["properties"] = json!({"broken":{"nested":true}});
        let error = serde_json::from_value::<VanillaInstrumentationArtifact>(wire)
            .unwrap_err()
            .to_string();
        assert!(error.contains("broken"), "{pointer}: {error}");
        assert!(error.contains("Object"), "{pointer}: {error}");
        assert!(error.contains(" at Pos"), "{pointer}: {error}");
        assert!(
            error.contains(pointer.rsplit('/').next().unwrap()),
            "{pointer}: {error}"
        );
    }
}
