//! Compare an independently captured four-block clock against the pinned model.
//! This is a finite block-state comparison, not proof of infinite live behavior.
use std::collections::BTreeMap;

use dustroute_library::assembly::AssemblyRevision;
use dustroute_library::behavior_type::{FiniteBurst, Periodic, PhysicalBehaviorProfile};
use dustroute_library::blueprint::{
    AssemblyRevisionId, BlueprintRevisionId, TypeContract, TypeRevision, TypeRevisionId,
};
use dustroute_library::builtin_laws::{DUST_LAW_REVISION, TORCH_LAW_REVISION, builtin_laws};
use dustroute_minecraft::{Pos, Region};
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel};
use dustroute_translate::physical_behavior::{
    PhysicalBehaviorModel, PhysicalBehaviorSelection, PhysicalOutput,
};
use dustroute_translate::snapshot::{
    MinecraftSnapshot, MinecraftSnapshotBlock, assembly_from_snapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Capture {
    schema: String,
    minecraft_version: String,
    evidence: String,
    clock: String,
    sampling: String,
    construction: String,
    external_inputs: Vec<Value>,
    diagnostic: Option<String>,
    internal_callback_order: String,
    server_jar_sha1: String,
    complete: bool,
    observation_complete: bool,
    cleanup: BTreeMap<String, bool>,
    duration_game_ticks: usize,
    placement: Value,
    known_region: Region,
    samples: Vec<Sample>,
}

#[derive(Deserialize, Serialize)]
struct Sample {
    game_tick: usize,
    torch_lit: bool,
    torch_facing: String,
    dust_power: u8,
    dust_connections: BTreeMap<String, String>,
    supports_intact: bool,
}

pub fn compare(input: &str) -> Result<Value, String> {
    compare_with_profile(
        input,
        PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
    )
}

pub fn compare_with_profile(
    input: &str,
    profile: PhysicalBehaviorProfile,
) -> Result<Value, String> {
    compare_capture(input, profile, false)
}

pub fn compare_recovery(input: &str) -> Result<Value, String> {
    compare_capture(
        input,
        PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        true,
    )
}

fn compare_capture(
    input: &str,
    profile: PhysicalBehaviorProfile,
    recovery: bool,
) -> Result<Value, String> {
    let capture: Capture = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let expected_stimuli = if recovery {
        vec![json!({"game_tick":220,"kind":"adjacent_glass_place_remove_neighbor_notification"})]
    } else {
        vec![]
    };
    if capture.schema != "dustroute.periodic-clock-observation.v1"
        || capture.minecraft_version != "1.21.11"
        || capture.evidence != "observed_server_block_state"
        || capture.clock != "game_tick"
        || capture.sampling != "frozen_console_after_each_explicit_tick"
        || capture.construction
            != "support_then_upper_block_then_dust_then_lit_wall_torch_in_one_frozen_tick"
        || capture.server_jar_sha1 != "64bb6d763bed0a9f1d632ec347938594144943ed"
        || capture.internal_callback_order != "unavailable"
        || capture.external_inputs != expected_stimuli
        || (recovery
            && (capture.duration_game_ticks < 252
                || capture.diagnostic.as_deref()
                    != Some("saved_block_ticks_and_one_post_burnout_neighbor_notification")))
        || !capture.complete
        || !capture.observation_complete
        || ![
            "blocks_removed",
            "tickets_removed",
            "server_stopped",
            "properties_restored",
        ]
        .iter()
        .all(|key| capture.cleanup.get(*key) == Some(&true))
        || capture.duration_game_ticks == 0
        || capture.duration_game_ticks > 2000
        || capture.samples.len() != capture.duration_game_ticks + 1
    {
        return Err("unsupported or incomplete capture provenance, scope or coverage".into());
    }
    if capture.placement
        != json!([
            {"position":{"x":0,"y":0,"z":0},"state":"minecraft:stone"},
            {"position":{"x":1,"y":1,"z":0},"state":"minecraft:stone"},
            {"position":{"x":0,"y":1,"z":0},"state":"minecraft:redstone_wire"},
            {"position":{"x":1,"y":0,"z":0},"state":"minecraft:redstone_wall_torch[facing=east,lit=true]"}
        ])
        || capture.known_region != Region::new(Pos::new(-2, -2, -2), Pos::new(3, 3, 2))
    {
        return Err(
            "comparison supports only the declared fixed four-block placement and halo".into(),
        );
    }
    let initial = &capture.samples[0];
    if !initial.torch_lit {
        return Err("initial sample does not retain the freshly placed lit torch".into());
    }
    for (tick, sample) in capture.samples.iter().enumerate() {
        if sample.game_tick != tick
            || sample.dust_power > 15
            || sample.dust_connections.len() != 4
            || !["north", "east", "south", "west"].iter().all(|name| {
                sample
                    .dust_connections
                    .get(*name)
                    .is_some_and(|v| ["none", "side", "up"].contains(&v.as_str()))
            })
        {
            return Err(format!(
                "missing, unordered or malformed block sample at tick {tick}"
            ));
        }
    }
    let mut dust_properties = initial.dust_connections.clone();
    dust_properties.insert("power".into(), initial.dust_power.to_string());
    let snapshot = MinecraftSnapshot {
        min: capture.known_region.min,
        max: capture.known_region.max,
        blocks: vec![
            MinecraftSnapshotBlock {
                pos: Pos::new(0, 0, 0),
                name: "minecraft:stone".into(),
                properties: BTreeMap::new(),
            },
            MinecraftSnapshotBlock {
                pos: Pos::new(1, 1, 0),
                name: "minecraft:stone".into(),
                properties: BTreeMap::new(),
            },
            MinecraftSnapshotBlock {
                pos: Pos::new(0, 1, 0),
                name: "minecraft:redstone_wire".into(),
                properties: dust_properties,
            },
            MinecraftSnapshotBlock {
                pos: Pos::new(1, 0, 0),
                name: "minecraft:redstone_wall_torch".into(),
                properties: BTreeMap::from([
                    ("facing".into(), "east".into()),
                    ("lit".into(), "true".into()),
                ]),
            },
        ],
    };
    // Preserve the observed dot/line/side connections; do not infer a new shape.
    let assembly = assembly_from_snapshot(
        &snapshot,
        "Observed autonomous four-block clock",
        vec![capture.known_region],
    )
    .map_err(|e| e.to_string())?;
    let mut catalog = builtin_laws().clone();
    let assembly_id = AssemblyRevisionId::new("clock.observation.actual.v1").unwrap();
    let type_id = TypeRevisionId::new("clock.observation.periodic.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: type_id.clone(),
            name: "Autonomous recurring output".into(),
            contract: TypeContract::Periodic {
                requirement: Periodic {
                    output: "out".into(),
                },
            },
        })
        .map_err(|e| e.to_string())?;
    catalog
        .insert_assembly(AssemblyRevision {
            id: assembly_id.clone(),
            parents: vec![],
            assembly,
        })
        .map_err(|e| e.to_string())?;
    let selection = PhysicalBehaviorSelection {
        assembly: assembly_id,
        behavior_type: type_id,
        dust_law: BlueprintRevisionId::new(DUST_LAW_REVISION).unwrap(),
        torch_law: BlueprintRevisionId::new(TORCH_LAW_REVISION).unwrap(),
        inputs: BTreeMap::new(),
        outputs: BTreeMap::from([(
            "out".into(),
            PhysicalOutput::Signal {
                position: Pos::new(0, 1, 0),
            },
        )]),
        max_electrical_iterations: 128,
    };
    let model = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
        &catalog,
        selection.clone(),
        profile,
    )?;
    // The new contract is a separate immutable record; the periodic requirement
    // and physical laws are retained. Diagnostic restart traces cannot certify it.
    let finite_burst_proof = if recovery {
        None
    } else {
        let burst_id = TypeRevisionId::new("clock.observation.finite-burst.v1").unwrap();
        catalog
            .insert_type(TypeRevision {
                id: burst_id.clone(),
                name: "Finite burst followed by OFF".into(),
                contract: TypeContract::FiniteBurst {
                    requirement: FiniteBurst {
                        output: "out".into(),
                    },
                },
            })
            .map_err(|e| e.to_string())?;
        let burst = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
            &catalog,
            PhysicalBehaviorSelection {
                behavior_type: burst_id,
                ..selection
            },
            profile,
        )?;
        Some(burst.verify_finite_burst(BehaviorBudget::default()))
    };
    let mut state = model.initial_state()?;
    let mut differences = vec![];
    let mut modeled_edges = vec![];
    let mut observed_edges = vec![];
    let mut previous_model = None;
    let mut previous_observed = None;
    for sample in &capture.samples {
        let local = model.torch_state(&state, Pos::new(1, 0, 0))?;
        let lit = local.register("lit") == Some(1);
        let power = model.electrical_state(&state)?.signal(Pos::new(0, 1, 0));
        if previous_model.is_some_and(|old| old != lit) {
            modeled_edges.push(json!({"game_tick":sample.game_tick,"lit":lit}));
        }
        if previous_observed.is_some_and(|old| old != sample.torch_lit) {
            observed_edges.push(json!({"game_tick":sample.game_tick,"lit":sample.torch_lit}));
        }
        if lit != sample.torch_lit
            || power != sample.dust_power
            || sample.dust_connections != initial.dust_connections
            || sample.torch_facing != "east"
            || !sample.supports_intact
        {
            differences.push(json!({"game_tick":sample.game_tick,"observed":sample,"model":{"torch_lit":lit,"dust_power":power,"dust_connections":initial.dust_connections,"torch_facing":"east","supports_intact":true,"torch_hidden_state":local}}));
        }
        previous_model = Some(lit);
        previous_observed = Some(sample.torch_lit);
        if recovery && sample.game_tick == 220 {
            state = model.notify_torch_neighbors(&state)?;
        }
        if sample.game_tick < capture.duration_game_ticks {
            state = model.step(&state)?;
        }
    }
    Ok(json!({
        "schema":"dustroute.periodic-clock-comparison.v1", "samples_compared":capture.samples.len(),
        "profile":profile.as_str(), "autonomous_observation":!recovery,
        "scope":"fixed_four_block_clock_after_each_game_tick_only",
        "all_sampled_block_states_match":differences.is_empty(), "difference_count":differences.len(),
        "differences":differences, "observed_torch_edges":observed_edges, "modeled_torch_edges":modeled_edges,
        "model_proof": model.verify_periodic(BehaviorBudget::default()),
        "finite_burst_model_proof":finite_burst_proof, "restartability_verified":false,
        "live_infinite_recurrence_proven":false, "internal_callback_order_verified":false,
    }))
}

#[cfg(not(test))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: compare_periodic_clock <capture.json> [--legacy|--recovery]")?;
    let input = std::fs::read_to_string(path)?;
    let report = match args.next().as_deref().and_then(|arg| arg.to_str()) {
        None => compare(&input)?,
        Some("--legacy") => compare_with_profile(
            &input,
            PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
        )?,
        Some("--recovery") => compare_recovery(&input)?,
        _ => return Err("unknown comparison mode".into()),
    };
    if args.next().is_some() {
        return Err("unexpected comparison argument".into());
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["all_sampled_block_states_match"] != true {
        std::process::exit(1);
    }
    Ok(())
}
