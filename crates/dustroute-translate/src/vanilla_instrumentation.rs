//! Contract for evidence captured inside a version-pinned Java server.
//!
//! Mineflayer packet order is intentionally not accepted by this module as a
//! substitute for an internal scheduler observation.  The artifact described
//! here must be produced by a server-side Fabric/Mixin, Java agent, or an
//! equivalent instrumentation layer.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::Pos;

pub const VANILLA_INSTRUMENTATION_SCHEMA: &str = "dustroute.vanilla-instrumentation.v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentationMethod {
    FabricMixin,
    JavaAgent,
    ServerPatch,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentationEvidence {
    ObservedInternal,
    ContractExample,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VanillaInstrumentationMetadata {
    pub method: InstrumentationMethod,
    pub mapping_namespace: String,
    pub build_id: String,
    pub target_class: String,
    pub target_member: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstrumentationClock {
    pub unit: String,
    pub activation_origin: String,
    pub absolute_ticks_omitted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InputTimingObservation {
    pub kind: String,
    pub position: Pos,
    pub activation_game_tick: u64,
    pub first_redstone_change_game_tick: Option<u64>,
    pub first_packet_update_game_tick: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstrumentedBlockState {
    pub name: String,
    #[serde(default)]
    pub properties: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrderedTickObservation {
    pub sequence: u64,
    /// Relative ticks may be negative when a bounded capture retains its
    /// pre-roll before server input.
    pub trigger_game_tick: i64,
    pub execution_game_tick: i64,
    pub priority: String,
    /// Vanilla exposes `OrderedTick.subTickOrder` as a signed Java `long`.
    /// Negative values and resets within one execution tick are valid runtime
    /// observations, so this remains evidence rather than a monotonic clock.
    pub sub_tick_order: i64,
    pub position: Pos,
    pub block_name: String,
    pub event_kind: String,
    /// `block` and `fluid` schedulers have independent sub-tick streams.
    /// `None` keeps compatibility with older single-stream artifacts.
    #[serde(default)]
    pub scheduler: Option<String>,
    /// Internal phase is optional because a server hook may expose the
    /// ordered tick without exposing a stable phase label.
    pub phase: Option<String>,
    pub phase_evidence: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PistonStateKind {
    Stable,
    Moving,
    Completion,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonStateObservation {
    pub sequence: u64,
    /// Relative ticks may be negative for retained pre-roll observations.
    pub game_tick: i64,
    pub position: Pos,
    pub state_kind: PistonStateKind,
    pub body: InstrumentedBlockState,
    pub head: Option<InstrumentedBlockState>,
    pub moving_block: Option<InstrumentedBlockState>,
    pub block_entity_present: bool,
    pub block_entity_extending: Option<bool>,
    /// Normalized sequence of the server OrderedTick callback that was active
    /// when this observation was emitted, when the hook could establish one.
    /// `None` is intentional for observations outside an OrderedTick callback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordered_tick_sequence: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstrumentedStateEvent {
    pub sequence: u64,
    /// Relative ticks may be negative for retained pre-roll observations.
    pub game_tick: i64,
    pub sub_tick_order: u64,
    pub position: Pos,
    pub source: String,
    pub before: InstrumentedBlockState,
    pub after: InstrumentedBlockState,
    pub changed: bool,
    /// Normalized sequence of the server OrderedTick callback that was active
    /// when this state mutation was emitted, when the hook could establish one.
    /// This is a causal reference, not a replacement for same-tick order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordered_tick_sequence: Option<u64>,
}

/// A server-side callback to one observed block during Vanilla's chained
/// neighbor-update propagation. The sequence is local to this evidence family;
/// it preserves callback order without pretending to be a scheduler ID.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NeighborUpdateObservation {
    pub sequence: u64,
    /// Relative ticks may be negative for retained pre-roll observations.
    pub game_tick: i64,
    pub sub_tick_order: u64,
    pub position: Pos,
    pub target: InstrumentedBlockState,
    pub source_block: String,
    pub orientation: Option<String>,
    pub notify: bool,
    /// Normalized sequence of the OrderedTick callback active at the callback,
    /// when one exists in the promoted capture window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordered_tick_sequence: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstrumentationCompleteness {
    pub ordered_ticks: bool,
    pub input_timing: bool,
    pub state_events: bool,
    pub piston_state: bool,
    /// True only when neighbor callback evidence is present for the promoted
    /// capture scope; filtered or missing callbacks remain unavailable.
    #[serde(default)]
    pub neighbor_updates: bool,
}

/// Completeness of one evidence stream within the declared capture scope.
/// `partial` is intentional for filtered hooks or bounded captures; it must
/// never be treated as proof that an event did not occur.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamCompleteness {
    Complete,
    Partial,
    #[default]
    Unavailable,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstrumentationCapture {
    pub mode: String,
    pub heartbeat_mode: String,
    pub capture_state: String,
    #[serde(default)]
    pub pre_roll_ticks: Option<u64>,
    #[serde(default)]
    pub max_ticks_after_input: Option<u64>,
    #[serde(default)]
    pub drain_ticks: Option<u64>,
    pub input_observed: bool,
    pub artifact_start_present: bool,
    pub artifact_end_present: bool,
    pub sequence_contiguous: bool,
    pub sequence_gap_count: u64,
    pub last_sequence: u64,
    pub suppressed_records: u64,
    pub evicted_records: u64,
    pub write_errors: u64,
    #[serde(default)]
    pub stream_completeness: BTreeMap<String, StreamCompleteness>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VanillaInstrumentationArtifact {
    pub schema_version: String,
    pub minecraft_version: String,
    pub evidence: InstrumentationEvidence,
    pub source: String,
    pub scenario: String,
    pub source_artifact: String,
    pub instrumentation: VanillaInstrumentationMetadata,
    pub clock: InstrumentationClock,
    pub input: InputTimingObservation,
    #[serde(default)]
    pub capture: Option<InstrumentationCapture>,
    #[serde(default)]
    pub ordered_ticks: Vec<OrderedTickObservation>,
    #[serde(default)]
    pub state_events: Vec<InstrumentedStateEvent>,
    #[serde(default)]
    pub piston_states: Vec<PistonStateObservation>,
    #[serde(default)]
    pub neighbor_updates: Vec<NeighborUpdateObservation>,
    pub completeness: InstrumentationCompleteness,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstrumentationValidationError {
    InvalidField {
        field: &'static str,
        reason: String,
    },
    Sequence {
        field: &'static str,
        expected: u64,
        actual: u64,
    },
}

impl Display for InstrumentationValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidField { field, reason } => write!(formatter, "{field}: {reason}"),
            Self::Sequence {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "{field}: expected sequence {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for InstrumentationValidationError {}

impl VanillaInstrumentationArtifact {
    fn ordered_ticks_complete_for_links(&self) -> bool {
        self.completeness.ordered_ticks
            && self.capture.as_ref().is_none_or(|capture| {
                capture
                    .stream_completeness
                    .get("ordered_tick")
                    .is_none_or(|status| *status == StreamCompleteness::Complete)
            })
    }

    pub fn validate(&self) -> Result<(), InstrumentationValidationError> {
        if self.schema_version != VANILLA_INSTRUMENTATION_SCHEMA {
            return Err(invalid(
                "schema_version",
                format!("expected {VANILLA_INSTRUMENTATION_SCHEMA}"),
            ));
        }
        if self.minecraft_version != "1.21.11" {
            return Err(invalid("minecraft_version", "must be 1.21.11"));
        }
        if self.evidence != InstrumentationEvidence::ObservedInternal {
            return Err(invalid(
                "evidence",
                "instrumentation inputs must be observed_internal",
            ));
        }
        if self.source != "vanilla_server_instrumentation" {
            return Err(invalid(
                "source",
                "must identify a server-side instrumentation hook",
            ));
        }
        for (field, value) in [
            ("scenario", self.scenario.as_str()),
            ("source_artifact", self.source_artifact.as_str()),
            (
                "instrumentation.mapping_namespace",
                self.instrumentation.mapping_namespace.as_str(),
            ),
            (
                "instrumentation.build_id",
                self.instrumentation.build_id.as_str(),
            ),
            (
                "instrumentation.target_class",
                self.instrumentation.target_class.as_str(),
            ),
            (
                "instrumentation.target_member",
                self.instrumentation.target_member.as_str(),
            ),
        ] {
            if value.is_empty() {
                return Err(invalid(field, "must not be empty"));
            }
        }
        if self.clock.unit != "game_tick" {
            return Err(invalid("clock.unit", "must be game_tick"));
        }
        if self.clock.activation_origin != "server_input_received" {
            return Err(invalid(
                "clock.activation_origin",
                "must identify server input receipt",
            ));
        }
        if !self.clock.absolute_ticks_omitted {
            return Err(invalid(
                "clock.absolute_ticks_omitted",
                "absolute server ticks must not become fixture identity",
            ));
        }
        if self.input.kind.is_empty() {
            return Err(invalid("input.kind", "must not be empty"));
        }
        if let Some(capture) = &self.capture {
            validate_capture(capture, &self.input)?;
        }
        if self.completeness.ordered_ticks && self.ordered_ticks.is_empty() {
            return Err(invalid(
                "ordered_ticks",
                "must be non-empty when ordered_ticks is complete",
            ));
        }
        validate_ordered_ticks(&self.ordered_ticks)?;
        validate_causal_links(
            &self.ordered_ticks,
            &self.state_events,
            &self.piston_states,
            &self.neighbor_updates,
            self.ordered_ticks_complete_for_links(),
        )?;
        validate_state_events(&self.state_events)?;
        validate_piston_states(&self.piston_states)?;
        validate_neighbor_updates(&self.neighbor_updates)?;
        if self.completeness.neighbor_updates && self.neighbor_updates.is_empty() {
            return Err(invalid(
                "neighbor_updates",
                "must be non-empty when neighbor_updates is complete",
            ));
        }
        if self.completeness.piston_state && self.piston_states.is_empty() {
            return Err(invalid(
                "piston_states",
                "must be non-empty when piston_state is complete",
            ));
        }
        if self.input.activation_game_tick
            > self
                .input
                .first_redstone_change_game_tick
                .unwrap_or(u64::MAX)
            || self.input.activation_game_tick
                > self.input.first_packet_update_game_tick.unwrap_or(u64::MAX)
        {
            return Err(invalid(
                "input",
                "observed downstream events cannot precede server input receipt",
            ));
        }
        if self.completeness.input_timing && self.input.first_redstone_change_game_tick.is_none() {
            return Err(invalid(
                "input.first_redstone_change_game_tick",
                "required when input_timing is complete",
            ));
        }
        Ok(())
    }
}

fn validate_capture(
    capture: &InstrumentationCapture,
    input: &InputTimingObservation,
) -> Result<(), InstrumentationValidationError> {
    if capture.mode != "continuous" && capture.mode != "bounded" {
        return Err(invalid("capture.mode", "must be continuous or bounded"));
    }
    if capture.heartbeat_mode != "full" && capture.heartbeat_mode != "omitted" {
        return Err(invalid("capture.heartbeat_mode", "must be full or omitted"));
    }
    if capture.capture_state.is_empty() {
        return Err(invalid("capture.capture_state", "must not be empty"));
    }
    if !matches!(
        capture.capture_state.as_str(),
        "continuous"
            | "armed_no_input"
            | "capturing"
            | "closed"
            | "closed_no_input"
            | "closed_early"
    ) {
        return Err(invalid(
            "capture.capture_state",
            "is not a recognized capture lifecycle state",
        ));
    }
    if !capture.artifact_start_present || !capture.artifact_end_present {
        return Err(invalid(
            "capture.artifact_boundaries",
            "both artifact_start and artifact_end are required",
        ));
    }
    if capture.sequence_contiguous != (capture.sequence_gap_count == 0) {
        return Err(invalid(
            "capture.sequence_gap_count",
            "sequence_contiguous must agree with sequence_gap_count",
        ));
    }
    if capture.stream_completeness.values().any(|status| {
        *status == StreamCompleteness::Complete
            && (!capture.sequence_contiguous
                || capture.write_errors != 0
                || capture.evicted_records != 0
                || capture.suppressed_records != 0)
    }) {
        return Err(invalid(
            "capture.stream_completeness",
            "a stream with a gap, write error, or eviction cannot be complete",
        ));
    }
    if capture.input_observed == (input.kind == "input_unavailable") {
        return Err(invalid(
            "capture.input_observed",
            "must agree with input.kind",
        ));
    }
    if capture.mode == "bounded"
        && capture.input_observed
        && capture.max_ticks_after_input.is_none()
    {
        return Err(invalid(
            "capture.max_ticks_after_input",
            "bounded captures with input require a post-input bound",
        ));
    }
    Ok(())
}

pub fn parse_and_validate_instrumentation(
    source: &str,
) -> Result<VanillaInstrumentationArtifact, String> {
    let artifact: VanillaInstrumentationArtifact = serde_json::from_str(source)
        .map_err(|error| format!("instrumentation artifact JSON is invalid: {error}"))?;
    artifact
        .validate()
        .map_err(|error| format!("instrumentation artifact contract is invalid: {error}"))?;
    Ok(artifact)
}

fn validate_ordered_ticks(
    observations: &[OrderedTickObservation],
) -> Result<(), InstrumentationValidationError> {
    let mut previous = BTreeMap::<String, i64>::new();
    for (index, observation) in observations.iter().enumerate() {
        let expected = index as u64 + 1;
        if observation.sequence != expected {
            return Err(InstrumentationValidationError::Sequence {
                field: "ordered_ticks",
                expected,
                actual: observation.sequence,
            });
        }
        if observation.trigger_game_tick > observation.execution_game_tick {
            return Err(invalid(
                "ordered_ticks.trigger_game_tick",
                "cannot be after execution_game_tick",
            ));
        }
        if observation.block_name.is_empty()
            || observation.event_kind.is_empty()
            || observation.priority.is_empty()
        {
            return Err(invalid(
                "ordered_ticks",
                "block_name and event_kind must not be empty",
            ));
        }
        if observation.phase.is_some() && observation.phase_evidence != "observed_internal" {
            return Err(invalid(
                "ordered_ticks.phase_evidence",
                "a claimed phase requires observed_internal evidence",
            ));
        }
        if observation.phase.is_none() && observation.phase_evidence != "unknown" {
            return Err(invalid(
                "ordered_ticks.phase_evidence",
                "an absent phase must remain unknown",
            ));
        }
        let scheduler = observation
            .scheduler
            .as_deref()
            .unwrap_or("unknown")
            .to_owned();
        if let Some(tick) = previous.get(&scheduler).copied() {
            if observation.execution_game_tick < tick {
                return Err(invalid(
                    "ordered_ticks",
                    "execution tick must not move backwards within a scheduler stream",
                ));
            }
        }
        previous.insert(scheduler, observation.execution_game_tick);
    }
    Ok(())
}

fn validate_state_events(
    events: &[InstrumentedStateEvent],
) -> Result<(), InstrumentationValidationError> {
    for (index, event) in events.iter().enumerate() {
        let expected = index as u64 + 1;
        if event.sequence != expected {
            return Err(InstrumentationValidationError::Sequence {
                field: "state_events",
                expected,
                actual: event.sequence,
            });
        }
        if event.source.is_empty() || event.before.name.is_empty() || event.after.name.is_empty() {
            return Err(invalid(
                "state_events",
                "source and state names must not be empty",
            ));
        }
        if event.changed == (event.before == event.after) {
            return Err(invalid(
                "state_events.changed",
                "must agree with before/after state equality",
            ));
        }
    }
    Ok(())
}

fn validate_causal_links(
    ordered_ticks: &[OrderedTickObservation],
    state_events: &[InstrumentedStateEvent],
    piston_states: &[PistonStateObservation],
    neighbor_updates: &[NeighborUpdateObservation],
    ordered_ticks_complete: bool,
) -> Result<(), InstrumentationValidationError> {
    let known_sequences: std::collections::BTreeSet<_> = ordered_ticks
        .iter()
        .map(|observation| observation.sequence)
        .collect();

    for (index, sequence) in state_events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            event
                .ordered_tick_sequence
                .map(|sequence| (index, sequence))
        })
    {
        validate_causal_sequence(
            sequence,
            "state_events.ordered_tick_sequence",
            index,
            &known_sequences,
            ordered_ticks_complete,
        )?;
    }
    for (index, sequence) in piston_states
        .iter()
        .enumerate()
        .filter_map(|(index, observation)| {
            observation
                .ordered_tick_sequence
                .map(|sequence| (index, sequence))
        })
    {
        validate_causal_sequence(
            sequence,
            "piston_states.ordered_tick_sequence",
            index,
            &known_sequences,
            ordered_ticks_complete,
        )?;
    }
    for (index, sequence) in
        neighbor_updates
            .iter()
            .enumerate()
            .filter_map(|(index, observation)| {
                observation
                    .ordered_tick_sequence
                    .map(|sequence| (index, sequence))
            })
    {
        validate_causal_sequence(
            sequence,
            "neighbor_updates.ordered_tick_sequence",
            index,
            &known_sequences,
            ordered_ticks_complete,
        )?;
    }
    Ok(())
}

fn validate_causal_sequence(
    sequence: u64,
    field: &'static str,
    index: usize,
    known_sequences: &std::collections::BTreeSet<u64>,
    ordered_ticks_complete: bool,
) -> Result<(), InstrumentationValidationError> {
    if sequence == 0 {
        return Err(invalid(
            field,
            format!("entry {index} must reference sequence >= 1"),
        ));
    }
    if ordered_ticks_complete && !known_sequences.contains(&sequence) {
        return Err(invalid(
            field,
            format!("entry {index} references an OrderedTick not present in the artifact"),
        ));
    }
    Ok(())
}

fn validate_piston_states(
    observations: &[PistonStateObservation],
) -> Result<(), InstrumentationValidationError> {
    for (index, observation) in observations.iter().enumerate() {
        let expected = index as u64 + 1;
        if observation.sequence != expected {
            return Err(InstrumentationValidationError::Sequence {
                field: "piston_states",
                expected,
                actual: observation.sequence,
            });
        }
        let body_name = short_name(&observation.body.name);
        let body_is_valid = match observation.state_kind {
            PistonStateKind::Stable => body_name == "piston",
            PistonStateKind::Moving | PistonStateKind::Completion => {
                body_name == "piston" || body_name == "moving_piston"
            }
        };
        if !body_is_valid
            || (observation.state_kind == PistonStateKind::Stable && observation.head.is_none())
        {
            return Err(invalid(
                "piston_states",
                "stable observations require piston + head; moving observations may use moving_piston",
            ));
        }
        if observation.state_kind == PistonStateKind::Moving && !observation.block_entity_present {
            return Err(invalid(
                "piston_states.block_entity_present",
                "moving observations require a PistonBlockEntity",
            ));
        }
    }
    Ok(())
}

fn validate_neighbor_updates(
    observations: &[NeighborUpdateObservation],
) -> Result<(), InstrumentationValidationError> {
    let mut previous_tick = None;
    let mut previous_sub_tick = 0;
    for (index, observation) in observations.iter().enumerate() {
        let expected = index as u64 + 1;
        if observation.sequence != expected {
            return Err(InstrumentationValidationError::Sequence {
                field: "neighbor_updates",
                expected,
                actual: observation.sequence,
            });
        }
        if observation.target.name.is_empty() || observation.source_block.is_empty() {
            return Err(invalid(
                "neighbor_updates",
                "target name and source_block must not be empty",
            ));
        }
        if previous_tick == Some(observation.game_tick) {
            if observation.sub_tick_order < previous_sub_tick {
                return Err(invalid(
                    "neighbor_updates.sub_tick_order",
                    "same-tick callback order must not move backwards",
                ));
            }
        } else {
            previous_tick = Some(observation.game_tick);
        }
        previous_sub_tick = observation.sub_tick_order;
    }
    Ok(())
}

fn invalid(field: &'static str, reason: impl Into<String>) -> InstrumentationValidationError {
    InstrumentationValidationError::InvalidField {
        field,
        reason: reason.into(),
    }
}

fn short_name(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact() -> VanillaInstrumentationArtifact {
        VanillaInstrumentationArtifact {
            schema_version: VANILLA_INSTRUMENTATION_SCHEMA.into(),
            minecraft_version: "1.21.11".into(),
            evidence: InstrumentationEvidence::ObservedInternal,
            source: "vanilla_server_instrumentation".into(),
            scenario: "piston_motion_trace".into(),
            source_artifact: ".local/instrumentation/piston.json".into(),
            instrumentation: VanillaInstrumentationMetadata {
                method: InstrumentationMethod::FabricMixin,
                mapping_namespace: "intermediary".into(),
                build_id: "test-hook".into(),
                target_class: "ServerLevel".into(),
                target_member: "tickBlockEntities".into(),
            },
            clock: InstrumentationClock {
                unit: "game_tick".into(),
                activation_origin: "server_input_received".into(),
                absolute_ticks_omitted: true,
            },
            input: InputTimingObservation {
                kind: "lever".into(),
                position: Pos::new(0, 0, 0),
                activation_game_tick: 0,
                first_redstone_change_game_tick: Some(1),
                first_packet_update_game_tick: Some(1),
            },
            capture: None,
            ordered_ticks: vec![OrderedTickObservation {
                sequence: 1,
                trigger_game_tick: 0,
                execution_game_tick: 1,
                priority: "NORMAL".into(),
                sub_tick_order: 0,
                position: Pos::new(1, 0, 0),
                block_name: "minecraft:piston".into(),
                event_kind: "block_event".into(),
                scheduler: None,
                phase: None,
                phase_evidence: "unknown".into(),
            }],
            state_events: vec![],
            piston_states: vec![PistonStateObservation {
                sequence: 1,
                game_tick: 2,
                position: Pos::new(1, 0, 0),
                state_kind: PistonStateKind::Stable,
                body: InstrumentedBlockState {
                    name: "minecraft:piston".into(),
                    properties: BTreeMap::new(),
                },
                head: Some(InstrumentedBlockState {
                    name: "minecraft:piston_head".into(),
                    properties: BTreeMap::new(),
                }),
                moving_block: None,
                block_entity_present: false,
                block_entity_extending: None,
                ordered_tick_sequence: None,
            }],
            neighbor_updates: vec![],
            completeness: InstrumentationCompleteness {
                ordered_ticks: true,
                input_timing: true,
                state_events: false,
                piston_state: true,
                neighbor_updates: false,
            },
            notes: vec!["server-side hook example".into()],
        }
    }

    fn complete_capture() -> InstrumentationCapture {
        InstrumentationCapture {
            mode: "continuous".into(),
            heartbeat_mode: "full".into(),
            capture_state: "closed".into(),
            pre_roll_ticks: None,
            max_ticks_after_input: None,
            drain_ticks: None,
            input_observed: true,
            artifact_start_present: true,
            artifact_end_present: true,
            sequence_contiguous: true,
            sequence_gap_count: 0,
            last_sequence: 1,
            suppressed_records: 0,
            evicted_records: 0,
            write_errors: 0,
            stream_completeness: BTreeMap::from([
                ("ordered_tick".into(), StreamCompleteness::Complete),
                ("block_state_change".into(), StreamCompleteness::Complete),
            ]),
        }
    }

    #[test]
    fn validates_internal_instrumentation_contract() {
        assert!(artifact().validate().is_ok());
    }

    #[test]
    fn parser_round_trips_a_reviewed_artifact() {
        let encoded = serde_json::to_string(&artifact()).unwrap();
        let parsed = parse_and_validate_instrumentation(&encoded).unwrap();
        assert_eq!(parsed.scenario, "piston_motion_trace");
        assert_eq!(parsed.ordered_ticks.len(), 1);
    }

    #[test]
    fn rejects_packet_only_evidence_and_unclaimed_phase() {
        let mut value = artifact();
        value.evidence = InstrumentationEvidence::ContractExample;
        assert!(value.validate().is_err());
        let mut value = artifact();
        value.ordered_ticks[0].phase = Some("scheduled_tick".into());
        value.ordered_ticks[0].phase_evidence = "unknown".into();
        assert!(value.validate().is_err());
    }

    #[test]
    fn rejects_unordered_ticks_and_missing_stable_head() {
        let mut value = artifact();
        value.ordered_ticks.push(OrderedTickObservation {
            sequence: 2,
            trigger_game_tick: 0,
            execution_game_tick: 0,
            priority: "NORMAL".into(),
            sub_tick_order: -1,
            position: Pos::new(2, 0, 0),
            block_name: "minecraft:piston".into(),
            event_kind: "block_event".into(),
            scheduler: None,
            phase: None,
            phase_evidence: "unknown".into(),
        });
        assert!(value.validate().is_err());
        let mut value = artifact();
        value.piston_states[0].head = None;
        assert!(value.validate().is_err());
    }

    #[test]
    fn preserves_signed_vanilla_sub_tick_order() {
        let mut value = artifact();
        value.ordered_ticks[0].sub_tick_order = -2;
        assert!(value.validate().is_ok());
    }

    #[test]
    fn accepts_signed_relative_pre_roll_game_ticks() {
        let mut value = artifact();
        value.ordered_ticks[0].trigger_game_tick = -2;
        value.ordered_ticks[0].execution_game_tick = -1;
        value.piston_states[0].game_tick = -1;
        assert!(value.validate().is_ok());
        let encoded = serde_json::to_string(&value).unwrap();
        let parsed = parse_and_validate_instrumentation(&encoded).unwrap();
        assert_eq!(parsed.ordered_ticks[0].trigger_game_tick, -2);
    }

    #[test]
    fn validates_neighbor_update_order_and_causal_reference() {
        let mut value = artifact();
        value.neighbor_updates.push(NeighborUpdateObservation {
            sequence: 1,
            game_tick: 1,
            sub_tick_order: 0,
            position: Pos::new(2, 0, 0),
            target: InstrumentedBlockState {
                name: "minecraft:repeater".into(),
                properties: BTreeMap::new(),
            },
            source_block: "minecraft:lever".into(),
            orientation: None,
            notify: false,
            ordered_tick_sequence: Some(1),
        });
        value.completeness.neighbor_updates = true;
        assert!(value.validate().is_ok());

        value.neighbor_updates[0].ordered_tick_sequence = Some(0);
        assert!(value.validate().is_err());
    }

    #[test]
    fn allows_signed_resets_and_equal_vanilla_sub_tick_orders() {
        let mut value = artifact();
        value.ordered_ticks.push(OrderedTickObservation {
            sequence: 2,
            trigger_game_tick: 0,
            execution_game_tick: 1,
            priority: "NORMAL".into(),
            sub_tick_order: -385,
            position: Pos::new(2, 0, 0),
            block_name: "minecraft:stone".into(),
            event_kind: "scheduled_tick".into(),
            scheduler: None,
            phase: None,
            phase_evidence: "unknown".into(),
        });
        assert!(value.validate().is_ok());
    }

    #[test]
    fn accepts_state_cause_link_to_retained_ordered_tick() {
        let mut value = artifact();
        value.piston_states[0].ordered_tick_sequence = Some(1);
        assert!(value.validate().is_ok());
    }

    #[test]
    fn rejects_cause_link_to_missing_ordered_tick_when_complete() {
        let mut value = artifact();
        value.piston_states[0].ordered_tick_sequence = Some(2);
        assert!(value.validate().is_err());
    }

    #[test]
    fn allows_unresolved_cause_link_when_ordered_ticks_are_incomplete() {
        let mut value = artifact();
        value.completeness.ordered_ticks = false;
        value.piston_states[0].ordered_tick_sequence = Some(999);
        assert!(value.validate().is_ok());
    }

    #[test]
    fn validates_capture_integrity_metadata() {
        let mut value = artifact();
        value.capture = Some(complete_capture());
        assert!(value.validate().is_ok());
    }

    #[test]
    fn rejects_complete_stream_with_a_declared_gap() {
        let mut value = artifact();
        let mut capture = complete_capture();
        capture.sequence_contiguous = false;
        capture.sequence_gap_count = 1;
        value.capture = Some(capture);
        assert!(value.validate().is_err());
    }

    #[test]
    fn rejects_complete_stream_with_suppressed_records() {
        let mut value = artifact();
        let mut capture = complete_capture();
        capture.suppressed_records = 1;
        value.capture = Some(capture);
        assert!(value.validate().is_err());
    }

    #[test]
    fn rejects_capture_input_disagreement() {
        let mut value = artifact();
        let mut capture = complete_capture();
        capture.input_observed = false;
        value.capture = Some(capture);
        assert!(value.validate().is_err());
    }

    #[test]
    fn rejects_unknown_capture_lifecycle_state() {
        let mut value = artifact();
        let mut capture = complete_capture();
        capture.capture_state = "finished_maybe".into();
        value.capture = Some(capture);
        assert!(value.validate().is_err());
    }
}
