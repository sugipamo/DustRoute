use std::collections::BTreeSet;
use std::env;
use std::fs;

use dustroute_translate::{
    PhysicalTrace, compare_physical_traces, external_xor_cell, simulate_cell_trace,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: compare_external_xor_trace <minecraft-trace.json> <a> <b>")?;
    let a = args.next().ok_or("missing input a")?.parse::<u8>()? != 0;
    let b = args.next().ok_or("missing input b")?.parse::<u8>()? != 0;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let minecraft: PhysicalTrace = serde_json::from_str(&fs::read_to_string(path)?)?;
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
    if env::var_os("DUSTROUTE_TRACE_DUMP_SIMULATOR").is_some() {
        eprintln!("{}", serde_json::to_string_pretty(&simulator)?);
    }
    let comparison = compare_physical_traces(&minecraft, &simulator);
    println!("{}", serde_json::to_string_pretty(&comparison)?);
    Ok(())
}
