use super::*;
use crate::ir::expr::Expr;
use crate::wire::update_wire_shapes;
use crate::world::{Block, BlockKind, Pos, World};
use crate::{
    circuits::decoder_1_to_2, circuits::full_adder, circuits::half_adder,
    circuits::half_subtractor, circuits::mux_2_to_1, compiler::BaselineCompileConfig,
    compiler::BaselineCompiler,
};
use std::collections::BTreeSet;

#[test]
fn infers_half_adder_boundaries_from_directionality() {
    let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
        .compile(&half_adder())
        .unwrap();
    let (min, max) = compiled.world.bounds().unwrap();
    let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
    assert_eq!(analysis.inputs.len(), 2, "{analysis:#?}");
    assert_eq!(analysis.outputs.len(), 2, "{analysis:#?}");
    assert!(analysis.unsupported.is_empty());
    let table = infer_truth_table(&compiled.world, &analysis, 16, 16).unwrap();
    let columns: Vec<Vec<_>> = (0..table.outputs.len())
        .map(|output| table.rows.iter().map(|row| row.outputs[output]).collect())
        .collect();
    assert!(columns.contains(&vec![false, false, false, true]));
    assert!(columns.contains(&vec![false, true, true, false]));
    let expressions = infer_output_expressions(&table);
    assert!(expressions.iter().any(|expr| matches!(expr, Expr::And(_))));
    assert!(expressions.iter().any(|expr| matches!(expr, Expr::Xor(_))));
    assert_eq!(analysis.diagnostics.signal_islands.len(), 1);
    assert!(analysis.diagnostics.unreachable_from_inputs.is_empty());
    assert!(analysis.diagnostics.cannot_reach_outputs.is_empty());
    assert!(analysis.diagnostics.non_controllable_torches.is_empty());
}

#[test]
fn infers_boundaries_for_all_regression_circuits() {
    for (dag, expected_inputs, expected_outputs) in [
        (half_subtractor(), 2, 2),
        (mux_2_to_1(), 3, 1),
        (decoder_1_to_2(), 2, 2),
        (full_adder(), 3, 2),
    ] {
        let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&dag)
            .unwrap();
        let (min, max) = compiled.world.bounds().unwrap();
        let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
        assert_eq!(analysis.inputs.len(), expected_inputs, "{analysis:#?}");
        assert_eq!(analysis.outputs.len(), expected_outputs, "{analysis:#?}");
        assert_eq!(
            analysis.diagnostics.signal_islands.len(),
            1,
            "{analysis:#?}"
        );
        if expected_inputs == 3 && expected_outputs == 2 {
            let table = infer_truth_table(&compiled.world, &analysis, 16, 60).unwrap();
            let columns: Vec<Vec<_>> = (0..table.outputs.len())
                .map(|output| table.rows.iter().map(|row| row.outputs[output]).collect())
                .collect();
            assert!(columns.contains(&vec![false, false, false, true, false, true, true, true]));
            assert!(columns.contains(&vec![false, true, true, false, true, false, false, true]));
            let expressions = infer_output_expressions(&table);
            assert!(expressions.iter().any(|expr| matches!(expr, Expr::Or(_))));
            assert!(expressions.iter().any(|expr| matches!(expr, Expr::Xor(_))));
        }
    }
}

#[test]
fn broken_torch_support_is_detected_and_changes_truth_table() {
    let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
        .compile(&half_adder())
        .unwrap();
    let (min, max) = compiled.world.bounds().unwrap();
    let bounds = RegionBounds::new(min, max);
    let healthy_analysis = analyze_world_region(&compiled.world, bounds);
    let healthy = infer_truth_table(&compiled.world, &healthy_analysis, 16, 16).unwrap();
    let mut broken_world = compiled.world.clone().into_world();
    let (torch, support) = broken_world
        .iter()
        .find(|(_, block)| block.kind == BlockKind::RedstoneTorch)
        .and_then(|(pos, block)| block.support_pos(*pos).map(|support| (*pos, support)))
        .unwrap();
    broken_world.set(support, Block::new(BlockKind::Transparent));
    let broken_analysis = analyze_world_region(&broken_world, bounds);
    assert!(
        broken_analysis
            .diagnostics
            .non_controllable_torches
            .contains(&torch)
    );
    let broken = infer_truth_table(&broken_world, &broken_analysis, 16, 16).unwrap();
    let comparison = compare_truth_tables(&healthy, &broken);
    assert!(comparison.comparable, "{comparison:?}");
    assert_eq!(comparison.actual_outputs, 2, "{comparison:?}");
    assert_eq!(comparison.terminal_count_delta, 0, "{comparison:?}");
    assert!(comparison.differing_bits > 0, "{comparison:?}");
    assert!(comparison.fitness_penalty > 0, "{comparison:?}");
}

