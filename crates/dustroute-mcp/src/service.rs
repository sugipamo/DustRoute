use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use dustroute_app::DustRouteService;
use dustroute_optimize::{
    AnchorPolicy, BehavioralVerificationConfig, CompressionAxis, CompressionDirection,
    OptimizationPlan, OptimizationRoutingConfig, OptimizationSafety, TemporalCapabilities,
    assess_optimization_safety, realize_staged_optimization, verify_realized_optimization,
};
use dustroute_physical::{BlockKind, Pos};
use dustroute_physical::{PhysicalBlockChange, PhysicalPatch};
use dustroute_translate::{
    ForwardOptions, JavaExportConfig, ReverseRequest, java_block_state, world_from_snapshot_json,
};
use rmcp::{
    ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{GetPromptResult, Implementation, PromptMessage, Role, ServerCapabilities, ServerInfo},
    prompt, prompt_handler, prompt_router, schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::McpConfig;
use crate::api::{
    DIAGNOSTIC_SCHEMA_V1, ErrorResponse, McpErrorCode, PLACEMENT_SCHEMA_V1, REPAIR_SCHEMA_V1,
    TRANSITION_SCHEMA_V1, TransitionTraceResponse,
};
use crate::state::PlanStateStore;
use crate::{
    BlockChange, BotBridge, McpPolicy, OperationKind, OperationRegistry, OperationStatus,
    PlacementPlan, SelectionSession, TransitionSafety, TransitionSafetyAssessment,
    assess_transition_safety, behavior_trace_from_recording, discover_connected_region,
    plan_world_overlay, scenario_trace_from_recording_with_initial,
};

const MAX_FLAT_ANALYSIS_COMPONENTS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolProfile {
    Default,
    Debug,
}

impl ToolProfile {
    fn from_environment() -> Self {
        match std::env::var("DUSTROUTE_MCP_TOOL_PROFILE").as_deref() {
            Ok("debug") => Self::Debug,
            _ => Self::Default,
        }
    }
}

const DEBUG_ONLY_TOOLS: [&str; 7] = [
    "get_visible_player",
    "get_player_gaze",
    "resolve_looked_at_circuit",
    "get_circuit_placement",
    "new_component_removal_plan",
    "start_selected_region_conversion",
    "stop_operation",
];

#[derive(Clone)]
pub struct DustRouteMcp {
    bridge: BotBridge,
    selections: Arc<Mutex<HashMap<String, SelectionSession>>>,
    selection_dimensions: Arc<Mutex<HashMap<String, String>>>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
    plans: Arc<Mutex<HashMap<uuid::Uuid, PlacementPlan>>>,
    plan_dimensions: Arc<Mutex<HashMap<uuid::Uuid, String>>>,
    applied_plans: Arc<Mutex<HashMap<uuid::Uuid, bool>>>,
    repair_plans: Arc<Mutex<HashMap<uuid::Uuid, StoredRepairPlan>>>,
    transition_plans: Arc<Mutex<HashMap<uuid::Uuid, StoredTransitionPlan>>>,
    state_store: PlanStateStore,
    policy: McpPolicy,
    app: DustRouteService,
    operations: OperationRegistry,
    mutation_lock: Arc<Mutex<()>>,
    assist_player: Option<String>,
    server_address: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PlayerParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ObserveParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Ray-cast limit in blocks. Defaults to 64.
    max_distance: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct InspectLookedAtWorldParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Maximum redstone components followed from the gaze target, from 1 through 32768. Defaults to 8192.
    max_components: Option<usize>,
    /// Maximum Manhattan gap followed between nearby components. Defaults to 2 so a one-block break remains visible.
    component_gap: Option<u32>,
    /// Ray-cast limit in blocks. Defaults to 64.
    max_distance: Option<f64>,
    /// Include a raw non-air block list in addition to the redstone list. Defaults to false.
    include_block_list: Option<bool>,
    /// Maximum entries returned in each block list, from 1 through 2048. Defaults to 256.
    max_listed_blocks: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MarkCornerParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Either `first` or `second`.
    corner: String,
    /// Ray-cast limit in blocks. Defaults to 64.
    max_distance: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DiscoverCircuitParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Maximum redstone components followed from the gaze target. Defaults to 8192.
    max_components: Option<usize>,
    /// Extra blocks around the discovered circuit. Defaults to 1.
    padding: Option<i32>,
    /// Maximum Manhattan distance used to discover a nearby disconnected fragment. Defaults to 2.
    fragment_gap: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AnalyzeLookedAtParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Maximum redstone components followed from the gaze target. Defaults to 8192.
    max_components: Option<usize>,
    /// Maximum Manhattan gap considered for broken connections. Defaults to 2.
    fragment_gap: Option<u32>,
    /// Explicitly enumerate a truth table for a small circuit. Defaults to false.
    include_truth_table: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DiagnoseLookedAtParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Maximum redstone components followed from the gaze target. Defaults to 8192.
    max_components: Option<usize>,
    /// Maximum Manhattan gap considered when discovering broken fragments. Defaults to 2.
    fragment_gap: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PreviewPlacementParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Built-in circuit: half-adder, half-subtractor, mux2, decoder1to2, or full-adder.
    circuit: String,
    /// Maximum number of blocks allowed in one placement plan. Defaults to 32768.
    max_blocks: Option<usize>,
    /// Run directional compression followed by global compaction before creating the placement plan.
    optimize: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct OperationParams {
    /// Operation UUID returned by a start or preview tool.
    operation_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConfirmedOperationParams {
    /// Operation UUID returned by new_circuit_placement.
    operation_id: String,
    /// Must be true to acknowledge that this call changes the test world.
    confirm: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredRepairPlan {
    patch: PhysicalPatch,
    dimension: String,
    analysis_bounds: dustroute_translate::RegionBounds,
    fragments_before: usize,
    baseline_truth_table: Option<dustroute_translate::InferredTruthTable>,
    previewed: bool,
    applied: bool,
}

#[derive(Clone, Debug)]
struct StoredTransitionPlan {
    player: String,
    dimension: String,
    bounds: dustroute_translate::RegionBounds,
    lever: Pos,
    original_powered: bool,
    initial_snapshot: dustroute_translate::MinecraftSnapshot,
    observation_ticks: u16,
    max_events: usize,
    safety: TransitionSafetyAssessment,
    previewed: bool,
    executed: bool,
    restoration_verified: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ProposeTransitionParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Maximum redstone components followed from the gaze target. Defaults to 8192.
    max_components: Option<usize>,
    /// Number of game ticks observed after activation, from 1 through 200. Defaults to 20.
    observation_ticks: Option<u16>,
    /// Maximum block update events recorded, from 1 through 65536. Defaults to 16384.
    max_events: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PreviewTransitionParams {
    operation_id: String,
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RunTransitionParams {
    operation_id: String,
    /// Must be true because this normally activates a lever in the test world.
    confirm: bool,
    /// Optional output contracts used to distinguish candidates, confirmed hazards, and intended pulses.
    contracts: Option<Vec<TransitionContractParam>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TransitionContractParam {
    x: i32,
    y: i32,
    z: i32,
    /// steady_state_only, stable_high, stable_low, intentional_high_pulse, intentional_low_pulse, maximum_high_pulse, or maximum_low_pulse.
    intent: String,
    /// Widths for live scenarios are measured in game ticks.
    minimum_width_ticks: Option<u64>,
    /// Required for intentional and maximum-width pulse contracts.
    maximum_width_ticks: Option<u64>,
}

#[derive(Debug)]
struct AdaptiveComponentScan {
    snapshot: dustroute_translate::MinecraftSnapshot,
    component_count: usize,
    component_limit: usize,
    limit_reached: bool,
    scanned_tiles: usize,
    scanned_block_positions: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ProposeRepairsParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
    /// Maximum Manhattan gap between disconnected fragments. Defaults to 2.
    max_gap: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PreviewRepairParams {
    /// Repair operation UUID returned by new_repair_plan.
    operation_id: String,
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    player: Option<String>,
}

fn json_text(mut value: Value) -> String {
    if value.get("ok") == Some(&Value::Bool(false))
        && let Some(object) = value.as_object_mut()
    {
        object
            .entry("schema_version")
            .or_insert_with(|| Value::String(crate::api::ERROR_SCHEMA_V1.to_owned()));
        object
            .entry("error_code")
            .or_insert_with(|| Value::String("internal".to_owned()));
        object.entry("retryable").or_insert(Value::Bool(false));
    }
    serde_json::to_string_pretty(&value)
        .unwrap_or_else(|error| json!({ "ok": false, "error": error.to_string() }).to_string())
}

fn error_text(code: McpErrorCode, message: impl Into<String>, retryable: bool) -> String {
    match serde_json::to_value(ErrorResponse::new(code, message, retryable)) {
        Ok(value) => json_text(value),
        Err(error) => format!(
            "{{\"ok\":false,\"schema_version\":\"dustroute.error.v1\",\"error\":\"failed to serialize MCP error: {error}\",\"error_code\":\"internal\",\"retryable\":false}}"
        ),
    }
}

fn scenario_trace_json(trace: &dustroute_translate::ScenarioTrace) -> Value {
    serde_json::to_value(TransitionTraceResponse::from(trace)).unwrap_or_else(|error| {
        json!({
            "serialization_error": error.to_string(),
            "duration_redstone_ticks": trace.duration_redstone_ticks,
            "events": [],
            "final_strengths": [],
            "final_powered": [],
        })
    })
}

fn scenario_run_json(run: &dustroute_translate::ScenarioRun) -> Value {
    json!({
        "label": run.label,
        "safety": run.safety,
        "trace": scenario_trace_json(&run.trace),
        "differences": run.differences,
    })
}

fn transition_contracts(
    params: Option<&[TransitionContractParam]>,
    scene: &dustroute_physical::PhysicalScene,
) -> Result<BTreeMap<dustroute_physical::ComponentId, dustroute_ir::SignalIntent>, String> {
    let mut contracts = BTreeMap::new();
    for contract in params.into_iter().flatten() {
        let position = Pos::new(contract.x, contract.y, contract.z);
        let component = scene
            .component_at(position)
            .ok_or_else(|| format!("contract position {position:?} is not a circuit component"))?
            .id;
        let high = contract.intent.contains("high");
        let polarity = if high {
            dustroute_ir::PulsePolarity::High
        } else {
            dustroute_ir::PulsePolarity::Low
        };
        let intent = match contract.intent.as_str() {
            "steady_state_only" => dustroute_ir::SignalIntent::SteadyStateOnly,
            "stable_high" => dustroute_ir::SignalIntent::Stable { powered: true },
            "stable_low" => dustroute_ir::SignalIntent::Stable { powered: false },
            "intentional_high_pulse" | "intentional_low_pulse" => {
                let minimum = contract.minimum_width_ticks.unwrap_or(1);
                let maximum = contract
                    .maximum_width_ticks
                    .ok_or_else(|| format!("{} requires maximum_width_ticks", contract.intent))?;
                if minimum > maximum {
                    return Err("minimum_width_ticks exceeds maximum_width_ticks".to_owned());
                }
                dustroute_ir::SignalIntent::IntentionalPulse {
                    polarity,
                    time_unit: dustroute_ir::TraceTimeUnit::GameTick,
                    minimum_width_ticks: minimum,
                    maximum_width_ticks: maximum,
                }
            }
            "maximum_high_pulse" | "maximum_low_pulse" => {
                dustroute_ir::SignalIntent::MaximumPulseWidth {
                    polarity,
                    time_unit: dustroute_ir::TraceTimeUnit::GameTick,
                    maximum_width_ticks: contract.maximum_width_ticks.ok_or_else(|| {
                        format!("{} requires maximum_width_ticks", contract.intent)
                    })?,
                }
            }
            _ => return Err(format!("unknown transition intent {:?}", contract.intent)),
        };
        if contracts.insert(component, intent).is_some() {
            return Err(format!("duplicate contract for component {}", component.0));
        }
    }
    Ok(contracts)
}

fn bounds_json(bounds: dustroute_translate::RegionBounds) -> Value {
    json!({ "min": bounds.min, "max": bounds.max })
}

fn is_supported_redstone_name(name: &str) -> bool {
    matches!(
        name,
        "minecraft:redstone_wire"
            | "minecraft:redstone_torch"
            | "minecraft:redstone_wall_torch"
            | "minecraft:repeater"
            | "minecraft:comparator"
            | "minecraft:lever"
            | "minecraft:redstone_block"
            | "minecraft:piston"
            | "minecraft:sticky_piston"
    )
}

fn is_redstone_candidate_name(name: &str) -> bool {
    is_supported_redstone_name(name)
        || matches!(
            name,
            "minecraft:observer"
                | "minecraft:redstone_lamp"
                | "minecraft:target"
                | "minecraft:dispenser"
                | "minecraft:dropper"
                | "minecraft:hopper"
                | "minecraft:daylight_detector"
                | "minecraft:tripwire_hook"
                | "minecraft:sculk_sensor"
                | "minecraft:calibrated_sculk_sensor"
        )
        || name.ends_with("_button")
        || name.ends_with("_pressure_plate")
}

fn position_on_boundary(position: Pos, min: Pos, max: Pos) -> bool {
    position.x == min.x
        || position.x == max.x
        || position.y == min.y
        || position.y == max.y
        || position.z == min.z
        || position.z == max.z
}

fn raw_world_inspection(
    snapshot: &dustroute_translate::MinecraftSnapshot,
    target: Pos,
    dimension: &str,
    include_block_list: bool,
    max_listed_blocks: usize,
) -> Value {
    let size = Pos::new(
        snapshot.max.x - snapshot.min.x + 1,
        snapshot.max.y - snapshot.min.y + 1,
        snapshot.max.z - snapshot.min.z + 1,
    );
    let volume = i64::from(size.x) * i64::from(size.y) * i64::from(size.z);
    let mut counts = BTreeMap::<String, usize>::new();
    let mut chunks = BTreeSet::new();
    let mut redstone = Vec::new();
    let mut modeled_redstone_count = 0_usize;
    let mut boundary_non_air_count = 0_usize;
    let mut boundary_redstone_count = 0_usize;
    let mut state_property_counts = BTreeMap::<String, usize>::new();
    let mut target_block = None;
    for block in &snapshot.blocks {
        *counts.entry(block.name.clone()).or_default() += 1;
        chunks.insert((block.pos.x.div_euclid(16), block.pos.z.div_euclid(16)));
        if position_on_boundary(block.pos, snapshot.min, snapshot.max) {
            boundary_non_air_count += 1;
        }
        for property in block.properties.keys() {
            *state_property_counts.entry(property.clone()).or_default() += 1;
        }
        if block.pos == target {
            target_block = Some(block);
        }
        if is_redstone_candidate_name(&block.name) {
            if is_supported_redstone_name(&block.name) {
                modeled_redstone_count += 1;
            }
            if position_on_boundary(block.pos, snapshot.min, snapshot.max) {
                boundary_redstone_count += 1;
            }
            redstone.push(block);
        }
    }
    redstone.sort_by_key(|block| {
        (
            block.pos.x.abs_diff(target.x)
                + block.pos.y.abs_diff(target.y)
                + block.pos.z.abs_diff(target.z),
            block.pos,
        )
    });
    let listed_redstone = redstone.iter().take(max_listed_blocks).collect::<Vec<_>>();
    let listed_blocks = include_block_list.then(|| {
        snapshot
            .blocks
            .iter()
            .take(max_listed_blocks)
            .collect::<Vec<_>>()
    });
    let non_air_count = snapshot.blocks.len();
    let air_count = usize::try_from(volume)
        .unwrap_or(usize::MAX)
        .saturating_sub(non_air_count);
    json!({
        "ok": true,
        "mode": "raw_world_inspection",
        "inference_applied": false,
        "dimension": dimension,
        "target": target,
        "target_block": target_block,
        "scan": {
            "requested_and_returned_bounds": { "min": snapshot.min, "max": snapshot.max },
            "size": size,
            "volume": volume,
            "complete": true,
            "completeness_basis": "the bridge rejects the entire scan if any coordinate is unavailable",
            "chunk_columns_with_non_air_blocks": chunks.len()
        },
        "counts": {
            "air": air_count,
            "non_air": non_air_count,
            "redstone_candidates": redstone.len(),
            "modeled_redstone": modeled_redstone_count,
            "unmodeled_redstone_candidates": redstone.len().saturating_sub(modeled_redstone_count),
            "by_block_name": counts,
            "state_properties_present": state_property_counts
        },
        "boundary": {
            "non_air_blocks": boundary_non_air_count,
            "redstone_candidates": boundary_redstone_count,
            "redstone_touches_boundary": boundary_redstone_count > 0,
            "guidance": (boundary_redstone_count > 0).then_some(
                "redstone reaches the raw scan boundary; increase the radius before assuming the circuit is complete"
            )
        },
        "redstone_blocks": listed_redstone,
        "redstone_blocks_truncated": redstone.len() > max_listed_blocks,
        "blocks": listed_blocks,
        "blocks_truncated": include_block_list && non_air_count > max_listed_blocks
    })
}

fn logical_role_json(translated: &dustroute_translate::ReverseResult) -> Value {
    serde_json::to_value(dustroute_translate::derive_local_logic(translated))
        .unwrap_or_else(|error| json!({ "classification": "unknown", "reason": error.to_string() }))
}

fn focused_role_json(translated: &dustroute_translate::ReverseResult, target: Pos) -> Value {
    let focused = dustroute_translate::classify_focused_role(translated, target);
    let physical_component = translated
        .analysis
        .scene
        .component_at(target)
        .map(|component| component.id);
    let recognized_gates = physical_component
        .map(|component| {
            translated
                .gate_view
                .gates
                .iter()
                .filter(|gate| gate.physical_components.contains(&component))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let observed_block = translated
        .analysis
        .scene
        .component_at(target)
        .map(|component| &component.block);
    let Some(component_id) = focused.signal_component else {
        return json!({
            "position": target,
            "block": observed_block.map(|block| block.kind),
            "observed_name": observed_block.and_then(|block| block.observed_name.as_deref()),
            "observed_properties": observed_block.map(|block| &block.observed_properties),
            "capabilities": observed_block.map(dustroute_physical::Block::capabilities),
            "physical_component": physical_component,
            "recognized_gates": recognized_gates,
            "role": "support_or_unresolved"
        });
    };
    json!({
        "position": target,
        "block": observed_block.map(|block| block.kind),
        "observed_name": observed_block.and_then(|block| block.observed_name.as_deref()),
        "observed_properties": observed_block.map(|block| &block.observed_properties),
        "capabilities": observed_block.map(dustroute_physical::Block::capabilities),
        "physical_component": physical_component,
        "recognized_gates": recognized_gates,
        "signal_component": component_id,
        "incoming_components": focused.incoming_components,
        "outgoing_components": focused.outgoing_components,
        "role": focused.role
    })
}

fn focused_hierarchy_role_json(
    scene: &dustroute_physical::PhysicalScene,
    hierarchy: &dustroute_ir::HierarchicalIr,
    target: Pos,
) -> Value {
    let Some(component) = scene.component_at(target) else {
        return json!({
            "position": target,
            "role": "support_or_unresolved",
            "recognized_cells": []
        });
    };
    let incoming = scene
        .connections
        .iter()
        .filter(|connection| connection.sink.component == component.id)
        .map(|connection| connection.source.component)
        .collect::<BTreeSet<_>>();
    let outgoing = scene
        .connections
        .iter()
        .filter(|connection| connection.source.component == component.id)
        .map(|connection| connection.sink.component)
        .collect::<BTreeSet<_>>();
    let cells = hierarchy
        .cell_graph
        .value
        .cells
        .gates
        .iter()
        .filter(|cell| cell.physical_components.contains(&component.id))
        .collect::<Vec<_>>();
    let role = if incoming.len() > 1 {
        "signal_merge"
    } else if outgoing.len() > 1 {
        "signal_branch"
    } else if !incoming.is_empty() || !outgoing.is_empty() {
        "intermediate_path"
    } else {
        "isolated_or_unresolved"
    };
    json!({
        "position": target,
        "block": component.block.kind,
        "observed_name": component.block.observed_name,
        "observed_properties": component.block.observed_properties,
        "capabilities": component.block.capabilities(),
        "physical_component": component.id,
        "incoming_components": incoming,
        "outgoing_components": outgoing,
        "recognized_cells": cells,
        "role": role,
        "physical_origin": hierarchy.cell_graph.provenance.physical_positions.get(&component.id)
    })
}

fn capability_report_json(scene: &dustroute_physical::PhysicalScene) -> Value {
    const MAX_CAPABILITY_ISSUE_SAMPLES: usize = 32;
    let report = scene.capability_report();
    let mut counts = BTreeMap::<String, usize>::new();
    for issue in &report.issues {
        *counts
            .entry(format!("{:?}:{:?}", issue.stage, issue.level).to_lowercase())
            .or_default() += 1;
    }
    json!({
        "groups": report.groups,
        "issue_count": report.issues.len(),
        "issue_counts_by_stage_and_level": counts,
        "issue_samples": report.issues.iter().take(MAX_CAPABILITY_ISSUE_SAMPLES).collect::<Vec<_>>(),
        "issues_truncated": report.issues.len() > MAX_CAPABILITY_ISSUE_SAMPLES
    })
}

fn signal_liveness_json(
    scene: &dustroute_physical::PhysicalScene,
    focus: Option<dustroute_physical::Pos>,
) -> Value {
    const MAX_FINDINGS: usize = 64;
    const MAX_RANKED_FINDINGS: usize = 16;
    let report = dustroute_translate::analyze_signal_liveness(scene);
    let ranked = focus.map(|focus| {
        dustroute_translate::rank_liveness_findings(scene, &report, focus)
            .into_iter()
            .take(MAX_RANKED_FINDINGS)
            .collect::<Vec<_>>()
    });
    let source_counts =
        report
            .sources
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, source| {
                let kind = match source.kind {
                    dustroute_translate::SignalSourceKind::ControllableInput => {
                        "controllable_input"
                    }
                    dustroute_translate::SignalSourceKind::IntrinsicSource => "intrinsic_source",
                    dustroute_translate::SignalSourceKind::ObservationBoundary => {
                        "observation_boundary"
                    }
                    dustroute_translate::SignalSourceKind::InferredPrimaryInput => {
                        "inferred_primary_input"
                    }
                };
                *counts.entry(kind.to_owned()).or_default() += 1;
                counts
            });
    let external_input_waiting = report
        .required_input_assessments
        .iter()
        .filter(|assessment| {
            assessment.status == dustroute_translate::RequiredInputStatus::AwaitingExternalInput
        })
        .take(MAX_FINDINGS)
        .collect::<Vec<_>>();
    json!({
        "physical_traversal_group_count": scene.physical_traversal_groups().len(),
        "directed_signal_region_count": report.directed_regions.len(),
        "cyclic_directed_signal_region_count": report.directed_regions.iter().filter(|region| region.cyclic).count(),
        "drive_source_count": report.drive_sources.len(),
        "source_counts_by_kind": source_counts,
        "source_evidence": report.sources.iter().take(MAX_FINDINGS).collect::<Vec<_>>(),
        "drive_reachable_component_count": report.drive_reachable.len(),
        "potentially_drive_reachable_component_count": report.potential_drive_reachable.len(),
        "external_input_waiting_count": report.required_input_assessments.iter().filter(|assessment| assessment.status == dustroute_translate::RequiredInputStatus::AwaitingExternalInput).count(),
        "external_input_waiting": external_input_waiting,
        "undriven_required_input_count": report.undriven_inputs.len(),
        "undriven_required_inputs": report.undriven_inputs.iter().take(MAX_FINDINGS).collect::<Vec<_>>(),
        "ranked_findings_near_focus": ranked,
        "findings_truncated": report.undriven_inputs.len() > MAX_FINDINGS,
        "interpretation": "confirmed sources, inferred primary inputs, and genuine no-source failures are separate; inferred external inputs are not automatic repair evidence"
    })
}

fn hierarchical_result_json(
    bounds: dustroute_translate::RegionBounds,
    hierarchy: &dustroute_ir::HierarchicalIr,
    focused: Value,
    expansion: &Value,
    focus: Option<dustroute_physical::Pos>,
) -> Value {
    let scene = &hierarchy.physical_graph.value.scene;
    json!({
        "ok": true,
        "analysis_mode": "hierarchical_local_first",
        "bounds": bounds_json(bounds),
        "analysis_complete": expansion["limit_reached"] != Value::Bool(true),
        "focused_component": focused,
        "expansion": expansion,
        "block_capabilities": capability_report_json(scene),
        "signal_liveness": signal_liveness_json(scene, focus),
        "stages": {
            "physical_snapshot": {
                "completeness": hierarchy.physical_snapshot.completeness,
                "components": scene.components.len(),
                "diagnostic_count": hierarchy.physical_snapshot.diagnostics.len(),
                "diagnostics": hierarchy.physical_snapshot.diagnostics.iter().take(16).collect::<Vec<_>>(),
                "diagnostics_truncated": hierarchy.physical_snapshot.diagnostics.len() > 16
            },
            "physical_graph": {
                "completeness": hierarchy.physical_graph.completeness,
                "directed_connections": scene.connections.len(),
                "physical_traversal_groups": scene.physical_traversal_groups().len(),
                "fragments": scene.fragments.len(),
                "unresolved": hierarchy.physical_graph.unresolved
            },
            "cell_graph": {
                "completeness": hierarchy.cell_graph.completeness,
                "cell_count": hierarchy.cell_graph.value.cells.gates.len(),
                "cells": hierarchy.cell_graph.value.cells.gates,
                "unresolved_component_count": hierarchy.cell_graph.unresolved.len()
            },
            "logic_graph": {
                "completeness": hierarchy.logic_graph.completeness,
                "expression_count": hierarchy.logic_graph.value.expressions.expressions.len(),
                "expressions": hierarchy.logic_graph.value.expressions
            },
            "functional_graph": {
                "completeness": hierarchy.functional_graph.completeness,
                "functions": hierarchy.functional_graph.value.functions,
                "validity": hierarchy.temporal.timing,
            }
        },
        "temporal": {
            "timing": hierarchy.temporal.timing,
            "timed_nodes": hierarchy.temporal.timed_circuit.nodes.len(),
            "timed_edges": hierarchy.temporal.timed_circuit.edges.len(),
            "steady_state_retained_components": hierarchy.temporal.steady_state.retained_components.len(),
            "steady_state_compressed_components": hierarchy.temporal.steady_state.compressed_components.len(),
            "steady_state_edges": hierarchy.temporal.steady_state.edges.len(),
            "transient_assessment": {
                "status": "not_simulated",
                "findings": [],
                "guidance": "timing risk is structural only; run transition scenarios before claiming that a pulse was observed"
            }
        },
        "truth_table": null,
        "truth_table_skipped": "large circuits use local cells and hierarchical summaries instead of a flat whole-circuit truth table",
        "repair_proposals": [],
        "repair_guidance": "select or look at a smaller cell before generating a physical repair proposal"
    })
}

fn reverse_result_json(
    bounds: dustroute_translate::RegionBounds,
    translated: &dustroute_translate::ReverseResult,
) -> Value {
    let mut hierarchy = dustroute_ir::hierarchy_from_views(
        &translated.analysis.scene,
        translated.gate_view.clone(),
        translated.expression_view.clone(),
        translated.functional_view.clone(),
    );
    hierarchy.temporal = translated.temporal.clone();
    json!({
        "ok": true,
        "bounds": bounds_json(bounds),
        "redstone_blocks": translated.analysis.redstone_blocks.len(),
        "physical": {
            "components": translated.analysis.scene.components.len(),
            "verified_connections": translated.analysis.scene.connections.len(),
            "physical_traversal_groups": translated.analysis.scene.physical_traversal_groups().len(),
            "connected_fragments": translated.analysis.scene.fragments.len(),
            "nearby_gap_candidates": translated.analysis.scene.gap_candidates(2),
            "observation": translated.analysis.scene.observation,
            "analysis_complete": translated.analysis.scene.observation.is_complete(),
            "block_capabilities": capability_report_json(&translated.analysis.scene),
        },
        "signal_liveness": signal_liveness_json(&translated.analysis.scene, None),
        "stages": {
            "observed_world": {
                "bounds": bounds_json(bounds),
                "redstone_blocks": translated.analysis.redstone_blocks.len()
            },
            "physical_scene": {
                "completeness": hierarchy.physical_snapshot.completeness,
                "components": hierarchy.physical_snapshot.value.scene.components.len(),
                "diagnostics": hierarchy.physical_snapshot.diagnostics
            },
            "electrical_network": {
                "completeness": hierarchy.physical_graph.completeness,
                "directed_connections": hierarchy.physical_graph.value.scene.connections.len(),
                "unresolved": hierarchy.physical_graph.unresolved
            },
            "timed_behavior": {
                "timing": hierarchy.temporal.timing,
                "devices": hierarchy.temporal.behavior.devices.len(),
                "traces": hierarchy.temporal.behavior.traces.len()
            },
            "local_logic": {
                "completeness": hierarchy.logic_graph.completeness,
                "cells": hierarchy.cell_graph.value.cells,
                "expressions": hierarchy.logic_graph.value.expressions
            },
            "functional_candidates": {
                "completeness": hierarchy.functional_graph.completeness,
                "functions": hierarchy.functional_graph.value.functions,
                "unresolved": hierarchy.functional_graph.unresolved
            }
        },
        "gate_view": translated.gate_view,
        "expression_view": translated.expression_view,
        "functional_view": translated.functional_view,
        "functional_validity": translated.temporal.timing,
        "behavior_ir": {
            "temporal_devices": translated.temporal.behavior.devices,
            "trace_count": translated.temporal.behavior.traces.len(),
            "timing": translated.temporal.timing,
            "timed_nodes": translated.temporal.timed_circuit.nodes.len(),
            "timed_edges": translated.temporal.timed_circuit.edges.len(),
            "steady_state_projection": translated.temporal.steady_state,
            "transient_assessment": {
                "status": if translated.temporal.behavior.traces.is_empty() { "not_simulated" } else { "observed_initial_state_only" },
                "assessments": translated.temporal.transients,
                "guidance": "hazard_candidate means a measured transient has no registered intent; hazard_confirmed requires an explicit signal contract. Initial-state settling does not cover every input transition."
            },
        },
        "inputs": translated.analysis.inputs.iter().map(|terminal| json!({
            "position": terminal.anchor,
            "component": terminal.component,
            "confidence": format!("{:?}", terminal.confidence).to_lowercase(),
        })).collect::<Vec<_>>(),
        "outputs": translated.analysis.outputs.iter().map(|terminal| json!({
            "position": terminal.anchor,
            "component": terminal.component,
            "confidence": format!("{:?}", terminal.confidence).to_lowercase(),
        })).collect::<Vec<_>>(),
        "expressions": translated.expressions.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "logical_role": logical_role_json(translated),
        "truth_table": translated.truth_table.as_ref().map(|table| table.rows.iter().map(|row| json!({
            "inputs": row.inputs,
            "outputs": row.outputs,
        })).collect::<Vec<_>>()),
        "truth_table_error": translated.truth_table_error.as_ref().map(ToString::to_string),
        "diagnostics": {
            "signal_islands": translated.analysis.diagnostics.signal_islands.len(),
            "isolated_redstone": translated.analysis.diagnostics.isolated_redstone.len(),
            "unreachable_components": translated.analysis.diagnostics.unreachable_from_inputs.len(),
            "components_without_output_path": translated.analysis.diagnostics.cannot_reach_outputs.len(),
            "invalid_supports": translated.analysis.diagnostics.invalid_supports.len(),
            "non_controllable_torches": translated.analysis.diagnostics.non_controllable_torches.len(),
        }
    })
}

impl DustRouteMcp {
    #[must_use]
    pub fn new(bridge_address: impl Into<String>) -> Self {
        Self::with_policy(bridge_address, McpPolicy::default())
    }

    #[must_use]
    pub fn with_policy(bridge_address: impl Into<String>, policy: McpPolicy) -> Self {
        Self::with_policy_and_profile(bridge_address, policy, ToolProfile::from_environment())
    }

    #[must_use]
    pub fn with_policy_and_profile(
        bridge_address: impl Into<String>,
        policy: McpPolicy,
        profile: ToolProfile,
    ) -> Self {
        let mut tool_router = Self::tool_router();
        if profile == ToolProfile::Default {
            for tool in DEBUG_ONLY_TOOLS {
                tool_router.disable_route(tool.to_owned());
            }
        }
        Self {
            bridge: BotBridge::new(bridge_address),
            selections: Arc::new(Mutex::new(HashMap::new())),
            selection_dimensions: Arc::new(Mutex::new(HashMap::new())),
            tool_router,
            prompt_router: Self::prompt_router(),
            plans: Arc::new(Mutex::new(HashMap::new())),
            plan_dimensions: Arc::new(Mutex::new(HashMap::new())),
            applied_plans: Arc::new(Mutex::new(HashMap::new())),
            repair_plans: Arc::new(Mutex::new(HashMap::new())),
            transition_plans: Arc::new(Mutex::new(HashMap::new())),
            state_store: PlanStateStore::from_environment("default"),
            policy,
            app: DustRouteService::default(),
            operations: OperationRegistry::default(),
            mutation_lock: Arc::new(Mutex::new(())),
            assist_player: None,
            server_address: None,
        }
    }

    #[must_use]
    pub fn with_policy_and_player(
        bridge_address: impl Into<String>,
        policy: McpPolicy,
        assist_player: impl Into<String>,
    ) -> Self {
        let mut service = Self::with_policy(bridge_address, policy);
        service.assist_player = Some(assist_player.into());
        service
    }

    #[must_use]
    pub fn with_config(config: McpConfig, policy: McpPolicy) -> Self {
        let state_scope = format!("{}\n{}", config.server_address, config.assist_player);
        let mut service =
            Self::with_policy_and_player(config.bridge_address, policy, config.assist_player);
        service.server_address = Some(config.server_address);
        service.state_store = PlanStateStore::from_environment(&state_scope);
        service
    }

    async fn store_repair_plan(
        &self,
        operation_id: uuid::Uuid,
        plan: StoredRepairPlan,
    ) -> Result<(), String> {
        self.state_store.save("repairs", operation_id, &plan)?;
        self.repair_plans.lock().await.insert(operation_id, plan);
        Ok(())
    }

    async fn repair_plan(
        &self,
        operation_id: uuid::Uuid,
    ) -> Result<Option<StoredRepairPlan>, String> {
        let plan: Option<StoredRepairPlan> = self.state_store.load("repairs", operation_id)?;
        if let Some(plan) = &plan {
            self.repair_plans
                .lock()
                .await
                .insert(operation_id, plan.clone());
            return Ok(Some(plan.clone()));
        }
        Ok(self.repair_plans.lock().await.get(&operation_id).cloned())
    }

    fn resolve_player(&self, requested: Option<&str>) -> Result<String, String> {
        match (&self.assist_player, requested) {
            (Some(configured), None) => Ok(configured.clone()),
            (Some(configured), Some(requested)) if configured == requested => {
                Ok(configured.clone())
            }
            (Some(configured), Some(_)) => Err(format!(
                "player override is not allowed; configured assist player is {configured:?}"
            )),
            (None, Some(requested)) => Ok(requested.to_owned()),
            (None, None) => {
                Err("player is required when DUSTROUTE_ASSIST_PLAYER is not configured".to_owned())
            }
        }
    }

    fn authorize_player(&self, player: &str) -> Option<String> {
        self.policy
            .authorize_player(player)
            .err()
            .map(|error| json_text(json!({ "ok": false, "error": error.to_string() })))
    }

    async fn mutate_placement(&self, params: ConfirmedOperationParams, undo: bool) -> String {
        if !params.confirm {
            return json_text(json!({
                "ok": false,
                "error": "confirm must be true because this operation changes the world"
            }));
        }
        if let Err(error) = self.policy.authorize_mutation() {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let _mutation_guard = self.mutation_lock.lock().await;
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let plan = match self.plans.lock().await.get(&operation_id).cloned() {
            Some(plan) => plan,
            None => return json_text(json!({ "ok": false, "error": "unknown operation ID" })),
        };
        let dimension = match self
            .plan_dimensions
            .lock()
            .await
            .get(&operation_id)
            .cloned()
        {
            Some(dimension) => dimension,
            None => {
                return json_text(json!({
                    "ok": false,
                    "error": "placement plan has no captured dimension"
                }));
            }
        };
        if let Err(error) = self.policy.authorize_dimension(&dimension) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let is_applied = self
            .applied_plans
            .lock()
            .await
            .get(&operation_id)
            .copied()
            .unwrap_or(false);
        if undo && !is_applied {
            return json_text(json!({ "ok": false, "error": "placement plan is not applied" }));
        }
        if !undo && is_applied {
            return json_text(json!({ "ok": false, "error": "placement plan is already applied" }));
        }

        let source = if undo {
            &plan.undo.changes
        } else {
            &plan.changes
        };
        if let Err(error) = self.policy.validate_placement_size(source.len()) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let (baseline_matches, baseline_mismatches) = match self
            .verify_placement_changes(source, &dimension, false)
            .await
        {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if !baseline_matches {
            return json_text(json!({
                "ok": false,
                "error": "placement baseline is stale; preview again before changing the world",
                "mismatches": baseline_mismatches
            }));
        }
        let mut changes = source.iter().collect::<Vec<_>>();
        changes.sort_by_key(|change| {
            let priority = match change.after.kind {
                BlockKind::Solid
                | BlockKind::Transparent
                | BlockKind::RedstoneBlock
                | BlockKind::Piston => 0,
                BlockKind::RedstoneTorch | BlockKind::Lever => 2,
                _ => 1,
            };
            (priority, change.pos.y, change.pos.x, change.pos.z)
        });
        let export = JavaExportConfig {
            relative: false,
            ..JavaExportConfig::default()
        };
        let writes = match changes
            .into_iter()
            .map(|change| {
                java_block_state(&change.after, &export).map(|state| {
                    json!({
                        "pos": Pos::new(change.pos.x, change.pos.y, change.pos.z),
                        "state": state,
                    })
                })
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(writes) => writes,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let bridge_result = match self.bridge.write_blocks(json!(writes), &dimension).await {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (verified, verification_mismatches) = match self
            .verify_placement_changes(source, &dimension, true)
            .await
        {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if !verified {
            return json_text(json!({
                "ok": false,
                "error": format!("placement write completed but live verification failed at {:?}", verification_mismatches),
                "mismatches": verification_mismatches,
                "bridge": bridge_result
            }));
        }
        self.applied_plans.lock().await.insert(operation_id, !undo);
        self.operations
            .record_completed(
                uuid::Uuid::new_v4(),
                if undo {
                    OperationKind::PlacementUndo
                } else {
                    OperationKind::PlacementApply
                },
                json!({
                    "source_operation_id": operation_id,
                    "changed_blocks": source.len(),
                    "dimension": dimension,
                }),
            )
            .await;
        json_text(json!({
            "schema_version": PLACEMENT_SCHEMA_V1,
            "ok": true,
            "operation_id": operation_id,
            "action": if undo { "undo" } else { "apply" },
            "changed_blocks": source.len(),
            "verified": verified,
            "dimension": dimension,
            "bridge": bridge_result,
        }))
    }

    async fn verify_placement_changes(
        &self,
        changes: &[BlockChange],
        dimension: &str,
        verify_after: bool,
    ) -> Result<(bool, Vec<Pos>), String> {
        let Some(first) = changes.first() else {
            return Ok((true, Vec::new()));
        };
        let (mut min, mut max) = (first.pos, first.pos);
        for change in &changes[1..] {
            min = Pos::new(
                min.x.min(change.pos.x),
                min.y.min(change.pos.y),
                min.z.min(change.pos.z),
            );
            max = Pos::new(
                max.x.max(change.pos.x),
                max.y.max(change.pos.y),
                max.z.max(change.pos.z),
            );
        }
        let snapshot = self
            .bridge
            .scan_region(min, max, dimension)
            .await
            .map_err(|error| error.to_string())?;
        let snapshot_json = serde_json::to_string(&snapshot).map_err(|error| error.to_string())?;
        let (_, world) =
            world_from_snapshot_json(&snapshot_json).map_err(|error| error.to_string())?;
        let mismatches = changes
            .iter()
            .filter(|change| {
                let expected = if verify_after {
                    &change.after
                } else {
                    &change.before
                };
                !block_matches(world.get(change.pos), expected)
            })
            .map(|change| change.pos)
            .collect::<Vec<_>>();
        Ok((mismatches.is_empty(), mismatches))
    }

    async fn write_physical_changes(
        &self,
        changes: &[PhysicalBlockChange],
        dimension: &str,
    ) -> Result<Value, String> {
        self.policy
            .validate_placement_size(changes.len())
            .map_err(|error| error.to_string())?;
        let mut changes = changes.iter().collect::<Vec<_>>();
        changes.sort_by_key(|change| {
            let priority = match change.after.kind {
                BlockKind::Solid
                | BlockKind::Transparent
                | BlockKind::RedstoneBlock
                | BlockKind::Piston => 0,
                BlockKind::RedstoneTorch | BlockKind::Lever => 2,
                _ => 1,
            };
            (priority, change.pos.y, change.pos.x, change.pos.z)
        });
        let export = JavaExportConfig {
            relative: false,
            ..JavaExportConfig::default()
        };
        let actions = changes
            .into_iter()
            .map(|change| -> Result<Value, String> {
                if change.after.kind == BlockKind::Air {
                    return Ok(json!({ "action": "dig", "pos": change.pos }));
                }
                let state =
                    java_block_state(&change.after, &export).map_err(|error| error.to_string())?;
                let item = if change.after.kind == BlockKind::RedstoneWire {
                    "minecraft:redstone".to_owned()
                } else {
                    state.split('[').next().unwrap_or(&state).to_owned()
                };
                let support_offset = change.after.support_offset.unwrap_or(Pos::new(0, -1, 0));
                let reference =
                    change
                        .pos
                        .offset(support_offset.x, support_offset.y, support_offset.z);
                Ok(json!({
                    "action": "place",
                    "pos": change.pos,
                    "item": item,
                    "state": state,
                    "reference": reference,
                    "face": Pos::new(-support_offset.x, -support_offset.y, -support_offset.z),
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.bridge
            .place_physical_blocks(json!(actions), dimension)
            .await
            .map_err(|error| error.to_string())
    }

    async fn verify_physical_changes(
        &self,
        changes: &[PhysicalBlockChange],
        dimension: &str,
    ) -> Result<(bool, Vec<Pos>), String> {
        let Some(bounds) = bounds_for_changes(changes) else {
            return Ok((true, Vec::new()));
        };
        let snapshot = self
            .bridge
            .scan_region(bounds.min, bounds.max, dimension)
            .await
            .map_err(|error| error.to_string())?;
        let snapshot_json = serde_json::to_string(&snapshot).map_err(|error| error.to_string())?;
        let (_, world) =
            world_from_snapshot_json(&snapshot_json).map_err(|error| error.to_string())?;
        let mismatches = changes
            .iter()
            .filter(|change| !block_matches(world.get(change.pos), &change.after))
            .map(|change| change.pos)
            .collect::<Vec<_>>();
        Ok((mismatches.is_empty(), mismatches))
    }

    async fn mutate_repair(&self, params: ConfirmedOperationParams, undo: bool) -> String {
        if !params.confirm {
            return json_text(json!({
                "ok": false,
                "error": "confirm must be true because this operation changes the world"
            }));
        }
        if let Err(error) = self.policy.authorize_mutation() {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let _mutation_guard = self.mutation_lock.lock().await;
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let plan = match self.repair_plan(operation_id).await {
            Ok(Some(plan)) => plan,
            Ok(None) => {
                return json_text(json!({ "ok": false, "error": "unknown or expired repair ID" }));
            }
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if !undo && self.policy.preview_required && !plan.previewed {
            return json_text(json!({ "ok": false, "error": "repair must be previewed first" }));
        }
        if undo && !plan.applied {
            return json_text(json!({ "ok": false, "error": "repair is not applied" }));
        }
        if !undo && plan.applied {
            return json_text(json!({ "ok": false, "error": "repair is already applied" }));
        }
        let patch = if undo {
            plan.patch.inverse()
        } else {
            plan.patch.clone()
        };
        let bridge = match self
            .write_physical_changes(&patch.changes, &plan.dimension)
            .await
        {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let (verified, mismatches) = match self
            .verify_physical_changes(&patch.changes, &plan.dimension)
            .await
        {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if !verified && !undo {
            let rollback = plan.patch.inverse();
            let rollback_result = self
                .write_physical_changes(&rollback.changes, &plan.dimension)
                .await;
            if rollback_result.is_ok() {
                let mut rolled_back = plan.clone();
                rolled_back.applied = false;
                let _ = self.store_repair_plan(operation_id, rolled_back).await;
            }
            return json_text(json!({
                "ok": false,
                "error": "repair verification failed; automatic rollback attempted",
                "mismatches": mismatches,
                "rollback_ok": rollback_result.is_ok(),
            }));
        }
        let mut updated_plan = plan.clone();
        updated_plan.applied = !undo;
        if let Err(error) = self.store_repair_plan(operation_id, updated_plan).await {
            return json_text(json!({
                "ok": false,
                "error": format!("world changed but repair state could not be persisted: {error}"),
                "verification_ok": verified,
                "mismatches": mismatches
            }));
        }
        let snapshot = match self
            .bridge
            .scan_region(
                plan.analysis_bounds.min,
                plan.analysis_bounds.max,
                &plan.dimension,
            )
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let post_world = serde_json::to_string(&snapshot)
            .map_err(|error| error.to_string())
            .and_then(|snapshot| {
                world_from_snapshot_json(&snapshot)
                    .map(|(_, world)| world)
                    .map_err(|error| error.to_string())
            });
        let post_analysis = post_world.as_ref().ok().map(|world| {
            let request = if plan.baseline_truth_table.is_some() {
                ReverseRequest::new(plan.analysis_bounds).with_truth_table(8)
            } else {
                ReverseRequest::new(plan.analysis_bounds)
            };
            self.app.analyze_physical(world, request)
        });
        let fragments_after = post_analysis
            .as_ref()
            .map(|analysis| analysis.reverse.analysis.scene.fragments.len());
        let semantic_verification = match (
            plan.baseline_truth_table.as_ref(),
            post_analysis
                .as_ref()
                .and_then(|analysis| analysis.reverse.truth_table.as_ref()),
        ) {
            (Some(expected), Some(actual)) => {
                let comparison = dustroute_translate::compare_truth_tables(expected, actual);
                json!({
                    "available": true,
                    "equivalent": comparison.comparable && comparison.fitness_penalty == 0,
                    "comparison": comparison
                })
            }
            _ => json!({
                "available": false,
                "reason": "the repair proposal did not include a baseline truth table; structural and block-state verification still ran"
            }),
        };
        let resulting_logic = post_analysis
            .as_ref()
            .map(|analysis| &analysis.logical_role);
        self.operations
            .record_completed(
                uuid::Uuid::new_v4(),
                if undo {
                    OperationKind::RepairUndo
                } else {
                    OperationKind::RepairApply
                },
                json!({ "source_operation_id": operation_id, "verified": verified }),
            )
            .await;
        json_text(json!({
            "schema_version": REPAIR_SCHEMA_V1,
            "ok": true,
            "operation_id": operation_id,
            "action": if undo { "undo" } else { "apply" },
            "verified": verified,
            "changed_blocks": patch.changes.len(),
            "fragments_before": plan.fragments_before,
            "fragments_after": fragments_after,
            "resulting_logic": resulting_logic,
            "semantic_verification": semantic_verification,
            "bridge": bridge,
        }))
    }

    async fn create_transition_proposals(
        &self,
        player: &str,
        dimension: &str,
        bounds: dustroute_translate::RegionBounds,
        snapshot: &dustroute_translate::MinecraftSnapshot,
        observation_ticks: u16,
        max_events: usize,
    ) -> (TransitionSafetyAssessment, Vec<Value>) {
        let safety = assess_transition_safety(snapshot);
        if safety.safety == TransitionSafety::Rejected {
            return (safety, Vec::new());
        }
        let levers = snapshot
            .blocks
            .iter()
            .filter(|block| block.name == "minecraft:lever")
            .filter_map(|block| {
                block
                    .properties
                    .get("powered")
                    .and_then(|powered| powered.parse::<bool>().ok())
                    .map(|powered| (block.pos, powered))
            })
            .collect::<Vec<_>>();
        let mut proposals = Vec::new();
        for (lever, original_powered) in levers {
            let operation_id = uuid::Uuid::new_v4();
            self.transition_plans.lock().await.insert(
                operation_id,
                StoredTransitionPlan {
                    player: player.to_owned(),
                    dimension: dimension.to_owned(),
                    bounds,
                    lever,
                    original_powered,
                    initial_snapshot: snapshot.clone(),
                    observation_ticks,
                    max_events,
                    safety: safety.clone(),
                    previewed: false,
                    executed: false,
                    restoration_verified: false,
                },
            );
            let proposal = json!({
                "operation_id": operation_id,
                "lever": lever,
                "transition": if original_powered { "on_to_off" } else { "off_to_on" },
                "observation_ticks": observation_ticks,
                "max_events": max_events,
                "safety": safety,
            });
            self.operations
                .record_completed(
                    operation_id,
                    OperationKind::TransitionProposal,
                    proposal.clone(),
                )
                .await;
            proposals.push(proposal);
        }
        (safety, proposals)
    }

    async fn selected_region(
        &self,
        player: &str,
    ) -> Result<(dustroute_translate::RegionBounds, String), String> {
        let bounds = self
            .selections
            .lock()
            .await
            .get(player)
            .ok_or_else(|| "no selection session for player".to_owned())?
            .bounds()
            .map_err(|error| error.to_string())?;
        let dimension = self
            .selection_dimensions
            .lock()
            .await
            .get(player)
            .cloned()
            .ok_or_else(|| "selection has no dimension".to_owned())?;
        Ok((bounds, dimension))
    }

    async fn scan_connected_components(
        &self,
        target: Pos,
        dimension: &str,
        max_components: usize,
        component_gap: i32,
    ) -> Result<AdaptiveComponentScan, String> {
        const TILE_SIZE: i32 = 16;
        const SEED_DISTANCE: i32 = 2;

        let seed_bounds = dustroute_translate::RegionBounds::new(
            Pos::new(
                target.x - SEED_DISTANCE,
                target.y - SEED_DISTANCE,
                target.z - SEED_DISTANCE,
            ),
            Pos::new(
                target.x + SEED_DISTANCE,
                target.y + SEED_DISTANCE,
                target.z + SEED_DISTANCE,
            ),
        );
        self.policy
            .validate_region(seed_bounds)
            .map_err(|error| error.to_string())?;
        let seed_snapshot = self
            .bridge
            .scan_region(seed_bounds.min, seed_bounds.max, dimension)
            .await
            .map_err(|error| error.to_string())?;
        let seed = seed_snapshot
            .blocks
            .iter()
            .filter(|block| is_redstone_candidate_name(&block.name))
            .min_by_key(|block| (manhattan_pos(block.pos, target), block.pos))
            .filter(|block| manhattan_pos(block.pos, target) <= SEED_DISTANCE)
            .map(|block| block.pos)
            .ok_or_else(|| {
                "no redstone component was found within 2 blocks of the gaze target".to_owned()
            })?;

        let mut blocks = seed_snapshot
            .blocks
            .into_iter()
            .map(|block| (block.pos, block))
            .collect::<BTreeMap<_, _>>();
        let mut candidates = blocks
            .values()
            .filter(|block| is_redstone_candidate_name(&block.name))
            .map(|block| block.pos)
            .collect::<BTreeSet<_>>();
        let mut loaded_tiles = BTreeSet::<(i32, i32, i32)>::new();
        let mut queued = BTreeSet::from([seed]);
        let mut queue = VecDeque::from([seed]);
        let mut connected = BTreeSet::new();
        let mut limit_reached = false;

        while let Some(current) = queue.pop_front() {
            if connected.len() == max_components {
                limit_reached = true;
                break;
            }
            queued.remove(&current);
            if !connected.insert(current) {
                continue;
            }

            let min_tile = Pos::new(
                (current.x - component_gap).div_euclid(TILE_SIZE),
                (current.y - component_gap).div_euclid(TILE_SIZE),
                (current.z - component_gap).div_euclid(TILE_SIZE),
            );
            let max_tile = Pos::new(
                (current.x + component_gap).div_euclid(TILE_SIZE),
                (current.y + component_gap).div_euclid(TILE_SIZE),
                (current.z + component_gap).div_euclid(TILE_SIZE),
            );
            for tile_x in min_tile.x..=max_tile.x {
                for tile_y in min_tile.y..=max_tile.y {
                    for tile_z in min_tile.z..=max_tile.z {
                        let tile = (tile_x, tile_y, tile_z);
                        if !loaded_tiles.insert(tile) {
                            continue;
                        }
                        let min =
                            Pos::new(tile_x * TILE_SIZE, tile_y * TILE_SIZE, tile_z * TILE_SIZE);
                        let max = Pos::new(
                            min.x + TILE_SIZE - 1,
                            min.y + TILE_SIZE - 1,
                            min.z + TILE_SIZE - 1,
                        );
                        let bounds = dustroute_translate::RegionBounds::new(min, max);
                        self.policy
                            .validate_region(bounds)
                            .map_err(|error| error.to_string())?;
                        let snapshot = self
                            .bridge
                            .scan_region(min, max, dimension)
                            .await
                            .map_err(|error| error.to_string())?;
                        for block in snapshot.blocks {
                            if is_redstone_candidate_name(&block.name) {
                                candidates.insert(block.pos);
                            }
                            blocks.insert(block.pos, block);
                        }
                    }
                }
            }

            for neighbor in candidates.iter().copied().filter(|candidate| {
                !connected.contains(candidate)
                    && manhattan_pos(*candidate, current) <= component_gap
            }) {
                if queued.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        if !queue.is_empty() {
            limit_reached = true;
        }

        let mut min = target;
        let mut max = target;
        for pos in &connected {
            min = Pos::new(min.x.min(pos.x), min.y.min(pos.y), min.z.min(pos.z));
            max = Pos::new(max.x.max(pos.x), max.y.max(pos.y), max.z.max(pos.z));
        }
        min = Pos::new(min.x - 1, min.y - 1, min.z - 1);
        max = Pos::new(max.x + 1, max.y + 1, max.z + 1);
        let snapshot_blocks = blocks
            .into_values()
            .filter(|block| {
                block.pos.x >= min.x
                    && block.pos.x <= max.x
                    && block.pos.y >= min.y
                    && block.pos.y <= max.y
                    && block.pos.z >= min.z
                    && block.pos.z <= max.z
            })
            .collect();
        let scanned_tiles = loaded_tiles.len();
        Ok(AdaptiveComponentScan {
            snapshot: dustroute_translate::MinecraftSnapshot {
                min,
                max,
                blocks: snapshot_blocks,
            },
            component_count: connected.len(),
            component_limit: max_components,
            limit_reached,
            scanned_tiles,
            scanned_block_positions: 125 + scanned_tiles * 4096,
        })
    }
}

fn manhattan_pos(a: Pos, b: Pos) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs()
}

fn bounds_for_changes(
    changes: &[PhysicalBlockChange],
) -> Option<dustroute_translate::RegionBounds> {
    let first = changes.first()?.pos;
    let (min, max) = changes
        .iter()
        .skip(1)
        .fold((first, first), |(min, max), change| {
            (
                Pos::new(
                    min.x.min(change.pos.x),
                    min.y.min(change.pos.y),
                    min.z.min(change.pos.z),
                ),
                Pos::new(
                    max.x.max(change.pos.x),
                    max.y.max(change.pos.y),
                    max.z.max(change.pos.z),
                ),
            )
        });
    Some(dustroute_translate::RegionBounds::new(min, max))
}

fn block_matches(
    actual: Option<&dustroute_physical::Block>,
    expected: &dustroute_physical::Block,
) -> bool {
    let actual_kind = actual.map_or(BlockKind::Air, |block| block.kind);
    if actual_kind != expected.kind {
        return false;
    }
    let Some(actual) = actual else {
        return expected.kind == BlockKind::Air;
    };
    if expected.kind == BlockKind::RedstoneTorch {
        return expected
            .support_offset
            .is_none_or(|support| actual.support_offset == Some(support));
    }
    expected
        .facing
        .is_none_or(|facing| actual.facing == Some(facing))
        && expected
            .delay
            .is_none_or(|delay| actual.delay == Some(delay))
}

#[tool_router]
impl DustRouteMcp {
    #[tool(
        description = "Get the visible Minecraft bot connection status",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_bot_status(&self) -> String {
        match self.bridge.status().await {
            Ok(status) => json_text(json!({
                "ok": true,
                "bot": status,
                "configured_server": self.server_address,
                "assist_player": self.assist_player,
                "policy": self.policy
            })),
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    #[tool(
        description = "List players visible to the Minecraft bot. If the configured assist player is outside tracking range, move only the bot to that player and retry.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_visible_player(&self) -> String {
        match self.bridge.visible_players().await {
            Ok(mut players) => {
                let mut reacquire_error = None;
                let assist_missing = self
                    .assist_player
                    .as_ref()
                    .is_some_and(|assist| !players.iter().any(|player| &player.player == assist));
                if assist_missing {
                    let assist = self.assist_player.as_deref().unwrap_or_default();
                    if let Err(error) = self.bridge.observe_player(assist, 64.0).await {
                        reacquire_error = Some(error.to_string());
                    } else if let Ok(refreshed) = self.bridge.visible_players().await {
                        players = refreshed;
                    }
                }
                let players = players
                    .into_iter()
                    .filter(|player| {
                        self.policy.authorize_player(&player.player).is_ok()
                            && self.policy.authorize_dimension(&player.dimension).is_ok()
                    })
                    .collect::<Vec<_>>();
                json_text(json!({
                    "ok": reacquire_error.is_none(),
                    "players": players,
                    "assist_player": self.assist_player,
                    "reacquire_error": reacquire_error
                }))
            }
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    #[tool(
        description = "Observe a player's eye position, gaze direction, and targeted block",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_player_gaze(&self, Parameters(params): Parameters<ObserveParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        match self
            .bridge
            .observe_player(&player, params.max_distance.unwrap_or(64.0))
            .await
        {
            Ok(observation) => match self.policy.authorize_dimension(&observation.dimension) {
                Ok(()) => json_text(json!({ "ok": true, "observation": observation })),
                Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
            },
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    #[tool(
        description = "Inspect raw Minecraft blocks by starting near the block a player is looking at and progressively following adjacent redstone components. Expansion stops naturally at the circuit edge or explicitly at max_components; no scan radius is required.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_looked_at_world(
        &self,
        Parameters(params): Parameters<InspectLookedAtWorldParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let max_components = params.max_components.unwrap_or(8192);
        let component_gap = params.component_gap.unwrap_or(2);
        let max_distance = params.max_distance.unwrap_or(64.0);
        let max_listed_blocks = params.max_listed_blocks.unwrap_or(256);
        if !(1..=32768).contains(&max_components)
            || !(1..=8).contains(&component_gap)
            || !(1.0..=256.0).contains(&max_distance)
            || !(1..=2048).contains(&max_listed_blocks)
        {
            return json_text(json!({
                "ok": false,
                "error": "max_components must be 1..32768, component_gap 1..8, max_distance 1..256, and max_listed_blocks 1..2048"
            }));
        }
        let observation = match self.bridge.observe_player(&player, max_distance).await {
            Ok(observation) => observation,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let Some(target) = observation.targeted_block else {
            return json_text(json!({
                "ok": false,
                "error": "the player is not looking at a block",
                "observation": observation
            }));
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let scan = match self
            .scan_connected_components(
                target,
                &observation.dimension,
                max_components,
                component_gap as i32,
            )
            .await
        {
            Ok(scan) => scan,
            Err(error) => {
                return json_text(json!({
                    "ok": false,
                    "error": error,
                    "observation": observation,
                    "scan_complete": false
                }));
            }
        };
        let mut result = raw_world_inspection(
            &scan.snapshot,
            target,
            &observation.dimension,
            params.include_block_list.unwrap_or(false),
            max_listed_blocks,
        );
        if let Some(object) = result.as_object_mut() {
            object.insert("player_observation".to_owned(), json!(observation));
            object.insert(
                "expansion".to_owned(),
                json!({
                    "strategy": "adjacent_component_flood_fill",
                    "component_gap": component_gap,
                    "components_loaded": scan.component_count,
                    "component_limit": scan.component_limit,
                    "limit_reached": scan.limit_reached,
                    "complete": !scan.limit_reached,
                    "scanned_tiles": scan.scanned_tiles,
                    "scanned_block_positions": scan.scanned_block_positions,
                    "guidance": scan.limit_reached.then_some(
                        "the circuit is larger than the configured component limit; treat this inspection as incomplete"
                    )
                }),
            );
            if let Some(scan_json) = object.get_mut("scan").and_then(Value::as_object_mut) {
                scan_json.insert("complete".to_owned(), json!(!scan.limit_reached));
                scan_json.insert(
                    "completeness_basis".to_owned(),
                    json!(if scan.limit_reached {
                        "component limit reached before the adjacency frontier was exhausted"
                    } else {
                        "the adjacency frontier was exhausted without reaching the component limit"
                    }),
                );
            }
            object.insert(
                "boundary".to_owned(),
                json!({
                    "component_frontier_remaining": scan.limit_reached,
                    "redstone_touches_boundary": scan.limit_reached,
                    "guidance": scan.limit_reached.then_some(
                        "raise max_components or explicitly select a smaller functional area"
                    )
                }),
            );
        }
        json_text(result)
    }

    #[tool(
        description = "Mark the first or second region corner at the block a player is looking at"
    )]
    async fn set_region_corner(&self, Parameters(params): Parameters<MarkCornerParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let observation = match self
            .bridge
            .observe_player(&player, params.max_distance.unwrap_or(64.0))
            .await
        {
            Ok(observation) => observation,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let Some(target) = observation.targeted_block else {
            return json_text(
                json!({ "ok": false, "error": "the player is not looking at a block" }),
            );
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let mut selections = self.selections.lock().await;
        let session = selections
            .entry(player.clone())
            .or_insert_with(|| SelectionSession::new(&player));
        let result = match params.corner.as_str() {
            "first" => {
                session.mark_first(target);
                self.selection_dimensions
                    .lock()
                    .await
                    .insert(player.clone(), observation.dimension.clone());
                json!({ "ok": true, "corner": "first", "position": target })
            }
            "second" => match self
                .selection_dimensions
                .lock()
                .await
                .get(&player)
                .filter(|dimension| *dimension == &observation.dimension)
            {
                None => {
                    json!({ "ok": false, "error": "player changed dimension between region corners" })
                }
                Some(_) => match session.mark_second(target) {
                    Ok(bounds) => {
                        json!({ "ok": true, "corner": "second", "position": target, "bounds": bounds_json(bounds) })
                    }
                    Err(error) => json!({ "ok": false, "error": error.to_string() }),
                },
            },
            _ => json!({ "ok": false, "error": "corner must be first or second" }),
        };
        json_text(result)
    }

    #[tool(
        description = "Infer the bounds of 'this circuit' by progressively following adjacent redstone from the block a player is looking at, stopping at the circuit edge or max_components"
    )]
    async fn resolve_looked_at_circuit(
        &self,
        Parameters(params): Parameters<DiscoverCircuitParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let max_components = params.max_components.unwrap_or(8192);
        let padding = params.padding.unwrap_or(1);
        let fragment_gap = params.fragment_gap.unwrap_or(2);
        if !(1..=32768).contains(&max_components)
            || !(0..=8).contains(&padding)
            || !(1..=8).contains(&fragment_gap)
        {
            return json_text(json!({
                "ok": false,
                "error": "max_components must be 1..32768, padding 0..8, and fragment_gap 1..8"
            }));
        }
        let observation = match self.bridge.observe_player(&player, 64.0).await {
            Ok(observation) => observation,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let Some(target) = observation.targeted_block else {
            return json_text(
                json!({ "ok": false, "error": "the player is not looking at a block" }),
            );
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let scan = match self
            .scan_connected_components(
                target,
                &observation.dimension,
                max_components,
                fragment_gap as i32,
            )
            .await
        {
            Ok(scan) => scan,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error }));
            }
        };
        let scan_bounds =
            dustroute_translate::RegionBounds::new(scan.snapshot.min, scan.snapshot.max);
        let snapshot = scan.snapshot.clone();
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(json) => json,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, world) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let analysis = dustroute_translate::analyze_world_region(&world, scan_bounds);
        let discovery = match discover_connected_region(
            &analysis,
            target,
            2,
            fragment_gap,
            padding,
            usize::MAX,
        ) {
            Ok(discovery) => discovery,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let bounds: dustroute_translate::RegionBounds = discovery.bounds.into();
        self.selections
            .lock()
            .await
            .entry(player.clone())
            .or_insert_with(|| SelectionSession::new(&player))
            .set_bounds(bounds);
        self.selection_dimensions
            .lock()
            .await
            .insert(player, observation.dimension);
        json_text(json!({
            "ok": true,
            "candidate": discovery,
            "expansion": {
                "strategy": "adjacent_component_flood_fill",
                "components_loaded": scan.component_count,
                "component_limit": scan.component_limit,
                "limit_reached": scan.limit_reached,
                "scanned_tiles": scan.scanned_tiles,
                "scanned_block_positions": scan.scanned_block_positions
            },
            "warning": scan.limit_reached.then_some(
                "the circuit exceeds the component limit; analysis is incomplete"
            ),
            "next_step": "call show_selected_region and ask the player to confirm the highlighted candidate"
        }))
    }

    #[tool(
        description = "Quickly diagnose the circuit around the player's gaze. Returns a compact health summary, typed findings, evidence, and one safe recommended next action without generating repairs or changing the world",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn test_looked_at_circuit(
        &self,
        Parameters(params): Parameters<DiagnoseLookedAtParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let discovery_text = self
            .resolve_looked_at_circuit(Parameters(DiscoverCircuitParams {
                player: Some(player.clone()),
                max_components: params.max_components,
                padding: Some(1),
                fragment_gap: Some(params.fragment_gap.unwrap_or(2)),
            }))
            .await;
        let discovery: Value = match serde_json::from_str(&discovery_text) {
            Ok(value) => value,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        if discovery.get("ok") != Some(&Value::Bool(true)) {
            return discovery_text;
        }
        let target = match serde_json::from_value::<Pos>(discovery["candidate"]["seed"].clone()) {
            Ok(target) => target,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let snapshot = match self
            .bridge
            .scan_region(bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, world) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let mut analysis = dustroute_translate::analyze_world_region(&world, bounds);
        analysis.scene.observation.dimension = dimension;
        let complete = discovery["expansion"]["limit_reached"] != Value::Bool(true);
        let diagnostic =
            dustroute_translate::diagnose_scene(&analysis.scene, Some(target), complete);
        json_text(json!({
            "ok": true,
            "schema_version": DIAGNOSTIC_SCHEMA_V1,
            "analysis_mode": "focused_fast",
            "mutation_performed": false,
            "target": target,
            "bounds": bounds_json(bounds),
            "expansion": discovery["expansion"],
            "diagnostic": diagnostic,
            "detail_tools": {
                "full_conversion": "convert_from_looked_at_circuit",
                "raw_observation": "get_looked_at_world",
                "repair_planning": "new_repair_plan"
            }
        }))
    }

    #[tool(
        description = "Start the complete gaze-grounded workflow for questions such as 'what is this?': discover the connected circuit, explain physical/local/higher roles, and return non-mutating transition scenarios and repair proposals",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn convert_from_looked_at_circuit(
        &self,
        Parameters(params): Parameters<AnalyzeLookedAtParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let fragment_gap = params.fragment_gap.unwrap_or(2);
        let discovery_text = self
            .resolve_looked_at_circuit(Parameters(DiscoverCircuitParams {
                player: Some(player.clone()),
                max_components: params.max_components,
                padding: Some(1),
                fragment_gap: Some(fragment_gap),
            }))
            .await;
        let discovery: Value = match serde_json::from_str(&discovery_text) {
            Ok(value) => value,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        if discovery.get("ok") != Some(&Value::Bool(true)) {
            return discovery_text;
        }
        let target = match serde_json::from_value::<Pos>(discovery["candidate"]["seed"].clone()) {
            Ok(target) => target,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let snapshot = match self
            .bridge
            .scan_region(bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, world) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let discovered_components = discovery["expansion"]["components_loaded"]
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .unwrap_or(0);
        let (transition_safety, transition_proposals) = self
            .create_transition_proposals(&player, &dimension, bounds, &snapshot, 20, 16_384)
            .await;
        if discovered_components > MAX_FLAT_ANALYSIS_COMPONENTS {
            let mut analysis = dustroute_translate::analyze_world_region(&world, bounds);
            analysis.scene.observation.dimension = dimension;
            let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
            let focused = focused_hierarchy_role_json(&analysis.scene, &hierarchy, target);
            let repairs = dustroute_translate::propose_scene_repairs_near(
                &world,
                &analysis.scene,
                fragment_gap,
                target,
                12,
            );
            let mut repair_json = Vec::new();
            for proposal in repairs.into_iter().take(16) {
                let operation_id = uuid::Uuid::new_v4();
                if let Err(error) = self
                    .store_repair_plan(
                        operation_id,
                        StoredRepairPlan {
                            patch: proposal.patch.clone(),
                            dimension: analysis.scene.observation.dimension.clone(),
                            analysis_bounds: bounds,
                            fragments_before: analysis.scene.fragments.len(),
                            baseline_truth_table: None,
                            previewed: false,
                            applied: false,
                        },
                    )
                    .await
                {
                    return json_text(json!({ "ok": false, "error": error }));
                }
                repair_json.push(json!({
                    "operation_id": operation_id,
                    "patch": proposal.patch,
                    "evidence": proposal.evidence,
                    "virtual_impact": proposal.impact
                }));
            }
            let mut result = hierarchical_result_json(
                bounds,
                &hierarchy,
                focused,
                &discovery["expansion"],
                Some(target),
            );
            if let Some(object) = result.as_object_mut() {
                object.insert(
                    "diagnostic".to_owned(),
                    serde_json::to_value(dustroute_translate::diagnose_scene(
                        &analysis.scene,
                        Some(target),
                        discovery["expansion"]["limit_reached"] != Value::Bool(true),
                    ))
                    .unwrap_or(Value::Null),
                );
                object.insert("repair_proposals".to_owned(), Value::Array(repair_json));
                object.insert(
                    "repair_guidance".to_owned(),
                    Value::String(
                        "ranked local virtual patches; preview an operation before any world mutation"
                            .to_owned(),
                    ),
                );
                object.insert(
                    "transition_scenarios".to_owned(),
                    json!({
                        "safety": transition_safety,
                        "proposals": transition_proposals,
                        "next_step": "show_transition_test before requesting confirmation"
                    }),
                );
            }
            return json_text(result);
        }
        let mut request = ReverseRequest::new(bounds);
        if params.include_truth_table.unwrap_or(false) {
            request = request.with_truth_table(16);
        }
        let mut staged = self.app.analyze_physical(&world, request);
        staged.reverse.analysis.scene.observation.dimension = dimension.clone();
        staged
            .hierarchy
            .physical_snapshot
            .value
            .scene
            .observation
            .dimension = dimension.clone();
        staged
            .hierarchy
            .physical_graph
            .value
            .scene
            .observation
            .dimension = dimension.clone();
        let translated = &staged.reverse;
        let focused = focused_role_json(translated, target);
        let repairs = dustroute_translate::propose_scene_repairs_near(
            &world,
            &translated.analysis.scene,
            fragment_gap,
            target,
            12,
        );
        let mut repair_json = Vec::new();
        for proposal in repairs.into_iter().take(16) {
            let operation_id = uuid::Uuid::new_v4();
            if let Err(error) = self
                .store_repair_plan(
                    operation_id,
                    StoredRepairPlan {
                        patch: proposal.patch.clone(),
                        dimension: dimension.clone(),
                        analysis_bounds: bounds,
                        fragments_before: translated.analysis.scene.fragments.len(),
                        baseline_truth_table: translated.truth_table.clone(),
                        previewed: false,
                        applied: false,
                    },
                )
                .await
            {
                return json_text(json!({ "ok": false, "error": error }));
            }
            repair_json.push(json!({
                "operation_id": operation_id,
                "patch": proposal.patch,
                "evidence": proposal.evidence,
                "virtual_impact": proposal.impact
            }));
        }
        let incomplete = discovery["expansion"]["limit_reached"] == Value::Bool(true);
        let mut result = reverse_result_json(bounds, translated);
        if let Some(object) = result.as_object_mut() {
            object.insert(
                "diagnostic".to_owned(),
                serde_json::to_value(dustroute_translate::diagnose_scene(
                    &translated.analysis.scene,
                    Some(target),
                    !incomplete,
                ))
                .unwrap_or(Value::Null),
            );
            object.insert("focused_component".to_owned(), focused);
            object.insert("discovery".to_owned(), discovery["candidate"].clone());
            object.insert("analysis_complete".to_owned(), Value::Bool(!incomplete));
            object.insert("repair_proposals".to_owned(), Value::Array(repair_json));
            object.insert(
                "transition_scenarios".to_owned(),
                json!({
                    "safety": transition_safety,
                    "proposals": transition_proposals,
                    "next_step": "show_transition_test before requesting confirmation"
                }),
            );
            object.insert(
                "interpretation_guidance".to_owned(),
                Value::String(if incomplete {
                    "Treat the logical classification as provisional because the connected circuit continues beyond the scan boundary. Explain the local role, then ask the user to select or isolate a larger functional region before applying a repair."
                } else {
                    "Explain the focused component in the context of the inferred logical function. Repairs are proposals only; preview one and obtain confirmation before mutation."
                }.to_owned()),
            );
        }
        json_text(result)
    }

    #[tool(
        description = "Compile a built-in circuit at a player's gaze target and return a block diff, collisions, materials, operation ID, and exact undo plan without changing the world"
    )]
    async fn new_circuit_placement(
        &self,
        Parameters(params): Parameters<PreviewPlacementParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let observation = match self.bridge.observe_player(&player, 64.0).await {
            Ok(observation) => observation,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let Some(origin) = observation.targeted_block else {
            return json_text(
                json!({ "ok": false, "error": "the player is not looking at a block" }),
            );
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let translated = match self
            .app
            .compile_builtin(&params.circuit, ForwardOptions::default())
        {
            Ok(Some(result)) => result,
            Ok(None) => {
                return json_text(json!({
                    "ok": false,
                    "error": "unknown circuit; expected half-adder, half-subtractor, mux2, decoder1to2, or full-adder"
                }));
            }
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (proposed_world, optimization) = if params.optimize.unwrap_or(false) {
            let optimization_plan = OptimizationPlan::directional_then_global(
                CompressionAxis::X,
                CompressionDirection::TowardMinimum,
                AnchorPolicy::Inputs,
            );
            let realized = match realize_staged_optimization(
                &translated.compiled.physical,
                &optimization_plan,
                OptimizationRoutingConfig::default(),
            ) {
                Ok(realized) => realized,
                Err(error) => {
                    return json_text(json!({
                        "ok": false,
                        "error": format!("placement optimization failed: {error}")
                    }));
                }
            };
            let verification = verify_realized_optimization(
                &translated.compiled.world,
                &translated.compiled.physical,
                &realized,
                BehavioralVerificationConfig::default(),
            );
            let safety = assess_optimization_safety(&verification, TemporalCapabilities::current());
            let safety_label = match &safety {
                OptimizationSafety::Verified { .. } => "verified",
                OptimizationSafety::PreviewOnly { .. } => "preview_only",
                OptimizationSafety::Rejected { .. } => {
                    return json_text(json!({
                        "ok": false,
                        "error": format!("optimized placement was rejected: {safety:?}"),
                        "optimization": {
                            "safety": "rejected",
                            "topology_preserved": verification.topology_preserved,
                            "behavior": format!("{:?}", verification.behavior)
                        }
                    }));
                }
            };
            let phases = realized
                .optimization
                .phases
                .iter()
                .map(|phase| {
                    json!({
                        "accepted_mutations": phase.accepted.len(),
                        "initial_score": phase.initial_score.total,
                        "final_score": phase.final_score.total
                    })
                })
                .collect::<Vec<_>>();
            (
                realized.world,
                Some(json!({
                    "strategy": "directional_x_toward_minimum_then_global",
                    "safety": safety_label,
                    "safety_details": format!("{safety:?}"),
                    "topology_preserved": verification.topology_preserved,
                    "phases": phases
                })),
            )
        } else {
            (translated.compiled.world.clone(), None)
        };
        let Some((local_min, local_max)) = proposed_world.bounds() else {
            return json_text(json!({ "ok": false, "error": "compiled circuit is empty" }));
        };
        let min = Pos::new(
            local_min.x + origin.x,
            local_min.y + origin.y,
            local_min.z + origin.z,
        );
        let max = Pos::new(
            local_max.x + origin.x,
            local_max.y + origin.y,
            local_max.z + origin.z,
        );
        let placement_bounds = dustroute_translate::RegionBounds::new(min, max);
        if let Err(error) = self.policy.validate_region(placement_bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let proposed_blocks = proposed_world.iter().count();
        if let Err(error) = self.policy.validate_placement_size(proposed_blocks) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let snapshot = match self
            .bridge
            .scan_region(min, max, &observation.dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(json) => json,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, existing) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let plan = match plan_world_overlay(
            &existing,
            &proposed_world,
            origin,
            params
                .max_blocks
                .unwrap_or(self.policy.max_placement_blocks)
                .min(self.policy.max_placement_blocks),
        ) {
            Ok(plan) => plan,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let operation_id = plan.operation_id;
        let collision_samples = plan
            .changes
            .iter()
            .filter(|change| change.collision)
            .take(32)
            .map(|change| change.pos)
            .collect::<Vec<_>>();
        let response = json!({
            "ok": true,
            "read_only": self.policy.read_only,
            "operation_id": operation_id,
            "origin": origin,
            "bounds": { "min": min, "max": max },
            "changed_blocks": plan.changes.len(),
            "collision_count": plan.collision_count,
            "collision_samples": collision_samples,
            "materials": &plan.materials,
            "undo_change_count": plan.undo.changes.len(),
            "optimization": optimization,
            "next_step": if self.policy.read_only {
                "review this plan; writes are disabled by policy"
            } else {
                "review this plan, obtain explicit player confirmation, then call invoke_circuit_placement with confirm=true"
            }
        });
        self.plans.lock().await.insert(operation_id, plan);
        self.plan_dimensions
            .lock()
            .await
            .insert(operation_id, observation.dimension);
        self.operations
            .record_completed(
                operation_id,
                OperationKind::PlacementPreview,
                response.clone(),
            )
            .await;
        json_text(response)
    }

    #[tool(
        description = "Retrieve the complete placement and exact undo plan by operation ID",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_circuit_placement(
        &self,
        Parameters(params): Parameters<OperationParams>,
    ) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        match self.plans.lock().await.get(&operation_id) {
            Some(plan) => {
                json_text(json!({ "ok": true, "read_only": self.policy.read_only, "plan": plan }))
            }
            None => json_text(json!({ "ok": false, "error": "unknown operation ID" })),
        }
    }

    #[tool(
        description = "Apply a previously previewed placement plan to the test world. Requires confirm=true and write-enabled policy.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn invoke_circuit_placement(
        &self,
        Parameters(params): Parameters<ConfirmedOperationParams>,
    ) -> String {
        self.mutate_placement(params, false).await
    }

    #[tool(
        description = "Restore the exact blocks captured before an applied placement plan. Requires confirm=true and write-enabled policy.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn undo_circuit_placement(
        &self,
        Parameters(params): Parameters<ConfirmedOperationParams>,
    ) -> String {
        self.mutate_placement(params, true).await
    }

    #[tool(
        description = "Show the player's selected region in the Minecraft world before analysis or mutation"
    )]
    async fn show_selected_region(&self, Parameters(params): Parameters<PlayerParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        match self
            .bridge
            .preview_region(&player, bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(preview) => {
                json_text(json!({ "ok": true, "bounds": bounds_json(bounds), "preview": preview }))
            }
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    #[tool(
        description = "Scan and reverse-translate the region previously selected with the player's gaze"
    )]
    async fn convert_from_selected_region(
        &self,
        Parameters(params): Parameters<PlayerParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let snapshot = match self
            .bridge
            .scan_region(bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        let redstone_components = snapshot
            .blocks
            .iter()
            .filter(|block| is_redstone_candidate_name(&block.name))
            .count();
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(json) => json,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, world) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        if redstone_components > MAX_FLAT_ANALYSIS_COMPONENTS {
            let mut analysis = dustroute_translate::analyze_world_region(&world, bounds);
            analysis.scene.observation.dimension = dimension;
            let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
            return json_text(hierarchical_result_json(
                bounds,
                &hierarchy,
                Value::Null,
                &json!({
                    "strategy": "explicit_selected_region",
                    "components_loaded": redstone_components,
                    "component_limit": null,
                    "limit_reached": false
                }),
                None,
            ));
        }
        let mut staged = self
            .app
            .analyze_physical(&world, ReverseRequest::new(bounds));
        staged.reverse.analysis.scene.observation.dimension = dimension;
        json_text(reverse_result_json(bounds, &staged.reverse))
    }

    #[tool(
        description = "Diagnose the selected physical circuit and create ranked, non-mutating partial repair plans"
    )]
    async fn new_repair_plan(
        &self,
        Parameters(params): Parameters<ProposeRepairsParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let max_gap = params.max_gap.unwrap_or(2);
        if !(1..=8).contains(&max_gap) {
            return json_text(json!({ "ok": false, "error": "max_gap must be 1..8" }));
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let snapshot = match self
            .bridge
            .scan_region(bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, world) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let analysis = dustroute_translate::analyze_world_region(&world, bounds);
        let fragments_before = analysis.scene.fragments.len();
        let proposals =
            dustroute_translate::propose_scene_repairs(&world, &analysis.scene, max_gap);
        let mut response = Vec::new();
        for proposal in proposals.into_iter().take(32) {
            let operation_id = uuid::Uuid::new_v4();
            if let Err(error) = self
                .store_repair_plan(
                    operation_id,
                    StoredRepairPlan {
                        patch: proposal.patch.clone(),
                        dimension: dimension.clone(),
                        analysis_bounds: bounds,
                        fragments_before,
                        baseline_truth_table: None,
                        previewed: false,
                        applied: false,
                    },
                )
                .await
            {
                return json_text(json!({ "ok": false, "error": error }));
            }
            self.operations
                .record_completed(
                    operation_id,
                    OperationKind::RepairProposal,
                    json!({ "patch": &proposal.patch, "evidence": &proposal.evidence }),
                )
                .await;
            response.push(json!({
                "operation_id": operation_id,
                "patch": proposal.patch,
                "evidence": proposal.evidence,
            }));
        }
        json_text(json!({
            "schema_version": REPAIR_SCHEMA_V1,
            "ok": true,
            "bounds": bounds_json(bounds),
            "fragments": fragments_before,
            "proposal_count": response.len(),
            "proposals": response,
            "next_step": "review a proposal, call show_repair_plan, ask for explicit confirmation, then call invoke_repair with confirm=true"
        }))
    }

    #[tool(
        description = "Create a low-confidence removal repair for the redstone component the player is looking at. Use only when the player explicitly identifies it as an unwanted connection."
    )]
    async fn new_component_removal_plan(
        &self,
        Parameters(params): Parameters<PlayerParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let observation = match self.bridge.observe_player(&player, 64.0).await {
            Ok(observation) => observation,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let Some(target) = observation.targeted_block else {
            return json_text(json!({ "ok": false, "error": "player is not looking at a block" }));
        };
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if !bounds.contains(target) {
            return json_text(
                json!({ "ok": false, "error": "target is outside the selected region" }),
            );
        }
        let snapshot = match self
            .bridge
            .scan_region(bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, world) = match world_from_snapshot_json(&snapshot_json) {
            Ok(result) => result,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let analysis = dustroute_translate::analyze_world_region(&world, bounds);
        let Some(proposal) =
            dustroute_translate::propose_scene_component_removal(&world, &analysis.scene, target)
        else {
            return json_text(
                json!({ "ok": false, "error": "target is not a removable redstone component" }),
            );
        };
        let operation_id = uuid::Uuid::new_v4();
        if let Err(error) = self
            .store_repair_plan(
                operation_id,
                StoredRepairPlan {
                    patch: proposal.patch.clone(),
                    dimension,
                    analysis_bounds: bounds,
                    fragments_before: analysis.scene.fragments.len(),
                    baseline_truth_table: None,
                    previewed: false,
                    applied: false,
                },
            )
            .await
        {
            return json_text(json!({ "ok": false, "error": error }));
        }
        json_text(json!({
            "schema_version": REPAIR_SCHEMA_V1,
            "ok": true,
            "operation_id": operation_id,
            "proposal": proposal,
            "warning": "removal intent cannot be inferred from geometry alone; preview and explicit confirmation are required",
            "next_step": "call show_repair_plan, then invoke_repair with confirm=true only after confirmation"
        }))
    }

    #[tool(description = "Highlight the blocks affected by a proposed partial repair")]
    async fn show_repair_plan(
        &self,
        Parameters(params): Parameters<PreviewRepairParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let plan = match self.repair_plan(operation_id).await {
            Ok(Some(plan)) => plan,
            Ok(None) => {
                return json_text(json!({ "ok": false, "error": "unknown or expired repair ID" }));
            }
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let Some(bounds) = bounds_for_changes(&plan.patch.changes) else {
            return json_text(json!({ "ok": false, "error": "repair has no changes" }));
        };
        match self
            .bridge
            .preview_region(&player, bounds.min, bounds.max, &plan.dimension)
            .await
        {
            Ok(preview) => {
                let mut previewed = plan.clone();
                previewed.previewed = true;
                if let Err(error) = self.store_repair_plan(operation_id, previewed).await {
                    return json_text(json!({ "ok": false, "error": error }));
                }
                json_text(json!({
                    "schema_version": REPAIR_SCHEMA_V1,
                    "ok": true,
                    "operation_id": operation_id,
                    "bounds": bounds_json(bounds),
                    "patch": plan.patch,
                    "preview": preview,
                    "next_step": "obtain explicit player confirmation before invoke_repair"
                }))
            }
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    #[tool(
        description = "Apply a previewed partial repair and verify the resulting blocks. Requires confirm=true.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn invoke_repair(
        &self,
        Parameters(params): Parameters<ConfirmedOperationParams>,
    ) -> String {
        self.mutate_repair(params, false).await
    }

    #[tool(
        description = "Undo an applied partial repair using its exact captured before-state. Requires confirm=true.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn undo_repair(
        &self,
        Parameters(params): Parameters<ConfirmedOperationParams>,
    ) -> String {
        self.mutate_repair(params, true).await
    }

    #[tool(
        description = "Discover single-lever transition scenarios around the player's gaze without changing the world",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn new_transition_test(
        &self,
        Parameters(params): Parameters<ProposeTransitionParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let observation_ticks = params.observation_ticks.unwrap_or(20);
        let max_events = params.max_events.unwrap_or(16_384);
        if !(1..=200).contains(&observation_ticks) || !(1..=65_536).contains(&max_events) {
            return error_text(
                McpErrorCode::InvalidArgument,
                "observation_ticks must be 1..200 and max_events must be 1..65536",
                false,
            );
        }
        let discovery_text = self
            .resolve_looked_at_circuit(Parameters(DiscoverCircuitParams {
                player: Some(player.clone()),
                max_components: params.max_components,
                padding: Some(1),
                fragment_gap: Some(2),
            }))
            .await;
        let discovery: Value = match serde_json::from_str(&discovery_text) {
            Ok(value) => value,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        if discovery.get("ok") != Some(&Value::Bool(true)) {
            return discovery_text;
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let snapshot = match self
            .bridge
            .scan_region(bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (safety, proposals) = self
            .create_transition_proposals(
                &player,
                &dimension,
                bounds,
                &snapshot,
                observation_ticks,
                max_events,
            )
            .await;
        if safety.safety == TransitionSafety::Rejected {
            return json_text(json!({
                "ok": false,
                "safety": safety,
                "error": "the observed region is not eligible for an automatic transition scenario"
            }));
        }
        json_text(json!({
            "schema_version": TRANSITION_SCHEMA_V1,
            "ok": true,
            "bounds": bounds_json(bounds),
            "dimension": dimension,
            "proposals": proposals,
            "next_step": "show_transition_test, then invoke_transition_test(confirm=true) only for a ready proposal"
        }))
    }

    #[tool(
        description = "Highlight a proposed lever transition and observation region without activating it",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn show_transition_test(
        &self,
        Parameters(params): Parameters<PreviewTransitionParams>,
    ) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let mut plans = self.transition_plans.lock().await;
        let Some(plan) = plans.get_mut(&operation_id) else {
            return error_text(
                McpErrorCode::NotFound,
                "transition scenario not found",
                false,
            );
        };
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if player != plan.player {
            return json_text(
                json!({ "ok": false, "error": "scenario belongs to another player" }),
            );
        }
        let preview = match self
            .bridge
            .preview_region(&player, plan.bounds.min, plan.bounds.max, &plan.dimension)
            .await
        {
            Ok(preview) => preview,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        plan.previewed = true;
        json_text(json!({
            "schema_version": TRANSITION_SCHEMA_V1,
            "ok": true,
            "operation_id": operation_id,
            "lever": plan.lever,
            "original_powered": plan.original_powered,
            "safety": plan.safety,
            "preview": preview,
            "warning": "running moves the bot within reach when necessary, normally activates this lever once, and restores it after observation"
        }))
    }

    #[tool(
        description = "Run a previewed single-lever transition, record block updates, analyze transients, and restore the original state",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn invoke_transition_test(
        &self,
        Parameters(params): Parameters<RunTransitionParams>,
    ) -> String {
        if !params.confirm {
            return error_text(
                McpErrorCode::InvalidArgument,
                "confirm=true is required",
                false,
            );
        }
        if let Err(error) = self.policy.authorize_mutation() {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let _mutation_guard = self.mutation_lock.lock().await;
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let plan = match self
            .transition_plans
            .lock()
            .await
            .get(&operation_id)
            .cloned()
        {
            Some(plan) => plan,
            None => {
                return error_text(
                    McpErrorCode::NotFound,
                    "transition scenario not found",
                    false,
                );
            }
        };
        if self.policy.preview_required && !plan.previewed {
            return error_text(
                McpErrorCode::InvalidState,
                "show_transition_test is required first",
                false,
            );
        }
        if plan.executed {
            return json_text(json!({
                "ok": false,
                "error": "scenario was already executed; create and preview a new scenario"
            }));
        }
        if plan.safety.safety != TransitionSafety::Ready {
            return json_text(json!({
                "ok": false,
                "error": "scenario is preview-only because the region contains temporal or unsupported devices",
                "safety": plan.safety
            }));
        }
        let current = match self.bridge.get_block(plan.lever, &plan.dimension).await {
            Ok(block) => block,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let current_powered = current
            .state
            .properties
            .get("powered")
            .and_then(|value| value.parse::<bool>().ok());
        if current.state.name != "minecraft:lever" || current_powered != Some(plan.original_powered)
        {
            return json_text(json!({
                "ok": false,
                "error": "lever state changed since proposal; create a new scenario"
            }));
        }
        let snapshot_json = match serde_json::to_string(&plan.initial_snapshot) {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (_, world) = match world_from_snapshot_json(&snapshot_json) {
            Ok(value) => value,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let mut analysis = dustroute_translate::analyze_world_region(&world, plan.bounds);
        analysis.scene.observation.dimension = plan.dimension.clone();
        let contracts = match transition_contracts(params.contracts.as_deref(), &analysis.scene) {
            Ok(contracts) => contracts,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let approach = match self
            .bridge
            .approach_lever(plan.lever, &plan.dimension)
            .await
        {
            Ok(approach) => approach,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let started = match self
            .bridge
            .start_update_recording(
                plan.bounds.min,
                plan.bounds.max,
                &plan.dimension,
                plan.max_events,
            )
            .await
        {
            Ok(started) => started,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let mut activation = match self
            .bridge
            .activate_lever(plan.lever, &plan.dimension)
            .await
        {
            Ok(activation) => activation,
            Err(error) => {
                let _ = self
                    .bridge
                    .stop_update_recording(&started.recording_id, &plan.dimension)
                    .await;
                return json_text(json!({ "ok": false, "error": error.to_string() }));
            }
        };
        activation.bot_approached |= approach.moved;
        let wait_error = self
            .bridge
            .wait_ticks(plan.observation_ticks, &plan.dimension)
            .await
            .err()
            .map(|error| error.to_string());
        let recording_result = self
            .bridge
            .stop_update_recording(&started.recording_id, &plan.dimension)
            .await;

        let restore_error = self
            .bridge
            .activate_lever(plan.lever, &plan.dimension)
            .await
            .err()
            .map(|error| error.to_string());
        let _ = self
            .bridge
            .wait_ticks(plan.observation_ticks, &plan.dimension)
            .await;
        let restored_block = self.bridge.get_block(plan.lever, &plan.dimension).await;
        let restored_snapshot = self
            .bridge
            .scan_region(plan.bounds.min, plan.bounds.max, &plan.dimension)
            .await;
        let lever_restored = restored_block.as_ref().ok().is_some_and(|block| {
            block.state.name == "minecraft:lever"
                && block
                    .state
                    .properties
                    .get("powered")
                    .and_then(|value| value.parse::<bool>().ok())
                    == Some(plan.original_powered)
        });
        let region_restored = restored_snapshot
            .as_ref()
            .is_ok_and(|snapshot| snapshot == &plan.initial_snapshot);
        let restoration_verified = restore_error.is_none() && lever_restored && region_restored;
        if let Some(stored) = self.transition_plans.lock().await.get_mut(&operation_id) {
            stored.executed = true;
            stored.restoration_verified = restoration_verified;
        }
        let recording = match recording_result {
            Ok(recording) => recording,
            Err(error) => {
                return json_text(json!({
                    "ok": false,
                    "error": error.to_string(),
                    "restoration_verified": restoration_verified,
                    "restore_error": restore_error
                }));
            }
        };
        let trace = behavior_trace_from_recording(
            &recording,
            &analysis.scene,
            format!(
                "lever {} -> {}",
                activation.before_powered, activation.after_powered
            ),
        );
        let transient = dustroute_ir::assess_transients(&trace, &contracts);
        let observe = plan
            .initial_snapshot
            .blocks
            .iter()
            .filter(|block| is_redstone_candidate_name(&block.name))
            .map(|block| block.pos)
            .collect::<BTreeSet<_>>();
        let duration_redstone_ticks = u64::from(plan.observation_ticks).div_ceil(2);
        let scenario = dustroute_translate::Scenario {
            label: format!("lever transition at {:?}", plan.lever),
            initial: plan.initial_snapshot.clone(),
            actions: vec![dustroute_translate::ScenarioAction::SetPowered {
                redstone_tick: 0,
                position: plan.lever,
                powered: !plan.original_powered,
            }],
            observe: observe.clone(),
            duration_redstone_ticks,
            required_capabilities: Vec::new(),
            expectation: dustroute_translate::ScenarioExpectation::default(),
        };
        let simulated = dustroute_translate::simulate_scenario(&scenario);
        let live_scenario_trace = scenario_trace_from_recording_with_initial(
            &recording,
            &observe,
            duration_redstone_ticks,
            Some(&plan.initial_snapshot),
        );
        let simulation_comparison = simulated.as_ref().ok().map(|simulated| {
            dustroute_translate::compare_live_trace(&simulated.trace, &live_scenario_trace)
        });
        let steady_state_equivalent = simulated.as_ref().is_ok_and(|simulated| {
            simulated.trace.final_strengths == live_scenario_trace.final_strengths
                && simulated.trace.final_powered == live_scenario_trace.final_powered
        });
        let scenario = match serde_json::to_value(&scenario) {
            Ok(value) => value,
            Err(error) => {
                return json_text(json!({
                    "ok": false,
                    "error": format!("failed to serialize transition scenario: {error}"),
                    "restoration_verified": restoration_verified,
                    "restore_error": restore_error,
                }));
            }
        };
        let simulated = match &simulated {
            Ok(run) => json!({ "ok": true, "run": scenario_run_json(run) }),
            Err(error) => json!({ "ok": false, "error": error }),
        };
        let live_scenario_trace = scenario_trace_json(&live_scenario_trace);
        let result = json!({
            "schema_version": TRANSITION_SCHEMA_V1,
            "ok": restoration_verified && wait_error.is_none() && !recording.truncated,
            "operation_id": operation_id,
            "activation": activation,
            "observation_ticks": plan.observation_ticks,
            "recording": {
                "started_game_tick": recording.started_game_tick,
                "stopped_game_tick": recording.stopped_game_tick,
                "seen_events": recording.seen_events,
                "stored_events": recording.events.len(),
                "truncated": recording.truncated
            },
            "trace": trace,
            "transient_assessment": transient,
            "scenario_verification": {
                "scenario": scenario,
                "simulated": simulated,
                "live_trace": live_scenario_trace,
                "differences": simulation_comparison,
                "trace_equivalent": simulation_comparison.as_ref().is_some_and(Vec::is_empty),
                "steady_state_equivalent": steady_state_equivalent
            },
            "restoration": {
                "lever_restored": lever_restored,
                "region_restored": region_restored,
                "verified": restoration_verified,
                "activation_error": restore_error,
            },
            "wait_error": wait_error,
            "guidance": "hazard_candidate is an observed pulse without registered intent; register a signal contract before calling it a confirmed hazard"
        });
        self.operations
            .record_completed(operation_id, OperationKind::TransitionRun, result.clone())
            .await;
        json_text(result)
    }

    #[tool(
        description = "Attempt to restore a transition scenario lever and verify its original region snapshot",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn restore_transition_test(
        &self,
        Parameters(params): Parameters<RunTransitionParams>,
    ) -> String {
        if !params.confirm {
            return error_text(
                McpErrorCode::InvalidArgument,
                "confirm=true is required",
                false,
            );
        }
        if let Err(error) = self.policy.authorize_mutation() {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let _mutation_guard = self.mutation_lock.lock().await;
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let plan = match self
            .transition_plans
            .lock()
            .await
            .get(&operation_id)
            .cloned()
        {
            Some(plan) => plan,
            None => {
                return error_text(
                    McpErrorCode::NotFound,
                    "transition scenario not found",
                    false,
                );
            }
        };
        let current = match self.bridge.get_block(plan.lever, &plan.dimension).await {
            Ok(block) => block,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let powered = current
            .state
            .properties
            .get("powered")
            .and_then(|value| value.parse::<bool>().ok());
        let activation_error = if powered != Some(plan.original_powered) {
            self.bridge
                .activate_lever(plan.lever, &plan.dimension)
                .await
                .err()
                .map(|error| error.to_string())
        } else {
            None
        };
        let _ = self
            .bridge
            .wait_ticks(plan.observation_ticks, &plan.dimension)
            .await;
        let block = self.bridge.get_block(plan.lever, &plan.dimension).await;
        let snapshot = self
            .bridge
            .scan_region(plan.bounds.min, plan.bounds.max, &plan.dimension)
            .await;
        let verified = activation_error.is_none()
            && block.as_ref().ok().is_some_and(|block| {
                block
                    .state
                    .properties
                    .get("powered")
                    .and_then(|value| value.parse::<bool>().ok())
                    == Some(plan.original_powered)
            })
            && snapshot
                .as_ref()
                .is_ok_and(|snapshot| snapshot == &plan.initial_snapshot);
        if let Some(stored) = self.transition_plans.lock().await.get_mut(&operation_id) {
            stored.restoration_verified = verified;
        }
        let result = json!({
            "schema_version": TRANSITION_SCHEMA_V1,
            "ok": verified,
            "operation_id": operation_id,
            "restoration_verified": verified,
            "activation_error": activation_error
        });
        self.operations
            .record_completed(
                operation_id,
                OperationKind::TransitionRestore,
                result.clone(),
            )
            .await;
        json_text(result)
    }

    #[tool(
        description = "Start cancellable reverse analysis of the selected region and return an operation ID for progress polling"
    )]
    async fn start_selected_region_conversion(
        &self,
        Parameters(params): Parameters<PlayerParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let operation_id = self
            .operations
            .create(OperationKind::AnalyzeRegion, "queued for snapshot scan")
            .await;
        let operations = self.operations.clone();
        let bridge = self.bridge.clone();
        let app = self.app;
        tokio::spawn(async move {
            operations
                .update(
                    operation_id,
                    OperationStatus::Running,
                    10,
                    "scanning region",
                )
                .await;
            let snapshot = match bridge.scan_region(bounds.min, bounds.max, &dimension).await {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    operations.fail(operation_id, error.to_string()).await;
                    return;
                }
            };
            if operations.is_cancelled(operation_id).await {
                return;
            }
            let redstone_components = snapshot
                .blocks
                .iter()
                .filter(|block| is_redstone_candidate_name(&block.name))
                .count();
            operations
                .update(
                    operation_id,
                    OperationStatus::Running,
                    35,
                    "normalizing Minecraft snapshot",
                )
                .await;
            let snapshot_json = match serde_json::to_string(&snapshot) {
                Ok(json) => json,
                Err(error) => {
                    operations.fail(operation_id, error.to_string()).await;
                    return;
                }
            };
            let (_, world) = match world_from_snapshot_json(&snapshot_json) {
                Ok(result) => result,
                Err(error) => {
                    operations.fail(operation_id, error.to_string()).await;
                    return;
                }
            };
            operations
                .update(
                    operation_id,
                    OperationStatus::Running,
                    50,
                    if redstone_components > MAX_FLAT_ANALYSIS_COMPONENTS {
                        "deriving hierarchical circuit views"
                    } else {
                        "analyzing selected circuit"
                    },
                )
                .await;
            let result = match tokio::task::spawn_blocking(move || {
                if redstone_components > MAX_FLAT_ANALYSIS_COMPONENTS {
                    let mut analysis = dustroute_translate::analyze_world_region(&world, bounds);
                    analysis.scene.observation.dimension = dimension;
                    let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
                    hierarchical_result_json(
                        bounds,
                        &hierarchy,
                        Value::Null,
                        &json!({
                            "strategy": "explicit_selected_region",
                            "components_loaded": redstone_components,
                            "component_limit": null,
                            "limit_reached": false
                        }),
                        None,
                    )
                } else {
                    let mut staged = app.analyze_physical(&world, ReverseRequest::new(bounds));
                    staged.reverse.analysis.scene.observation.dimension = dimension;
                    reverse_result_json(bounds, &staged.reverse)
                }
            })
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    operations.fail(operation_id, error.to_string()).await;
                    return;
                }
            };
            if operations.is_cancelled(operation_id).await {
                return;
            }
            operations.complete(operation_id, result).await;
        });
        json_text(json!({
            "ok": true,
            "operation_id": operation_id,
            "status": "queued",
            "next_step": "poll get_operation; call stop_operation if the conversion is no longer needed"
        }))
    }

    #[tool(
        description = "Get progress and result for a long-running DustRoute operation",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_operation(&self, Parameters(params): Parameters<OperationParams>) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        match self.operations.get(operation_id).await {
            Some(operation) => json_text(json!({ "ok": true, "operation": operation })),
            None => json_text(json!({ "ok": false, "error": "unknown operation ID" })),
        }
    }

    #[tool(description = "Cancel a queued or running DustRoute operation")]
    async fn stop_operation(&self, Parameters(params): Parameters<OperationParams>) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let cancelled = self.operations.cancel(operation_id).await;
        json_text(json!({ "ok": cancelled, "operation_id": operation_id }))
    }

    #[tool(description = "Clear a player's pending gaze-based region selection")]
    async fn clear_region_selection(&self, Parameters(params): Parameters<PlayerParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        if let Some(session) = self.selections.lock().await.get_mut(&player) {
            session.clear();
        }
        self.selection_dimensions.lock().await.remove(&player);
        json_text(json!({ "ok": true, "player": player }))
    }
}

#[prompt_router]
impl DustRouteMcp {
    #[prompt(
        name = "collaborate-on-redstone-circuit",
        description = "Ground natural-language references in a player's gaze and safely inspect a redstone circuit"
    )]
    async fn collaboration_prompt(&self) -> GetPromptResult {
        GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            "Work with the player on a Minecraft redstone circuit. Follow the DustRoute PowerShell-style Verb-Noun contract expressed as snake_case. Use get_looked_at_world for literal visibility, test_looked_at_circuit for compact circuit health, and convert_from_looked_at_circuit only when higher-level logic or plans are needed. Treat inferred external inputs as informational, not automatic repair evidence. New operations only create plans; show operations render previews; invoke operations change the world and require explicit confirmation; undo or restore operations recover prior state. For a transition, call show_transition_test, explain exactly which lever and region will be touched, obtain explicit confirmation, then call invoke_transition_test with confirm=true; report both the live trace and Rust-simulator differences, and verify restoration. For a repair, call show_repair_plan, explain the block diff, obtain explicit confirmation, call invoke_repair with confirm=true, and report the post-repair reanalysis. If observation_complete is false, expand observation before higher-level claims. For an explicitly selected region, use two set_region_corner calls, show_selected_region, and convert_from_selected_region. Never infer coordinates from prose when gaze tools can ground them, and never mutate the world without preview and explicit confirmation.".to_owned(),
        )])
        .with_description("Safe gaze-grounded DustRoute collaboration workflow")
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for DustRouteMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new(
            "dustroute-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Use the collaborate-on-redstone-circuit prompt. Use get_looked_at_world for raw visibility, test_looked_at_circuit for compact health, and convert_from_looked_at_circuit for deeper logic or planning. The naming contract is get/resolve/test/convert_from/new/show/invoke/undo/restore/start/stop/clear. New creates a plan, show previews it, and invoke requires explicit confirmation before mutation."
        )
    }
}

#[cfg(test)]
mod tests {
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, ClientInfo, ContentBlock},
    };
    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn transition_trace_json_uses_arrays_for_coordinate_keyed_state() {
        let position = Pos::new(1, 64, -2);
        let trace = dustroute_translate::ScenarioTrace {
            duration_redstone_ticks: 2,
            events: Vec::new(),
            final_strengths: BTreeMap::from([(position, 15)]),
            final_powered: BTreeMap::from([(position, true)]),
        };

        let value = scenario_trace_json(&trace);
        assert_eq!(value["final_strengths"][0]["position"], json!(position));
        assert_eq!(value["final_strengths"][0]["strength"], 15);
        assert_eq!(value["final_powered"][0]["powered"], true);
        assert!(serde_json::to_string(&value).is_ok());
    }

    #[test]
    fn legacy_tool_errors_receive_the_common_error_contract() {
        let value: Value = serde_json::from_str(&json_text(json!({
            "ok": false,
            "error": "legacy failure"
        })))
        .unwrap();
        assert_eq!(value["schema_version"], crate::api::ERROR_SCHEMA_V1);
        assert_eq!(value["error_code"], "internal");
        assert_eq!(value["retryable"], false);
        assert_eq!(value["error"], "legacy failure");
    }

    #[tokio::test]
    async fn collaboration_prompt_requires_gaze_grounding_and_preview() {
        let prompt = DustRouteMcp::new("127.0.0.1:1")
            .collaboration_prompt()
            .await;
        let ContentBlock::Text(text) = &prompt.messages[0].content else {
            panic!("expected text prompt");
        };
        assert!(text.text.contains("get_looked_at_world"));
        assert!(text.text.contains("test_looked_at_circuit"));
        assert!(text.text.contains("show_selected_region"));
        assert!(text.text.contains("confirmation"));
    }

    #[test]
    fn tool_profiles_keep_low_level_operations_out_of_the_default_surface() {
        let default = DustRouteMcp::with_policy_and_profile(
            "127.0.0.1:1",
            McpPolicy::default(),
            ToolProfile::Default,
        );
        let debug = DustRouteMcp::with_policy_and_profile(
            "127.0.0.1:1",
            McpPolicy::default(),
            ToolProfile::Debug,
        );
        let default_names = default
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<BTreeSet<_>>();
        let debug_names = debug
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<BTreeSet<_>>();

        assert_eq!(default_names.len(), 20);
        assert_eq!(debug_names.len(), 27);
        assert!(default_names.contains("test_looked_at_circuit"));
        assert!(default_names.contains("invoke_repair"));
        for name in DEBUG_ONLY_TOOLS {
            assert!(!default_names.contains(name), "{name}");
            assert!(debug_names.contains(name), "{name}");
        }
    }

    #[tokio::test]
    async fn exposes_gaze_tools_prompt_and_fake_bot_status_over_mcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut request)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            let response = json!({
                "id": request["id"],
                "result": {
                    "connected": true,
                    "username": "DustRouteBot",
                    "host": "test",
                    "port": 25565,
                    "version": "1.21.11",
                    "dimension": "minecraft:overworld"
                }
            });
            stream
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        });

        let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
        let server = tokio::spawn(async move {
            DustRouteMcp::new(address)
                .serve(server_transport)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let client = ClientInfo::default().serve(client_transport).await.unwrap();
        let tools = client.list_tools(None).await.unwrap();
        assert!(
            !tools
                .tools
                .iter()
                .any(|tool| tool.name == "resolve_looked_at_circuit")
        );
        assert!(
            tools
                .tools
                .iter()
                .any(|tool| tool.name == "convert_from_looked_at_circuit")
        );
        assert!(
            tools
                .tools
                .iter()
                .any(|tool| tool.name == "test_looked_at_circuit")
        );
        assert!(
            tools
                .tools
                .iter()
                .any(|tool| tool.name == "get_looked_at_world")
        );
        let prompts = client.list_prompts(None).await.unwrap();
        assert!(
            prompts
                .prompts
                .iter()
                .any(|prompt| prompt.name == "collaborate-on-redstone-circuit")
        );
        let result = client
            .call_tool(CallToolRequestParams::new("get_bot_status"))
            .await
            .unwrap();
        let ContentBlock::Text(text) = &result.content[0] else {
            panic!("expected text tool result");
        };
        assert!(text.text.contains("DustRouteBot"));
        client.cancel().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn runs_two_gaze_points_preview_and_reverse_analysis_over_mcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let mut observation = 0;
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut request)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&request).unwrap();
                let result = match request["method"].as_str().unwrap() {
                    "observe_player" => {
                        let target = if observation == 0 {
                            json!({ "x": 0, "y": 64, "z": 0 })
                        } else {
                            json!({ "x": 3, "y": 66, "z": 1 })
                        };
                        observation += 1;
                        json!({
                            "player": "builder",
                            "eye_position": { "x": 0.5, "y": 65.62, "z": 4.5 },
                            "yaw": 0.0,
                            "pitch": 0.0,
                            "targeted_block": target,
                            "targeted_face": "up",
                            "distance": 4.0,
                            "dimension": "minecraft:overworld"
                        })
                    }
                    "preview_region" => json!({ "particle_corners": 8 }),
                    "scan_region" => json!({
                        "min": { "x": 0, "y": 64, "z": 0 },
                        "max": { "x": 3, "y": 66, "z": 1 },
                        "blocks": []
                    }),
                    method => panic!("unexpected fake bridge method {method}"),
                };
                let response = json!({ "id": request["id"], "result": result });
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });

        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(async move {
            DustRouteMcp::with_policy_and_player(address, McpPolicy::default(), "builder")
                .serve(server_transport)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let client = ClientInfo::default().serve(client_transport).await.unwrap();
        for corner in ["first", "second"] {
            let arguments = serde_json::from_value(json!({
                "corner": corner
            }))
            .unwrap();
            let result = client
                .call_tool(
                    CallToolRequestParams::new("set_region_corner").with_arguments(arguments),
                )
                .await
                .unwrap();
            let ContentBlock::Text(text) = &result.content[0] else {
                panic!("expected text tool result");
            };
            assert!(text.text.contains("\"ok\": true"));
        }
        for tool in ["show_selected_region", "convert_from_selected_region"] {
            let arguments = serde_json::Map::new();
            let result = client
                .call_tool(CallToolRequestParams::new(tool).with_arguments(arguments))
                .await
                .unwrap();
            let ContentBlock::Text(text) = &result.content[0] else {
                panic!("expected text tool result");
            };
            assert!(text.text.contains("\"ok\": true"));
        }
        client.cancel().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_an_override_of_the_configured_assist_player() {
        let service =
            DustRouteMcp::with_policy_and_player("127.0.0.1:1", McpPolicy::default(), "builder");
        let result = service
            .get_player_gaze(Parameters(ObserveParams {
                player: Some("someone_else".to_owned()),
                max_distance: None,
            }))
            .await;
        assert!(result.contains("player override is not allowed"));
    }

    #[tokio::test]
    async fn what_is_this_returns_physical_gate_and_boundary_views() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            for _ in 0..11 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut request)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&request).unwrap();
                let result = match request["method"].as_str().unwrap() {
                    "observe_player" => json!({
                        "player": "builder",
                        "eye_position": { "x": 0.5, "y": 3.0, "z": 0.5 },
                        "yaw": 0.0,
                        "pitch": -1.0,
                        "targeted_block": { "x": 1, "y": 1, "z": 0 },
                        "targeted_face": "up",
                        "distance": 2.0,
                        "dimension": "minecraft:overworld"
                    }),
                    "scan_region" => json!({
                        "min": request["params"]["min"],
                        "max": request["params"]["max"],
                        "blocks": [
                            { "pos": { "x": 0, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                            { "pos": { "x": 1, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                            { "pos": { "x": 2, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                            { "pos": { "x": 0, "y": 1, "z": 0 }, "name": "minecraft:redstone_wire", "properties": { "east": "side", "power": "0" } },
                            { "pos": { "x": 1, "y": 1, "z": 0 }, "name": "minecraft:repeater", "properties": { "facing": "west", "delay": "1", "powered": "false" } },
                            { "pos": { "x": 2, "y": 1, "z": 0 }, "name": "minecraft:redstone_wire", "properties": { "west": "side", "power": "0" } }
                        ]
                    }),
                    method => panic!("unexpected fake bridge method {method}"),
                };
                stream
                    .write_all(
                        format!("{}\n", json!({ "id": request["id"], "result": result }))
                            .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let service =
            DustRouteMcp::with_policy_and_player(address, McpPolicy::default(), "builder");
        let result = service
            .convert_from_looked_at_circuit(Parameters(AnalyzeLookedAtParams {
                player: None,
                max_components: Some(64),
                fragment_gap: Some(2),
                include_truth_table: Some(false),
            }))
            .await;
        let value: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(value["ok"], true, "{result}");
        assert_eq!(value["focused_component"]["block"], "Repeater");
        assert_eq!(
            value["focused_component"]["observed_name"],
            "minecraft:repeater"
        );
        assert_eq!(
            value["focused_component"]["observed_properties"]["delay"],
            "1"
        );
        assert_eq!(
            value["focused_component"]["capabilities"]["temporal"],
            "partial"
        );
        assert_eq!(
            value["focused_component"]["recognized_gates"][0]["kind"],
            "buffer"
        );
        assert!(!value["gate_view"]["gates"].as_array().unwrap().is_empty());
        assert!(value["physical"]["observation"].is_object());
        assert!(value["physical"]["block_capabilities"]["groups"].is_array());
        assert!(value["stages"]["physical_scene"].is_object());
        assert!(value["transition_scenarios"]["safety"].is_object());
        assert_eq!(value["diagnostic"]["observation_complete"], true);
        assert!(value["diagnostic"]["counts"].is_object());
        assert!(value["diagnostic"]["recommended_next_action"].is_object());
    }

    #[tokio::test]
    async fn focused_diagnostic_returns_a_compact_read_only_contract() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let bridge = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut request)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&request).unwrap();
                let result = match request["method"].as_str().unwrap() {
                    "observe_player" => json!({
                        "player": "builder",
                        "eye_position": { "x": 0.5, "y": 3.0, "z": 0.5 },
                        "yaw": 0.0,
                        "pitch": -1.0,
                        "targeted_block": { "x": 1, "y": 1, "z": 0 },
                        "targeted_face": "up",
                        "distance": 2.0,
                        "dimension": "minecraft:overworld"
                    }),
                    "scan_region" => json!({
                        "min": request["params"]["min"],
                        "max": request["params"]["max"],
                        "blocks": [
                            { "pos": { "x": 0, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                            { "pos": { "x": 1, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                            { "pos": { "x": 0, "y": 1, "z": 0 }, "name": "minecraft:redstone_wire", "properties": { "east": "side", "power": "0" } },
                            { "pos": { "x": 1, "y": 1, "z": 0 }, "name": "minecraft:repeater", "properties": { "facing": "west", "delay": "1", "powered": "false" } }
                        ]
                    }),
                    method => panic!("unexpected fake bridge method {method}"),
                };
                stream
                    .write_all(
                        format!("{}\n", json!({ "id": request["id"], "result": result }))
                            .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let service =
            DustRouteMcp::with_policy_and_player(address, McpPolicy::default(), "builder");
        let result = service
            .test_looked_at_circuit(Parameters(DiagnoseLookedAtParams {
                player: None,
                max_components: Some(64),
                fragment_gap: Some(2),
            }))
            .await;
        bridge.abort();

        let value: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(value["ok"], true, "{result}");
        assert_eq!(value["schema_version"], "dustroute.diagnostic.v1");
        assert_eq!(value["analysis_mode"], "focused_fast");
        assert_eq!(value["mutation_performed"], false);
        assert!(value["diagnostic"]["counts"].is_object());
        assert!(value["diagnostic"]["findings"].is_array());
        assert!(value["diagnostic"]["recommended_next_action"].is_object());
    }

    #[tokio::test]
    async fn repairs_and_undoes_a_broken_wire_through_the_mcp_workflow() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let mut repaired = false;
            for _ in 0..8 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut request)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&request).unwrap();
                let result = match request["method"].as_str().unwrap() {
                    "scan_region" => broken_wire_snapshot(repaired),
                    "preview_region" => json!({ "particle_corners": 8 }),
                    "place_physical_blocks" => {
                        repaired = request["params"]["changes"][0]["action"] == "place";
                        json!({
                            "placed_changes": 1,
                            "placement_mode": "mineflayer_player",
                            "retreat": { "x": 2.5, "y": 18.0, "z": 0.5 }
                        })
                    }
                    method => panic!("unexpected fake bridge method {method}"),
                };
                let response = json!({ "id": request["id"], "result": result });
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });

        let policy = McpPolicy {
            read_only: false,
            ..McpPolicy::default()
        };
        let service = DustRouteMcp::with_policy_and_player(address, policy, "builder");
        let bounds = dustroute_translate::RegionBounds::new(Pos::new(0, 0, 0), Pos::new(2, 2, 0));
        service
            .selections
            .lock()
            .await
            .entry("builder".into())
            .or_insert_with(|| SelectionSession::new("builder"))
            .set_bounds(bounds);
        service
            .selection_dimensions
            .lock()
            .await
            .insert("builder".into(), "minecraft:overworld".into());

        let proposed: Value = serde_json::from_str(
            &service
                .new_repair_plan(Parameters(ProposeRepairsParams {
                    player: None,
                    max_gap: Some(2),
                }))
                .await,
        )
        .unwrap();
        assert_eq!(proposed["proposal_count"], 3);
        let operation_id = proposed["proposals"][0]["operation_id"]
            .as_str()
            .unwrap()
            .to_owned();

        let preview = service
            .show_repair_plan(Parameters(PreviewRepairParams {
                operation_id: operation_id.clone(),
                player: None,
            }))
            .await;
        assert!(preview.contains("\"ok\": true"));
        let applied = service
            .invoke_repair(Parameters(ConfirmedOperationParams {
                operation_id: operation_id.clone(),
                confirm: true,
            }))
            .await;
        assert!(applied.contains("\"verified\": true"), "{applied}");
        let applied_value: Value = serde_json::from_str(&applied).unwrap();
        assert!(applied_value["resulting_logic"].is_object());
        assert_eq!(applied_value["semantic_verification"]["available"], false);
        let undone = service
            .undo_repair(Parameters(ConfirmedOperationParams {
                operation_id,
                confirm: true,
            }))
            .await;
        assert!(undone.contains("\"verified\": true"), "{undone}");
    }

    #[test]
    fn raw_inspection_preserves_states_and_reports_scan_boundaries() {
        let snapshot: dustroute_translate::MinecraftSnapshot = serde_json::from_value(json!({
            "min": { "x": 0, "y": 0, "z": 0 },
            "max": { "x": 2, "y": 1, "z": 0 },
            "blocks": [
                snapshot_block(0, 0, 0, "minecraft:stone", json!({})),
                snapshot_block(1, 0, 0, "minecraft:stone", json!({})),
                snapshot_block(2, 0, 0, "minecraft:stone", json!({})),
                snapshot_block(1, 1, 0, "minecraft:redstone_wire", json!({
                    "north": "none", "east": "side", "south": "none",
                    "west": "side", "power": "7"
                })),
                snapshot_block(2, 1, 0, "minecraft:repeater", json!({
                    "facing": "east", "delay": "3", "powered": "true"
                }))
            ]
        }))
        .unwrap();
        let result = raw_world_inspection(
            &snapshot,
            Pos::new(1, 1, 0),
            "minecraft:overworld",
            false,
            16,
        );
        assert_eq!(result["inference_applied"], false);
        assert_eq!(result["scan"]["volume"], 6);
        assert_eq!(result["counts"]["air"], 1);
        assert_eq!(result["counts"]["redstone_candidates"], 2);
        assert_eq!(result["counts"]["modeled_redstone"], 2);
        assert_eq!(result["boundary"]["redstone_touches_boundary"], true);
        assert_eq!(result["target_block"]["properties"]["power"], "7");
        assert_eq!(result["redstone_blocks"][1]["properties"]["delay"], "3");
        assert!(result["blocks"].is_null());
    }

    fn broken_wire_snapshot(repaired: bool) -> Value {
        let mut blocks = vec![
            snapshot_block(0, 0, 0, "minecraft:stone", json!({})),
            snapshot_block(1, 0, 0, "minecraft:stone", json!({})),
            snapshot_block(2, 0, 0, "minecraft:stone", json!({})),
            snapshot_block(0, 1, 0, "minecraft:redstone_wire", wire_properties()),
            snapshot_block(2, 1, 0, "minecraft:redstone_wire", wire_properties()),
        ];
        if repaired {
            blocks.push(snapshot_block(
                1,
                1,
                0,
                "minecraft:redstone_wire",
                wire_properties(),
            ));
        }
        json!({
            "min": { "x": 0, "y": 0, "z": 0 },
            "max": { "x": 2, "y": 2, "z": 0 },
            "blocks": blocks,
        })
    }

    fn snapshot_block(x: i32, y: i32, z: i32, name: &str, properties: Value) -> Value {
        json!({ "pos": { "x": x, "y": y, "z": z }, "name": name, "properties": properties })
    }

    fn wire_properties() -> Value {
        json!({
            "north": "none",
            "east": "side",
            "south": "none",
            "west": "side",
            "power": "0",
        })
    }
}
