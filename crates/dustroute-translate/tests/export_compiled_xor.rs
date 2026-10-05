#[allow(dead_code)]
#[path = "../../../tests/support/diagnostic_fixture.rs"]
mod diagnostic_fixture;

use dustroute_translate::{
    cells::compiled_xor_cell, cells::compiled_xor_cell_with_config,
    compiler::BaselineCompileConfig, minecraft_export::JavaExportConfig,
    minecraft_export::world_setblock_commands,
};
use serde_json::json;

#[test]
#[ignore = "explicit offline fixture export; requires new absolute DUSTROUTE_DIAGNOSTIC_OUTPUT"]
fn retain_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = diagnostic_fixture::output()?;
    let cell = if let Ok(spacing) = std::env::var("DUSTROUTE_XOR_SPACING_X") {
        compiled_xor_cell_with_config(BaselineCompileConfig {
            spacing_x: spacing.parse()?,
            lane_gap: std::env::var("DUSTROUTE_XOR_LANE_GAP")?.parse()?,
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
    diagnostic_fixture::report(
        &mut output,
        &json!({
            "name": cell.name,
            "bounds": cell.world.bounds(),
            "inputs": cell.inputs.iter().map(|port| json!({"name": port.name, "position": port.pos})).collect::<Vec<_>>(),
            "outputs": cell.outputs.iter().map(|port| json!({"name": port.name, "position": port.pos})).collect::<Vec<_>>(),
            "commands": world_setblock_commands(&dustroute_minecraft::ValidatedWorld::try_from(cell.world.clone())?, &config)?,
        }),
    )?;
    Ok(())
}
