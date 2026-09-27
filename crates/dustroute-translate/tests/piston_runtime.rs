//! Old partial observations retain their provenance, but must not be completed
//! with guessed wire arms or unobserved air to run the current electrical model.
//! Actual electrical replays live in `electrical_piston_observations.rs`.
use dustroute_minecraft::time::piston_runtime::new_piston_runtime;
use dustroute_minecraft::time::runtime::{RuntimeError, RuntimeLimits};
use dustroute_minecraft::{Pos, Region};
use dustroute_translate::{MinecraftSnapshot, world_from_snapshot};
use serde_json::Value;

#[test]
fn historical_two_row_observation_lacks_electrical_wire_state() {
    let case: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer/07-single-input-two-row.json"
    ))
    .unwrap();
    let initial: MinecraftSnapshot = serde_json::from_value(case["initial"].clone()).unwrap();
    let error = new_piston_runtime(
        world_from_snapshot(&initial).unwrap(),
        Region::new(initial.min, initial.max),
        RuntimeLimits::default(),
    )
    .err()
    .expect("power-only wire observations must not start electrical execution");
    assert!(
        matches!(&error, RuntimeError::Handler(detail) if detail.contains("complete four-arm wire state required")),
        "{error:?}"
    );
}

#[test]
fn historical_payload_observations_do_not_cover_electrical_queries() {
    for json in [
        include_str!("fixtures/piston-low-layer/03-sticky-piston.json"),
        include_str!("fixtures/piston-low-layer/04-moved-piston-actuation.json"),
    ] {
        let case: Value = serde_json::from_str(json).unwrap();
        let initial: MinecraftSnapshot = serde_json::from_value(case["initial"].clone()).unwrap();
        let region = Region::new(initial.min, initial.max);
        let error = new_piston_runtime(
            world_from_snapshot(&initial).unwrap(),
            region,
            RuntimeLimits::default(),
        )
        .err()
        .expect("old region bounds must not silently gain known air");
        assert!(
            matches!(error, RuntimeError::UnknownSpace(pos) if pos == Pos::new(0, -2, 0)),
            "{error:?}"
        );
        assert!(!region.contains(Pos::new(0, -2, 0)));
    }
}
