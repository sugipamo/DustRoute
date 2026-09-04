use std::env;
use std::fs;

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!(
            "usage: cargo run -p dustroute-translate --example validate_vanilla_instrumentation -- ARTIFACT.json"
        );
        std::process::exit(2);
    };
    if matches!(path.as_str(), "--help" | "-h") {
        println!(
            "usage: cargo run -p dustroute-translate --example validate_vanilla_instrumentation -- ARTIFACT.json"
        );
        return;
    }
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            std::process::exit(1);
        }
    };
    match dustroute_translate::parse_and_validate_instrumentation(&source) {
        Ok(artifact) => {
            println!(
                "valid {} instrumentation: scenario={}, ordered_ticks={}, state_events={}, piston_states={}, neighbor_updates={}",
                artifact.minecraft_version,
                artifact.scenario,
                artifact.ordered_ticks.len(),
                artifact.state_events.len(),
                artifact.piston_states.len(),
                artifact.neighbor_updates.len()
            );
        }
        Err(error) => {
            eprintln!("invalid instrumentation artifact: {error}");
            std::process::exit(1);
        }
    }
}
