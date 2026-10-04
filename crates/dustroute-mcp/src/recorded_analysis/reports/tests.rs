//! Wire compatibility fixtures captured with the pre-migration renderers at 1715ca1.
//! JSON here is an independent MCP boundary expectation, never a production model.
use super::*;
use dustroute_physical::{Block, BlockKind, Pos, World};
use dustroute_translate::{
    api::{ReverseRequest, ReverseResult, Translator},
    world_reverse::{RegionBounds, TruthTableError},
};
use serde_json::{Value, json};

pub(crate) fn fixture(case: usize) -> (RegionBounds, ReverseResult) {
    let world = match case {
        0 => World::new(),
        1 => dustroute_translate::compiler::BaselineCompiler::new(Default::default())
            .compile(&dustroute_translate::circuits::half_adder())
            .unwrap()
            .world
            .into_world(),
        2 => {
            let mut world = World::new();
            for n in 0..70 {
                world.set(Pos::new(n * 4, 0, 0), Block::new(BlockKind::Lever));
                world.set(Pos::new(n * 4, 0, 2), Block::new(BlockKind::RedstoneLamp));
                let mut coarse = Block::new(BlockKind::Solid);
                coarse.observed_name = Some("minecraft:fixture_unmodeled".into());
                coarse.observation_classification =
                    dustroute_physical::ObservationClassification::Coarse;
                world.set(Pos::new(n * 4, 0, 4), coarse);
            }
            world
        }
        _ => unreachable!(),
    };
    let (min, max) = world
        .bounds()
        .unwrap_or((Pos::new(0, 0, 0), Pos::new(0, 0, 0)));
    let bounds = RegionBounds::new(min, max);
    let mut reverse = Translator.reverse(&world, ReverseRequest::new(bounds));
    if case == 1 {
        reverse.truth_table_error = Some(TruthTableError::BudgetExceeded {
            rows: 4,
            max_rows: 1,
            estimated_work_units: 12,
            max_work_units: u128::MAX,
        });
    }
    (bounds, reverse)
}
pub(crate) fn truth_errors() -> Vec<TruthTableError> {
    use TruthTableError::*;
    let pos = Pos::new(1, 2, 3);
    vec![
        BudgetExceeded {
            rows: 4,
            max_rows: 1,
            estimated_work_units: u128::from(u64::MAX) + 1,
            max_work_units: u128::MAX,
        },
        RuntimeBudgetExceeded {
            rows: 4,
            completed_rows: 0,
            solver_iterations: 5,
            max_solver_iterations: 0,
        },
        ElapsedBudgetExceeded {
            rows: 4,
            completed_rows: 0,
            elapsed_millis: u128::from(u64::MAX) + 1,
            max_elapsed_millis: 1,
        },
        NonSettling {
            row: 0,
            settle_ticks: 60,
            pending_events: true,
        },
        TooManyInputs(17),
        IncompleteObservation,
        NoInputs,
        NoOutputs,
        UnmappedExternalInputs(vec![pos]),
        UnmappedObservableOutputs(vec![pos]),
        AmbiguousInputMapping {
            external_inputs: 0,
            inferred_inputs: 1,
        },
        AmbiguousOutputMapping {
            observable_outputs: 0,
            inferred_outputs: 1,
        },
        NoDriverPosition(pos),
        InvalidDriver {
            position: pos,
            expected: "Lever",
            actual: BlockKind::Solid,
        },
        Simulation("original simulation error".into()),
    ]
}

