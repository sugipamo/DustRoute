//! MCP optimization input defaults and explicit timing intent.
use super::requests::{LogicalContractParam, TimingContractParam};
use super::*;

#[test]
fn optimization_contract_defaults_are_explicit_and_conservative() {
    let contract = optimization_contract_from_param(None).expect("default contract");
    assert_eq!(contract.timing.mode, TimingContractMode::BoundedDelay);
    assert!(!contract.pulse.allow_new_pulses);
    assert!(!contract.pulse.allow_removed_pulses);
    assert!(contract.boundary.preserve_driver_positions);
    assert!(!contract.mutation.automatic_apply);
}

#[test]
fn optimization_contract_rejects_an_unknown_logical_mode() {
    let error = optimization_contract_from_param(Some(OptimizationContractParam {
        logical: Some(LogicalContractParam {
            mode: Some("approximate".to_owned()),
        }),
        ..OptimizationContractParam::default()
    }))
    .expect_err("unknown mode must fail");
    assert!(error.contains("approximate"));
}

#[test]
fn optimization_contract_accepts_transition_edge_comparison() {
    let contract = optimization_contract_from_param(Some(OptimizationContractParam {
        timing: Some(TimingContractParam {
            mode: Some("exact_transitions".to_owned()),
            ..TimingContractParam::default()
        }),
        ..OptimizationContractParam::default()
    }))
    .unwrap();
    assert_eq!(contract.timing.mode, TimingContractMode::ExactTransitions);
    assert_eq!(
        serde_json::to_value(OptimizationContractView(contract)).unwrap()["timing"]["mode"],
        "exact_transitions"
    );
}
