//! Declared block-state examples; not observed Java conformance artifacts.
use dustroute_translate::piston_construction::{ElectricalModification, electrical_snapshot};
use dustroute_translate::snapshot::{
    MinecraftSnapshot, index_literal_snapshot, world_from_snapshot,
};
use dustroute_translate::world::time::piston_runtime::{
    ElectricalPistonRuntime, schedule_electrical_input_after_tick,
};
use dustroute_translate::world::{Pos, Region, time::piston_runtime::new_piston_runtime};

#[allow(dead_code)]
#[path = "support/flying_machine_blueprint.rs"]
mod flight_fixture;

fn snapshot() -> MinecraftSnapshot {
    serde_json::from_str(include_str!("fixtures/sugar-cane-static-root.json")).unwrap()
}

#[test]
fn native_export_preserves_soil_water_and_ages_through_the_common_runtime() {
    let before = snapshot();
    let region = Region::new(before.min, before.max);
    let world = world_from_snapshot(&before).unwrap();
    let mut run = new_piston_runtime(world, region, Default::default()).unwrap();
    run.run_until_idle().unwrap();
    let actual = electrical_snapshot(run.view().world(), region).unwrap();
    assert_eq!(
        index_literal_snapshot(&actual).unwrap(),
        index_literal_snapshot(&before).unwrap()
    );
}

#[test]
fn differential_harvest_declares_the_cascade_and_can_restore_the_original_column() {
    let before = snapshot();
    let mut after = before.clone();
    after
        .blocks
        .retain(|b| ![Pos::new(0, 2, 0), Pos::new(0, 3, 0)].contains(&b.pos));
    let plan = ElectricalModification::new(&before, &after, Default::default()).unwrap();
    assert_eq!(plan.steps(false).len(), 1);
    assert_eq!(plan.steps(false)[0].position, Pos::new(0, 2, 0));
    assert!(plan.steps(false)[0].wait_ticks >= 1);
    assert_eq!(plan.steps(true).len(), 2);
    assert_eq!(
        index_literal_snapshot(&plan.steps(true).last().unwrap().expected.materialize()).unwrap(),
        index_literal_snapshot(&before).unwrap()
    );
    let mut incomplete_target = before.clone();
    incomplete_target
        .blocks
        .retain(|b| b.pos != Pos::new(0, 2, 0));
    assert!(ElectricalModification::new(&before, &incomplete_target, Default::default()).is_err());
}

#[test]
fn fixed_water_cannot_be_removed_or_have_its_container_opened_by_a_revision() {
    let before = snapshot();
    for pos in [Pos::new(1, 0, 0), Pos::new(2, 0, 0)] {
        let mut after = before.clone();
        after.blocks.retain(|b| b.pos != pos);
        assert!(ElectricalModification::new(&before, &after, Default::default()).is_err());
    }
}

#[test]
fn observed_engine_harvests_four_columns_and_preserves_the_declared_native_environment() {
    let before = flight_fixture::observed_cane_initial();
    let region = Region::new(before.min, before.max);
    let world = dustroute_translate::snapshot::assembly_from_snapshot(
        &before,
        "literal observed engine with proposed field",
        vec![region],
    )
    .unwrap()
    .inspect(&Default::default())
    .unwrap()
    .proposed_world();
    let mut run = new_piston_runtime(world, region, Default::default()).unwrap();
    schedule_electrical_input_after_tick(&mut run, 1, Pos::new(0, 1, 2), true).unwrap();
    let mut checkpoint = None;
    let mut behavior = None;
    while let Some(record) = run.microstep().unwrap() {
        if checkpoint.is_none()
            && record
                .carrier_changes
                .iter()
                .any(|(_, _, after)| after.is_some())
        {
            checkpoint = Some(run.checkpoint());
        }
        if behavior.is_none()
            && run.at_input_boundary()
            && run
                .view()
                .world()
                .iter()
                .any(|(_, b)| b.kind == dustroute_translate::world::BlockKind::MovingPiston)
        {
            behavior = Some(run.behavior_state().unwrap());
        }
    }
    let expected: MinecraftSnapshot = serde_json::from_str(include_str!(
        "fixtures/observed-machine-cane-flight.expected.json"
    ))
    .unwrap();
    // Expected is the declared four-block translation and retained root/field
    // states, not an output snapshot captured from this execution.
    assert_eq!(
        index_literal_snapshot(&electrical_snapshot(run.view().world(), region).unwrap()).unwrap(),
        index_literal_snapshot(&expected).unwrap()
    );
    let mut exact = ElectricalPistonRuntime::from_checkpoint(&checkpoint.unwrap()).unwrap();
    let mut explored = ElectricalPistonRuntime::from_behavior_state(&behavior.unwrap()).unwrap();
    exact.run_until_idle().unwrap();
    explored.run_until_idle().unwrap();
    assert_eq!(exact.state_key(), run.state_key());
    assert_eq!(
        explored.behavior_state().unwrap(),
        run.behavior_state().unwrap()
    );
}