fn native_wire() -> Value {
    let (empty_bounds, empty) = fixture(0);
    let (bounds, mut translated) = fixture(1);
    let (_, bulk) = fixture(2);
    translated.analysis.scene.observation.regions[0].completeness =
        dustroute_physical::RegionCompleteness::OpenBoundary;
    let mut hierarchy = dustroute_ir::derive_hierarchy(&translated.analysis.scene);
    for n in 0..20 {
        hierarchy
            .physical_snapshot
            .diagnostics
            .push(dustroute_ir::IrDiagnostic {
                stage: dustroute_ir::IrStage::PhysicalSnapshot,
                severity: dustroute_ir::DiagnosticSeverity::Warning,
                code: format!("fixture_{n}"),
                message: format!("fixture diagnostic {n}"),
                physical_components: Default::default(),
            });
    }
    let known = translated.analysis.scene.components[0].pos;
    let missing = Pos::new(-100, 0, 0);
    let expansion = crate::recorded_analysis::RecordedExpansion::ExplicitSelectedRegion {
        components_loaded: Some(600),
        component_limit: None,
        limit_reached: false,
    };
    let errors=truth_errors().into_iter().map(|error| {
        let mut reverse=empty.clone();reverse.truth_table_error=Some(error);
        serde_json::to_value(super::truth::TruthDiagnostics::new(&reverse)).unwrap()["truth_table_error_details"].clone()
    }).collect::<Vec<_>>();
    json!({
        "empty":reverse_report(empty_bounds,&empty),
        "adder":reverse_report(bounds,&translated),
        "hierarchy_partial":hierarchical_report(bounds,&hierarchy,Some(focused_hierarchy(&translated.analysis.scene,&hierarchy,known)),expansion,Some(known)),
        "flat_focus":[focused_component(&translated,known),focused_component(&translated,missing)],
        "hierarchy_focus":[focused_hierarchy(&translated.analysis.scene,&hierarchy,known),focused_hierarchy(&translated.analysis.scene,&hierarchy,missing)],
        "capabilities":super::common::capability_report(&bulk.analysis.scene),
        "liveness":super::common::signal_liveness(&bulk.analysis.scene,None),
        "focused_liveness":super::common::signal_liveness(&bulk.analysis.scene,Some(known)),
        "truth_errors":errors,
    })
}
fn compare_wire(path: &str, actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                e.keys().collect::<Vec<_>>(),
                "{path}: field set"
            );
            for (key, value) in a {
                compare_wire(&format!("{path}/{key}"), value, &e[key]);
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "{path}: array length");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                compare_wire(&format!("{path}/{i}"), a, e);
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn native_analysis_matches_pre_migration_mcp_wire() {
    let expected: Value =
        serde_json::from_str(include_str!("analysis-wire-before-1715ca1.json")).unwrap();
    let actual = native_wire();
    compare_wire("", &actual, &expected);
    assert_eq!(
        actual["capabilities"]["issue_samples"]
            .as_array()
            .unwrap()
            .len(),
        32
    );
    assert!(actual["capabilities"]["issue_count"].as_u64().unwrap() > 32);
    assert_eq!(actual["capabilities"]["issues_truncated"], true);
    assert_eq!(
        actual["liveness"]["source_evidence"]
            .as_array()
            .unwrap()
            .len(),
        64
    );
    assert!(actual["liveness"]["drive_source_count"].as_u64().unwrap() > 64);
    let partial = &actual["hierarchy_partial"];
    assert_eq!(partial["analysis_complete"], true);
    assert_eq!(
        partial["focused_explanation"]["observation_complete"],
        false
    ); // Historical root flag, not a new validity gate.
    assert_eq!(
        partial["stages"]["physical_snapshot"]["diagnostics"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
    assert!(
        partial["stages"]["physical_snapshot"]["diagnostic_count"]
            .as_u64()
            .unwrap()
            > 16
    );
    assert_eq!(
        partial["stages"]["physical_snapshot"]["diagnostics_truncated"],
        true
    );
    assert!(actual["flat_focus"][1]["block"].is_null());
    assert!(actual["flat_focus"][1].get("signal_component").is_none());
    assert!(actual["hierarchy_focus"][1].get("block").is_none());
}
#[test]
fn truth_diagnostics_keep_computed_priority_and_large_counts_native() {
    let (_, mut reverse) = fixture(0);
    for error in truth_errors() {
        reverse.truth_table = None;
        reverse.truth_table_error = Some(error.clone());
        let report = super::truth::TruthDiagnostics::new(&reverse);
        assert_eq!(report.error, Some(error));
        let wire = serde_json::to_value(&report).unwrap();
        assert!(wire["truth_table_error_details"].is_object());
        reverse.truth_table = Some(dustroute_translate::world_reverse::InferredTruthTable {
            inputs: vec![],
            outputs: vec![],
            rows: vec![],
        });
        assert_eq!(
            serde_json::to_value(super::truth::TruthDiagnostics::new(&reverse)).unwrap()["truth_table_status"],
            "computed"
        );
    }
    reverse.truth_table = None;
    reverse.truth_table_error = None;
    let wire = serde_json::to_value(super::truth::TruthDiagnostics::new(&reverse)).unwrap();
    assert_eq!(
        wire,
        json!({"truth_table_status":"not_requested","truth_table_error":null,"truth_table_error_details":null})
    );
    reverse.truth_table_error = Some(TruthTableError::BudgetExceeded {
        rows: 0,
        max_rows: 0,
        estimated_work_units: u128::from(u64::MAX),
        max_work_units: u128::from(u64::MAX) + 1,
    });
    let report = super::truth::TruthDiagnostics::new(&reverse);
    let wire = serde_json::to_value(&report).unwrap();
    assert_eq!(
        wire["truth_table_error_details"]["estimated_work_units"],
        json!(u64::MAX)
    );
    assert_eq!(
        wire["truth_table_error_details"]["max_work_units"],
        "18446744073709551616"
    );
    assert_eq!(wire["truth_table_error_details"]["rows"], 0);
}

#[test]
fn liveness_samples_keep_full_totals_and_known_isolated_focus() {
    let mut world = World::new();
    for n in 0..70 {
        let z = n * 4;
        for x in 0..=4 {
            world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
        }
        world.set(Pos::new(1, 1, z), Block::new(BlockKind::RedstoneWire));
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.facing = Some(dustroute_physical::Facing::East);
        world.set(Pos::new(2, 1, z), repeater.clone());
        world.set(Pos::new(4, 1, z), repeater);
    }
    let isolated = Pos::new(4, 1, 0);
    let analysis = dustroute_translate::world_reverse::analyze_world_region(
        &world,
        RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(10, 2, 280)),
    );
    let native = dustroute_translate::liveness::analyze_signal_liveness(&analysis.scene);
    assert_eq!(native.undriven_inputs.len(), 70);
    assert_eq!(
        native
            .required_input_assessments
            .iter()
            .filter(|a| a.status
                == dustroute_translate::liveness::RequiredInputStatus::AwaitingExternalInput)
            .count(),
        70
    );
    let report = serde_json::to_value(super::common::signal_liveness(
        &analysis.scene,
        Some(Pos::new(2, 1, 0)),
    ))
    .unwrap();
    assert_eq!(report["external_input_waiting_count"], 70);
    assert_eq!(
        report["external_input_waiting"].as_array().unwrap().len(),
        64
    );
    assert_eq!(report["undriven_required_input_count"], 70);
    assert_eq!(
        report["undriven_required_inputs"].as_array().unwrap().len(),
        64
    );
    assert_eq!(
        report["ranked_findings_near_focus"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
    assert_eq!(report["findings_truncated"], true);
    let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
    let focus =
        serde_json::to_value(focused_hierarchy(&analysis.scene, &hierarchy, isolated)).unwrap();
    assert_eq!(focus["role"], "isolated_or_unresolved");
    assert_eq!(focus["block"], "Repeater");
    let missing = serde_json::to_value(focused_hierarchy(
        &analysis.scene,
        &hierarchy,
        Pos::new(9, 1, 0),
    ))
    .unwrap();
    assert_eq!(missing["role"], "support_or_unresolved");
    assert!(missing.get("block").is_none());
}
