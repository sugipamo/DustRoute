use dustroute_translate::{
    BaselineCompileConfig, GateKind, compiled_xor_cell_with_config, verify_cell_with_settle_ticks,
};

fn main() {
    println!("spacing,lane,valid,size_x,size_y,size_z,blocks");
    for spacing_x in 8..=12 {
        for lane_gap in 5..=8 {
            let config = BaselineCompileConfig {
                spacing_x,
                lane_gap,
                ..BaselineCompileConfig::default()
            };
            let Ok(cell) = compiled_xor_cell_with_config(config) else {
                println!("{spacing_x},{lane_gap},compile_failed,,,,");
                continue;
            };
            let valid = verify_cell_with_settle_ticks(GateKind::Xor, &cell, 64).valid;
            let (low, high) = cell.world.bounds().expect("compiled cell is non-empty");
            println!(
                "{spacing_x},{lane_gap},{valid},{},{},{},{}",
                high.x - low.x + 1,
                high.y - low.y + 1,
                high.z - low.z + 1,
                cell.world.iter().count()
            );
        }
    }
}
