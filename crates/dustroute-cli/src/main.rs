use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs;

use dustroute_app::DustRouteService;
use dustroute_translate::{
    ForwardOptions, JavaExportConfig, RegionBounds, ReverseRequest, compiled_circuit_datapack,
    semantics_datapack, world_from_snapshot_json,
};

fn main() -> Result<(), Box<dyn Error>> {
    let app = DustRouteService::default();
    let mut args = env::args().skip(1);
    let command = args
        .next()
        .ok_or("usage: dustroute-cli eval|export|export-semantics|analyze-snapshot ...")?;
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
            "usage: dustroute-cli eval CIRCUIT key=0|1 ... | export CIRCUIT OUTPUT.zip [namespace] | export-semantics OUTPUT.zip [namespace] | analyze-snapshot SNAPSHOT.json"
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
