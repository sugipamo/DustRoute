use std::time::Instant;

use dustroute_translate::{
    BaselineCompileConfig, BaselineCompiler, LogicDag, RegionBounds, analyze_signal_liveness,
    analyze_world_region, decoder_1_to_2, extract_connectivity, full_adder, half_adder,
    half_subtractor, infer_truth_table, mux_2_to_1,
};
use serde::Serialize;

#[derive(Serialize)]
struct Observation {
    circuit: String,
    world_blocks: usize,
    graph_nodes: usize,
    graph_edges: usize,
    components: usize,
    inputs: usize,
    outputs: usize,
    unsupported: usize,
    settle_ticks: usize,
    liveness_sources: usize,
    liveness_drive_reachable: usize,
    liveness_potential_reachable: usize,
    undriven_inputs: usize,
    directed_regions: usize,
    signal_islands: usize,
    unreachable_components: usize,
    dead_end_components: usize,
    invalid_supports: usize,
    compile_ms: f64,
    extract_ms: f64,
    analyze_ms: f64,
    liveness_ms: f64,
    truth_table_ms: f64,
    truth_table_rows: usize,
    truth_table_ok: bool,
    truth_table_error: Option<String>,
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

fn observe(circuit: &str, dag: LogicDag, settle_ticks: usize) -> Observation {
    let started = Instant::now();
    let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
        .compile(&dag)
        .unwrap_or_else(|error| panic!("{circuit} failed to compile: {error}"));
    let compile_ms = elapsed_ms(started);
    let world = compiled.world;
    let (min, max) = world
        .bounds()
        .unwrap_or_else(|| panic!("{circuit} produced an empty world"));
    let bounds = RegionBounds::new(min, max);

    let started = Instant::now();
    let graph = extract_connectivity(&world);
    let extract_ms = elapsed_ms(started);

    let started = Instant::now();
    let analysis = analyze_world_region(&world, bounds);
    let analyze_ms = elapsed_ms(started);

    let started = Instant::now();
    let liveness = analyze_signal_liveness(&analysis.scene);
    let liveness_ms = elapsed_ms(started);

    let started = Instant::now();
    let truth_table = infer_truth_table(&world, &analysis, 16, settle_ticks);
    let truth_table_ms = elapsed_ms(started);
    let (truth_table_rows, truth_table_ok, truth_table_error) = match truth_table {
        Ok(table) => (table.rows.len(), true, None),
        Err(error) => (0, false, Some(error.to_string())),
    };

    let observation = Observation {
        circuit: circuit.into(),
        world_blocks: world.iter().count(),
        graph_nodes: graph.nodes.len(),
        graph_edges: graph.edges.len(),
        components: analysis.components.len(),
        inputs: analysis.inputs.len(),
        outputs: analysis.outputs.len(),
        unsupported: analysis.unsupported.len(),
        settle_ticks,
        liveness_sources: liveness.sources.len(),
        liveness_drive_reachable: liveness.drive_reachable.len(),
        liveness_potential_reachable: liveness.potential_drive_reachable.len(),
        undriven_inputs: liveness.undriven_inputs.len(),
        directed_regions: liveness.directed_regions.len(),
        signal_islands: analysis.diagnostics.signal_islands.len(),
        unreachable_components: analysis.diagnostics.unreachable_from_inputs.len(),
        dead_end_components: analysis.diagnostics.cannot_reach_outputs.len(),
        invalid_supports: analysis.diagnostics.invalid_supports.len(),
        compile_ms,
        extract_ms,
        analyze_ms,
        liveness_ms,
        truth_table_ms,
        truth_table_rows,
        truth_table_ok,
        truth_table_error,
    };
    std::hint::black_box((&observation, liveness));
    observation
}

fn main() {
    // Keep this list small and deterministic: it is intended to identify the
    // dominant reverse-analysis phase before any optimization is attempted.
    let circuits = [
        ("half_adder", half_adder(), 16),
        ("half_subtractor", half_subtractor(), 16),
        ("mux_2_to_1", mux_2_to_1(), 16),
        ("decoder_1_to_2", decoder_1_to_2(), 16),
        ("full_adder", full_adder(), 60),
    ];
    for (name, dag, settle_ticks) in circuits {
        let observation = observe(name, dag, settle_ticks);
        println!(
            "{}",
            serde_json::to_string(&observation).expect("observation is JSON serializable")
        );
    }
}
