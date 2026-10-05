#[allow(dead_code)]
#[path = "../../../tests/support/diagnostic_fixture.rs"]
mod diagnostic_fixture;

use std::collections::BTreeSet;
use std::env;

use dustroute_translate::{
    cells::external_xor_cell, physics_trace::PhysicalTrace, physics_trace::compare_physical_traces,
    physics_trace::simulate_cell_trace,
};

#[test]
#[ignore = "explicit offline fixture export; requires new absolute DUSTROUTE_DIAGNOSTIC_OUTPUT"]
fn retain_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = diagnostic_fixture::output()?;
    let path = env::var("DUSTROUTE_XOR_TRACE_INPUT")?;
    let a = env::var("DUSTROUTE_XOR_INPUT_A")?.parse::<u8>()? != 0;
    let b = env::var("DUSTROUTE_XOR_INPUT_B")?.parse::<u8>()? != 0;
    let minecraft: PhysicalTrace = diagnostic_fixture::input(&path)?;
    let positions = minecraft
        .observations
        .iter()
        .map(|item| item.position)
        .collect::<BTreeSet<_>>();
    let duration = minecraft
        .observations
        .iter()
        .map(|item| item.redstone_tick)
        .max()
        .unwrap_or(0);
    let simulator = simulate_cell_trace(&external_xor_cell(), &[a, b], &positions, 8, duration)?;
    if let Ok(path) = env::var("DUSTROUTE_XOR_SIMULATOR_OUTPUT") {
        let mut dump = diagnostic_fixture::new_output(&path)?;
        diagnostic_fixture::report(&mut dump, &simulator)?;
    }
    let comparison = compare_physical_traces(&minecraft, &simulator);
    diagnostic_fixture::report(&mut output, &comparison)?;
    Ok(())
}