#[test]
fn compact_xor_is_derived_from_shared_physics_without_gate_partitioning() {
    let cell = crate::cells::compact_compiled_xor_cell().unwrap();
    let (min, max) = cell.world.bounds().unwrap();
    let analysis = analyze_world_region(&cell.world, RegionBounds::new(min, max));
    let model = derive_functional_network(&cell.world, &analysis, 16, 64).unwrap();

    assert_eq!(model.truth_table.inputs.len(), 2);
    assert_eq!(model.truth_table.outputs.len(), 1);
    assert!(matches!(model.output_functions[0].expression, Expr::Xor(_)));
    assert_eq!(
        model.output_functions[0].truth_column,
        vec![false, true, true, false]
    );
    assert!(model.physical_influences.iter().any(|influence| {
        influence.shared_role && influence.input_dependencies == BTreeSet::from([0, 1])
    }));
}

#[test]
fn truth_table_requires_non_empty_interface_evidence() {
    let world = World::new();
    let analysis = analyze_world_region(
        &world,
        RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(1, 1, 1)),
    );
    assert_eq!(
        infer_truth_table(&world, &analysis, 4, 1),
        Err(TruthTableError::NoInputs)
    );

    let mut source_only = World::new();
    source_only.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    source_only.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
    let source = source_only.place(BlockKind::Lever, Pos::new(0, 1, 0));
    source.powered = Some(false);
    source_only.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
    update_wire_shapes(&mut source_only);
    let analysis = analyze_world_region(
        &source_only,
        RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(2, 2, 1)),
    );
    assert!(!analysis.inputs.is_empty(), "{analysis:#?}");
    let mut no_outputs = analysis.clone();
    no_outputs.outputs.clear();
    assert_eq!(
        infer_truth_table(&source_only, &no_outputs, 4, 1),
        Err(TruthTableError::NoOutputs)
    );

    let mut ambiguous = analysis.clone();
    ambiguous
        .interface
        .external_inputs
        .insert(Pos::new(9, 9, 9));
    ambiguous.interface.mapped_inputs.insert(Pos::new(9, 9, 9));
    assert!(matches!(
        infer_truth_table(&source_only, &ambiguous, 4, 1),
        Err(TruthTableError::AmbiguousInputMapping { .. })
    ));
}

#[test]
fn truth_table_budget_reports_rows_before_simulation() {
    let budget = TruthTableBudget::new(2, u128::MAX);
    assert_eq!(budget.estimate_work_units(100, 3, 4), Some((8, 4_000)));

    let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
        .compile(&half_adder())
        .unwrap();
    let (min, max) = compiled.world.bounds().unwrap();
    let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
    let error = infer_truth_table_with_budget(&compiled.world, &analysis, 16, 16, budget)
        .expect_err("row budget should reject before creating simulators");
    assert!(matches!(
        error,
        TruthTableError::BudgetExceeded {
            rows: 4,
            max_rows: 2,
            ..
        }
    ));
}

#[test]
fn runtime_budget_fails_closed_without_returning_partial_rows() {
    let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
        .compile(&half_adder())
        .unwrap();
    let (min, max) = compiled.world.bounds().unwrap();
    let bounds = RegionBounds::new(min, max);
    let request = crate::api::ReverseRequest::new(bounds)
        .with_truth_table(16)
        .with_settle_ticks(16)
        .with_truth_table_budget(
            TruthTableBudget::new(usize::MAX, u128::MAX)
                .with_max_solver_iterations(0)
                .with_max_elapsed_millis(None),
        );
    let result = crate::api::Translator.reverse(&compiled.world, request);
    assert!(result.truth_table.is_none());
    assert!(result.functional_network.is_none());
    assert!(matches!(
        result.truth_table_error,
        Some(TruthTableError::RuntimeBudgetExceeded {
            completed_rows: 0,
            max_solver_iterations: 0,
            ..
        })
    ));
}

