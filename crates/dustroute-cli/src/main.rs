use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs;
use std::io::Read;

use dustroute_app::DustRouteService;
use dustroute_translate::{
    ForwardOptions, JavaExportConfig, PistonDoorScenario, PistonDoorScenarioError, Pos,
    RegionBounds, ReverseRequest, compiled_circuit_datapack, semantics_datapack,
    world_from_snapshot_json,
};
use serde_json::{Value, json};

const PISTON_DOOR_EXECUTION_REPORT_SCHEMA: &str = "dustroute.piston-door-execution-report.v1";

fn main() -> Result<(), Box<dyn Error>> {
    let app = DustRouteService::default();
    let mut args = env::args().skip(1);
    let command = args.next().ok_or(
        "usage: dustroute-cli eval|export|export-semantics|analyze-snapshot|run-piston-door ...",
    )?;
    if command == "run-piston-door" {
        let report = run_piston_door_report(args);
        let success = report.get("ok").and_then(Value::as_bool).unwrap_or(false);
        println!("{}", serde_json::to_string_pretty(&report)?);
        if !success {
            std::process::exit(2);
        }
        return Ok(());
    }
    if command == "analyze-snapshot" {
        let input = args.next().ok_or("missing snapshot JSON path")?;
        let json = fs::read_to_string(input)?;
        let (snapshot, world) = world_from_snapshot_json(&json)?;
        let result = app.analyze_world(
            &world,
            ReverseRequest::new(RegionBounds::new(snapshot.min, snapshot.max)).with_truth_table(16),
        );
        let analysis = &result.analysis;
        let (truth_table, expressions) = match &result.truth_table {
            Some(table) => {
                (
                    table.rows.iter().map(|row| serde_json::json!({
                        "inputs": row.inputs.iter().map(|value| u8::from(*value)).collect::<Vec<_>>(),
                        "outputs": row.outputs.iter().map(|value| u8::from(*value)).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                    analysis.outputs.iter().zip(&result.expressions).map(|(terminal, expression)| serde_json::json!({
                        "output": { "x": terminal.anchor.x, "y": terminal.anchor.y, "z": terminal.anchor.z },
                        "expression": expression.to_string(),
                    })).collect::<Vec<_>>(),
                )
            }
            None => (Vec::new(), Vec::new()),
        };
        let report = serde_json::json!({
            "redstone_block_count": analysis.redstone_blocks.len(),
            "component_count": analysis.components.len(),
            "edge_count": analysis.graph.edges.len(),
            "inputs": analysis.inputs.iter().map(|terminal| serde_json::json!({
                "x": terminal.anchor.x,
                "y": terminal.anchor.y,
                "z": terminal.anchor.z,
                "confidence": format!("{:?}", terminal.confidence).to_lowercase(),
            })).collect::<Vec<_>>(),
            "outputs": analysis.outputs.iter().map(|terminal| serde_json::json!({
                "x": terminal.anchor.x,
                "y": terminal.anchor.y,
                "z": terminal.anchor.z,
                "confidence": format!("{:?}", terminal.confidence).to_lowercase(),
            })).collect::<Vec<_>>(),
            "unsupported": analysis.unsupported.iter().map(|(pos, kind)| serde_json::json!({
                "x": pos.x, "y": pos.y, "z": pos.z, "kind": format!("{kind:?}")
            })).collect::<Vec<_>>(),
            "truth_table": truth_table,
            "expressions": expressions,
            "truth_table_error": result.truth_table_error.as_ref().map(ToString::to_string),
            "diagnostics": {
                "signal_island_count": analysis.diagnostics.signal_islands.len(),
                "isolated_redstone": analysis.diagnostics.isolated_redstone.len(),
                "unreachable_components": analysis.diagnostics.unreachable_from_inputs.len(),
                "components_without_output_path": analysis.diagnostics.cannot_reach_outputs.len(),
                "invalid_supports": analysis.diagnostics.invalid_supports.len(),
                "non_controllable_torches": analysis.diagnostics.non_controllable_torches.len(),
            },
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if command == "export-semantics" {
        let output = args.next().ok_or("missing output ZIP path")?;
        let namespace = args.next().unwrap_or_else(|| "ro_sem".into());
        let config = JavaExportConfig {
            namespace,
            ..JavaExportConfig::default()
        };
        let path = semantics_datapack(&config)?.write_zip(std::path::Path::new(&output))?;
        println!("{}", path.display());
        return Ok(());
    }
    let name = args.next().ok_or("missing circuit name")?;
    let dag = DustRouteService::built_in_circuit(&name).ok_or("unknown circuit")?;
    if command == "export" {
        let output = args.next().ok_or("missing output ZIP path")?;
        let namespace = args.next().unwrap_or_else(|| "dustroute".into());
        let translated = app
            .compile_builtin(&name, ForwardOptions::default())?
            .ok_or("unknown circuit")?;
        let config = JavaExportConfig {
            namespace,
            ..JavaExportConfig::default()
        };
        let pack =
            compiled_circuit_datapack(&translated.compiled, &name.replace('-', "_"), &config)?;
        let path = pack.write_zip(std::path::Path::new(&output))?;
        println!("{}", path.display());
        return Ok(());
    }
    if command != "eval" {
        return Err(
            "usage: dustroute-cli eval CIRCUIT key=0|1 ... | export CIRCUIT OUTPUT.zip [namespace] | export-semantics OUTPUT.zip [namespace] | analyze-snapshot SNAPSHOT.json | run-piston-door SCENARIO.json [open|cycle] [--translate x,y,z]"
                .into(),
        );
    }
    let mut inputs = HashMap::new();
    for assignment in args {
        let (key, value) = assignment.split_once('=').ok_or("expected key=0|1")?;
        let value = match value {
            "0" | "false" => false,
            "1" | "true" => true,
            _ => return Err(format!("invalid Boolean value: {value}").into()),
        };
        inputs.insert(key.to_owned(), value);
    }
    for (output, value) in dag.evaluate(&inputs)? {
        println!("{output}={}", u8::from(value));
    }
    Ok(())
}

fn run_piston_door_report(args: impl IntoIterator<Item = String>) -> Value {
    let mut args = args.into_iter();
    let Some(path) = args.next() else {
        return piston_door_failure(
            "arguments",
            "missing scenario JSON path",
            "invalid_arguments",
        );
    };
    let mut mode = None;
    let mut translation = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "open" | "cycle" if mode.is_none() => mode = Some(argument),
            "--translate" => {
                let Some(offset) = args.next() else {
                    return piston_door_failure(
                        "arguments",
                        "--translate requires x,y,z",
                        "invalid_arguments",
                    );
                };
                match parse_translation(&offset) {
                    Ok(offset) if translation.is_none() => translation = Some(offset),
                    Ok(_) => {
                        return piston_door_failure(
                            "arguments",
                            "--translate may be specified only once",
                            "invalid_arguments",
                        );
                    }
                    Err(message) => {
                        return piston_door_failure("arguments", &message, "invalid_arguments");
                    }
                }
            }
            _ => {
                return piston_door_failure(
                    "arguments",
                    &format!("unknown or duplicate argument {argument:?}"),
                    "invalid_arguments",
                );
            }
        }
    }

    let json = if path == "-" {
        let mut json = String::new();
        match std::io::stdin().read_to_string(&mut json) {
            Ok(_) => json,
            Err(error) => {
                return piston_door_failure(
                    "input",
                    &format!("cannot read scenario from stdin: {error}"),
                    "input_read_failed",
                );
            }
        }
    } else {
        match fs::read_to_string(&path) {
            Ok(json) => json,
            Err(error) => {
                return piston_door_failure(
                    "input",
                    &format!("cannot read scenario {path:?}: {error}"),
                    "input_read_failed",
                );
            }
        }
    };
    let mut scenario = match PistonDoorScenario::from_json(&json) {
        Ok(scenario) => scenario,
        Err(error) => return piston_door_error("parse", error),
    };
    if let Some(offset) = translation {
        scenario = scenario.translated(offset);
    }
    let mode = mode.as_deref().unwrap_or("cycle");
    let materialized = match scenario.build_world() {
        Ok(materialized) => materialized,
        Err(error) => return piston_door_error("materialize", error),
    };
    let initial_state = world_state_json(materialized.world());
    let known_region = materialized.known_region();
    let engine = match mode {
        "open" => scenario.run_open(),
        "cycle" => scenario.run_cycle(),
        _ => unreachable!("mode is validated while parsing arguments"),
    };
    let engine = match engine {
        Ok(engine) => engine,
        Err(error) => return piston_door_error("execute", error),
    };
    json!({
        "ok": true,
        "status": "complete",
        "command": "run-piston-door",
        "report_schema": PISTON_DOOR_EXECUTION_REPORT_SCHEMA,
        "mode": mode,
        "translation": translation,
        "scenario_id": scenario.id,
        "schema_version": scenario.schema_version,
        "minecraft_version": scenario.minecraft_version,
        "known_region": known_region,
        "initial_state": initial_state,
        "final_state": world_state_json(engine.world()),
        "trace_status": engine.trace_status(),
        "transition_trace": engine.transition_trace(),
        "event_trace": engine.event_trace(),
    })
}

fn parse_translation(value: &str) -> Result<Pos, String> {
    let values = value.split(',').collect::<Vec<_>>();
    if values.len() != 3 {
        return Err("translation must contain exactly three comma-separated integers".to_owned());
    }
    let parse = |axis: &str, value: &str| {
        value
            .parse::<i32>()
            .map_err(|_| format!("translation {axis} component {value:?} is not an integer"))
    };
    Ok(Pos::new(
        parse("x", values[0])?,
        parse("y", values[1])?,
        parse("z", values[2])?,
    ))
}

fn world_state_json(world: &dustroute_translate::World) -> Value {
    let blocks = world
        .iter()
        .map(|(position, block)| json!({ "position": position, "block": block }))
        .collect::<Vec<_>>();
    json!({
        "state_id": world.state_id(),
        "shape_id": world.shape_id(),
        "block_count": blocks.len(),
        "blocks": blocks,
    })
}

fn piston_door_error(stage: &str, error: PistonDoorScenarioError) -> Value {
    let code = match error {
        PistonDoorScenarioError::Json(_) => "invalid_json",
        PistonDoorScenarioError::Invalid { .. } => "invalid_scenario",
        PistonDoorScenarioError::Collision { .. } => "layout_collision",
        PistonDoorScenarioError::EmptyWorld => "empty_layout",
        PistonDoorScenarioError::Physics(_) => "physics_execution_failed",
    };
    piston_door_failure(stage, &error.to_string(), code)
}

fn piston_door_failure(stage: &str, message: &str, code: &str) -> Value {
    json!({
        "ok": false,
        "status": "failed",
        "command": "run-piston-door",
        "report_schema": PISTON_DOOR_EXECUTION_REPORT_SCHEMA,
        "error": {
            "code": code,
            "stage": stage,
            "message": message,
        },
    })
}
