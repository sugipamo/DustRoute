//! Replay actual server-applied input intervals at their observed coordinates.
//! These fixtures prove only the recorded settled states, not all root orders.
use std::collections::BTreeMap;

use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::time::piston_runtime::{new_piston_runtime, schedule_electrical_input};
use dustroute_minecraft::{Pos, Region};
use dustroute_translate::MinecraftSnapshot;
use dustroute_translate::snapshot::assembly_from_snapshot;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
struct Input {
    tick: u64,
    position: Pos,
    powered: bool,
}
#[derive(Deserialize)]
struct Observation {
    run_id: String,
    initial: MinecraftSnapshot,
    inputs: Vec<Input>,
    observed_final: MinecraftSnapshot,
    #[serde(default)]
    observed_prefixes: Vec<Prefix>,
}

#[derive(Deserialize)]
struct Prefix {
    input_count: usize,
    blocks: Vec<SampleBlock>,
}
#[derive(Deserialize)]
struct SampleBlock {
    position: Pos,
    name: String,
    properties: BTreeMap<String, String>,
}

#[test]
fn observed_mixed_states_moving_sources_and_short_pulses_replay() {
    for json in [
        include_str!("fixtures/mixed-electrical-observed-b-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-c-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-d-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-e-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-f-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-g-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-h-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-j-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-k-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-l-v1.json"),
    ] {
        let trial: Observation = serde_json::from_str(json).unwrap();
        let region = Region::new(trial.initial.min, trial.initial.max);
        let catalog = BlueprintCatalog::default();
        let import = |snapshot: &MinecraftSnapshot| {
            assembly_from_snapshot(snapshot, "literal observation", vec![region])
                .unwrap()
                .inspect(&catalog)
                .unwrap()
                .proposed_world()
        };
        let mut rt =
            new_piston_runtime(import(&trial.initial), region, Default::default()).unwrap();
        for input in &trial.inputs {
            schedule_electrical_input(&mut rt, input.tick, input.position, input.powered).unwrap();
        }
        rt.run_until_idle().unwrap();
        let identities = |world: &dustroute_minecraft::World| -> BTreeMap<_, _> {
            world
                .iter()
                .map(|(p, b)| (*p, (b.observed_name.clone(), b.observed_properties.clone())))
                .collect()
        };
        assert_eq!(
            identities(rt.view().world()),
            identities(&import(&trial.observed_final)),
            "{}",
            trial.run_id
        );
        assert_eq!(rt.pending_count(), 0);
        for prefix in &trial.observed_prefixes {
            let mut rt =
                new_piston_runtime(import(&trial.initial), region, Default::default()).unwrap();
            for input in trial.inputs.iter().take(prefix.input_count) {
                schedule_electrical_input(&mut rt, input.tick, input.position, input.powered)
                    .unwrap();
            }
            rt.run_until_idle().unwrap();
            for sample in &prefix.blocks {
                let actual = rt.view().block(sample.position).unwrap();
                let name = actual.observed_name.as_deref().unwrap_or("minecraft:air");
                assert_eq!(
                    (name, &actual.observed_properties),
                    (sample.name.as_str(), &sample.properties),
                    "{} prefix {} at {:?}",
                    trial.run_id,
                    prefix.input_count,
                    sample.position
                );
            }
        }
    }
}
