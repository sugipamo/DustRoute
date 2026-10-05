//! Pure comparison of recorded evidence with signal contracts and simulation.
//! Restoration evidence and simulation equivalence are independent conclusions.
use super::StoredTransitionPlan;
use crate::bridge::{LeverActivation, UpdateRecording};
use crate::failure::{CauseKind, ExecutionProgress, FailureCause, FailurePhase, FailureReport};
use crate::operations::transition::{
    RecordingSummary, RestorationSummary, ScenarioVerification, TransitionOutcome,
    TransitionRunDetails, TransitionRunResult,
};
use crate::service::circuit_capture::is_redstone_candidate_name;
use crate::{behavior_trace_from_recording, scenario_trace_from_recording_with_initial};
use dustroute_ir::SignalIntent;
use dustroute_physical::ComponentId;
use dustroute_physical::PhysicalScene;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub(super) struct ObservedTransition {
    pub activation: LeverActivation,
    pub recording: UpdateRecording,
    pub restoration: RestorationSummary,
    pub wait_error: Option<String>,
    pub progress: ExecutionProgress,
    pub failure: Option<FailureReport>,
}

pub(super) fn assess_observation(
    operation_id: Uuid,
    plan: &StoredTransitionPlan,
    scene: &PhysicalScene,
    contracts: &BTreeMap<ComponentId, SignalIntent>,
    observed: ObservedTransition,
) -> TransitionRunResult {
    let ObservedTransition {
        activation,
        recording,
        restoration,
        wait_error,
        mut progress,
        mut failure,
    } = observed;
    progress.phase = FailurePhase::PostAnalysis;
    let trace = behavior_trace_from_recording(
        &recording,
        scene,
        format!(
            "lever {} -> {}",
            activation.before_powered, activation.after_powered
        ),
    );
    let transient = dustroute_ir::assess_transients(&trace, contracts);
    let transition_trace = trace.transition_trace();
    let observe = plan
        .initial_snapshot
        .blocks
        .iter()
        .filter(|block| is_redstone_candidate_name(&block.name))
        .map(|block| block.pos)
        .collect::<BTreeSet<_>>();
    let duration_redstone_ticks = u64::from(plan.observation_ticks).div_ceil(2);
    let scenario = dustroute_translate::scenario::Scenario {
        label: format!("lever transition at {:?}", plan.lever),
        initial: plan.initial_snapshot.clone(),
        actions: vec![
            dustroute_translate::scenario::ScenarioAction::SetLeverState {
                redstone_tick: 0,
                position: plan.lever,
                powered: !plan.original_powered,
            },
        ],
        observe: observe.clone(),
        duration_redstone_ticks,
        required_capabilities: Vec::new(),
        expectation: dustroute_translate::scenario::ScenarioExpectation::default(),
    };
    let simulated = dustroute_translate::analysis::simulate_scenario(&scenario);
    let live_scenario_trace = scenario_trace_from_recording_with_initial(
        &recording,
        &observe,
        duration_redstone_ticks,
        Some(&plan.initial_snapshot),
    );
    let simulation_comparison = simulated.as_ref().ok().map(|simulated| {
        dustroute_translate::analysis::compare_live_trace(&simulated.trace, &live_scenario_trace)
    });
    let steady_state_equivalent = simulated.as_ref().is_ok_and(|simulated| {
        simulated.trace.final_strengths == live_scenario_trace.final_strengths
            && simulated.trace.final_powered == live_scenario_trace.final_powered
    });
    if recording.truncated {
        FailureReport::append(
            &progress,
            &mut failure,
            FailureCause::new(
                CauseKind::ObservationIncomplete,
                "update recording truncated",
            )
            .at(FailurePhase::AfterReadback),
        );
    }
    TransitionRunResult::completed(TransitionRunDetails {
        operation_id,
        activation,
        observation_ticks: plan.observation_ticks,
        recording: RecordingSummary {
            started_game_tick: recording.started_game_tick,
            stopped_game_tick: recording.stopped_game_tick,
            seen_events: recording.seen_events,
            stored_events: recording.events.len(),
            truncated: recording.truncated,
        },
        trace,
        transition_trace,
        transient_assessment: transient,
        scenario_verification: ScenarioVerification {
            scenario,
            simulated: simulated.into(),
            live_trace: live_scenario_trace,
            trace_equivalent: simulation_comparison.as_ref().is_some_and(Vec::is_empty),
            differences: simulation_comparison,
            steady_state_equivalent,
        },
        restoration,
        wait_error,
        guidance: "hazard_candidate is an observed pulse without registered intent; register a signal contract before calling it a confirmed hazard",
        outcome: TransitionOutcome::from_attempt(progress.clone(), failure),
    })
}