#[test]
fn elapsed_budget_fails_closed_before_simulation() {
    let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
        .compile(&half_adder())
        .unwrap();
    let (min, max) = compiled.world.bounds().unwrap();
    let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
    let error = infer_truth_table_with_budget(
        &compiled.world,
        &analysis,
        16,
        16,
        TruthTableBudget::new(usize::MAX, u128::MAX)
            .with_max_solver_iterations(usize::MAX)
            .with_max_elapsed_millis(Some(0)),
    )
    .expect_err("zero elapsed budget must reject before the first row");
    assert!(matches!(
        error,
        TruthTableError::ElapsedBudgetExceeded {
            completed_rows: 0,
            max_elapsed_millis: 0,
            ..
        }
    ));
}

#[test]
fn incomplete_settle_window_is_not_claimed_as_a_truth_table() {
    let mut world = World::new();
    world.fill(
        Pos::new(0, 0, 0),
        Pos::new(3, 0, 0),
        Block::new(BlockKind::Solid),
    );
    let lever = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
    lever.support_offset = Some(Pos::new(0, -1, 0));
    world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
    let repeater = world.place(BlockKind::Repeater, Pos::new(2, 1, 0));
    repeater.facing = Some(crate::world::Facing::East);
    repeater.delay = Some(1);
    world.place(BlockKind::RedstoneWire, Pos::new(3, 1, 0));
    update_wire_shapes(&mut world);
    let bounds = RegionBounds::new(Pos::new(0, 0, 0), Pos::new(3, 1, 0));
    let analysis = analyze_world_region(&world, bounds);
    assert_eq!(analysis.inputs.len(), 1, "{analysis:#?}");
    assert_eq!(analysis.outputs.len(), 1, "{analysis:#?}");
    let error = infer_truth_table_with_budget(
        &world,
        &analysis,
        4,
        1,
        TruthTableBudget::new(4, u128::MAX).with_max_elapsed_millis(None),
    )
    .expect_err("a changing final tick is not settled evidence");
    assert!(matches!(
        error,
        TruthTableError::NonSettling {
            row: 1,
            settle_ticks: 1,
            pending_events: false,
        }
    ));
}

#[test]
fn execution_stats_report_actual_settle_work_without_changing_table() {
    let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
        .compile(&half_adder())
        .unwrap();
    let (min, max) = compiled.world.bounds().unwrap();
    let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
    let (instrumented, stats) = infer_truth_table_with_budget_and_stats(
        &compiled.world,
        &analysis,
        16,
        16,
        TruthTableBudget::default(),
    )
    .unwrap();
    let ordinary = infer_truth_table(&compiled.world, &analysis, 16, 16).unwrap();

    assert_eq!(instrumented, ordinary);
    assert_eq!(stats.rows_requested, 4);
    assert_eq!(stats.rows_completed, 4);
    assert_eq!(stats.settle_ticks_requested, 16);
    assert!(stats.settle_ticks_executed > 0);
    assert!(stats.settle_ticks_executed <= 4 * stats.settle_ticks_requested);
    assert!(stats.solver_iterations > 0);
}

#[test]
fn isolated_observable_sink_is_reported_as_unmapped() {
    let mut world = World::new();
    world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    world.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
    let source = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
    source.powered = Some(false);
    world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
    update_wire_shapes(&mut world);
    world.set(Pos::new(3, 1, 0), Block::new(BlockKind::RedstoneLamp));
    let analysis = analyze_world_region(
        &world,
        RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(4, 2, 1)),
    );
    assert!(
        analysis
            .interface
            .unmapped_outputs
            .contains(&Pos::new(3, 1, 0))
    );
    assert!(matches!(
        infer_truth_table(&world, &analysis, 4, 1),
        Err(TruthTableError::NoOutputs) | Err(TruthTableError::UnmappedObservableOutputs(_))
    ));
}
