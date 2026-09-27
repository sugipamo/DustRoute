//! Retained Java counterexample: fresh prefix is an explicit test precondition,
//! not a claim that a client snapshot reconstructs hidden runtime history.
use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::time::piston_runtime::new_piston_runtime;
use dustroute_translate::piston_construction::electrical_snapshot;
use dustroute_translate::snapshot::assembly_from_snapshot;
use dustroute_translate::{MinecraftSnapshot, Region};

#[test]
fn observer_command_reproduces_retained_stage_38_displacement() {
    let evidence: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/reference-door-command-placement-v1.json"
    ))
    .unwrap();
    let prefix: MinecraftSnapshot =
        serde_json::from_value(evidence["expected_after_verified_prefix"].clone()).unwrap();
    let expected: MinecraftSnapshot =
        serde_json::from_value(evidence["attempted_step"]["expected"].clone()).unwrap();
    let position = serde_json::from_value(evidence["attempted_step"]["position"].clone()).unwrap();
    let region = Region::new(prefix.min, prefix.max);
    let world = |snapshot: &MinecraftSnapshot| {
        assembly_from_snapshot(snapshot, "retained command counterexample", vec![region])
            .unwrap()
            .inspect(&BlueprintCatalog::default())
            .unwrap()
            .proposed_world()
    };
    let commands: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/reference-door-command-prefix-v1.json"
    ))
    .unwrap();
    let requested: Vec<dustroute_translate::MinecraftSnapshotBlock> =
        serde_json::from_value(commands["commands"].clone()).unwrap();
    let mut run = new_piston_runtime(
        dustroute_translate::World::new(),
        region,
        Default::default(),
    )
    .unwrap();
    run.run_until_idle().unwrap();
    for command in &requested[..37] {
        let snapshot = MinecraftSnapshot {
            min: region.min,
            max: region.max,
            blocks: vec![command.clone()],
        };
        run.install_now(
            command.pos,
            world(&snapshot).get(command.pos).unwrap().clone(),
        )
        .unwrap();
        run.run_until_idle().unwrap();
    }
    assert_eq!(
        electrical_snapshot(run.view().world(), region).unwrap(),
        prefix
    );
    let block = world(&expected).get(position).unwrap().clone();
    run.install_now(position, block).unwrap();
    run.run_until_idle().unwrap();
    let actual = electrical_snapshot(run.view().world(), region).unwrap();
    assert_ne!(
        actual, expected,
        "the historical expected pass was incorrect"
    );
    for observed in evidence["failure_observations"].as_array().unwrap() {
        let mut live: MinecraftSnapshot =
            serde_json::from_value(observed["snapshot"].clone()).unwrap();
        live.blocks.sort_by_key(|b| b.pos);
        assert_eq!(actual, live);
    }
}
