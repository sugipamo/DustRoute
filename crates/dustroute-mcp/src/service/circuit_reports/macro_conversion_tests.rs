//! MCP compatibility only: fixed native inputs and an independently captured legacy response.
use super::super::*;
use super::macro_conversion::*;
use dustroute_optimize::{
    MacroRealizationError, MacroSteadyStateReport, MacroTransitionCase, MacroTransitionReport,
};
use dustroute_translate::world_reverse::RegionBounds;
use serde_json::{Value, json};
fn fixture() -> MacroProposals {
    let world = dustroute_translate::cells::compiled_xor_cell()
        .unwrap()
        .world;
    let (min, max) = world.bounds().unwrap();
    let bounds = RegionBounds::new(min.offset(-8, -3, -8), max.offset(8, 6, 8));
    let staged = dustroute_translate::api::Translator
        .reverse(&world, ReverseRequest::new(bounds).with_truth_table(16));
    let model = staged.functional_network.as_ref().unwrap();
    let candidates = find_builtin_verified_macro_replacements(
        model,
        "java",
        "1.21.11",
        ObservedMacroMetrics::from_world(&world),
    );
    let candidate = candidates
        .iter()
        .find(|c| c.component_id.as_str() == dustroute_library::DUSTROUTE_COMPACT_XOR_ID)
        .unwrap();
    let boundary = extract_model_boundary_with_context(model, &world, &staged.analysis);
    let reserved = boundary.iter().filter_map(|p| p.driver_position).collect();
    let boundaries = boundary.iter().map(|p| p.position).collect::<BTreeSet<_>>();
    let replaceable = world
        .positions()
        .filter(|p| !boundaries.contains(p))
        .collect();
    let mut plan = plan_macro_replacement_with_reserved(candidate, &boundary, &reserved).unwrap();
    let structural = validate_macro_structure(&plan, &world, &replaceable);
    assert!(structural.valid());
    let materialized = materialize_macro_replacement_in_known_regions(
        &plan,
        &world,
        &[dustroute_translate::world::Region::new(
            Pos::new(-128, -16, -128),
            Pos::new(128, 32, 128),
        )],
        &replaceable,
        14,
    )
    .unwrap();
    plan.verification.structural = ContextualVerificationState::Passed;
    plan.verification.steady_state = ContextualVerificationState::Failed;
    let steady = MacroSteadyStateReport {
        state: ContextualVerificationState::Failed,
        comparison: None,
        input_mapping: vec![1, 0],
        output_mapping: vec![0],
        differing_assignments: vec![vec![true, false]],
        reason: Some("fixture mismatch".into()),
    };
    let transitions = MacroTransitionReport {
        unavailable_reason: None,
        state: ContextualVerificationState::Failed,
        cases: vec![MacroTransitionCase {
            from: vec![false, false],
            to: vec![true, false],
            original_outputs: vec![vec![false], vec![true], vec![true], vec![false]],
            candidate_outputs: vec![vec![false], vec![false], vec![true], vec![false]],
            equivalent: false,
            first_difference_tick: Some(1),
        }],
        differing_cases: 1,
        reason: None,
    };
    let contract = OptimizationContract::default();
    let assessment = assess_macro_contract(
        contract,
        &structural,
        Some(&steady),
        Some(&transitions),
        materialized.patch.changes.len(),
        None,
    );
    let success = MacroPlanReport {
        plan: plan.clone(),
        structural: structural.clone(),
        materialized: Ok(materialized),
        steady_state: Some(steady),
        transitions: Some(transitions),
        contract,
        contract_assessment: Some(assessment),
    };
    let mut failed_structure = structural;
    failed_structure
        .route_cross_net_contacts
        .push((0, 1, Pos::new(3, 2, 1), Pos::new(4, 2, 1)));
    plan.verification.structural = ContextualVerificationState::Failed;
    plan.verification.steady_state = ContextualVerificationState::Pending;
    let failed = MacroPlanReport {
        plan,
        structural: failed_structure.clone(),
        materialized: Err(MacroRealizationError::StructurallyInvalid(Box::new(
            failed_structure,
        ))),
        steady_state: None,
        transitions: None,
        contract,
        contract_assessment: None,
    };
    MacroProposals {
        candidates,
        placement_plans: vec![success, failed],
    }
}
#[test]
fn macro_reports_preserve_the_wire_and_add_typed_transition_provenance() {
    let mut native = fixture();
    let mut expected: Value =
        serde_json::from_str(include_str!("macro-wire-before-bdc08c6.json")).unwrap();
    // Retain the historical fixture unchanged; the new diagnostic field is
    // additive and absence of a failure kind must not invent one.
    expected["placement_plans"][0]["transition_report"]["reason_code"] = Value::Null;
    assert_eq!(serde_json::to_value(&native).unwrap(), expected);
    let transitions = native.placement_plans[0].transitions.as_mut().unwrap();
    transitions.state = ContextualVerificationState::Pending;
    transitions.unavailable_reason =
        Some(dustroute_optimize::TransitionUnavailableReason::VerificationBudget);
    transitions.reason = Some("ambiguous words are not a category".into());
    transitions.cases.clear();
    transitions.differing_cases = 0;
    let wire = serde_json::to_value(native).unwrap();
    let pending = &wire["placement_plans"][0]["transition_report"];
    assert_eq!(pending["reason_code"], "verification_budget");
    assert_eq!(pending["state"], "pending");
    assert_eq!(pending["case_count"], 0);
}
#[test]
fn no_candidate_is_distinct_from_a_known_empty_proposal_list() {
    assert_eq!(
        serde_json::to_value(None::<MacroProposals>).unwrap(),
        Value::Null
    );
    assert_eq!(
        serde_json::to_value(MacroProposals {
            candidates: vec![],
            placement_plans: vec![]
        })
        .unwrap(),
        json!({"status":"proposal_only","realization":"contextual placement and transition verification are required before a mutation plan can be created","candidates":[],"placement_plans":[]})
    );
}
