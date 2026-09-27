//! Bounded counterexample search using the unchanged public behavior model.
//! A bad exact fixed point disproves the candidate relation. Finding none does
//! not prove adoption, and this diagnostic cannot publish candidate records.
#[allow(dead_code)]
#[path = "../tests/support/reference_door_blueprint.rs"]
mod fixture;

use std::time::{Duration, Instant};

use dustroute_library::blueprint::{InstanceId, TypeContract};
use dustroute_minecraft::time::piston_runtime::{
    PistonEvent, new_piston_runtime, schedule_electrical_input_after_tick,
};
use dustroute_translate::behavior_type::{BehaviorCounterexample, BehaviorModel, WitnessAction};
use dustroute_translate::piston_construction::electrical_snapshot;
use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;
use dustroute_translate::{BlockKind, Pos};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let f = fixture::fixture();
    let mut catalog = f.catalog;
    catalog.insert_revisions(f.request.revisions)?;
    let mechanism = catalog.revision(&f.request.next_child).unwrap();
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog,
        &f.request.candidate_state.assembly,
        &vec![InstanceId::new("root")?, InstanceId::new("mechanism")?],
        &mechanism.behavior_bindings[0],
        &f.context,
    )?;
    let TypeContract::RepeatedSettling { relation } = &model.definition().contract else {
        return Err("expected repeated-settling candidate".into());
    };
    let expected = &relation
        .rows
        .iter()
        .find(|r| r.inputs == [false])
        .unwrap()
        .outputs;
    let start = Instant::now();
    let mut on = model.with_inputs(&model.initial_state()?, &[true])?;
    let mut cases = Vec::new();
    let mut witness = None;
    'cases: for advances_before_off in 0..64 {
        let mut state = model.with_inputs(&on, &[false])?;
        for held_advances in 0..1024 {
            if start.elapsed() >= Duration::from_secs(30) {
                cases.push(
                    json!({"advances_before_off":advances_before_off,"status":"budget_exhausted"}),
                );
                break 'cases;
            }
            let (next, intermediate) = model.step_with_observations(&state)?;
            if next == state {
                let outputs = model.outputs(&state)?;
                let mismatch = outputs != *expected || intermediate.iter().any(|o| o != expected);
                cases.push(json!({"advances_before_off":advances_before_off,
                    "held_advances_to_fixed_point":held_advances,
                    "status":if mismatch {"counterexample"} else {"correct_fixed_point"},
                    "outputs":outputs}));
                if mismatch {
                    let mut prefix = vec![WitnessAction::SetInputs { inputs: vec![true] }];
                    prefix.extend((0..advances_before_off).map(|_| WitnessAction::Advance));
                    prefix.push(WitnessAction::SetInputs {
                        inputs: vec![false],
                    });
                    prefix.extend((0..held_advances).map(|_| WitnessAction::Advance));
                    // Replay from the declared fresh state, without branch
                    // state reuse, and verify exact recurrence twice.
                    let mut replay = model.initial_state()?;
                    for action in &prefix {
                        replay = match action {
                            WitnessAction::SetInputs { inputs } => {
                                model.with_inputs(&replay, inputs)?
                            }
                            WitnessAction::Advance => model.step(&replay)?,
                        };
                    }
                    assert_eq!(replay, state);
                    assert_eq!(model.with_inputs(&replay, &[false])?, replay);
                    for _ in 0..2 {
                        let (again, samples) = model.step_with_observations(&replay)?;
                        assert_eq!(again, replay);
                        assert_eq!(samples, intermediate);
                        assert_eq!(model.outputs(&again)?, outputs);
                    }
                    let mut cycle_outputs = vec![outputs];
                    cycle_outputs.extend(intermediate);
                    witness = Some(BehaviorCounterexample {
                        prefix,
                        held_inputs: vec![false],
                        expected_outputs: expected.clone(),
                        cycle_outputs,
                    });
                    break 'cases;
                }
                break;
            }
            state = next;
            if held_advances == 1023 {
                cases.push(json!({"advances_before_off":advances_before_off,"status":"no_fixed_point_within_bound"}));
            }
        }
        on = model.step(&on)?;
    }
    let model_elapsed = start.elapsed().as_secs_f64();
    // Independent replay with the existing post-world-tick input helper.
    // These are model schedules, not evidence of actual packet application.
    let mut tick_cases = Vec::new();
    let mut first_tick_failure = None;
    for width in (1..=32).chain([100]) {
        let mut run = new_piston_runtime(
            f.world.clone(),
            f.context.known_region,
            f.context.root_limits,
        )?;
        let input = Pos::new(0, 11, 0);
        schedule_electrical_input_after_tick(&mut run, 0, input, true)?;
        schedule_electrical_input_after_tick(&mut run, width, input, false)?;
        run.run_until_idle()?;
        let mut occupied = Vec::new();
        for y in 6..=8 {
            for z in -1..=1 {
                let position = Pos::new(0, y, z);
                let observed = run.view().observe_location(position)?;
                if observed.block().kind != BlockKind::Air {
                    occupied.push(json!({"position":position,"block":observed.block()}));
                }
            }
        }
        let wrong = !occupied.is_empty();
        let case = json!({
            "on_after_world_tick":0,"off_after_world_tick":width,
            "status":if wrong {"aperture_not_open"} else {"aperture_open"},
            "occupied_aperture":occupied,
            "initial_world_restored":run.view().world()==&f.world,
            "pending_roots":run.pending_count(),"settled_at":run.view().time(),
            "input_deliveries":run.trace().iter().filter(|r|
                matches!(r.invocation.call.payload,PistonEvent::Input{..})
            ).map(|r|&r.invocation).collect::<Vec<_>>(),
        });
        if wrong && first_tick_failure.is_none() {
            first_tick_failure = Some(json!({
                "case":case,"trace":run.trace(),
                "settled":electrical_snapshot(run.view().world(),f.context.known_region)?,
            }));
        }
        tick_cases.push(case);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version":"dustroute.reference-door-interruption-probe.v2",
            "context":model.context(),"type_revision":model.definition(),
            "status":if witness.is_some() {"counterexample_found"} else {"no_counterexample_within_probe"},
            "cases":cases,"counterexample":witness,"fresh_replay_checked":witness.is_some(),
            "model_probe_elapsed_seconds":model_elapsed,
            "limits":{"prefixes":64,"held_advances":1024,"seconds":30},
            "input_boundary":"existing runtime model advances, not game ticks or measured server input times",
            "post_world_tick_cases":tick_cases,"first_post_world_tick_failure":first_tick_failure,
            "public_adoption_performed":false,"live_evidence":false,
        }))?
    );
    Ok(())
}
