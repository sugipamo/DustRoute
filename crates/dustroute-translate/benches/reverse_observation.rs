use std::time::Instant;

use dustroute_translate::{
    BaselineCompileConfig, BaselineCompiler, Block, BlockKind, LogicDag, Pos, RegionBounds,
    TruthTableBudget, analyze_signal_liveness, analyze_world_region, decoder_1_to_2,
    extract_connectivity, full_adder, half_adder, half_subtractor,
    infer_truth_table_with_budget_and_stats, mux_2_to_1,
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
    truth_table_rows_requested: usize,
    truth_table_settle_ticks_executed: usize,
    truth_table_solver_iterations: usize,
    truth_table_execution_elapsed_ms: u64,
    truth_table_world_clone_ms: f64,
    truth_table_input_drive_ms: f64,
    truth_table_wire_shape_update_ms: f64,
    truth_table_simulator_init_ms: f64,
    truth_table_settle_ms: f64,
    truth_table_output_read_ms: f64,
    truth_table_unattributed_ms: f64,
    truth_table_ok: bool,
    truth_table_error: Option<String>,
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

fn nanos_to_ms(nanos: u64) -> f64 {
    nanos as f64 / 1_000_000.0
}

fn observe(
    circuit: &str,
    dag: LogicDag,
    settle_ticks: usize,
    pad_world_to: Option<usize>,
) -> Observation {
    let started = Instant::now();
    let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
        .compile(&dag)
        .unwrap_or_else(|error| panic!("{circuit} failed to compile: {error}"));
    let compile_ms = elapsed_ms(started);
    let mut world = compiled.world;
    if let Some(target_blocks) = pad_world_to {
        pad_world_with_non_conductive_blocks(&mut world, target_blocks);
    }
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
    let truth_table = infer_truth_table_with_budget_and_stats(
        &world,
        &analysis,
        16,
        settle_ticks,
        TruthTableBudget::default(),
    );
    let truth_table_ms = elapsed_ms(started);
    let mut truth_table_rows = 0;
    let mut truth_table_rows_requested = 0;
    let mut truth_table_settle_ticks_executed = 0;
    let mut truth_table_solver_iterations = 0;
    let mut truth_table_execution_elapsed_ms = 0;
    let mut truth_table_world_clone_ms = 0.0;
    let mut truth_table_input_drive_ms = 0.0;
    let mut truth_table_wire_shape_update_ms = 0.0;
    let mut truth_table_simulator_init_ms = 0.0;
    let mut truth_table_settle_ms = 0.0;
    let mut truth_table_output_read_ms = 0.0;
    let mut truth_table_ok = false;
    let mut truth_table_error = None;
    match truth_table {
        Ok((table, stats)) => {
            truth_table_rows = table.rows.len();
            truth_table_rows_requested = stats.rows_requested;
            truth_table_settle_ticks_executed = stats.settle_ticks_executed;
            truth_table_solver_iterations = stats.solver_iterations;
            truth_table_execution_elapsed_ms = stats.elapsed_millis;
            truth_table_world_clone_ms = nanos_to_ms(stats.world_clone_nanos);
            truth_table_input_drive_ms = nanos_to_ms(stats.input_drive_nanos);
            truth_table_wire_shape_update_ms = nanos_to_ms(stats.wire_shape_update_nanos);
            truth_table_simulator_init_ms = nanos_to_ms(stats.simulator_init_nanos);
            truth_table_settle_ms = nanos_to_ms(stats.settle_nanos);
            truth_table_output_read_ms = nanos_to_ms(stats.output_read_nanos);
            truth_table_ok = true;
        }
        Err(error) => {
            truth_table_error = Some(error.to_string());
        }
    }
    let truth_table_unattributed_ms = (truth_table_ms
        - truth_table_world_clone_ms
        - truth_table_input_drive_ms
        - truth_table_wire_shape_update_ms
        - truth_table_simulator_init_ms
        - truth_table_settle_ms
        - truth_table_output_read_ms)
        .max(0.0);

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
        truth_table_rows_requested,
        truth_table_settle_ticks_executed,
        truth_table_solver_iterations,
        truth_table_execution_elapsed_ms,
        truth_table_world_clone_ms,
        truth_table_input_drive_ms,
        truth_table_wire_shape_update_ms,
        truth_table_simulator_init_ms,
        truth_table_settle_ms,
        truth_table_output_read_ms,
        truth_table_unattributed_ms,
        truth_table_ok,
        truth_table_error,
    };
    std::hint::black_box((&observation, liveness));
    observation
}

fn pad_world_with_non_conductive_blocks(world: &mut dustroute_translate::World, target: usize) {
    let mut index = 0_i32;
    while world.iter().count() < target {
        let position = Pos::new(index, 100, 100);
        if world.get(position).is_none() {
            world.set(position, Block::new(BlockKind::Solid));
        }
        index += 1;
    }
}

fn main() {
    // Keep this list small and deterministic: it is intended to identify the
    // dominant reverse-analysis phase before any optimization is attempted.
    let circuits = [
        ("half_adder", half_adder(), 16, None),
        ("half_subtractor", half_subtractor(), 16, None),
        ("mux_2_to_1", mux_2_to_1(), 16, None),
        ("mux_2_to_1_padded_538", mux_2_to_1(), 16, Some(538)),
        ("decoder_1_to_2", decoder_1_to_2(), 16, None),
        ("full_adder", full_adder(), 60, None),
    ];
    for (name, dag, settle_ticks, pad_world_to) in circuits {
        let observation = observe(name, dag, settle_ticks, pad_world_to);
        println!(
            "{}",
            serde_json::to_string(&observation).expect("observation is JSON serializable")
        );
    }
}
