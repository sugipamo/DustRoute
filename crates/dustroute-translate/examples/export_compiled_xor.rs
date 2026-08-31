use dustroute_translate::{
    BaselineCompileConfig, JavaExportConfig, compiled_xor_cell, compiled_xor_cell_with_config,
    world_setblock_commands,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let cell = if let Some(spacing) = args.next() {
        let lane = args.next().ok_or("missing lane gap")?;
        if args.next().is_some() {
            return Err("usage: export_compiled_xor [spacing_x lane_gap]".into());
        }
        compiled_xor_cell_with_config(BaselineCompileConfig {
            spacing_x: spacing.parse()?,
            lane_gap: lane.parse()?,
            ..BaselineCompileConfig::default()
        })?
    } else {
        compiled_xor_cell()?
    };
    let config = JavaExportConfig {
        relative: true,
        solid_block: "minecraft:stone_bricks".into(),
        ..JavaExportConfig::default()
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "name": cell.name,
            "bounds": cell.world.bounds(),
            "inputs": cell.inputs.iter().map(|port| json!({"name": port.name, "position": port.pos})).collect::<Vec<_>>(),
            "outputs": cell.outputs.iter().map(|port| json!({"name": port.name, "position": port.pos})).collect::<Vec<_>>(),
            "commands": world_setblock_commands(&cell.world, &config)?,
        }))?
    );
    Ok(())
}
