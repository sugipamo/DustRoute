//! Evidence-aware comparison between live scheduler observations and the
//! Minecraft transition engine.

use std::collections::{BTreeMap, BTreeSet};

use dustroute_minecraft::time::{TraceStatus, TransitionTrace as MinecraftTransitionTrace};
use serde::{Deserialize, Serialize};

use crate::vanilla_instrumentation::{
    PistonStateKind, PistonStateObservation, StreamCompleteness, VanillaInstrumentationArtifact,
};
use crate::{Block, BlockKind, Facing, Pos, WireConnection};
use dustroute_minecraft::piston_state;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionEvidence {
    ObservedPacket,
    ObservedInternal,
    ModelledScheduler,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "order", rename_all = "snake_case")]
pub enum SameTickOrderEvidence {
    ObservedPacket(u64),
    ModelledScheduler(u64),
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NormalizedBlockState {
    pub name: String,
    pub properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NormalizedTransition {
    /// Signed so bounded pre-roll observations can remain distinct from the
    /// server-input origin instead of being clamped to zero.
    pub relative_game_tick: i64,
    pub same_tick_order: SameTickOrderEvidence,
    /// `None` when the source cannot observe Vanilla's internal phase.
    pub scheduler_phase: Option<String>,
    pub event_kind: Option<String>,
    pub position: Pos,
    pub before: NormalizedBlockState,
    pub after: NormalizedBlockState,
    pub changed: bool,
    pub evidence: TransitionEvidence,
    /// Source-local causal group. Vanilla observations use the retained
    /// OrderedTick sequence; modelled transitions use their scheduler EventId.
    /// Values are compared relationally (same cause vs different cause), never
    /// as cross-source scalar IDs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_cause_sequence: Option<u64>,
}

/// A projected server-side callback in Vanilla's chained neighbor updater.
/// Callback sequence/order is source-local and is never compared as a scalar
/// identifier with a model scheduler event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NormalizedNeighborUpdate {
    pub sequence: u64,
    /// Signed for the same bounded pre-roll reason as state transitions.
    pub relative_game_tick: i64,
    pub same_tick_order: u64,
    pub position: Pos,
    pub target: NormalizedBlockState,
    pub source_block: String,
    pub orientation: Option<String>,
    pub notify: bool,
    pub evidence: TransitionEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_cause_sequence: Option<u64>,
}

/// Version-neutral projection of a Vanilla piston observation. Unlike the
/// generic coordinate transition stream, this retains the stable/moving
/// distinction and block-entity evidence explicitly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NormalizedPistonState {
    pub relative_game_tick: i64,
    pub same_tick_order: SameTickOrderEvidence,
    pub position: Pos,
    pub state_kind: PistonStateKind,
    pub body: NormalizedBlockState,
    pub head: Option<NormalizedBlockState>,
    pub moving_block: Option<NormalizedBlockState>,
    pub block_entity_present: bool,
    pub block_entity_extending: Option<bool>,
    pub evidence: TransitionEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_cause_sequence: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NormalizedTransitionTrace {
    pub transitions: Vec<NormalizedTransition>,
    #[serde(default)]
    pub neighbor_updates: Vec<NormalizedNeighborUpdate>,
    #[serde(default)]
    pub piston_states: Vec<NormalizedPistonState>,
    pub complete: bool,
    pub unavailable_reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ObservedSchedulerFixture {
    pub schema_version: String,
    pub minecraft_version: String,
    pub profile_id: String,
    pub profile_evidence: String,
    pub evidence: String,
    pub source: String,
    pub scenario: String,
    pub source_artifact: String,
    pub clock: ObservedClock,
    pub input: ObservedInput,
    pub events: Vec<ObservedTransitionEvent>,
    #[serde(default)]
    pub measurements: BTreeMap<String, u64>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ObservedClock {
    pub unit: String,
    pub origin: String,
    pub absolute_ticks_omitted: bool,
    pub scheduler_phase: Option<String>,
    pub scheduler_phase_evidence: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ObservedInput {
    pub kind: String,
    pub transition: String,
    pub activation_is_baseline: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ObservedTransitionEvent {
    pub sequence: u64,
    pub kind: String,
    pub position: Pos,
    pub relative_game_tick: i64,
    pub sub_tick_order: u64,
    pub scheduler_phase: Option<String>,
    pub changed: bool,
    pub before: ObservedBlockState,
    pub after: ObservedBlockState,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ObservedBlockState {
    pub name: String,
    pub properties: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceStatus {
    Matched,
    Mismatch,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceField {
    Completeness,
    EventCount,
    RelativeGameTick,
    SameTickOrder,
    Position,
    BeforeName,
    BeforeProperty,
    AfterName,
    AfterProperty,
    NoOp,
    ChangeOrder,
    SchedulerCause,
    NeighborUpdate,
    PistonState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConformanceIssue {
    pub transition_index: Option<usize>,
    pub field: ConformanceField,
    pub status: ConformanceStatus,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionConformance {
    pub status: ConformanceStatus,
    pub compared_transitions: usize,
    pub issues: Vec<ConformanceIssue>,
}

pub fn observed_fixture_from_json(
    source: &str,
) -> Result<ObservedSchedulerFixture, serde_json::Error> {
    serde_json::from_str(source)
}

#[must_use]
pub fn normalize_observed_fixture(fixture: &ObservedSchedulerFixture) -> NormalizedTransitionTrace {
    let transitions = fixture
        .events
        .iter()
        .map(|event| NormalizedTransition {
            relative_game_tick: event.relative_game_tick,
            same_tick_order: SameTickOrderEvidence::ObservedPacket(event.sub_tick_order),
            scheduler_phase: event.scheduler_phase.clone(),
            event_kind: Some(event.kind.clone()),
            position: event.position,
            before: observed_state(&event.before),
            after: observed_state(&event.after),
            changed: event.changed,
            evidence: TransitionEvidence::ObservedPacket,
            scheduler_cause_sequence: None,
        })
        .collect();
    NormalizedTransitionTrace {
        transitions,
        neighbor_updates: Vec::new(),
        piston_states: Vec::new(),
        complete: true,
        unavailable_reasons: Vec::new(),
    }
}

/// Projects the server-side 1.21.11 instrumentation artifact into the common
/// transition form. State-event ordering remains unavailable at coordinate
/// granularity; an optional OrderedTick reference is retained separately as a
/// source-local causal group.
#[must_use]
pub fn normalize_vanilla_instrumentation_artifact(
    artifact: &VanillaInstrumentationArtifact,
) -> NormalizedTransitionTrace {
    let transitions = artifact
        .state_events
        .iter()
        .map(|event| NormalizedTransition {
            relative_game_tick: event.game_tick,
            same_tick_order: SameTickOrderEvidence::Unavailable,
            scheduler_phase: None,
            event_kind: Some(event.source.clone()),
            position: event.position,
            before: instrumented_state(&event.before),
            after: instrumented_state(&event.after),
            changed: event.changed,
            evidence: TransitionEvidence::ObservedInternal,
            scheduler_cause_sequence: event.ordered_tick_sequence,
        })
        .collect();
    let neighbor_updates = artifact
        .neighbor_updates
        .iter()
        .map(|event| NormalizedNeighborUpdate {
            sequence: event.sequence,
            relative_game_tick: event.game_tick,
            same_tick_order: event.sub_tick_order,
            position: event.position,
            target: instrumented_state(&event.target),
            source_block: event.source_block.clone(),
            orientation: event.orientation.clone(),
            notify: event.notify,
            evidence: TransitionEvidence::ObservedInternal,
            scheduler_cause_sequence: event.ordered_tick_sequence,
        })
        .collect();
    let piston_states = artifact
        .piston_states
        .iter()
        .map(normalize_observed_piston_state)
        .collect();
    let mut unavailable_reasons = Vec::new();
    if !artifact.completeness.state_events {
        unavailable_reasons.push("Vanilla state-event evidence is incomplete".into());
    }
    if !artifact.completeness.ordered_ticks {
        unavailable_reasons.push("Vanilla OrderedTick evidence is incomplete".into());
    }
    if !artifact.completeness.neighbor_updates {
        unavailable_reasons.push("Vanilla neighbor-update evidence is incomplete".into());
    }
    if !artifact.completeness.piston_state {
        unavailable_reasons.push("Vanilla piston-state evidence is incomplete".into());
    }
    if let Some(capture) = &artifact.capture {
        for (stream, status) in &capture.stream_completeness {
            if *status != StreamCompleteness::Complete {
                unavailable_reasons.push(format!(
                    "Vanilla capture stream {stream} is {status:?}; absence is not evidence of no event"
                ));
            }
        }
    }
    let state_stream_complete = artifact.capture.as_ref().is_none_or(|capture| {
        capture
            .stream_completeness
            .get("block_state_change")
            .is_none_or(|status| *status == StreamCompleteness::Complete)
    });
    let piston_stream_complete = artifact.capture.as_ref().is_none_or(|capture| {
        capture
            .stream_completeness
            .get("piston_state")
            .or_else(|| capture.stream_completeness.get("piston_states"))
            .is_none_or(|status| *status == StreamCompleteness::Complete)
    });
    NormalizedTransitionTrace {
        transitions,
        neighbor_updates,
        piston_states,
        complete: artifact.completeness.state_events
            && artifact.completeness.piston_state
            && state_stream_complete
            && piston_stream_complete,
        unavailable_reasons,
    }
}

/// Projects state changes relative to a caller-provided activation tick.
/// A record containing several changes exposes no coordinate-level order, so
/// each projected change carries `Unavailable` order evidence.
#[must_use]
pub fn normalize_transition_trace(
    trace: &MinecraftTransitionTrace,
    activation_game_tick: u64,
) -> NormalizedTransitionTrace {
    let mut unavailable_reasons = Vec::new();
    let mut transitions = Vec::new();
    let mut piston_baseline = BTreeMap::<Pos, NormalizedBlockState>::new();
    for record in &trace.records {
        for change in &record.changes {
            piston_baseline
                .entry(change.position)
                .or_insert_with(|| modelled_state(&change.before));
        }
    }
    for record in &trace.records {
        let multiple = record.changes.len() > 1;
        if multiple {
            unavailable_reasons.push(format!(
                "transition {} contains {} unordered block changes",
                record.id.0,
                record.changes.len()
            ));
        }
        for change in &record.changes {
            let before = if change.before.kind == BlockKind::MovingPiston {
                piston_baseline
                    .get(&change.position)
                    .cloned()
                    .unwrap_or_else(|| modelled_state(&change.before))
            } else {
                modelled_state(&change.before)
            };
            let after = modelled_state(&change.after);
            transitions.push(NormalizedTransition {
                relative_game_tick: relative_tick(record.time.game_tick, activation_game_tick),
                same_tick_order: if multiple {
                    SameTickOrderEvidence::Unavailable
                } else {
                    SameTickOrderEvidence::ModelledScheduler(record.time.sub_tick_order)
                },
                scheduler_phase: Some(format!("{:?}", record.time.phase).to_ascii_lowercase()),
                event_kind: None,
                position: change.position,
                changed: before != after,
                before,
                after,
                evidence: TransitionEvidence::ModelledScheduler,
                scheduler_cause_sequence: Some(record.trigger.0),
            });
        }
    }
    let piston_states = normalize_model_piston_states(trace, activation_game_tick);
    NormalizedTransitionTrace {
        transitions,
        neighbor_updates: Vec::new(),
        piston_states,
        complete: matches!(trace.status, TraceStatus::Complete),
        unavailable_reasons,
    }
}

#[must_use]
pub fn compare_transition_traces(
    observed: &NormalizedTransitionTrace,
    modelled: &NormalizedTransitionTrace,
) -> TransitionConformance {
    let mut issues = Vec::new();
    if !observed.complete || !modelled.complete {
        issues.push(unavailable(
            None,
            ConformanceField::Completeness,
            "a trace is incomplete",
        ));
    }
    let reasons: BTreeSet<_> = observed
        .unavailable_reasons
        .iter()
        .chain(&modelled.unavailable_reasons)
        .collect();
    for reason in reasons {
        issues.push(unavailable(None, ConformanceField::ChangeOrder, reason));
    }
    compare_neighbor_update_evidence(
        &observed.neighbor_updates,
        &modelled.neighbor_updates,
        &mut issues,
    );
    compare_piston_state_evidence(
        &observed.piston_states,
        &modelled.piston_states,
        &mut issues,
    );

    let observed_changes: Vec<_> = observed
        .transitions
        .iter()
        .filter(|item| {
            item.changed
                && if observed.piston_states.is_empty() {
                    !normalized_transition_is_moving_piston(item)
                } else {
                    !observed
                        .piston_states
                        .iter()
                        .any(|state| state.position == item.position)
                }
        })
        .collect();
    let modelled_changes: Vec<_> = modelled
        .transitions
        .iter()
        .filter(|item| {
            item.changed
                && if observed.piston_states.is_empty() {
                    !normalized_transition_is_moving_piston(item)
                } else {
                    !observed
                        .piston_states
                        .iter()
                        .any(|state| state.position == item.position)
                }
        })
        .collect();
    let no_ops = observed
        .transitions
        .iter()
        .filter(|item| !item.changed)
        .count();
    if no_ops > 0 {
        issues.push(unavailable(
            None,
            ConformanceField::NoOp,
            &format!("{no_ops} observed no-op event(s) require EventTrace evidence"),
        ));
    }
    if observed_changes.len() != modelled_changes.len() {
        issues.push(mismatch(
            None,
            ConformanceField::EventCount,
            format!(
                "observed {} state changes, modelled {}",
                observed_changes.len(),
                modelled_changes.len()
            ),
        ));
    }

    let mut used_modelled = BTreeSet::new();
    let mut matched = Vec::new();
    for (observed_index, expected) in observed_changes.iter().enumerate() {
        let actual = modelled_changes.iter().enumerate().find(|(index, actual)| {
            !used_modelled.contains(index) && actual.position == expected.position
        });
        if let Some((modelled_index, actual)) = actual {
            used_modelled.insert(modelled_index);
            matched.push((observed_index, modelled_index));
            compare_one(observed_index, expected, actual, &mut issues);
        } else {
            issues.push(mismatch(
                Some(observed_index),
                ConformanceField::Position,
                format!(
                    "no modelled transition at observed position {:?}",
                    expected.position
                ),
            ));
        }
    }
    for pair in matched.windows(2) {
        let [
            (left_observed, left_modelled),
            (right_observed, right_modelled),
        ] = pair
        else {
            unreachable!("windows(2) always contains two pairs")
        };
        let left_expected = observed_changes[*left_observed];
        let right_expected = observed_changes[*right_observed];
        let left_actual = modelled_changes[*left_modelled];
        let right_actual = modelled_changes[*right_modelled];
        if left_expected.relative_game_tick == right_expected.relative_game_tick
            && left_actual.relative_game_tick == right_actual.relative_game_tick
            && !matches!(
                (
                    &left_expected.same_tick_order,
                    &right_expected.same_tick_order
                ),
                (SameTickOrderEvidence::Unavailable, _) | (_, SameTickOrderEvidence::Unavailable)
            )
            && !matches!(
                (&left_actual.same_tick_order, &right_actual.same_tick_order),
                (SameTickOrderEvidence::Unavailable, _) | (_, SameTickOrderEvidence::Unavailable)
            )
            && right_modelled < left_modelled
        {
            issues.push(mismatch(
                Some(*right_observed),
                ConformanceField::SameTickOrder,
                format!(
                    "same-tick relative order differs between {:?} and {:?}",
                    left_expected.position, right_expected.position
                ),
            ));
        }
    }
    compare_scheduler_cause_relationships(
        &observed_changes,
        &modelled_changes,
        &matched,
        &mut issues,
    );
    let status = if issues
        .iter()
        .any(|issue| issue.status == ConformanceStatus::Mismatch)
    {
        ConformanceStatus::Mismatch
    } else if issues
        .iter()
        .any(|issue| issue.status == ConformanceStatus::Unavailable)
    {
        ConformanceStatus::Unavailable
    } else {
        ConformanceStatus::Matched
    };
    TransitionConformance {
        status,
        compared_transitions: matched.len(),
        issues,
    }
}

fn compare_piston_state_evidence(
    observed: &[NormalizedPistonState],
    modelled: &[NormalizedPistonState],
    issues: &mut Vec<ConformanceIssue>,
) {
    if observed_stream_is_partial("piston_state", issues)
        || observed_stream_is_partial("piston_states", issues)
    {
        return;
    }
    if observed.is_empty() && modelled.is_empty() {
        return;
    }
    if observed.is_empty() || modelled.is_empty() {
        issues.push(unavailable(
            None,
            ConformanceField::PistonState,
            if observed.is_empty() {
                "model exposes piston-state evidence but the observation does not"
            } else {
                "Vanilla piston-state evidence is observed but the model trace does not expose it"
            },
        ));
        return;
    }
    if observed.len() != modelled.len() {
        issues.push(mismatch(
            None,
            ConformanceField::PistonState,
            format!(
                "observed {} piston states, modelled {}",
                observed.len(),
                modelled.len()
            ),
        ));
    }
    for (index, (expected, actual)) in observed.iter().zip(modelled).enumerate() {
        if expected.relative_game_tick != actual.relative_game_tick
            || expected.position != actual.position
            || expected.state_kind != actual.state_kind
            || expected.block_entity_present != actual.block_entity_present
            || expected.block_entity_extending != actual.block_entity_extending
        {
            issues.push(mismatch(
                Some(index),
                ConformanceField::PistonState,
                format!(
                    "piston state differs at {:?}: observed tick/state {:?}/{:?}, modelled {:?}/{:?}",
                    expected.position,
                    expected.relative_game_tick,
                    expected.state_kind,
                    actual.relative_game_tick,
                    actual.state_kind
                ),
            ));
            continue;
        }
        compare_state_optional(index, "piston head", &expected.head, &actual.head, issues);
        compare_state_optional(
            index,
            "piston moving block",
            &expected.moving_block,
            &actual.moving_block,
            issues,
        );
        compare_state(index, "piston body", &expected.body, &actual.body, issues);
        if matches!(
            (&expected.same_tick_order, &actual.same_tick_order),
            (SameTickOrderEvidence::Unavailable, _) | (_, SameTickOrderEvidence::Unavailable)
        ) {
            issues.push(unavailable(
                Some(index),
                ConformanceField::PistonState,
                "piston same-tick order is unavailable",
            ));
        }
    }
}

fn normalized_transition_is_moving_piston(transition: &NormalizedTransition) -> bool {
    short_name(&transition.before.name) == "moving_piston"
        || short_name(&transition.after.name) == "moving_piston"
}

fn compare_state_optional(
    index: usize,
    side: &str,
    expected: &Option<NormalizedBlockState>,
    actual: &Option<NormalizedBlockState>,
    issues: &mut Vec<ConformanceIssue>,
) {
    match (expected, actual) {
        (None, None) => {}
        (Some(expected), Some(actual)) => compare_state(index, side, expected, actual, issues),
        _ => issues.push(mismatch(
            Some(index),
            ConformanceField::PistonState,
            format!("{side} presence differs"),
        )),
    }
}

fn compare_neighbor_update_evidence(
    observed: &[NormalizedNeighborUpdate],
    modelled: &[NormalizedNeighborUpdate],
    issues: &mut Vec<ConformanceIssue>,
) {
    if observed_stream_is_partial("neighbor_update", issues) {
        return;
    }
    if observed.is_empty() && modelled.is_empty() {
        return;
    }
    if observed.is_empty() || modelled.is_empty() {
        issues.push(unavailable(
            None,
            ConformanceField::NeighborUpdate,
            if observed.is_empty() {
                "model exposes neighbor-update callbacks but the observation does not"
            } else {
                "Vanilla neighbor-update callbacks are observed but the model trace does not expose them"
            },
        ));
        return;
    }
    if observed.len() != modelled.len() {
        issues.push(mismatch(
            None,
            ConformanceField::NeighborUpdate,
            format!(
                "observed {} neighbor updates, modelled {}",
                observed.len(),
                modelled.len()
            ),
        ));
    }
    for (index, (expected, actual)) in observed.iter().zip(modelled).enumerate() {
        if expected.relative_game_tick != actual.relative_game_tick
            || expected.position != actual.position
            || expected.target != actual.target
            || short_name(&expected.source_block) != short_name(&actual.source_block)
            || expected.orientation != actual.orientation
            || expected.notify != actual.notify
        {
            issues.push(mismatch(
                Some(index),
                ConformanceField::NeighborUpdate,
                format!(
                    "neighbor callback differs at {:?}: observed tick {}, modelled tick {}",
                    expected.position, expected.relative_game_tick, actual.relative_game_tick
                ),
            ));
        }
    }
    for pair in observed.iter().zip(modelled).collect::<Vec<_>>().windows(2) {
        let [(expected_left, actual_left), (expected_right, actual_right)] = pair else {
            unreachable!("windows(2) always contains two pairs")
        };
        if expected_left.relative_game_tick == expected_right.relative_game_tick
            && actual_left.relative_game_tick == actual_right.relative_game_tick
            && (expected_left.same_tick_order < expected_right.same_tick_order)
                != (actual_left.same_tick_order < actual_right.same_tick_order)
        {
            issues.push(mismatch(
                None,
                ConformanceField::NeighborUpdate,
                "same-tick neighbor callback order differs".into(),
            ));
        }
        match (
            expected_left.scheduler_cause_sequence,
            expected_right.scheduler_cause_sequence,
            actual_left.scheduler_cause_sequence,
            actual_right.scheduler_cause_sequence,
        ) {
            (Some(expected_left), Some(expected_right), Some(actual_left), Some(actual_right))
                if (expected_left == expected_right) != (actual_left == actual_right) =>
            {
                issues.push(mismatch(
                    None,
                    ConformanceField::NeighborUpdate,
                    "neighbor callback causal grouping differs".into(),
                ));
            }
            (Some(_), Some(_), Some(_), Some(_)) => {}
            _ => issues.push(unavailable(
                None,
                ConformanceField::NeighborUpdate,
                "neighbor callback causal grouping is unavailable",
            )),
        }
    }
}

fn observed_stream_is_partial(stream: &str, issues: &[ConformanceIssue]) -> bool {
    issues.iter().any(|issue| {
        issue.status == ConformanceStatus::Unavailable
            && issue
                .reason
                .contains(&format!("capture stream {stream} is"))
    })
}

fn compare_scheduler_cause_relationships(
    observed: &[&NormalizedTransition],
    modelled: &[&NormalizedTransition],
    matched: &[(usize, usize)],
    issues: &mut Vec<ConformanceIssue>,
) {
    if !observed
        .iter()
        .any(|transition| transition.evidence == TransitionEvidence::ObservedInternal)
    {
        return;
    }
    for pair in matched.windows(2) {
        let [
            (observed_left, modelled_left),
            (observed_right, modelled_right),
        ] = pair
        else {
            unreachable!("windows(2) always contains two pairs")
        };
        let expected_left = observed[*observed_left].scheduler_cause_sequence;
        let expected_right = observed[*observed_right].scheduler_cause_sequence;
        let actual_left = modelled[*modelled_left].scheduler_cause_sequence;
        let actual_right = modelled[*modelled_right].scheduler_cause_sequence;
        match (expected_left, expected_right, actual_left, actual_right) {
            (Some(expected_left), Some(expected_right), Some(actual_left), Some(actual_right))
                if (expected_left == expected_right) != (actual_left == actual_right) =>
            {
                issues.push(mismatch(
                    Some(*observed_right),
                    ConformanceField::SchedulerCause,
                    format!(
                        "causal grouping differs: observed {:?}/{:?}, modelled {:?}/{:?}",
                        expected_left, expected_right, actual_left, actual_right
                    ),
                ));
            }
            (Some(_), Some(_), Some(_), Some(_)) => {}
            _ => issues.push(unavailable(
                Some(*observed_right),
                ConformanceField::SchedulerCause,
                "scheduler cause is unavailable for one of the compared transitions",
            )),
        }
    }
}

fn compare_one(
    index: usize,
    expected: &NormalizedTransition,
    actual: &NormalizedTransition,
    issues: &mut Vec<ConformanceIssue>,
) {
    if expected.relative_game_tick != actual.relative_game_tick {
        issues.push(mismatch(
            Some(index),
            ConformanceField::RelativeGameTick,
            format!(
                "observed {}, modelled {}",
                expected.relative_game_tick, actual.relative_game_tick
            ),
        ));
    }
    match (&expected.same_tick_order, &actual.same_tick_order) {
        (SameTickOrderEvidence::Unavailable, _) | (_, SameTickOrderEvidence::Unavailable) => issues
            .push(unavailable(
                Some(index),
                ConformanceField::SameTickOrder,
                "same-tick order is unavailable",
            )),
        _ => {}
    }
    compare_state(index, "before", &expected.before, &actual.before, issues);
    compare_state(index, "after", &expected.after, &actual.after, issues);
}

fn compare_state(
    index: usize,
    side: &str,
    expected: &NormalizedBlockState,
    actual: &NormalizedBlockState,
    issues: &mut Vec<ConformanceIssue>,
) {
    let name_field = if side == "before" {
        ConformanceField::BeforeName
    } else {
        ConformanceField::AfterName
    };
    let property_field = if side == "before" {
        ConformanceField::BeforeProperty
    } else {
        ConformanceField::AfterProperty
    };
    if short_name(&expected.name) != short_name(&actual.name) {
        issues.push(mismatch(
            Some(index),
            name_field,
            format!("observed {}, modelled {}", expected.name, actual.name),
        ));
        return;
    }
    for (key, expected_value) in &expected.properties {
        match actual.properties.get(key) {
            Some(actual_value) if actual_value != expected_value => issues.push(mismatch(
                Some(index),
                property_field,
                format!("{side}.{key}: observed {expected_value}, modelled {actual_value}"),
            )),
            None => issues.push(unavailable(
                Some(index),
                property_field,
                &format!("model does not expose {side}.{key}"),
            )),
            _ => {}
        }
    }
}

fn observed_state(state: &ObservedBlockState) -> NormalizedBlockState {
    let properties = state
        .properties
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, value)| {
            let value = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            (key.clone(), value)
        })
        .collect();
    NormalizedBlockState {
        name: state.name.clone(),
        properties,
    }
}

fn instrumented_state(
    state: &crate::vanilla_instrumentation::InstrumentedBlockState,
) -> NormalizedBlockState {
    let properties = state
        .properties
        .iter()
        .map(|(key, value)| {
            let value = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            (key.clone(), value)
        })
        .collect();
    NormalizedBlockState {
        name: state.name.clone(),
        properties,
    }
}

fn normalize_observed_piston_state(observation: &PistonStateObservation) -> NormalizedPistonState {
    NormalizedPistonState {
        relative_game_tick: observation.game_tick,
        same_tick_order: SameTickOrderEvidence::Unavailable,
        position: observation.position,
        state_kind: observation.state_kind,
        body: instrumented_state(&observation.body),
        head: observation.head.as_ref().map(instrumented_state),
        moving_block: observation.moving_block.as_ref().map(instrumented_state),
        block_entity_present: observation.block_entity_present,
        block_entity_extending: observation.block_entity_extending,
        evidence: TransitionEvidence::ObservedInternal,
        scheduler_cause_sequence: observation.ordered_tick_sequence,
    }
}

fn normalize_model_piston_states(
    trace: &MinecraftTransitionTrace,
    activation_game_tick: u64,
) -> Vec<NormalizedPistonState> {
    let mut result = Vec::new();
    for record in &trace.records {
        let mut moving = record
            .changes
            .iter()
            .filter(|change| change.after.kind == BlockKind::MovingPiston)
            .collect::<Vec<_>>();
        // Vanilla observes carried blocks before the source/head carrier in
        // the fixture; source=false is therefore the stable local order for
        // the moving-state evidence. The order remains source-local only.
        moving.sort_by_key(|change| {
            change
                .after
                .piston_entity
                .as_deref()
                .map_or((1_u8, change.position), |entity| {
                    (u8::from(entity.source), change.position)
                })
        });
        for change in moving {
            let entity = change.after.piston_entity.as_deref();
            let head = entity
                .filter(|entity| entity.pushed_block.kind == BlockKind::PistonHead)
                .map(|entity| modelled_state(&entity.pushed_block));
            let moving_block = entity
                .filter(|entity| entity.pushed_block.kind != BlockKind::PistonHead)
                .map(|entity| modelled_state(&entity.pushed_block));
            result.push(NormalizedPistonState {
                relative_game_tick: relative_tick(record.time.game_tick, activation_game_tick),
                same_tick_order: SameTickOrderEvidence::ModelledScheduler(
                    record.time.sub_tick_order,
                ),
                position: change.position,
                state_kind: PistonStateKind::Moving,
                body: modelled_state(&change.after),
                head,
                moving_block,
                block_entity_present: entity.is_some(),
                block_entity_extending: entity.map(|entity| entity.extending),
                evidence: TransitionEvidence::ModelledScheduler,
                scheduler_cause_sequence: Some(record.trigger.0),
            });
        }

        let stable_body = record.changes.iter().find(|change| {
            change.after.kind == BlockKind::Piston && piston_state(&change.after).is_stable()
        });
        if let Some(body_change) = stable_body {
            let head = record
                .changes
                .iter()
                .find(|change| change.after.kind == BlockKind::PistonHead)
                .map(|change| modelled_state(&change.after));
            result.push(NormalizedPistonState {
                relative_game_tick: relative_tick(record.time.game_tick, activation_game_tick),
                same_tick_order: SameTickOrderEvidence::ModelledScheduler(
                    record.time.sub_tick_order,
                ),
                position: body_change.position,
                state_kind: PistonStateKind::Stable,
                body: modelled_state(&body_change.after),
                head,
                moving_block: None,
                block_entity_present: false,
                block_entity_extending: None,
                evidence: TransitionEvidence::ModelledScheduler,
                scheduler_cause_sequence: Some(record.trigger.0),
            });
        }
    }
    result
}

fn relative_tick(game_tick: u64, activation_game_tick: u64) -> i64 {
    let delta = i128::from(game_tick) - i128::from(activation_game_tick);
    delta.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

fn modelled_state(block: &Block) -> NormalizedBlockState {
    let mut properties = block.observed_properties.clone();
    let boolean_property = match block.kind {
        BlockKind::RedstoneLamp | BlockKind::RedstoneTorch => "lit",
        BlockKind::Lever
        | BlockKind::Button
        | BlockKind::PressurePlate
        | BlockKind::Repeater
        | BlockKind::Comparator
        | BlockKind::Observer => "powered",
        _ => "",
    };
    if let Some(value) = block.powered
        && !boolean_property.is_empty()
    {
        properties.insert(boolean_property.into(), value.to_string());
    }
    if let Some(value) = block.power_level {
        properties.insert("power".into(), value.to_string());
    }
    if let Some(value) = block.delay {
        properties.insert("delay".into(), value.to_string());
    }
    if let Some(mut value) = block.facing {
        if matches!(
            block.kind,
            BlockKind::Repeater | BlockKind::Comparator | BlockKind::Observer
        ) {
            value = value.opposite();
        }
        properties.insert("facing".into(), facing_name(value).into());
    }
    if block.kind == BlockKind::Piston {
        properties.insert(
            "extended".into(),
            piston_state(block).is_extended().to_string(),
        );
    }
    if matches!(block.kind, BlockKind::PistonHead | BlockKind::MovingPiston) {
        properties.insert(
            "type".into(),
            match block.piston_variant.unwrap_or_default() {
                dustroute_minecraft::PistonVariant::Normal => "normal",
                dustroute_minecraft::PistonVariant::Sticky => "sticky",
            }
            .into(),
        );
    }
    if let Some(head) = &block.piston_head {
        properties.insert("facing".into(), facing_name(head.facing).into());
        properties.insert(
            "type".into(),
            format!("{:?}", head.variant).to_ascii_lowercase(),
        );
        properties.insert("short".into(), head.short.to_string());
    }
    if let Some(entity) = &block.piston_entity {
        properties.insert("facing".into(), facing_name(entity.facing).into());
        properties.insert("extending".into(), entity.extending.to_string());
        properties.insert("source".into(), entity.source.to_string());
        properties.insert("progress".into(), entity.progress.to_string());
    }
    if let Some(connections) = &block.wire_connections {
        for facing in [Facing::North, Facing::East, Facing::South, Facing::West] {
            let value = connections
                .get(&facing)
                .copied()
                .unwrap_or(WireConnection::None);
            properties.insert(
                facing_name(facing).into(),
                format!("{value:?}").to_ascii_lowercase(),
            );
        }
    }
    NormalizedBlockState {
        name: block
            .observed_name
            .clone()
            .unwrap_or_else(|| block_name(block.kind).into()),
        properties,
    }
}

fn block_name(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Air => "air",
        BlockKind::Solid => "solid",
        BlockKind::Transparent => "transparent",
        BlockKind::RedstoneWire => "redstone_wire",
        BlockKind::RedstoneTorch => "redstone_torch",
        BlockKind::Repeater => "repeater",
        BlockKind::Comparator => "comparator",
        BlockKind::Lever => "lever",
        BlockKind::Button => "button",
        BlockKind::PressurePlate => "pressure_plate",
        BlockKind::RedstoneLamp => "redstone_lamp",
        BlockKind::RedstoneBlock => "redstone_block",
        BlockKind::Observer => "observer",
        BlockKind::Piston => "piston",
        BlockKind::PistonHead => "piston_head",
        BlockKind::MovingPiston => "moving_piston",
    }
}

fn facing_name(facing: Facing) -> &'static str {
    match facing {
        Facing::North => "north",
        Facing::East => "east",
        Facing::South => "south",
        Facing::West => "west",
        Facing::Up => "up",
        Facing::Down => "down",
    }
}

fn short_name(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}
fn mismatch(index: Option<usize>, field: ConformanceField, reason: String) -> ConformanceIssue {
    ConformanceIssue {
        transition_index: index,
        field,
        status: ConformanceStatus::Mismatch,
        reason,
    }
}
fn unavailable(index: Option<usize>, field: ConformanceField, reason: &str) -> ConformanceIssue {
    ConformanceIssue {
        transition_index: index,
        field,
        status: ConformanceStatus::Unavailable,
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_minecraft::time::{
        EventId, PhysicsEventPhase, PhysicsTime, TransitionId, TransitionRecord,
    };
    use dustroute_minecraft::{BlockChange, ChangeReason, ShapeId, StateId};

    fn state(name: &str, powered: bool) -> NormalizedBlockState {
        NormalizedBlockState {
            name: name.to_owned(),
            properties: BTreeMap::from([("powered".to_owned(), powered.to_string())]),
        }
    }

    fn transition(evidence: TransitionEvidence) -> NormalizedTransition {
        NormalizedTransition {
            relative_game_tick: 2,
            same_tick_order: match evidence {
                TransitionEvidence::ObservedPacket => SameTickOrderEvidence::ObservedPacket(1),
                TransitionEvidence::ObservedInternal => SameTickOrderEvidence::Unavailable,
                TransitionEvidence::ModelledScheduler => {
                    SameTickOrderEvidence::ModelledScheduler(1)
                }
            },
            scheduler_phase: None,
            event_kind: None,
            position: Pos::new(1, 2, 3),
            before: state("minecraft:lever", false),
            after: state("lever", true),
            changed: true,
            evidence,
            scheduler_cause_sequence: None,
        }
    }

    fn trace(item: NormalizedTransition) -> NormalizedTransitionTrace {
        NormalizedTransitionTrace {
            transitions: vec![item],
            neighbor_updates: Vec::new(),
            piston_states: Vec::new(),
            complete: true,
            unavailable_reasons: Vec::new(),
        }
    }

    #[test]
    fn equivalent_sources_match_without_absolute_ids() {
        let observed = trace(transition(TransitionEvidence::ObservedPacket));
        let modelled = trace(transition(TransitionEvidence::ModelledScheduler));
        let result = compare_transition_traces(&observed, &modelled);
        assert_eq!(result.status, ConformanceStatus::Matched);
        assert_eq!(result.compared_transitions, 1);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn tick_and_state_differences_are_reported() {
        let observed = trace(transition(TransitionEvidence::ObservedPacket));
        let mut actual = transition(TransitionEvidence::ModelledScheduler);
        actual.relative_game_tick = 3;
        actual
            .after
            .properties
            .insert("powered".into(), "false".into());
        let result = compare_transition_traces(&observed, &trace(actual));
        assert_eq!(result.status, ConformanceStatus::Mismatch);
        for field in [
            ConformanceField::RelativeGameTick,
            ConformanceField::AfterProperty,
        ] {
            assert!(result.issues.iter().any(|issue| issue.field == field));
        }
    }

    #[test]
    fn same_tick_relative_order_is_compared_without_equating_source_ordinals() {
        let first = transition(TransitionEvidence::ObservedPacket);
        let mut second = first.clone();
        second.position = Pos::new(2, 2, 3);
        second.same_tick_order = SameTickOrderEvidence::ObservedPacket(9);
        let observed = NormalizedTransitionTrace {
            transitions: vec![first.clone(), second.clone()],
            neighbor_updates: Vec::new(),
            piston_states: Vec::new(),
            complete: true,
            unavailable_reasons: Vec::new(),
        };
        let mut model_first = first;
        model_first.evidence = TransitionEvidence::ModelledScheduler;
        model_first.same_tick_order = SameTickOrderEvidence::ModelledScheduler(40);
        let mut model_second = second;
        model_second.evidence = TransitionEvidence::ModelledScheduler;
        model_second.same_tick_order = SameTickOrderEvidence::ModelledScheduler(41);
        let same_order = NormalizedTransitionTrace {
            transitions: vec![model_first.clone(), model_second.clone()],
            neighbor_updates: Vec::new(),
            piston_states: Vec::new(),
            complete: true,
            unavailable_reasons: Vec::new(),
        };
        assert_eq!(
            compare_transition_traces(&observed, &same_order).status,
            ConformanceStatus::Matched
        );

        let reversed = NormalizedTransitionTrace {
            transitions: vec![model_second, model_first],
            neighbor_updates: Vec::new(),
            piston_states: Vec::new(),
            complete: true,
            unavailable_reasons: Vec::new(),
        };
        let result = compare_transition_traces(&observed, &reversed);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.field == ConformanceField::SameTickOrder)
        );
    }

    #[test]
    fn no_op_and_incomplete_trace_fail_closed_as_unavailable() {
        let mut observed = trace(transition(TransitionEvidence::ObservedPacket));
        observed.transitions[0].changed = false;
        let mut modelled = trace(transition(TransitionEvidence::ModelledScheduler));
        modelled.transitions.clear();
        modelled.complete = false;
        let result = compare_transition_traces(&observed, &modelled);
        assert_eq!(result.status, ConformanceStatus::Unavailable);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.field == ConformanceField::NoOp)
        );
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.field == ConformanceField::Completeness)
        );
    }

    #[test]
    fn projection_uses_relative_tick_and_does_not_invent_change_order() {
        let before = Block::new(BlockKind::Lever);
        let mut after = before.clone();
        after.powered = Some(true);
        let change = |x| BlockChange {
            position: Pos::new(x, 0, 0),
            before: before.clone(),
            after: after.clone(),
            reason: ChangeReason::ExternalInput,
        };
        let raw = MinecraftTransitionTrace {
            records: vec![TransitionRecord {
                id: TransitionId(99),
                trigger: EventId(44),
                time: PhysicsTime {
                    game_tick: 102,
                    phase: PhysicsEventPhase::External,
                    sub_tick_order: 8,
                },
                elapsed_from_previous: None,
                from_state: StateId(1),
                to_state: StateId(2),
                from_shape: ShapeId(3),
                to_shape: ShapeId(3),
                changes: vec![change(0), change(1)],
                moves: Vec::new(),
                cause: None,
            }],
            status: TraceStatus::Complete,
        };
        let normalized = normalize_transition_trace(&raw, 100);
        assert_eq!(normalized.transitions[0].relative_game_tick, 2);
        assert!(
            normalized
                .transitions
                .iter()
                .all(|item| item.same_tick_order == SameTickOrderEvidence::Unavailable)
        );
        assert_eq!(normalized.unavailable_reasons.len(), 1);
    }

    #[test]
    fn fixture_keeps_packet_order_noop_and_unknown_phase() {
        let fixture = observed_fixture_from_json(include_str!(
            "../tests/fixtures/scheduler_1_21_11_observed_repeater_observer.json"
        ))
        .unwrap();
        let normalized = normalize_observed_fixture(&fixture);
        assert_eq!(normalized.transitions.len(), fixture.events.len());
        assert!(matches!(
            normalized.transitions[1].same_tick_order,
            SameTickOrderEvidence::ObservedPacket(1)
        ));
        assert!(!normalized.transitions[1].changed);
        assert!(
            normalized
                .transitions
                .iter()
                .all(|item| item.scheduler_phase.is_none())
        );
    }

    #[test]
    fn vanilla_artifact_projection_preserves_internal_causal_evidence() {
        let artifact = crate::vanilla_instrumentation::parse_and_validate_instrumentation(
            include_str!("../tests/fixtures/vanilla_1_21_11_offline_piston_input.json"),
        )
        .unwrap();
        let normalized = normalize_vanilla_instrumentation_artifact(&artifact);
        assert_eq!(normalized.transitions.len(), artifact.state_events.len());
        assert!(normalized.complete);
        assert!(
            normalized
                .unavailable_reasons
                .iter()
                .any(|reason| { reason.contains("OrderedTick evidence is incomplete") })
        );
        assert!(
            normalized
                .transitions
                .iter()
                .all(|transition| transition.evidence == TransitionEvidence::ObservedInternal)
        );
    }

    #[test]
    fn neighbor_update_evidence_is_projected_and_model_gap_is_unavailable() {
        let mut artifact = crate::vanilla_instrumentation::parse_and_validate_instrumentation(
            include_str!("../tests/fixtures/vanilla_1_21_11_offline_piston_input.json"),
        )
        .unwrap();
        artifact.neighbor_updates.clear();
        artifact
            .neighbor_updates
            .push(crate::vanilla_instrumentation::NeighborUpdateObservation {
                sequence: 1,
                game_tick: 1,
                sub_tick_order: 0,
                position: Pos::new(2, 0, 0),
                target: crate::vanilla_instrumentation::InstrumentedBlockState {
                    name: "minecraft:repeater".into(),
                    properties: BTreeMap::new(),
                },
                source_block: "minecraft:lever".into(),
                orientation: None,
                notify: false,
                ordered_tick_sequence: None,
            });
        let observed = normalize_vanilla_instrumentation_artifact(&artifact);
        assert_eq!(observed.neighbor_updates.len(), 1);
        assert_eq!(observed.neighbor_updates[0].relative_game_tick, 1);
        let modelled = trace(transition(TransitionEvidence::ModelledScheduler));
        let result = compare_transition_traces(&observed, &modelled);
        assert!(result.issues.iter().any(|issue| {
            issue.field == ConformanceField::NeighborUpdate
                && issue.status == ConformanceStatus::Unavailable
        }));
    }

    #[test]
    fn internal_causal_groups_are_compared_relationally_without_id_equality() {
        let mut first = transition(TransitionEvidence::ObservedInternal);
        first.scheduler_cause_sequence = Some(10);
        let mut second = first.clone();
        second.position = Pos::new(2, 2, 3);
        second.scheduler_cause_sequence = Some(10);
        let observed = NormalizedTransitionTrace {
            transitions: vec![first, second],
            neighbor_updates: Vec::new(),
            piston_states: Vec::new(),
            complete: true,
            unavailable_reasons: Vec::new(),
        };

        let mut model_first = transition(TransitionEvidence::ModelledScheduler);
        model_first.scheduler_cause_sequence = Some(100);
        let mut model_second = model_first.clone();
        model_second.position = Pos::new(2, 2, 3);
        model_second.scheduler_cause_sequence = Some(101);
        let modelled = NormalizedTransitionTrace {
            transitions: vec![model_first.clone(), model_second],
            neighbor_updates: Vec::new(),
            piston_states: Vec::new(),
            complete: true,
            unavailable_reasons: Vec::new(),
        };
        let result = compare_transition_traces(&observed, &modelled);
        assert_eq!(result.status, ConformanceStatus::Mismatch);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.field == ConformanceField::SchedulerCause)
        );

        model_first.scheduler_cause_sequence = Some(999);
        let mut model_same = model_first.clone();
        model_same.position = Pos::new(2, 2, 3);
        model_same.scheduler_cause_sequence = Some(999);
        let modelled = NormalizedTransitionTrace {
            transitions: vec![model_first, model_same],
            neighbor_updates: Vec::new(),
            piston_states: Vec::new(),
            complete: true,
            unavailable_reasons: Vec::new(),
        };
        let result = compare_transition_traces(&observed, &modelled);
        assert!(
            !result
                .issues
                .iter()
                .any(|issue| issue.field == ConformanceField::SchedulerCause)
        );
    }
}
