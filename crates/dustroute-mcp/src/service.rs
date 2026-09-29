use crate::bridge_protocol::CommandWrite;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

mod assembly_placement;
mod circuit_capture;
mod circuit_reports;
mod optimization_workflow;
mod placement_workflow;
mod player_scope;
mod repair_workflow;
mod world_editor;
use circuit_reports::{
    bounds_json, circuit_identity_json, focused_explanation_json, focused_hierarchy_role_json,
    focused_role_json, hierarchical_result_json, mixed_ir_json, raw_world_inspection,
    reverse_result_json, revision_json, revision_validation,
};
mod requests;
#[cfg(test)]
mod test_support;
use requests::{
    AnalyzeLookedAtParams, ConfirmedOperationParams, CoordinateParam, DiagnoseLookedAtParams,
    DiscoverCircuitParams, GetCircuitRevisionParams, GetLookedAtCircuitIrParams,
    GetRepairContextParams, InspectLookedAtWorldParams, InvokeOperationParams, MarkCornerParams,
    NewMacroOptimizationParams, NewOptimizationParams, NewPistonDoorParams, ObserveParams,
    OperationParams, OptimizationContractParam, OptimizationSearchParam, PlayerParams,
    PreviewPlacementParams, PreviewRepairParams, PreviewTransitionParams, ProposeRepairsParams,
    ProposeTransitionParams, RunTransitionParams, ShowOperationParams,
    StartSelectedRegionConversionParams, TestCircuitChangeParams, TransitionContractParam,
};
mod placement_registry;
use circuit_capture::{CircuitCapture, DiscoveryObservation, is_redstone_candidate_name};
mod operation_lifecycle;
use operation_lifecycle::{InvocationState, RepairLifecycle, RevisionLifecycle};
mod operation_plans;
use operation_plans::{OperationPlans, PlanKind};

use dustroute_app::DustRouteService;
use dustroute_optimize::{
    AnchorPolicy, BehavioralVerificationConfig, CompressionAxis, CompressionDirection,
    ContextualVerificationState, ContractCheck, ContractCheckState, MacroSteadyStateReport,
    MacroStructuralReport, ObservedMacroMetrics, OptimizationContract,
    OptimizationContractAssessment, OptimizationPlan, OptimizationRoutingConfig,
    OptimizationSafety, PhysicalOptimizationSearchBudget, TemporalCapabilities, TimingContractMode,
    assess_macro_contract, assess_optimization_safety, extract_model_boundary_with_context,
    find_builtin_verified_macro_replacements, materialize_macro_replacement_in_known_regions,
    optimize_physical_wire_path_with_budget, plan_macro_replacement_with_reserved,
    realize_staged_optimization_against, validate_macro_structure, verify_boundary_strengths,
    verify_macro_steady_state, verify_macro_transitions, verify_realized_optimization,
    verify_world_transitions,
};
use dustroute_physical::{BlockKind, Pos};
use dustroute_physical::{PhysicalBlockChange, PhysicalPatch};
use dustroute_translate::{
    api::ForwardOptions, api::ReverseRequest, minecraft_export::JavaExportConfig,
    minecraft_export::initial_java_block_state, snapshot::world_from_snapshot,
    world_reverse::TruthTableBudget,
};
use rmcp::{
    ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{GetPromptResult, Implementation, PromptMessage, Role, ServerCapabilities, ServerInfo},
    prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use crate::McpConfig;
use crate::api::{
    DIAGNOSTIC_SCHEMA_V1, ErrorResponse, McpErrorCode, OPTIMIZATION_SCHEMA_V1, PLACEMENT_SCHEMA_V1,
    REPAIR_CONTEXT_SCHEMA_V1, REPAIR_SCHEMA_V1, TRANSITION_SCHEMA_V1, TransitionTraceResponse,
};
use crate::state::{PlanRecordKind, PlanStateStore};
use crate::{
    BlockChange, BotBridge, McpPolicy, OperationKind, OperationRegistry, OperationStatus,
    PlacementPlan, SelectionSession, TransitionSafety, TransitionSafetyAssessment,
    assess_transition_safety, behavior_trace_from_recording, plan_world_overlay,
    scenario_trace_from_recording_with_initial,
};

use circuit_reports::MAX_FLAT_ANALYSIS_COMPONENTS;
const MAX_TRUTH_TABLE_INPUTS: usize = 16;
const MAX_TRUTH_TABLE_SETTLE_TICKS: usize = 256;
const MAX_TRUTH_TABLE_ROWS: usize = 65_536;
const MAX_TRUTH_TABLE_WORK_UNITS: u64 = 100_000_000;
const MAX_TRUTH_TABLE_SOLVER_ITERATIONS: usize = 10_000_000;
const MAX_TRUTH_TABLE_ELAPSED_MILLIS: u64 = 300_000;
const PLACEMENT_VERIFY_ATTEMPTS: usize = 10;
const PLACEMENT_VERIFY_INTERVAL: Duration = Duration::from_millis(100);
const CIRCUIT_SNAPSHOT_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_CIRCUIT_SNAPSHOTS: usize = 64;

#[cfg(test)]
#[path = "service_blueprint_tests.rs"]
mod blueprint_tests;

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

/// Selection geometry and its dimension share one lock and one lifetime.
struct LocatedSelection {
    session: SelectionSession,
    dimension: Option<String>,
}
impl LocatedSelection {
    fn new(player: &str) -> Self {
        Self {
            session: SelectionSession::new(player),
            dimension: None,
        }
    }
    fn with_bounds(
        player: &str,
        bounds: dustroute_translate::world_reverse::RegionBounds,
        dimension: String,
    ) -> Self {
        let mut selected = Self::new(player);
        selected.session.set_bounds(bounds);
        selected.dimension = Some(dimension);
        selected
    }
}

#[derive(Clone)]
pub struct DustRouteMcp {
    bridge: BotBridge,
    selections: Arc<Mutex<HashMap<String, LocatedSelection>>>,
    circuits: Arc<Mutex<HashMap<uuid::Uuid, StoredCircuit>>>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
    plans: OperationPlans,
    state_store: PlanStateStore,
    policy: McpPolicy,
    app: DustRouteService,
    operations: OperationRegistry,
    mutation_lock: Arc<Mutex<()>>,
    assist_player: Option<String>,
    server_address: Option<String>,
}

#[derive(Clone, Debug)]
struct RevisionPlacementContext {
    player: String,
    dimension: String,
    version: String,
    before: dustroute_translate::snapshot::MinecraftSnapshot,
    after: dustroute_translate::snapshot::MinecraftSnapshot,
    expires_at: Instant,
    lifecycle: RevisionLifecycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PistonPlacementState {
    Planned,
    Applied,
    Undone,
    NeedsInspection,
}
#[derive(Clone, Debug)]
struct StoredPistonPlacement {
    player: String,
    dimension: String,
    proof: crate::piston_door::ValidatedDoorPlacement,
    previewed: bool,
    state: PistonPlacementState,
    expires_at: Instant,
}

#[derive(Clone, Debug)]
struct StoredDoorPlan {
    player: String,
    dimension: String,
    door: crate::piston_door::VerifiedDoor,
    target: crate::piston_door::DoorState,
    lifecycle: InvocationState,
    expires_at: Instant,
}

#[derive(Clone, Debug)]
struct StoredCircuit {
    player: String,
    dimension: String,
    bounds: dustroute_translate::world_reverse::RegionBounds,
    target: Option<Pos>,
    snapshot: dustroute_translate::snapshot::MinecraftSnapshot,
    expansion: Value,
    complete: bool,
    expires_at: Instant,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredRepairPlan {
    patch: PhysicalPatch,
    dimension: String,
    analysis_bounds: dustroute_translate::world_reverse::RegionBounds,
    fragments_before: usize,
    baseline_truth_table: Option<dustroute_translate::world_reverse::InferredTruthTable>,
    lifecycle: RepairLifecycle,
    contract_satisfied: bool,
    preserved_boundary: Vec<BoundaryBlockRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct BoundaryBlockRecord {
    pos: Pos,
    name: String,
    static_properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
struct StoredTransitionPlan {
    player: String,
    dimension: String,
    bounds: dustroute_translate::world_reverse::RegionBounds,
    lever: Pos,
    original_powered: bool,
    initial_snapshot: dustroute_translate::snapshot::MinecraftSnapshot,
    observation_ticks: u16,
    max_events: usize,
    safety: TransitionSafetyAssessment,
    lifecycle: InvocationState,
}

fn optimization_search_budget(
    param: Option<OptimizationSearchParam>,
) -> Result<PhysicalOptimizationSearchBudget, String> {
    let mut budget = PhysicalOptimizationSearchBudget::default();
    if let Some(param) = param {
        budget.max_expansions = param.max_expansions.unwrap_or(budget.max_expansions);
        budget.max_candidates = param.max_candidates.unwrap_or(budget.max_candidates);
        budget.max_millis = param.max_millis.unwrap_or(budget.max_millis);
    }
    if !(1..=1_000_000).contains(&budget.max_expansions) {
        return Err("max_expansions must be 1..=1000000".to_owned());
    }
    if !(1..=10_000).contains(&budget.max_candidates) {
        return Err("max_candidates must be 1..=10000".to_owned());
    }
    if !(1..=10_000).contains(&budget.max_millis) {
        return Err("max_millis must be 1..=10000".to_owned());
    }
    Ok(budget)
}

fn optimization_contract_from_param(
    param: Option<OptimizationContractParam>,
) -> Result<OptimizationContract, String> {
    let mut contract = OptimizationContract::default();
    let Some(param) = param else {
        return Ok(contract);
    };
    if let Some(logical) = param.logical
        && logical
            .mode
            .as_deref()
            .is_some_and(|mode| mode != "exact_truth_table")
    {
        return Err(format!(
            "unknown logical contract mode {:?}",
            logical.mode.expect("mode was present")
        ));
    }
    if let Some(timing) = param.timing {
        if let Some(mode) = timing.mode {
            contract.timing.mode = match mode.as_str() {
                "exact_trace" => TimingContractMode::ExactTrace,
                "exact_transitions" => TimingContractMode::ExactTransitions,
                "bounded_delay" => TimingContractMode::BoundedDelay,
                "settled_value_only" => TimingContractMode::SettledValueOnly,
                "preserve_order" => TimingContractMode::PreserveOrder,
                _ => return Err(format!("unknown timing contract mode {mode:?}")),
            };
        }
        if let Some(value) = timing.maximum_added_redstone_ticks {
            contract.timing.maximum_added_redstone_ticks = value;
        }
        if let Some(value) = timing.settle_deadline_redstone_ticks {
            contract.timing.settle_deadline_redstone_ticks = value;
        }
    }
    if let Some(pulse) = param.pulse {
        contract.pulse.allow_new_pulses = pulse
            .allow_new_pulses
            .unwrap_or(contract.pulse.allow_new_pulses);
        contract.pulse.allow_removed_pulses = pulse
            .allow_removed_pulses
            .unwrap_or(contract.pulse.allow_removed_pulses);
        contract.pulse.maximum_width_delta_redstone_ticks = pulse
            .maximum_width_delta_redstone_ticks
            .unwrap_or(contract.pulse.maximum_width_delta_redstone_ticks);
    }
    if let Some(analog) = param.analog {
        contract.analog.preserve_strength = analog
            .preserve_strength
            .unwrap_or(contract.analog.preserve_strength);
    }
    if let Some(boundary) = param.boundary {
        contract.boundary.preserve_blocks = boundary
            .preserve_blocks
            .unwrap_or(contract.boundary.preserve_blocks);
        contract.boundary.preserve_facing = boundary
            .preserve_facing
            .unwrap_or(contract.boundary.preserve_facing);
        contract.boundary.preserve_driver_positions = boundary
            .preserve_driver_positions
            .unwrap_or(contract.boundary.preserve_driver_positions);
    }
    if let Some(mutation) = param.mutation {
        contract.mutation.focus_only = mutation.focus_only.unwrap_or(contract.mutation.focus_only);
        contract.mutation.allow_temporary_expansion = mutation
            .allow_temporary_expansion
            .unwrap_or(contract.mutation.allow_temporary_expansion);
        contract.mutation.maximum_changed_blocks = mutation
            .maximum_changed_blocks
            .unwrap_or(contract.mutation.maximum_changed_blocks);
        contract.mutation.automatic_apply = mutation
            .automatic_apply
            .unwrap_or(contract.mutation.automatic_apply);
    }
    if contract.mutation.maximum_changed_blocks == 0 {
        return Err("maximum_changed_blocks must be greater than zero".to_owned());
    }
    if contract.timing.maximum_added_redstone_ticks > 256 {
        return Err("maximum_added_redstone_ticks must be at most 256".to_owned());
    }
    if !(1..=256).contains(&contract.timing.settle_deadline_redstone_ticks) {
        return Err("settle_deadline_redstone_ticks must be 1..=256".to_owned());
    }
    if contract.pulse.maximum_width_delta_redstone_ticks > 256 {
        return Err("maximum_width_delta_redstone_ticks must be at most 256".to_owned());
    }
    Ok(contract)
}

fn optimization_contract_json(contract: OptimizationContract) -> Value {
    let timing_mode = match contract.timing.mode {
        TimingContractMode::ExactTrace => "exact_trace",
        TimingContractMode::ExactTransitions => "exact_transitions",
        TimingContractMode::BoundedDelay => "bounded_delay",
        TimingContractMode::SettledValueOnly => "settled_value_only",
        TimingContractMode::PreserveOrder => "preserve_order",
    };
    json!({
        "logical": { "mode": "exact_truth_table" },
        "timing": {
            "mode": timing_mode,
            "maximum_added_redstone_ticks": contract.timing.maximum_added_redstone_ticks,
            "settle_deadline_redstone_ticks": contract.timing.settle_deadline_redstone_ticks,
        },
        "pulse": {
            "allow_new_pulses": contract.pulse.allow_new_pulses,
            "allow_removed_pulses": contract.pulse.allow_removed_pulses,
            "maximum_width_delta_redstone_ticks": contract.pulse.maximum_width_delta_redstone_ticks,
        },
        "analog": { "preserve_strength": contract.analog.preserve_strength },
        "boundary": {
            "preserve_blocks": contract.boundary.preserve_blocks,
            "preserve_facing": contract.boundary.preserve_facing,
            "preserve_driver_positions": contract.boundary.preserve_driver_positions,
        },
        "mutation": {
            "focus_only": contract.mutation.focus_only,
            "allow_temporary_expansion": contract.mutation.allow_temporary_expansion,
            "maximum_changed_blocks": contract.mutation.maximum_changed_blocks,
            "automatic_apply": contract.mutation.automatic_apply,
        },
    })
}

fn contract_check_json(check: &ContractCheck) -> Value {
    let state = match check.state {
        ContractCheckState::Passed => "passed",
        ContractCheckState::Failed => "failed",
        ContractCheckState::Unavailable => "unavailable",
    };
    json!({ "state": state, "reason_codes": check.reason_codes, "reasons": check.reasons })
}

fn contract_assessment_json(assessment: &OptimizationContractAssessment) -> Value {
    json!({
        "satisfied": assessment.satisfied(),
        "logical": contract_check_json(&assessment.logical),
        "timing": contract_check_json(&assessment.timing),
        "pulse": contract_check_json(&assessment.pulse),
        "analog": contract_check_json(&assessment.analog),
        "boundary": contract_check_json(&assessment.boundary),
        "mutation": contract_check_json(&assessment.mutation),
    })
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

/// Convert an already decoded bridge snapshot directly into the simulator
/// world.  Bridge responses are typed before they reach the service, so
/// serializing them to JSON and parsing the same JSON again only adds copying
/// and allocation cost on every analysis path.
fn world_from_snapshot_for_service(
    snapshot: &dustroute_translate::snapshot::MinecraftSnapshot,
) -> Result<dustroute_translate::world::World, String> {
    world_from_snapshot(snapshot).map_err(|error| error.to_string())
}

fn snapshot_from_grounded_assembly(
    record: &dustroute_library::assembly::AssemblyRevision,
    min: Pos,
    max: Pos,
) -> Result<dustroute_translate::snapshot::MinecraftSnapshot, String> {
    if !record
        .assembly
        .known_regions
        .iter()
        .any(|region| region.contains(min) && region.contains(max))
    {
        return Err(
            "adopted Assembly does not declare the complete grounded observation region as known"
                .into(),
        );
    }
    let inside = |position: Pos| {
        position.x >= min.x
            && position.x <= max.x
            && position.y >= min.y
            && position.y <= max.y
            && position.z >= min.z
            && position.z <= max.z
    };
    let mut seen = BTreeSet::new();
    let mut blocks = Vec::new();
    for positioned in &record.assembly.blocks {
        if !inside(positioned.position) || !seen.insert(positioned.position) {
            return Err("adopted Assembly has an out-of-bounds or duplicate actual block".into());
        }
        if positioned.block.kind == BlockKind::Air {
            continue;
        }
        let state = initial_java_block_state(&positioned.block, &JavaExportConfig::default())
            .map_err(|error| {
                format!("adopted Assembly block is not losslessly exportable: {error}")
            })?;
        let (name, encoded) = state
            .split_once('[')
            .map_or((state.as_str(), ""), |(name, properties)| {
                (name, properties.trim_end_matches(']'))
            });
        let properties = encoded
            .split(',')
            .filter(|entry| !entry.is_empty())
            .map(|entry| {
                entry
                    .split_once('=')
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .ok_or_else(|| "exported block state has an invalid property".to_owned())
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        blocks.push(dustroute_translate::snapshot::MinecraftSnapshotBlock {
            pos: positioned.position,
            name: name.to_owned(),
            properties,
        });
    }
    Ok(dustroute_translate::snapshot::MinecraftSnapshot { min, max, blocks })
}

// Application workflows return structured reports. Only the MCP facade adds
// text framing and default tool error metadata.
fn workflow_error(code: McpErrorCode, message: impl Into<String>, retryable: bool) -> Value {
    serde_json::to_value(ErrorResponse::new(code, message, retryable))
        .unwrap_or_else(|e| json!({"ok":false,"error":format!("failed to encode error: {e}")}))
}

fn error_text(code: McpErrorCode, message: impl Into<String>, retryable: bool) -> String {
    json_text(workflow_error(code, message, retryable))
}

fn scenario_trace_json(trace: &dustroute_translate::scenario::ScenarioTrace) -> Value {
    serde_json::to_value(TransitionTraceResponse::from(trace)).unwrap_or_else(|error| {
        json!({
            "serialization_error": error.to_string(),
            "duration_redstone_ticks": trace.duration_redstone_ticks,
            "duration_game_ticks": trace.duration_game_ticks,
            "time_unit": trace.time_unit,
            "status": trace.status.clone(),
            "events": [],
            "transitions": [],
            "final_strengths": [],
            "final_powered": [],
        })
    })
}

fn scenario_run_json(run: &dustroute_translate::scenario::ScenarioRun) -> Value {
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

fn mark_observation_incomplete(scene: &mut dustroute_physical::PhysicalScene) {
    for region in &mut scene.observation.regions {
        region.completeness = dustroute_physical::RegionCompleteness::PartiallyUnavailable;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct TruthTableRequestOptions {
    include_truth_table: bool,
    max_inputs: Option<usize>,
    settle_ticks: Option<usize>,
    max_rows: Option<usize>,
    max_work_units: Option<u64>,
    max_solver_iterations: Option<usize>,
    max_elapsed_millis: Option<u64>,
}

fn reverse_request_for_truth_table(
    bounds: dustroute_translate::world_reverse::RegionBounds,
    options: TruthTableRequestOptions,
) -> Result<ReverseRequest, String> {
    let request = ReverseRequest::new(bounds);
    if !options.include_truth_table {
        return Ok(request);
    }
    let max_inputs = options.max_inputs.unwrap_or(request.max_inputs);
    if !(1..=MAX_TRUTH_TABLE_INPUTS).contains(&max_inputs) {
        return Err(format!(
            "truth_table_max_inputs must be 1..={MAX_TRUTH_TABLE_INPUTS}"
        ));
    }
    let settle_ticks = options.settle_ticks.unwrap_or(request.settle_ticks);
    if settle_ticks > MAX_TRUTH_TABLE_SETTLE_TICKS {
        return Err(format!(
            "truth_table_settle_ticks must be 0..={MAX_TRUTH_TABLE_SETTLE_TICKS}"
        ));
    }
    let max_rows = options
        .max_rows
        .unwrap_or(TruthTableBudget::DEFAULT.max_rows);
    if !(1..=MAX_TRUTH_TABLE_ROWS).contains(&max_rows) {
        return Err(format!(
            "truth_table_max_rows must be 1..={MAX_TRUTH_TABLE_ROWS}"
        ));
    }
    let max_work_units = options.max_work_units.unwrap_or_else(|| {
        u64::try_from(TruthTableBudget::DEFAULT.max_work_units).unwrap_or(u64::MAX)
    });
    if !(1..=MAX_TRUTH_TABLE_WORK_UNITS).contains(&max_work_units) {
        return Err(format!(
            "truth_table_max_work_units must be 1..={MAX_TRUTH_TABLE_WORK_UNITS}"
        ));
    }
    let max_solver_iterations = options
        .max_solver_iterations
        .unwrap_or(TruthTableBudget::DEFAULT.max_solver_iterations);
    if !(1..=MAX_TRUTH_TABLE_SOLVER_ITERATIONS).contains(&max_solver_iterations) {
        return Err(format!(
            "truth_table_max_solver_iterations must be 1..={MAX_TRUTH_TABLE_SOLVER_ITERATIONS}"
        ));
    }
    let max_elapsed_millis = options.max_elapsed_millis.unwrap_or(
        TruthTableBudget::DEFAULT
            .max_elapsed_millis
            .unwrap_or(120_000),
    );
    if !(1..=MAX_TRUTH_TABLE_ELAPSED_MILLIS).contains(&max_elapsed_millis) {
        return Err(format!(
            "truth_table_max_elapsed_millis must be 1..={MAX_TRUTH_TABLE_ELAPSED_MILLIS}"
        ));
    }
    Ok(request
        .with_truth_table(max_inputs)
        .with_settle_ticks(settle_ticks)
        .with_truth_table_budget(
            TruthTableBudget::new(max_rows, u128::from(max_work_units))
                .with_max_solver_iterations(max_solver_iterations)
                .with_max_elapsed_millis(Some(max_elapsed_millis)),
        ))
}

impl DustRouteMcp {
    async fn blueprint_command(
        &self,
        command: crate::blueprint_mcp::Command,
        player_override: Option<&str>,
        required: bool,
    ) -> Option<String> {
        let player = match self.resolve_player(player_override) {
            Ok(player) => player,
            Err(error) => return required.then(|| json_text(crate::blueprint_mcp::failure(error))),
        };
        if let Some(error) = self.authorize_player(&player) {
            return Some(error);
        }
        let store = self.state_store.clone();
        match tokio::task::spawn_blocking(move || {
            crate::blueprint_mcp::execute(&store, &player, command)
        })
        .await
        {
            Ok(Ok(Some(result))) => Some(json_text(result)),
            Ok(Ok(None)) => required.then(|| {
                json_text(crate::blueprint_mcp::failure(
                    "unknown Blueprint operation ID",
                ))
            }),
            Ok(Err(error)) => Some(json_text(crate::blueprint_mcp::failure(error))),
            Err(error) => Some(json_text(crate::blueprint_mcp::failure(error))),
        }
    }

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
            circuits: Arc::new(Mutex::new(HashMap::new())),
            tool_router,
            prompt_router: Self::prompt_router(),
            plans: OperationPlans::default(),
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

    fn optimization_workflow(&self) -> optimization_workflow::OptimizationWorkflow<'_> {
        optimization_workflow::OptimizationWorkflow {
            policy: &self.policy,
            state_store: &self.state_store,
            app: &self.app,
            operations: &self.operations,
        }
    }

    fn assembly_service(&self) -> assembly_placement::AssemblyService<'_> {
        assembly_placement::AssemblyService {
            bridge: &self.bridge,
            policy: &self.policy,
            state_store: &self.state_store,
            plans: &self.plans,
            operations: &self.operations,
            mutation_lock: &self.mutation_lock,
            player_scope: self.player_scope(),
        }
    }

    async fn store_repair_plan(
        &self,
        operation_id: uuid::Uuid,
        plan: StoredRepairPlan,
    ) -> Result<(), String> {
        repair_workflow::RepairPlans(&self.state_store).save(operation_id, &plan)
    }

    async fn repair_plan(
        &self,
        operation_id: uuid::Uuid,
    ) -> Result<Option<StoredRepairPlan>, String> {
        // Disk retention applies equally before and after a service restart.
        // An expired or removed record must not be resurrected from memory.
        repair_workflow::RepairPlans(&self.state_store).load(operation_id)
    }

    fn player_scope(&self) -> player_scope::PlayerScope<'_> {
        player_scope::PlayerScope {
            configured: self.assist_player.as_deref(),
            policy: &self.policy,
        }
    }
    fn resolve_player(&self, requested: Option<&str>) -> Result<String, String> {
        self.player_scope().resolve(requested)
    }

    fn authorize_player(&self, player: &str) -> Option<String> {
        self.policy
            .authorize_player(player)
            .err()
            .map(|error| json_text(json!({ "ok": false, "error": error.to_string() })))
    }

    async fn placement_view(&self, id: uuid::Uuid) -> Result<PlacementPlan, String> {
        if let Some(context) = self.plans.placements().lock().await.revision(&id) {
            let player = self.resolve_player(None)?;
            self.policy
                .authorize_player(&player)
                .map_err(|error| error.to_string())?;
            if context.player != player {
                return Err("placement belongs to another player".into());
            }
        }
        self.plans
            .placements()
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| "unknown operation ID".into())
    }

    async fn mutate_placement(&self, params: ConfirmedOperationParams, undo: bool) -> String {
        json_text(
            placement_workflow::PlacementWorkflow {
                bridge: &self.bridge,
                policy: &self.policy,
                plans: &self.plans,
                operations: &self.operations,
                mutation_lock: &self.mutation_lock,
                actor: self.resolve_player(None),
            }
            .mutate_placement(params, undo)
            .await,
        )
    }

    async fn mutate_repair(&self, params: ConfirmedOperationParams, undo: bool) -> String {
        json_text(
            repair_workflow::RepairWorkflow {
                bridge: &self.bridge,
                policy: &self.policy,
                state_store: &self.state_store,
                app: &self.app,
                operations: &self.operations,
                mutation_lock: &self.mutation_lock,
            }
            .mutate_repair(params, undo)
            .await,
        )
    }

    async fn create_transition_proposals(
        &self,
        player: &str,
        dimension: &str,
        bounds: dustroute_translate::world_reverse::RegionBounds,
        snapshot: &dustroute_translate::snapshot::MinecraftSnapshot,
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
            self.plans
                .table::<StoredTransitionPlan>()
                .lock()
                .await
                .insert(
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
                        lifecycle: InvocationState::Draft,
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
    ) -> Result<(dustroute_translate::world_reverse::RegionBounds, String), String> {
        let selections = self.selections.lock().await;
        let selected = selections
            .get(player)
            .ok_or_else(|| "no selection session for player".to_owned())?;
        let bounds = selected
            .session
            .bounds()
            .map_err(|error| error.to_string())?;
        let dimension = selected
            .dimension
            .clone()
            .ok_or_else(|| "selection has no dimension".to_owned())?;
        Ok((bounds, dimension))
    }

    async fn store_circuit(&self, circuit: StoredCircuit) -> uuid::Uuid {
        let now = Instant::now();
        let mut circuits = self.circuits.lock().await;
        circuits.retain(|_, stored| stored.expires_at > now);
        while circuits.len() >= MAX_CIRCUIT_SNAPSHOTS {
            let Some(oldest) = circuits
                .iter()
                .min_by_key(|(_, stored)| stored.expires_at)
                .map(|(id, _)| *id)
            else {
                break;
            };
            circuits.remove(&oldest);
        }
        let circuit_id = uuid::Uuid::new_v4();
        circuits.insert(circuit_id, circuit);
        circuit_id
    }

    async fn load_circuit(
        &self,
        circuit_id: &str,
        player: &str,
    ) -> Result<(uuid::Uuid, StoredCircuit), String> {
        let id = uuid::Uuid::parse_str(circuit_id)
            .map_err(|error| format!("invalid circuit_id: {error}"))?;
        let now = Instant::now();
        let mut circuits = self.circuits.lock().await;
        circuits.retain(|_, stored| stored.expires_at > now);
        let circuit = circuits
            .get(&id)
            .cloned()
            .ok_or_else(|| "unknown or expired circuit_id; capture the circuit again".to_owned())?;
        if circuit.player != player {
            return Err("circuit_id belongs to a different assisted player".to_owned());
        }
        Ok((id, circuit))
    }

    async fn discover_selection(
        &self,
        player: &str,
        max_components: Option<usize>,
        padding: i32,
        fragment_gap: u32,
    ) -> Result<DiscoveryObservation, String> {
        self.resolve_player(Some(player))?;
        self.policy
            .authorize_player(player)
            .map_err(|error| error.to_string())?;
        let discovery = CircuitCapture {
            bridge: &self.bridge,
            policy: &self.policy,
        }
        .discover(
            player,
            max_components.unwrap_or(8192),
            padding,
            fragment_gap,
        )
        .await?;
        self.selections.lock().await.insert(
            player.to_owned(),
            LocatedSelection::with_bounds(
                player,
                discovery.candidate.bounds.into(),
                discovery.dimension.clone(),
            ),
        );
        Ok(discovery)
    }

    async fn capture_looked_at_circuit(
        &self,
        player: &str,
        max_components: Option<usize>,
        fragment_gap: u32,
    ) -> Result<(uuid::Uuid, StoredCircuit), String> {
        let discovery = self
            .discover_selection(player, max_components, 1, fragment_gap)
            .await?;
        let target = discovery.candidate.seed;
        let (bounds, dimension) = self.selected_region(player).await?;
        let snapshot = self
            .bridge
            .scan_region(bounds.min, bounds.max, &dimension)
            .await
            .map_err(|error| error.to_string())?;
        let complete = !discovery.expansion.limit_reached;
        let circuit = StoredCircuit {
            player: player.to_owned(),
            dimension,
            bounds,
            target: Some(target),
            snapshot,
            expansion: serde_json::to_value(&discovery.expansion)
                .map_err(|error| error.to_string())?,
            complete,
            expires_at: Instant::now() + CIRCUIT_SNAPSHOT_TTL,
        };
        let circuit_id = self.store_circuit(circuit.clone()).await;
        Ok((circuit_id, circuit))
    }

    async fn resolve_circuit_snapshot(
        &self,
        player: &str,
        circuit_id: Option<&str>,
        max_components: Option<usize>,
        fragment_gap: u32,
    ) -> Result<(uuid::Uuid, StoredCircuit), String> {
        match circuit_id {
            Some(circuit_id) => self.load_circuit(circuit_id, player).await,
            None => {
                self.capture_looked_at_circuit(player, max_components, fragment_gap)
                    .await
            }
        }
    }
}

fn bounds_for_changes(
    changes: &[PhysicalBlockChange],
) -> Option<dustroute_translate::world_reverse::RegionBounds> {
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
    Some(dustroute_translate::world_reverse::RegionBounds::new(
        min, max,
    ))
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

/// Compare a live block with the state captured by a placement plan.
///
/// A plan can contain synthetic blocks produced by the Rust translator while
/// the live scan contains the namespaced Java block name and all block-state
/// properties.  Those representation details must not make an untouched plan
/// stale.  Conversely, static identity (for example stone vs. dirt, repeater
/// orientation, or delay) must still be checked.  Dynamic power is ignored for
/// circuit elements because neighbour updates legitimately change it after a
/// placement, but externally controllable inputs are checked so a user cannot
/// toggle an input between preview and apply/undo without invalidating the
/// plan.
fn placement_baseline_matches(
    actual: Option<&dustroute_physical::Block>,
    expected: &dustroute_physical::Block,
) -> bool {
    let Some(actual) = actual else {
        return expected.kind == BlockKind::Air;
    };
    if actual.kind != expected.kind {
        return false;
    }
    if expected.kind == BlockKind::Air {
        return true;
    }

    // An observed name is authoritative when the plan captured one.  A
    // synthetic block has no name, so the live name is intentionally allowed.
    if expected.observed_name.is_some() && actual.observed_name != expected.observed_name {
        return false;
    }
    if (expected.kind != BlockKind::RedstoneTorch
        && expected
            .facing
            .is_some_and(|facing| actual.facing != Some(facing)))
        || expected
            .delay
            .is_some_and(|delay| actual.delay != Some(delay))
        || expected
            .support_offset
            .is_some_and(|support| actual.support_offset != Some(support))
        || expected
            .wire_connections
            .as_ref()
            .is_some_and(|connections| actual.wire_connections.as_ref() != Some(connections))
    {
        return false;
    }

    // Keep static observed properties useful for stale detection without
    // comparing neighbour-driven state (power, lamp/torch lit state, wire arm
    // shape, or repeater lock).
    const DYNAMIC_PROPERTIES: [&str; 8] = [
        "power", "powered", "lit", "locked", "north", "east", "south", "west",
    ];
    if expected
        .observed_properties
        .iter()
        .filter(|(name, _)| !DYNAMIC_PROPERTIES.contains(&name.as_str()))
        .any(|(name, value)| actual.observed_properties.get(name) != Some(value))
    {
        return false;
    }

    if matches!(
        expected.kind,
        BlockKind::Lever | BlockKind::Button | BlockKind::PressurePlate
    ) {
        // Synthetic input blocks default to unpowered in Minecraft.  Treat
        // that default as part of the captured state so a manual toggle is
        // detected even when the plan did not carry an explicit `powered`.
        let expected_powered = expected.powered.unwrap_or(false);
        if actual.powered != Some(expected_powered) {
            return false;
        }
        if expected.kind == BlockKind::PressurePlate
            && expected
                .power_level
                .is_some_and(|level| actual.power_level != Some(level))
        {
            return false;
        }
    }
    true
}

fn boundary_block_record(
    block: &dustroute_translate::snapshot::MinecraftSnapshotBlock,
) -> BoundaryBlockRecord {
    let static_properties = block
        .properties
        .iter()
        .filter(|(name, _)| {
            !matches!(
                name.as_str(),
                "power" | "powered" | "lit" | "locked" | "north" | "east" | "south" | "west"
            )
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    BoundaryBlockRecord {
        pos: block.pos,
        name: block.name.clone(),
        static_properties,
    }
}

fn observed_java_block_state(
    block: &dustroute_translate::snapshot::MinecraftSnapshotBlock,
) -> String {
    if block.properties.is_empty() {
        return block.name.clone();
    }
    let properties = block
        .properties
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}[{properties}]", block.name)
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
    async fn get_world(
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
            || !(1..=16).contains(&component_gap)
            || !(1.0..=256.0).contains(&max_distance)
            || !(1..=2048).contains(&max_listed_blocks)
        {
            return json_text(json!({
                "ok": false,
                "error": "max_components must be 1..32768, component_gap 1..16, max_distance 1..256, and max_listed_blocks 1..2048"
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
        let scan = match (CircuitCapture {
            bridge: &self.bridge,
            policy: &self.policy,
        })
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
    async fn set_region(&self, Parameters(params): Parameters<MarkCornerParams>) -> String {
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
        let selected = selections
            .entry(player.clone())
            .or_insert_with(|| LocatedSelection::new(&player));
        let result = match params.corner.as_str() {
            "first" => {
                selected.session.mark_first(target);
                selected.dimension = Some(observation.dimension.clone());
                json!({ "ok": true, "corner": "first", "position": target })
            }
            "second" => match selected
                .dimension
                .as_ref()
                .filter(|dimension| *dimension == &observation.dimension)
            {
                None => {
                    json!({ "ok": false, "error": "player changed dimension between region corners" })
                }
                Some(_) => match selected.session.mark_second(target) {
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
        match self
            .discover_selection(
                &player,
                params.max_components,
                params.padding.unwrap_or(1),
                params.fragment_gap.unwrap_or(2),
            )
            .await
        {
            Ok(discovery) => json_text(discovery.response()),
            Err(error) => json_text(json!({"ok": false, "error": error})),
        }
    }

    #[tool(
        description = "Capture or reuse an immutable circuit snapshot and return a compact health summary with shared diagnostic findings and bounded focused_explanation (local role, directed edges, terminal candidates, paths, and timing caveats). Omit circuit_id to capture the current gaze; pass the returned circuit_id to keep later analysis on the same circuit",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn test_circuit(&self, Parameters(params): Parameters<DiagnoseLookedAtParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let (circuit_id, circuit) = match self
            .resolve_circuit_snapshot(
                &player,
                params.circuit_id.as_deref(),
                params.max_components,
                params.fragment_gap.unwrap_or(2),
            )
            .await
        {
            Ok(circuit) => circuit,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let target = circuit.target;
        let bounds = circuit.bounds;
        let dimension = circuit.dimension;
        let snapshot = circuit.snapshot;
        let mechanisms = self
            .observed_mechanisms(&snapshot, circuit.complete, &dimension)
            .await;
        let world = match world_from_snapshot_for_service(&snapshot) {
            Ok(world) => world,
            Err(error) => {
                return json_text(
                    json!({ "ok": false, "error": error, "circuit_id": circuit_id, "mechanisms": mechanisms }),
                );
            }
        };
        let mut analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
        analysis.scene.observation.dimension = dimension;
        let complete = circuit.complete;
        let diagnostic =
            dustroute_translate::diagnostic::diagnose_scene(&analysis.scene, target, complete);
        let focused_explanation = target.map(|target| {
            let focused = self.app.analyze_physical(
                &world,
                ReverseRequest::new(bounds).with_observation_complete(complete),
            );
            focused_explanation_json(&focused, target, complete)
        });
        json_text(json!({
            "ok": true,
            "schema_version": DIAGNOSTIC_SCHEMA_V1,
            "analysis_mode": "focused_fast",
            "mechanisms": mechanisms,
            "circuit_id": circuit_id,
            "circuit_expires_in_seconds": CIRCUIT_SNAPSHOT_TTL.as_secs(),
            "mutation_performed": false,
            "target": target,
            "bounds": bounds_json(bounds),
            "expansion": circuit.expansion,
            "diagnostic": diagnostic,
            "focused_explanation": focused_explanation,
            "detail_tools": {
                "full_conversion": "convert_from_circuit",
                "raw_observation": "get_world",
                "repair_planning": "new_repair"
            }
        }))
    }

    #[tool(
        description = "Get a bounded mixed-IR summary from an immutable circuit snapshot. Omit circuit_id to capture the current gaze, then reuse circuit_id with analysis_id and node_id for stable detail expansion.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_circuit_ir(
        &self,
        Parameters(params): Parameters<GetLookedAtCircuitIrParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let (circuit_id, circuit) = match self
            .resolve_circuit_snapshot(
                &player,
                params.circuit_id.as_deref(),
                params.max_components,
                params.fragment_gap.unwrap_or(2),
            )
            .await
        {
            Ok(circuit) => circuit,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let target = circuit.target;
        let bounds = circuit.bounds;
        let dimension = circuit.dimension;
        let snapshot = circuit.snapshot;
        let snapshot_json = match serde_json::to_string(&snapshot) {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let mut analysis_hasher = DefaultHasher::new();
        snapshot_json.hash(&mut analysis_hasher);
        let analysis_id = format!("{:016x}", analysis_hasher.finish());
        if params.node_id.is_some() && params.analysis_id.as_deref() != Some(analysis_id.as_str()) {
            return json_text(json!({
                "ok": false,
                "error": if params.analysis_id.is_some() {
                    "the observed circuit changed after the mixed-IR summary; request a new summary before expanding a node"
                } else {
                    "analysis_id from a mixed-IR summary is required when node_id is specified"
                },
                "current_analysis_id": analysis_id,
                "retryable": true,
            }));
        }
        let world = match world_from_snapshot_for_service(&snapshot) {
            Ok(world) => world,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let mut analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
        analysis.scene.observation.dimension = dimension;
        let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
        let mixed_ir = match mixed_ir_json(&hierarchy, params.node_id) {
            Ok(mixed_ir) => mixed_ir,
            Err(error) => {
                return json_text(json!({
                    "ok": false,
                    "error": error,
                    "available_node_count": dustroute_ir::build_mixed_ir(&hierarchy).nodes.len(),
                }));
            }
        };
        json_text(json!({
            "ok": true,
            "analysis_mode": "mixed_ir",
            "circuit_id": circuit_id,
            "analysis_id": analysis_id,
            "mutation_performed": false,
            "target": target,
            "bounds": bounds_json(bounds),
            "analysis_complete": circuit.complete,
            "expansion": circuit.expansion,
            "mixed_ir": mixed_ir,
            "guidance": if params.node_id.is_some() {
                "Use the physical block details and directed neighbors to explain or diagnose this node in context."
            } else {
                "Choose a node_id from this bounded graph and call this tool again to expand only that node."
            }
        }))
    }

    #[tool(
        description = "Use blueprint.action=generate_flying_machine to author a bounded finite-flight candidate with typed engine, body, distance, rotation, reflection and attachments; generation returns unadopted records and fresh checks, never world writes. Or use blueprint.action to import unverified source/state records, capture_revision from a saved circuit revision, optimize the supplied Assembly or an explicit component body for fewer actual blocks preserving only an explicit behavioral type, enumerate_layouts for freshly checked torch/support placements, or propose_update with explicit new definitions and candidate state. Component scope separates body_positions from fixed external equipment and reports body/total counts. Device outputs can directly observe a torch. Blueprint optimize and enumerate_layouts return candidate data without publishing it; inspect moved ports and removed interpretations before proposing parent changes. Blueprint proposals continue through show_operation and invoke_operation. Otherwise create an immutable hypothetical circuit revision from exactly one circuit_id or revision_id. Apply up to 64 full block-state edits (add, replace, or air to delete), save before/after diagnostics, bounded initial-state simulation and a separate modeled Assembly Revision when decodable. Blueprint source references stay pinned; observation alone does not infer them. Empty changes copies the source. Never changes Minecraft or authorizes placement.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn test_circuit_change(
        &self,
        Parameters(params): Parameters<TestCircuitChangeParams>,
    ) -> String {
        if let Some(write) = params.blueprint {
            if !params.changes.is_empty()
                || params.simulation_ticks.is_some()
                || params.circuit_id.is_some()
                || params.revision_id.is_some()
            {
                return json_text(crate::blueprint_mcp::failure(
                    "blueprint operations cannot be combined with circuit block-edit parameters",
                ));
            }
            let command = if let crate::blueprint_mcp::BlueprintWrite::CaptureRevision {
                revision_id,
            } = write
            {
                let player = match self.resolve_player(params.player.as_deref()) {
                    Ok(p) => p,
                    Err(e) => return json_text(crate::blueprint_mcp::failure(e)),
                };
                if let Some(error) = self.authorize_player(&player) {
                    return error;
                }
                let (record, grounding) = match self.load_revision(&revision_id, &player).and_then(|r| {
                    let record = r.assembly.clone().ok_or_else(|| "revision has no decodable Assembly".to_owned())?;
                    let base_snapshot = r.base_snapshot.clone().ok_or_else(|| "revision has no retained base snapshot; capture from a complete observation first".to_owned())?;
                    Ok((record.clone(), crate::blueprint_mcp::AssemblyGrounding {
                        assembly_revision_id: record.id,
                        circuit_revision_id: r.revision_id,
                        base_observation_id: r.base_observation_id,
                        dimension: r.dimension,
                        complete: r.complete,
                        base_snapshot,
                    }))
                }) {
                    Ok(value) => value,
                    Err(e) => return json_text(crate::blueprint_mcp::failure(e)),
                };
                crate::blueprint_mcp::Command::Capture {
                    record: Box::new(record),
                    grounding,
                }
            } else {
                crate::blueprint_mcp::Command::Write(write)
            };
            return self
                .blueprint_command(command, params.player.as_deref(), true)
                .await
                .expect("explicit blueprint response");
        }
        let result: Result<Value,String> = async {
            let player=self.resolve_player(params.player.as_deref())?;
            if let Some(error)=self.authorize_player(&player) {return Err(error);}
            let ticks=params.simulation_ticks.unwrap_or(64);
            if !(1..=256).contains(&ticks) {return Err("simulation_ticks must be 1 through 256".into());}
            let (base_observation_id,parents,dimension,target,complete,baseline,base_snapshot,parent_assembly)=match (params.circuit_id.as_deref(),params.revision_id.as_deref()) {
                (Some(id),None)=>{
                    let (id,c)=self.load_circuit(id,&player).await?;
                    (id,vec![],c.dimension,c.target,c.complete,c.snapshot.clone(),Some(c.snapshot),None)
                },
                (None,Some(id))=>{
                    let r=self.load_revision(id,&player)?;
                    (r.base_observation_id,vec![r.revision_id],r.dimension,r.target,r.complete,r.snapshot,r.base_snapshot,r.assembly)
                },
                _=>return Err("provide exactly one of circuit_id or revision_id".into()),
            };
            self.policy.authorize_dimension(&dimension).map_err(|e|e.to_string())?;
            self.policy.validate_region(dustroute_translate::world_reverse::RegionBounds::new(baseline.min,baseline.max)).map_err(|e|e.to_string())?;
            let edits=params.changes.into_iter().map(|c| dustroute_translate::snapshot::MinecraftSnapshotBlock{pos:Pos::new(c.position.x,c.position.y,c.position.z),name:c.block,properties:c.properties}).collect();
            let (snapshot,changes)=crate::revision::apply(&baseline,edits)?;
            let focus=target.unwrap_or(snapshot.min);
            let before=revision_validation(&baseline,&dimension,focus,complete,ticks);
            let after=revision_validation(&snapshot,&dimension,focus,complete,ticks);
            let mut revision=crate::revision::CircuitRevision{
                schema_version:"dustroute.circuit-revision.v1".into(),revision_id:uuid::Uuid::new_v4(),parent_revision_ids:parents,base_observation_id,player,dimension,target,complete,snapshot,base_snapshot,changes,
                validation:json!({"before":before,"after":after,"simulation_ticks":ticks,"scope":"initial_state_only; no functional equivalence or live-world guarantee"}),
                assembly:None,
            };
            match revision.capture_assembly(parent_assembly.as_ref()) {
                Ok(record)=>{
                    let checked=dustroute_translate::assembly::validate_assembly(dustroute_library::builtin_blueprints::builtin_blueprints(),&record.assembly);
                    revision.validation["assembly"]=match checked {
                        Ok(_)=>json!({"status":"placement_and_declared_connections_valid","scope":"modeled state only; no behavioral or live-world proof"}),
                        Err(error)=>json!({"status":"invalid_or_unsupported","error":error.to_string()}),
                    };
                    revision.assembly=Some(record);
                },
                Err(error)=>{
                    if parent_assembly.as_ref().is_some_and(|parent| !parent.assembly.instances.is_empty()) {
                        return Err(format!("cannot retain pinned blueprint interpretations for this state: {error}"));
                    }
                    revision.validation["assembly"]=json!({"status":"unavailable","error":error});
                },
            }
            if serde_json::to_vec(&revision).map_err(|e|e.to_string())?.len()>crate::revision::MAX_BYTES {return Err("revision record exceeds 4 MiB".into());}
            self.state_store.save(PlanRecordKind::CircuitRevisions,revision.revision_id,&revision)?;
            Ok(revision_json(&revision,false))
        }.await;
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }

    fn load_revision(
        &self,
        id: &str,
        player: &str,
    ) -> Result<crate::revision::CircuitRevision, String> {
        let id = uuid::Uuid::parse_str(id).map_err(|e| format!("invalid revision_id: {e}"))?;
        let r: crate::revision::CircuitRevision = self
            .state_store
            .load(PlanRecordKind::CircuitRevisions, id)?
            .ok_or("unknown or expired revision_id")?;
        if r.player != player
            || r.revision_id != id
            || r.schema_version != "dustroute.circuit-revision.v1"
            || r.parent_revision_ids.len() > 1
        {
            return Err("revision identity, owner or schema mismatch".into());
        }
        self.policy
            .authorize_dimension(&r.dimension)
            .map_err(|e| e.to_string())?;
        self.policy
            .validate_region(dustroute_translate::world_reverse::RegionBounds::new(
                r.snapshot.min,
                r.snapshot.max,
            ))
            .map_err(|e| e.to_string())?;
        r.check_assembly_record()?;
        Ok(r)
    }

    #[tool(
        description = "Use blueprint.kind to list the catalog or read a Blueprint, Assembly, type, classification, or exported archive. Blueprint IDs are separate from hypothetical revision_id and never grant placement permission. Otherwise read a saved immutable hypothetical revision: parent IDs, original observation ID, edits, stored validation and Assembly Revision summary. include_snapshot=true also returns the literal snapshot and modeled assembly, including pinned source references when present. Source definitions are separate and are not rewritten by edits. This is not fresh observation or a live circuit_id.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_circuit_revision(
        &self,
        Parameters(params): Parameters<GetCircuitRevisionParams>,
    ) -> String {
        if let Some(query) = params.blueprint {
            if !params.revision_id.is_empty() || params.include_snapshot.is_some() {
                return json_text(crate::blueprint_mcp::failure(
                    "blueprint queries cannot be combined with revision_id/include_snapshot",
                ));
            }
            return self
                .blueprint_command(crate::blueprint_mcp::Command::Read(query), None, true)
                .await
                .expect("explicit blueprint response");
        }
        let result: Result<Value, String> = (|| {
            let player = self.resolve_player(None)?;
            if let Some(error) = self.authorize_player(&player) {
                return Err(error);
            }
            let revision = self.load_revision(&params.revision_id, &player)?;
            Ok(revision_json(
                &revision,
                params.include_snapshot.unwrap_or(false),
            ))
        })();
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }

    #[tool(
        description = "Convert a captured or selected physical circuit into bounded physical, mixed-IR, and higher-level identity summaries with focused_explanation for the gaze target. Reuse circuit_id to avoid following a moved gaze. Set include_truth_table=true for explicitly bounded exhaustive functional inference, including large circuits. Repair and transition planning are separate tools.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn convert_from_circuit(
        &self,
        Parameters(params): Parameters<AnalyzeLookedAtParams>,
    ) -> String {
        match params.scope.as_deref() {
            None | Some("gaze") => {}
            Some("selected_region") => return self.convert_from_selected_region(params).await,
            Some(_) => {
                return error_text(
                    McpErrorCode::InvalidArgument,
                    "scope must be gaze or selected_region",
                    false,
                );
            }
        }
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let (circuit_id, circuit) = match self
            .resolve_circuit_snapshot(
                &player,
                params.circuit_id.as_deref(),
                params.max_components,
                params.fragment_gap.unwrap_or(2),
            )
            .await
        {
            Ok(circuit) => circuit,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let target = circuit.target;
        let bounds = circuit.bounds;
        let dimension = circuit.dimension;
        let snapshot = circuit.snapshot;
        let mechanisms = self
            .observed_mechanisms(&snapshot, circuit.complete, &dimension)
            .await;
        let world = match world_from_snapshot_for_service(&snapshot) {
            Ok(world) => world,
            Err(error) => {
                return json_text(
                    json!({ "ok": false, "error": error, "circuit_id": circuit_id, "mechanisms": mechanisms }),
                );
            }
        };
        let discovered_components = circuit.expansion["components_loaded"]
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .unwrap_or(0);
        let request = match reverse_request_for_truth_table(
            bounds,
            TruthTableRequestOptions {
                include_truth_table: params.include_truth_table.unwrap_or(false),
                max_inputs: params.truth_table_max_inputs,
                settle_ticks: params.truth_table_settle_ticks,
                max_rows: params.truth_table_max_rows,
                max_work_units: params.truth_table_max_work_units,
                max_solver_iterations: params.truth_table_max_solver_iterations,
                max_elapsed_millis: params.truth_table_max_elapsed_millis,
            },
        ) {
            Ok(request) => request,
            Err(error) => return error_text(McpErrorCode::InvalidArgument, error, false),
        }
        .with_observation_complete(circuit.complete);
        if discovered_components > MAX_FLAT_ANALYSIS_COMPONENTS && !request.infer_truth_table {
            let mut analysis =
                dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
            analysis.scene.observation.dimension = dimension;
            let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
            let focused = target
                .map(|target| focused_hierarchy_role_json(&analysis.scene, &hierarchy, target))
                .unwrap_or(Value::Null);
            let mut result =
                hierarchical_result_json(bounds, &hierarchy, focused, &circuit.expansion, target);
            if let Some(object) = result.as_object_mut() {
                object.insert("mechanisms".into(), mechanisms.clone());
                object.insert(
                    "circuit_identity".to_owned(),
                    circuit_identity_json(&hierarchy, None, circuit.complete, 0),
                );
                object.insert(
                    "diagnostic".to_owned(),
                    serde_json::to_value(dustroute_translate::diagnostic::diagnose_scene(
                        &analysis.scene,
                        target,
                        circuit.complete,
                    ))
                    .unwrap_or(Value::Null),
                );
                object.insert("circuit_id".to_owned(), json!(circuit_id));
                object.insert(
                    "next_tools".to_owned(),
                    json!({
                        "ir_detail": "get_circuit_ir",
                        "repair_planning": "new_repair",
                        "transition_planning": "new_transition_test"
                    }),
                );
            }
            return json_text(result);
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
        if !circuit.complete {
            mark_observation_incomplete(&mut staged.reverse.analysis.scene);
            mark_observation_incomplete(&mut staged.hierarchy.physical_snapshot.value.scene);
            mark_observation_incomplete(&mut staged.hierarchy.physical_graph.value.scene);
        }
        let translated = &staged.reverse;
        let macro_candidates = translated.functional_network.as_ref().map(|model| {
            find_builtin_verified_macro_replacements(
                model,
                "java",
                "1.21.11",
                ObservedMacroMetrics::from_world(&world),
            )
        });
        let macro_plans = translated.functional_network.as_ref().and_then(|model| {
            let boundary = extract_model_boundary_with_context(model, &world, &translated.analysis);
            let reserved = boundary
                .iter()
                .filter_map(|port| port.driver_position)
                .collect::<BTreeSet<_>>();
            let boundary_positions = boundary
                .iter()
                .map(|port| port.position)
                .collect::<BTreeSet<_>>();
            let replaceable = world
                .positions()
                .filter(|pos| !boundary_positions.contains(pos))
                .collect::<BTreeSet<_>>();
            macro_candidates.as_ref().map(|candidates| {
                candidates
                    .iter()
                    .filter_map(|candidate| {
                        let mut plan =
                            plan_macro_replacement_with_reserved(candidate, &boundary, &reserved)
                                .ok()?;
                        let structural = validate_macro_structure(&plan, &world, &replaceable);
                        plan.verification.structural = if structural.valid() {
                            ContextualVerificationState::Passed
                        } else {
                            ContextualVerificationState::Failed
                        };
                        let materialized = materialize_macro_replacement_in_known_regions(
                            &plan,
                            &world,
                            &[dustroute_translate::world::Region::new(
                                snapshot.min,
                                snapshot.max,
                            )],
                            &replaceable,
                            14,
                        );
                        let steady_state = materialized.as_ref().ok().map(|materialized| {
                            verify_macro_steady_state(
                                &model.truth_table,
                                &world,
                                &materialized.world,
                                8,
                                64,
                            )
                        });
                        if let Some(report) = &steady_state {
                            plan.verification.steady_state = report.state;
                        }
                        let transitions = materialized.as_ref().ok().and_then(|materialized| {
                            (plan.verification.steady_state == ContextualVerificationState::Passed)
                                .then(|| {
                                    verify_macro_transitions(
                                        &model.truth_table,
                                        &world,
                                        &materialized.world,
                                        64,
                                        16,
                                        4,
                                    )
                                })
                        });
                        if let Some(report) = &transitions {
                            plan.verification.transitions = report.state;
                        }
                        let contract = OptimizationContract::default();
                        let contract_assessment = materialized.as_ref().ok().map(|materialized| {
                            assess_macro_contract(
                                contract,
                                &structural,
                                steady_state.as_ref(),
                                transitions.as_ref(),
                                materialized.patch.changes.len(),
                                None,
                            )
                        });
                        Some((
                            plan,
                            structural,
                            materialized,
                            steady_state,
                            transitions,
                            contract,
                            contract_assessment,
                        ))
                    })
                    .collect::<Vec<_>>()
            })
        });
        let focused = target
            .map(|target| focused_role_json(translated, target))
            .unwrap_or(Value::Null);
        let incomplete = !circuit.complete;
        let mut result = reverse_result_json(bounds, translated);
        if let Some(object) = result.as_object_mut() {
            object.insert("mechanisms".into(), mechanisms.clone());
            object.insert("circuit_id".to_owned(), json!(circuit_id));
            object.insert(
                "circuit_identity".to_owned(),
                circuit_identity_json(
                    &staged.hierarchy,
                    Some(&staged.logical_role),
                    !incomplete,
                    0,
                ),
            );
            object.insert(
                "diagnostic".to_owned(),
                serde_json::to_value(dustroute_translate::diagnostic::diagnose_scene(
                    &translated.analysis.scene,
                    target,
                    !incomplete,
                ))
                .unwrap_or(Value::Null),
            );
            object.insert("focused_component".to_owned(), focused);
            if let Some(target) = target {
                object.insert(
                    "focused_explanation".to_owned(),
                    focused_explanation_json(&staged, target, !incomplete),
                );
            }
            object.insert(
                "discovery".to_owned(),
                json!({ "seed": target, "bounds": bounds_json(bounds) }),
            );
            object.insert("analysis_complete".to_owned(), Value::Bool(!incomplete));
            object.insert(
                "macro_replacement_candidates".to_owned(),
                macro_candidates.as_ref().map_or(Value::Null, |candidates| json!({
                    "status": "proposal_only",
                    "realization": "contextual placement and transition verification are required before a mutation plan can be created",
                    "candidates": candidates.iter().map(|candidate| json!({
                        "component_id": candidate.component_id.as_str(),
                        "name": candidate.name,
                        "kind": candidate.kind,
                        "layout_reference": candidate.layout_reference,
                        "input_ports": candidate.input_ports,
                        "output_ports": candidate.output_ports,
                        "physical": candidate.physical,
                        "saved_blocks": candidate.saved_blocks,
                        "saved_volume": candidate.saved_volume,
                        "requires_contextual_transition_verification": candidate.requires_contextual_transition_verification,
                    })).collect::<Vec<_>>()
                    ,"placement_plans": macro_plans.as_ref().map(|plans| plans.iter().map(|(plan, structural, materialized, steady_state, transitions, contract, contract_assessment)| json!({
                        "component_id": plan.component_id,
                        "origin": plan.placed.origin,
                        "rotation_y": format!("{:?}", plan.placed.rotation).to_lowercase(),
                        "total_route_length": plan.total_route_length,
                        "automatic_apply_allowed": plan.automatic_apply_allowed,
                        "contract": optimization_contract_json(*contract),
                        "contract_assessment": contract_assessment.as_ref().map(contract_assessment_json),
                        "structural_report": {
                            "valid": structural.valid(),
                            "candidate_collisions": structural.candidate_collisions,
                            "route_collisions": structural.route_collisions,
                            "route_cross_net_contacts": structural.route_cross_net_contacts.iter().map(|(first, second, a, b)| json!({
                                "first_route": first,
                                "second_route": second,
                                "first_position": a,
                                "second_position": b,
                            })).collect::<Vec<_>>(),
                            "candidate_support_issues": structural.candidate_support_issues,
                            "required_route_supports": structural.required_route_supports,
                            "blocked_route_supports": structural.blocked_route_supports,
                        },
                        "materialization": match materialized {
                            Ok(materialized) => json!({
                                "status": "preview_ready",
                                "change_count": materialized.patch.changes.len(),
                                "added_supports": materialized.added_supports,
                                "inserted_repeaters": materialized.inserted_repeaters,
                                "patch": materialized.patch,
                            }),
                            Err(error) => json!({
                                "status": "unavailable",
                                "reason": format!("{error:?}"),
                            }),
                        },
                        "steady_state_report": steady_state.as_ref().map(|report| json!({
                            "state": format!("{:?}", report.state).to_lowercase(),
                            "comparison": report.comparison,
                            "input_mapping": report.input_mapping,
                            "output_mapping": report.output_mapping,
                            "differing_assignments": report.differing_assignments,
                            "reason": report.reason,
                        })),
                        "transition_report": transitions.as_ref().map(|report| json!({
                            "state": format!("{:?}", report.state).to_lowercase(),
                            "case_count": report.cases.len(),
                            "differing_cases": report.differing_cases,
                            "reason": report.reason,
                            "cases": report.cases.iter().map(|case| json!({
                                "from": case.from,
                                "to": case.to,
                                "equivalent": case.equivalent,
                                "first_difference_tick": case.first_difference_tick,
                                "original_transitions": case
                                    .original_transition_edges()
                                    .iter()
                                    .map(|transition| json!({
                                        "at_tick": transition.at_tick,
                                        "from": transition.from,
                                        "to": transition.to,
                                        "elapsed_from_previous": transition.elapsed_from_previous,
                                    }))
                                    .collect::<Vec<_>>(),
                                "candidate_transitions": case
                                    .candidate_transition_edges()
                                    .iter()
                                    .map(|transition| json!({
                                        "at_tick": transition.at_tick,
                                        "from": transition.from,
                                        "to": transition.to,
                                        "elapsed_from_previous": transition.elapsed_from_previous,
                                    }))
                                    .collect::<Vec<_>>(),
                                "original_outputs": case.original_outputs,
                                "candidate_outputs": case.candidate_outputs,
                            })).collect::<Vec<_>>(),
                        })),
                        "verification": {
                            "structural": format!("{:?}", plan.verification.structural).to_lowercase(),
                            "steady_state": format!("{:?}", plan.verification.steady_state).to_lowercase(),
                            "transitions": format!("{:?}", plan.verification.transitions).to_lowercase(),
                        },
                        "routes": plan.routes.iter().map(|route| json!({
                            "direction": format!("{:?}", route.boundary.direction).to_lowercase(),
                            "observed_index": route.boundary.observed_index,
                            "boundary_position": route.boundary.position,
                            "boundary_facing": route.boundary.facing,
                            "driver_position": route.boundary.driver_position,
                            "candidate_port": route.candidate_port,
                            "candidate_position": route.candidate_position,
                            "path": route.path,
                        })).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>()).unwrap_or_default(),
                })),
            );
            object.insert(
                "next_tools".to_owned(),
                json!({
                    "ir_detail": "get_circuit_ir",
                    "repair_planning": "new_repair",
                    "transition_planning": "new_transition_test"
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
        description = "Plan a built-in circuit, a grounded revision at its original coordinates, or an adopted electrical Assembly at a new assembly_target (source_anchor, target_anchor, rotation). Custom construction freshly reviews the target and simulates ordered installation/removal with full readback after each step. Returns a previewable operation and conditional undo without changing the world"
    )]
    async fn new_placement(
        &self,
        Parameters(params): Parameters<PreviewPlacementParams>,
    ) -> String {
        if params.assembly_target.is_some() {
            return json_text(
                self.assembly_service()
                    .plan_assembly_construction(params)
                    .await,
            );
        }
        if params.assembly_revision_id.is_some() {
            return self.plan_adopted_assembly_placement(params).await;
        }
        if params.revision_id.is_some() {
            return self.plan_revision_placement(params).await;
        }
        if params.circuit == "piston-door-1x2" {
            return self.plan_piston_placement(params).await;
        }
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
                    "error": "unknown circuit; expected half-adder, half-subtractor, mux2, decoder1to2, full-adder, or piston-door-1x2"
                }));
            }
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        let (proposed_world, assembly, optimization) = if params.optimize.unwrap_or(false) {
            let optimization_plan = OptimizationPlan::directional_then_global(
                CompressionAxis::X,
                CompressionDirection::TowardMinimum,
                AnchorPolicy::Inputs,
            );
            let realized = match realize_staged_optimization_against(
                &translated.compiled.physical,
                &translated.compiled.world,
                &translated.compiled.routing,
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
            let assembly = match realized.capture_assembly() {
                Ok(assembly) => assembly,
                Err(error) => {
                    return json_text(
                        json!({"ok":false,"error":format!("cannot capture optimized blueprint state: {error}")}),
                    );
                }
            };
            (
                realized.world,
                assembly,
                Some(json!({
                    "strategy": "directional_x_toward_minimum_then_global",
                    "safety": safety_label,
                    "safety_details": format!("{safety:?}"),
                    "topology_preserved": verification.topology_preserved,
                    "phases": phases
                })),
            )
        } else {
            let assembly = match translated.compiled.capture_assembly(&params.circuit) {
                Ok(assembly) => assembly,
                Err(error) => {
                    return json_text(
                        json!({"ok":false,"error":format!("cannot capture compiled blueprint state: {error}")}),
                    );
                }
            };
            (
                translated.compiled.world.clone().into_world(),
                assembly,
                None,
            )
        };
        if let Err(error) = dustroute_translate::assembly::validate_assembly(
            dustroute_library::builtin_blueprints::builtin_blueprints(),
            &assembly,
        ) {
            return json_text(
                json!({"ok":false,"error":format!("proposed assembly failed validation: {error}")}),
            );
        }
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
        let placement_bounds = dustroute_translate::world_reverse::RegionBounds::new(min, max);
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
        let existing = match world_from_snapshot_for_service(&snapshot) {
            Ok(world) => world,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let proposed_world =
            match dustroute_translate::world::ValidatedWorld::try_from(proposed_world) {
                Ok(world) => world,
                Err(error) => {
                    return json_text(
                        json!({ "ok": false, "error": error.to_string(), "validation": error }),
                    );
                }
            };
        let mut plan = match plan_world_overlay(
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
        let assembly_id = crate::revision::assembly_id(uuid::Uuid::new_v4());
        let assembly_summary = json!({
            "assembly_revision_id": assembly_id,
            "coordinate_origin": origin,
            "source_revision_ids": assembly.instances.iter().map(|instance| &instance.revision).collect::<std::collections::BTreeSet<_>>(),
            "source_instances": assembly.instances.len(),
            "connections": assembly.connections.len(),
            "scope": "proposed composed circuit state; full data in show_operation"
        });
        plan.assembly = Some(dustroute_app::PlacementAssembly {
            coordinate_origin: origin,
            revision: dustroute_library::assembly::AssemblyRevision {
                id: assembly_id,
                parents: vec![],
                assembly,
            },
        });
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
            "assembly_state": assembly_summary,
            "next_step": if self.policy.read_only {
                "review this plan; writes are disabled by policy"
            } else {
                "call show_operation, obtain explicit player confirmation, then call invoke_operation with confirm=true"
            }
        });
        self.plans
            .placements()
            .lock()
            .await
            .insert(plan, observation.dimension, None);
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
        description = "Retrieve the complete placement and exact undo plan by operation ID, including a separate proposed Assembly Revision and its coordinate origin when available. Blueprint references are pinned source definitions; proposed state is not live evidence.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_circuit_placement(
        &self,
        Parameters(params): Parameters<OperationParams>,
    ) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        if self.plans.kind(&operation_id).await == Some(PlanKind::Assembly) {
            return json_text(
                self.assembly_service()
                    .get_assembly_construction(operation_id)
                    .await,
            );
        }
        if let Some(plan) = self
            .plans
            .table::<StoredPistonPlacement>()
            .lock()
            .await
            .get(&operation_id)
        {
            let player = match self.resolve_player(None) {
                Ok(player) => player,
                Err(error) => return json_text(json!({"ok":false,"error":error})),
            };
            if let Some(error) = self.authorize_player(&player) {
                return error;
            }
            if plan.player != player {
                return json_text(
                    json!({"ok":false,"error":"placement belongs to another player"}),
                );
            }
            return json_text(
                json!({"ok":true,"read_only":self.policy.read_only,"plan":{"operation_id":operation_id,"origin":plan.proof.origin(),"bounds":bounds_json(plan.proof.bounds()),"changes":plan.proof.writes(false),"undo_changes":plan.proof.writes(true),"previewed":plan.previewed,"state":format!("{:?}",plan.state)}}),
            );
        }
        match self.placement_view(operation_id).await {
            Ok(plan) => {
                json_text(json!({"ok": true, "read_only": self.policy.read_only, "plan": plan}))
            }
            Err(error) => json_text(json!({"ok": false, "error": error})),
        }
    }

    #[tool(
        description = "Show the player's selected region in the Minecraft world before analysis or mutation"
    )]
    async fn show_region(&self, Parameters(params): Parameters<PlayerParams>) -> String {
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
                let mechanisms = self.observed_mechanisms(&snapshot, true, &dimension).await;
                let circuit_id = self
                    .store_circuit(StoredCircuit {
                        player,
                        dimension,
                        bounds,
                        target: None,
                        snapshot,
                        expansion: json!({
                            "strategy": "explicit_selected_region",
                            "component_limit": null,
                            "limit_reached": false
                        }),
                        complete: true,
                        expires_at: Instant::now() + CIRCUIT_SNAPSHOT_TTL,
                    })
                    .await;
                json_text(json!({
                    "ok": true,
                    "circuit_id": circuit_id,
                    "circuit_expires_in_seconds": CIRCUIT_SNAPSHOT_TTL.as_secs(),
                    "bounds": bounds_json(bounds),
                    "preview": preview,
                    "source": "fresh_scan",
                    "mechanisms": mechanisms
                }))
            }
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    async fn convert_from_selected_region(&self, params: AnalyzeLookedAtParams) -> String {
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
        let circuit_id = self
            .store_circuit(StoredCircuit {
                player,
                dimension: dimension.clone(),
                bounds,
                target: None,
                snapshot: snapshot.clone(),
                expansion: json!({
                    "strategy": "explicit_selected_region",
                    "component_limit": null,
                    "limit_reached": false
                }),
                complete: true,
                expires_at: Instant::now() + CIRCUIT_SNAPSHOT_TTL,
            })
            .await;
        let redstone_components = snapshot
            .blocks
            .iter()
            .filter(|block| is_redstone_candidate_name(&block.name))
            .count();
        let mechanisms = self.observed_mechanisms(&snapshot, true, &dimension).await;
        let world = match world_from_snapshot_for_service(&snapshot) {
            Ok(world) => world,
            Err(error) => {
                return json_text(
                    json!({ "ok": false, "error": error, "circuit_id": circuit_id, "mechanisms": mechanisms }),
                );
            }
        };
        let request = match reverse_request_for_truth_table(
            bounds,
            TruthTableRequestOptions {
                include_truth_table: params.include_truth_table.unwrap_or(false),
                max_inputs: params.truth_table_max_inputs,
                settle_ticks: params.truth_table_settle_ticks,
                max_rows: params.truth_table_max_rows,
                max_work_units: params.truth_table_max_work_units,
                max_solver_iterations: params.truth_table_max_solver_iterations,
                max_elapsed_millis: params.truth_table_max_elapsed_millis,
            },
        ) {
            Ok(request) => request,
            Err(error) => return error_text(McpErrorCode::InvalidArgument, error, false),
        }
        .with_observation_complete(true);
        if redstone_components > MAX_FLAT_ANALYSIS_COMPONENTS && !request.infer_truth_table {
            let mut analysis =
                dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
            analysis.scene.observation.dimension = dimension;
            let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
            let mut result = hierarchical_result_json(
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
            );
            if let Some(object) = result.as_object_mut() {
                object.insert("mechanisms".into(), mechanisms.clone());
                object.insert("circuit_id".to_owned(), json!(circuit_id));
            }
            return json_text(result);
        }
        let mut staged = self.app.analyze_physical(&world, request);
        staged.reverse.analysis.scene.observation.dimension = dimension;
        let mut result = reverse_result_json(bounds, &staged.reverse);
        if let Some(object) = result.as_object_mut() {
            object.insert("mechanisms".into(), mechanisms.clone());
            object.insert("circuit_id".to_owned(), json!(circuit_id));
        }
        json_text(result)
    }

    #[tool(
        description = "Create ranked, non-mutating partial repair plans from the supplied immutable circuit_id, with shared diagnostic evidence and report-local finding references"
    )]
    async fn new_repair(&self, Parameters(params): Parameters<ProposeRepairsParams>) -> String {
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
        let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
            Ok(circuit) => circuit,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let bounds = circuit.bounds;
        let dimension = circuit.dimension;
        let snapshot = circuit.snapshot;
        let world = match world_from_snapshot_for_service(&snapshot) {
            Ok(world) => world,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
        let fragments_before = analysis.scene.fragments.len();
        let mut diagnostic = dustroute_translate::diagnostic::diagnose_scene(
            &analysis.scene,
            circuit.target,
            circuit.complete,
        );
        let proposals =
            dustroute_translate::repair::propose_scene_repairs(&world, &analysis.scene, max_gap);
        diagnostic.diagnosis.assess_repair(
            if proposals.is_empty() {
                dustroute_translate::diagnostic::report::RepairStatus::NoCandidate
            } else {
                dustroute_translate::diagnostic::report::RepairStatus::PlanAvailable
            },
            None,
        );
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
                        lifecycle: RepairLifecycle::Draft,
                        contract_satisfied: true,
                        preserved_boundary: Vec::new(),
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
                    json!({
                        "patch": &proposal.patch,
                        "evidence": &proposal.evidence,
                        "impact": proposal.impact
                    }),
                )
                .await;
            response.push(json!({
                "operation_id": operation_id,
                "diagnostic_finding_ids": diagnostic.diagnosis.findings_for_repair(&proposal, &analysis.scene),
                "patch": proposal.patch,
                "evidence": proposal.evidence,
                "impact": proposal.impact,
            }));
        }
        json_text(json!({
            "schema_version": REPAIR_SCHEMA_V1,
            "ok": true,
            "circuit_id": circuit_id,
            "bounds": bounds_json(bounds),
            "fragments": fragments_before,
            "diagnostic": diagnostic,
            "proposal_count": response.len(),
            "proposals": response,
            "next_step": "review a proposal, call show_operation, ask for explicit confirmation, then call invoke_operation with confirm=true"
        }))
    }

    #[tool(
        description = "Explain repair evidence for an immutable circuit as bounded facts, competing hypotheses, counterfactual impact, related physical components, and questions for the player",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_repair_context(
        &self,
        Parameters(params): Parameters<GetRepairContextParams>,
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
            return error_text(McpErrorCode::InvalidArgument, "max_gap must be 1..8", false);
        }
        let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
            Ok(circuit) => circuit,
            Err(error) => return error_text(McpErrorCode::NotFound, error, false),
        };
        let world = match world_from_snapshot_for_service(&circuit.snapshot) {
            Ok(world) => world,
            Err(error) => return error_text(McpErrorCode::SerializationFailed, error, false),
        };
        let analysis =
            dustroute_translate::world_reverse::analyze_world_region(&world, circuit.bounds);
        let mut diagnostic = dustroute_translate::diagnostic::diagnose_scene(
            &analysis.scene,
            circuit.target,
            circuit.complete,
        );
        let proposals =
            dustroute_translate::repair::propose_scene_repairs(&world, &analysis.scene, max_gap);
        diagnostic.diagnosis.assess_repair(
            if proposals.is_empty() {
                dustroute_translate::diagnostic::report::RepairStatus::NoCandidate
            } else {
                dustroute_translate::diagnostic::report::RepairStatus::PlanAvailable
            },
            None,
        );
        let operation_id = match params.operation_id.as_deref() {
            Some(value) => match uuid::Uuid::parse_str(value) {
                Ok(id) => Some(id),
                Err(error) => {
                    return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
                }
            },
            None => None,
        };
        let selected_patch = if let Some(operation_id) = operation_id {
            let plan = match self.repair_plan(operation_id).await {
                Ok(Some(plan)) => plan,
                Ok(None) => {
                    return error_text(McpErrorCode::NotFound, "repair operation not found", false);
                }
                Err(error) => return error_text(McpErrorCode::Internal, error, false),
            };
            if plan.dimension != circuit.dimension || plan.analysis_bounds != circuit.bounds {
                return error_text(
                    McpErrorCode::InvalidArgument,
                    "operation_id does not belong to this circuit snapshot",
                    false,
                );
            }
            Some(plan.patch)
        } else {
            proposals.first().map(|proposal| proposal.patch.clone())
        };
        let selected = selected_patch
            .as_ref()
            .and_then(|patch| proposals.iter().find(|proposal| proposal.patch == *patch));

        let relevant_positions = selected
            .into_iter()
            .flat_map(|proposal| proposal.patch.changes.iter().map(|change| change.pos))
            .collect::<Vec<_>>();
        let related_components = analysis
            .scene
            .components
            .iter()
            .filter(|component| {
                relevant_positions.iter().any(|position| {
                    component.pos.x.abs_diff(position.x)
                        + component.pos.y.abs_diff(position.y)
                        + component.pos.z.abs_diff(position.z)
                        <= 2
                })
            })
            .take(32)
            .map(|component| {
                let incoming = analysis
                    .scene
                    .connections
                    .iter()
                    .filter(|connection| connection.sink.component == component.id)
                    .map(|connection| {
                        json!({
                            "component": connection.source.component,
                            "transfer": connection.transfer,
                            "confidence": connection.confidence
                        })
                    })
                    .collect::<Vec<_>>();
                let outgoing = analysis
                    .scene
                    .connections
                    .iter()
                    .filter(|connection| connection.source.component == component.id)
                    .map(|connection| {
                        json!({
                            "component": connection.sink.component,
                            "transfer": connection.transfer,
                            "confidence": connection.confidence
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "id": component.id,
                    "position": component.pos,
                    "block": component.block.kind,
                    "facing": component.block.facing,
                    "support": component.support,
                    "incoming": incoming,
                    "outgoing": outgoing
                })
            })
            .collect::<Vec<_>>();

        let mut hypotheses = Vec::new();
        if let Some(proposal) = selected {
            let mut supporting_evidence = vec![format!(
                "virtual repair contains {} non-conflicting block change(s)",
                proposal.patch.changes.len()
            )];
            if let Some(impact) = proposal.impact {
                supporting_evidence.push(format!(
                    "physical fragments change from {} to {}",
                    impact.fragments_before, impact.fragments_after
                ));
                supporting_evidence.push(format!(
                    "drive-reachable components change from {} to {}",
                    impact.drive_reachable_components_before,
                    impact.drive_reachable_components_after
                ));
            }
            let mut contradictions = Vec::new();
            if diagnostic.counts.awaiting_external_input > 0 {
                contradictions.push(
                    "one or more disconnected paths are also valid inferred external-input boundaries"
                        .to_owned(),
                );
            }
            if proposal
                .impact
                .is_some_and(|impact| impact.requires_temporal_validation)
            {
                contradictions.push(
                    "the repaired path contains temporal behavior that still needs a live transition test"
                        .to_owned(),
                );
            }
            hypotheses.push(json!({
                "kind": proposal.patch.reason,
                "confidence_percent": proposal.patch.confidence_percent,
                "supporting_evidence": supporting_evidence,
                "contradictions": contradictions,
                "diagnostic_finding_ids": diagnostic.diagnosis.findings_for_repair(proposal, &analysis.scene),
                "physical_evidence": proposal.evidence,
                "counterfactual_impact": proposal.impact,
                "operation_id": operation_id
            }));
        }
        if diagnostic.counts.awaiting_external_input > 0 {
            hypotheses.push(json!({
                "kind": "intentional_external_inputs",
                "confidence": "plausible",
                "supporting_evidence": [
                    "the disconnected paths satisfy the structural rules for inferred input boundaries"
                ],
                "contradictions": selected.map_or_else(Vec::new, |proposal| vec![format!(
                    "the highest-ranked virtual repair improves connectivity with {} block change(s)",
                    proposal.patch.changes.len()
                )])
            }));
        }

        let gaps = analysis.scene.gap_candidates(max_gap);
        let mut questions = Vec::new();
        if diagnostic.counts.awaiting_external_input > 0 && selected.is_some() {
            questions.push(
                "Are the disconnected fragments intended as separate inputs, or should the known controllable input drive the downstream path?"
                    .to_owned(),
            );
        }
        if selected
            .and_then(|proposal| proposal.impact)
            .is_some_and(|impact| impact.requires_temporal_validation)
        {
            questions.push(
                "May DustRoute run a previewed transition scenario after repair to verify timing?"
                    .to_owned(),
            );
        }
        json_text(json!({
            "schema_version": REPAIR_CONTEXT_SCHEMA_V1,
            "ok": true,
            "circuit_id": circuit_id,
            "operation_id": operation_id,
            "summary": format!(
                "{} physical component(s), {} traversal fragment(s), {} nearby gap candidate(s), {} repair hypothesis/hypotheses",
                analysis.scene.components.len(),
                analysis.scene.fragments.len(),
                gaps.len(),
                hypotheses.len()
            ),
            "facts": {
                "observation_complete": diagnostic.observation_complete,
                "components": analysis.scene.components.len(),
                "fragments": analysis.scene.fragments.len(),
                "gap_candidates": gaps.iter().take(16).collect::<Vec<_>>(),
                "gap_candidates_truncated": gaps.len() > 16,
                "diagnostic": diagnostic,
                "temporal": analysis.scene.temporal_assessment()
            },
            "hypotheses": hypotheses,
            "related_components": related_components,
            "questions": questions,
            "next_step": "compare the hypotheses with the player's intent; use show_operation only after choosing a repair hypothesis"
        }))
    }

    #[tool(
        description = "Create a non-mutating, reversible macro replacement operation from a candidate component_id returned by convert_from_circuit",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn new_macro_optimization(
        &self,
        Parameters(params): Parameters<NewMacroOptimizationParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return error_text(McpErrorCode::InvalidArgument, error, false),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        let contract = match optimization_contract_from_param(params.contract) {
            Ok(contract) => contract,
            Err(error) => return error_text(McpErrorCode::InvalidArgument, error, false),
        };
        let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
            Ok(circuit) => circuit,
            Err(error) => return error_text(McpErrorCode::NotFound, error, false),
        };
        json_text(
            self.optimization_workflow()
                .propose_macro(circuit_id, circuit, params.component_id, contract)
                .await,
        )
    }

    #[tool(
        description = "Create a non-mutating, reversible optimization plan for a simple physical dust path inside an explicit focus while fixing everything outside",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn new_optimization(
        &self,
        Parameters(params): Parameters<NewOptimizationParams>,
    ) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        if !matches!(
            params.objective.as_str(),
            "wire_length" | "density_then_wire_length"
        ) {
            return error_text(
                McpErrorCode::InvalidArgument,
                "objective must be wire_length or density_then_wire_length",
                false,
            );
        }
        let contract = match optimization_contract_from_param(params.contract) {
            Ok(contract) => contract,
            Err(error) => return error_text(McpErrorCode::InvalidArgument, error, false),
        };
        let search_budget = match optimization_search_budget(params.search) {
            Ok(budget) => budget,
            Err(error) => return error_text(McpErrorCode::InvalidArgument, error, false),
        };
        let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
            Ok(circuit) => circuit,
            Err(error) => return error_text(McpErrorCode::NotFound, error, false),
        };
        let focus = dustroute_translate::world_reverse::RegionBounds::new(
            Pos::new(params.focus.min.x, params.focus.min.y, params.focus.min.z),
            Pos::new(params.focus.max.x, params.focus.max.y, params.focus.max.z),
        );
        json_text(
            self.optimization_workflow()
                .propose_wire(
                    circuit_id,
                    circuit,
                    focus,
                    params.objective,
                    contract,
                    search_budget,
                )
                .await,
        )
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
        let world = match world_from_snapshot_for_service(&snapshot) {
            Ok(world) => world,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
        let Some(proposal) = dustroute_translate::repair::propose_scene_component_removal(
            &world,
            &analysis.scene,
            target,
        ) else {
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
                    lifecycle: RepairLifecycle::Draft,
                    contract_satisfied: true,
                    preserved_boundary: Vec::new(),
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
            "next_step": "call show_operation, then invoke_operation with confirm=true only after confirmation"
        }))
    }

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
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        // Serialize preview persistence with attempts, so a delayed preview
        // cannot overwrite a saved NeedsInspection state.
        let _mutation_guard = self.mutation_lock.lock().await;
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
                if let Err(error) = previewed.lifecycle.preview() {
                    return error_text(McpErrorCode::InvalidState, error, false);
                }
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
                    "next_step": "obtain explicit player confirmation before invoke_operation"
                }))
            }
            Err(error) => json_text(json!({ "ok": false, "error": error.to_string() })),
        }
    }

    async fn plan_revision_placement(&self, params: PreviewPlacementParams) -> String {
        let result: Result<Value, String> = async {
            if !params.circuit.is_empty()
                || params.optimize.unwrap_or(false)
                || params.assembly_revision_id.is_some()
            {
                return Err("revision_id cannot be combined with a built-in circuit, Assembly Revision or optimization".into());
            }
            let player=self.resolve_player(params.player.as_deref())?;
            if let Some(error)=self.authorize_player(&player) {return Err(error);}
            let revision=self.load_revision(params.revision_id.as_deref().ok_or("revision_id required")?,&player)?;
            self.plan_grounded_revision_placement(
                &params,
                player,
                revision.clone(),
                json!({"kind":"circuit_revision","revision_id":revision.revision_id}),
            ).await
        }.await;
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }

    async fn plan_adopted_assembly_placement(&self, params: PreviewPlacementParams) -> String {
        let result: Result<Value, String> = async {
            if !params.circuit.is_empty()
                || params.revision_id.is_some()
                || params.optimize.unwrap_or(false)
            {
                return Err("assembly_revision_id cannot be combined with a built-in circuit, circuit revision or optimization".into());
            }
            let player = self.resolve_player(params.player.as_deref())?;
            if let Some(error) = self.authorize_player(&player) {
                return Err(error);
            }
            let assembly_id = params
                .assembly_revision_id
                .clone()
                .ok_or("assembly_revision_id required")?;
            let store = self.state_store.clone();
            let owner = player.clone();
            let basis = tokio::task::spawn_blocking(move || {
                crate::blueprint_mcp::grounded_source(&store, &owner, &assembly_id)
            }).await.map_err(|error| error.to_string())??;
            let record = basis.record;
            let grounding = basis.grounding;
            self.policy
                .authorize_dimension(&grounding.dimension)
                .map_err(|error| error.to_string())?;
            let target = snapshot_from_grounded_assembly(
                &record,
                grounding.base_snapshot.min,
                grounding.base_snapshot.max,
            )?;
            let revision = crate::revision::CircuitRevision {
                schema_version: "dustroute.circuit-revision.v1".into(),
                revision_id: grounding.circuit_revision_id,
                parent_revision_ids: vec![],
                base_observation_id: grounding.base_observation_id,
                player: player.clone(),
                dimension: grounding.dimension,
                target: None,
                complete: grounding.complete,
                snapshot: target,
                base_snapshot: Some(grounding.base_snapshot),
                changes: vec![],
                validation: json!({"source":"fresh adopted Assembly review"}),
                assembly: Some(record.clone()),
            };
            self.plan_grounded_revision_placement(
                &params,
                player,
                revision,
                json!({
                    "kind":"adopted_assembly_revision",
                    "assembly_revision_id":record.id,
                    "adopted_by":basis.adopted_by,
                    "grounding_assembly_revision_id":basis.grounding_assembly_revision_id,
                    "fresh_review":basis.fresh_review,
                    "literal_observation":"grounding.base_snapshot",
                    "candidate_interpretation":"record.assembly"
                }),
            )
            .await
        }
        .await;
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }

    async fn plan_grounded_revision_placement(
        &self,
        params: &PreviewPlacementParams,
        player: String,
        revision: crate::revision::CircuitRevision,
        source: Value,
    ) -> Result<Value, String> {
        if !revision.complete {
            return Err("incomplete source observation cannot authorize placement".into());
        }
        let base=revision.base_snapshot.as_ref().ok_or("revision has no retained base snapshot; create a new revision from a fresh observation")?;
        if base.min != revision.snapshot.min || base.max != revision.snapshot.max {
            return Err("revision cannot expand observation bounds".into());
        }
        let base_map = crate::revision::blocks(base)?;
        let target_map = crate::revision::blocks(&revision.snapshot)?;
        let offset = |p: Pos, d: i32| -> Result<Pos, String> {
            Ok(Pos::new(
                p.x.checked_add(d).ok_or("coordinate overflow")?,
                p.y.checked_add(d).ok_or("coordinate overflow")?,
                p.z.checked_add(d).ok_or("coordinate overflow")?,
            ))
        };
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(
            offset(base.min, -1)?,
            offset(base.max, 1)?,
        );
        self.policy
            .validate_region(bounds)
            .map_err(|e| e.to_string())?;
        let status = self.bridge.status().await.map_err(|e| e.to_string())?;
        if !status.connected || status.dimension.as_deref() != Some(revision.dimension.as_str()) {
            return Err("bot disconnected or dimension changed".into());
        }
        let before = self
            .bridge
            .scan_region(bounds.min, bounds.max, &revision.dimension)
            .await
            .map_err(|e| e.to_string())?;
        if before.min != bounds.min || before.max != bounds.max {
            return Err("complete context scan required".into());
        }
        let context = crate::revision::blocks(&before)?;
        let inside = |p: Pos| {
            p.x >= base.min.x
                && p.y >= base.min.y
                && p.z >= base.min.z
                && p.x <= base.max.x
                && p.y <= base.max.y
                && p.z <= base.max.z
        };
        let observed = context
            .iter()
            .filter(|(p, _)| inside(**p))
            .map(|(p, b)| (*p, b.clone()))
            .collect::<BTreeMap<_, _>>();
        if observed != base_map {
            return Err(
                "physical world differs from the base observation; capture and revise again".into(),
            );
        }
        let mut final_map = context.clone();
        final_map.retain(|p, _| !inside(*p));
        final_map.extend(target_map.clone());
        let after = dustroute_translate::snapshot::MinecraftSnapshot {
            min: bounds.min,
            max: bounds.max,
            blocks: final_map.into_values().collect(),
        };
        let old = world_from_snapshot(&before).map_err(|e| e.to_string())?;
        let new = world_from_snapshot(&after).map_err(|e| e.to_string())?;
        dustroute_translate::world::ValidatedWorld::try_from(old.clone())
            .map_err(|e| format!("baseline is invalid or unsupported: {e}"))?;
        let mut changes = Vec::new();
        let mut materials = BTreeMap::<String, usize>::new();
        let positions = base_map
            .keys()
            .chain(target_map.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for pos in positions {
            if base_map.get(&pos) == target_map.get(&pos) {
                continue;
            }
            let before_block = old
                .get(pos)
                .cloned()
                .unwrap_or_else(|| dustroute_translate::world::Block::new(BlockKind::Air));
            let after_block = new
                .get(pos)
                .cloned()
                .unwrap_or_else(|| dustroute_translate::world::Block::new(BlockKind::Air));
            // Reject lossy/defaulted metadata instead of silently installing a different revision.
            for (record, block) in [
                (base_map.get(&pos), &before_block),
                (target_map.get(&pos), &after_block),
            ] {
                if let Some(record) = record {
                    let exported = initial_java_block_state(block, &JavaExportConfig::default())
                        .map_err(|e| e.to_string())?;
                    let requested = crate::revision::state(record);
                    let split = |s: &str| -> (String, BTreeSet<String>) {
                        let (name, props) = s.split_once('[').unwrap_or((s, ""));
                        (
                            name.into(),
                            props
                                .trim_end_matches(']')
                                .split(',')
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .collect(),
                        )
                    };
                    if split(&exported) != split(&requested) {
                        return Err(format!(
                            "block at {pos:?} must have complete, losslessly exportable properties; expected {exported}, retained {requested}"
                        ));
                    }
                }
            }
            if let Some(b) = target_map.get(&pos) {
                *materials.entry(b.name.clone()).or_default() += 1;
            }
            changes.push(BlockChange {
                pos,
                before: before_block,
                after: after_block,
                collision: base_map.contains_key(&pos),
            });
        }
        if changes.is_empty() {
            return Err("revision has no cumulative changes from its base observation".into());
        }
        self.policy
            .validate_placement_size(changes.len())
            .map_err(|e| e.to_string())?;
        if params.max_blocks.is_some_and(|limit| changes.len() > limit) {
            return Err("revision diff exceeds max_blocks".into());
        }
        dustroute_app::ValidatedBlockChanges::new(&old, changes.clone())
            .map_err(|e| format!("revision placement rejected: {e}: {:?}", e.issues))?;
        let id = uuid::Uuid::new_v4();
        let undo = dustroute_app::UndoPlan {
            operation_id: id,
            changes: changes
                .iter()
                .map(|c| BlockChange {
                    pos: c.pos,
                    before: c.after.clone(),
                    after: c.before.clone(),
                    collision: false,
                })
                .collect(),
        };
        let plan = PlacementPlan {
            operation_id: id,
            origin: base.min,
            collision_count: changes.iter().filter(|c| c.collision).count(),
            changes,
            materials,
            undo,
            previewed: false,
            assembly: revision
                .assembly
                .clone()
                .map(|revision| dustroute_app::PlacementAssembly {
                    coordinate_origin: Pos::default(),
                    revision,
                }),
        };
        let response = json!({"ok":true,"operation_id":id,"source":source,"revision_id":revision.revision_id,"base_observation_id":revision.base_observation_id,"bounds":bounds_json(bounds),"plan":plan,"read_only":self.policy.read_only,"next_step":"show_operation then invoke_operation(confirm=true)","validation_scope":"fresh model review when declared, modeled placement, exact live base and surrounding context; not general functional equivalence"});
        self.plans.placements().lock().await.insert(
            plan,
            revision.dimension.clone(),
            Some(RevisionPlacementContext {
                player,
                dimension: revision.dimension.clone(),
                version: status.version,
                before,
                after,
                expires_at: Instant::now() + Duration::from_secs(300),
                lifecycle: RevisionLifecycle::Planned,
            }),
        );
        self.operations
            .record_completed(id, OperationKind::PlacementPreview, response.clone())
            .await;
        Ok(response)
    }

    async fn plan_piston_placement(&self, params: PreviewPlacementParams) -> String {
        let result: Result<Value,String> = async {
            if params.optimize.unwrap_or(false) { return Err("the pinned piston-door layout cannot be optimized".into()); }
            let player = self.resolve_player(params.player.as_deref())?;
            if let Some(error) = self.authorize_player(&player) { return Err(error); }
            let observation = self.bridge.observe_player(&player,64.0).await.map_err(|e|e.to_string())?;
            let anchor = observation.targeted_block.ok_or("look at the ground below the placement")?;
            let origin = Pos::new(anchor.x, anchor.y.checked_add(3).ok_or("coordinate overflow")?, anchor.z);
            if [origin.x,origin.y,origin.z].iter().any(|v| *v < i32::MIN+16 || *v > i32::MAX-16) { return Err("coordinate overflow".into()); }
            let bounds = dustroute_translate::world_reverse::RegionBounds::new(origin.offset(-3,-2,-4),origin.offset(7,3,6));
            self.policy.authorize_dimension(&observation.dimension).map_err(|e|e.to_string())?;
            self.policy.validate_region(bounds).map_err(|e|e.to_string())?;
            let status = self.bridge.status().await.map_err(|e|e.to_string())?;
            if !status.connected || status.dimension.as_deref()!=Some(observation.dimension.as_str()) { return Err("bot disconnected or dimension changed".into()); }
            let baseline = self.bridge.scan_region(bounds.min,bounds.max,&observation.dimension).await.map_err(|e|e.to_string())?;
            let proof = crate::piston_door::ValidatedDoorPlacement::new(origin,&baseline,&status.version)?;
            let count = proof.initial().blocks.len();
            self.policy.validate_placement_size(count).map_err(|e|e.to_string())?;
            if params.max_blocks.is_some_and(|limit| count > limit) { return Err("placement exceeds max_blocks".into()); }
            let mut materials = std::collections::BTreeMap::<String,usize>::new();
            for b in &proof.initial().blocks { *materials.entry(b.name.clone()).or_default() += 1; }
            let id = uuid::Uuid::new_v4();
            let response = json!({"ok":true,"operation_id":id,"circuit":"piston-door-1x2","origin":origin,"anchor":anchor,"bounds":bounds_json(bounds),"initial_state":"open","materials":materials,"changed_blocks":count,"collision_count":0,"undo_change_count":count,"read_only":self.policy.read_only,"changes":proof.writes(false),"undo_changes":proof.writes(true),"next_step":"show_operation, then invoke_operation(confirm=true)","placement_note":"origin is three blocks above the gaze target to preserve an empty guard above the ground"});
            let mut plans = self.plans.table::<StoredPistonPlacement>().lock().await;
            plans.retain(|_,p| p.state != PistonPlacementState::Planned || p.expires_at > Instant::now());
            if plans.len() >= 256 { return Err("too many retained piston placements".into()); }
            plans.insert(id,StoredPistonPlacement {player,dimension:observation.dimension,proof,previewed:false,state:PistonPlacementState::Planned,expires_at:Instant::now()+Duration::from_secs(300)});
            drop(plans);
            self.operations.record_completed(id,OperationKind::PlacementPreview,response.clone()).await;
            Ok(response)
        }.await;
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }

    async fn show_piston_placement(&self, id: uuid::Uuid, player: Option<&str>) -> String {
        let result: Result<Value,String> = async {
            let player = self.resolve_player(player)?;
            if let Some(error)=self.authorize_player(&player) { return Err(error); }
            let plan = self.plans.table::<StoredPistonPlacement>().lock().await.get(&id).cloned().ok_or("placement not found")?;
            if plan.player != player || plan.state != PistonPlacementState::Planned || plan.expires_at <= Instant::now() { return Err("placement is expired, used, or owned by another player".into()); }
            let bounds = plan.proof.bounds();
            self.policy.authorize_dimension(&plan.dimension).map_err(|e|e.to_string())?;
            self.policy.validate_region(bounds).map_err(|e|e.to_string())?;
            let preview = self.bridge.preview_region(&player,bounds.min,bounds.max,&plan.dimension).await.map_err(|e|e.to_string())?;
            self.plans.table::<StoredPistonPlacement>().lock().await.get_mut(&id).ok_or("placement not found")?.previewed=true;
            Ok(json!({"ok":true,"operation_id":id,"preview":preview,"bounds":bounds_json(bounds),"changes":plan.proof.writes(false),"undo_changes":plan.proof.writes(true),"initial_state":"open"}))
        }.await;
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }

    async fn mutate_piston_placement(&self, id: uuid::Uuid, confirm: bool, undo: bool) -> String {
        let result: Result<Value,String> = async {
            if !confirm { return Err("confirm=true is required".into()); }
            self.policy.authorize_mutation().map_err(|e|e.to_string())?;
            let player = self.resolve_player(None)?;
            if let Some(error)=self.authorize_player(&player) { return Err(error); }
            let _guard = self.mutation_lock.lock().await;
            let plan = self.plans.table::<StoredPistonPlacement>().lock().await.get(&id).cloned().ok_or("placement not found")?;
            if plan.player != player { return Err("placement belongs to another player".into()); }
            if (undo && plan.state != PistonPlacementState::Applied) || (!undo && (plan.state != PistonPlacementState::Planned || !plan.previewed || plan.expires_at <= Instant::now())) { return Err("placement requires an unused, unexpired preview; undo requires verified application".into()); }
            let bounds = plan.proof.bounds();
            self.policy.authorize_dimension(&plan.dimension).map_err(|e|e.to_string())?;
            self.policy.validate_region(bounds).map_err(|e|e.to_string())?;
            self.policy.validate_placement_size(plan.proof.initial().blocks.len()).map_err(|e|e.to_string())?;
            let status = self.bridge.status().await.map_err(|e|e.to_string())?;
            if !status.connected || status.dimension.as_deref()!=Some(plan.dimension.as_str()) { return Err("bot disconnected or dimension changed".into()); }
            let baseline = self.bridge.scan_region(bounds.min,bounds.max,&plan.dimension).await.map_err(|e|e.to_string())?;
            if undo { plan.proof.validate_built(&baseline,&status.version)?; }
            else { plan.proof.validate_empty(&baseline,&status.version)?; }
            {
                let mut plans = self.plans.table::<StoredPistonPlacement>().lock().await;
                let stored = plans.get_mut(&id).ok_or("placement not found")?;
                if !undo && stored.expires_at <= Instant::now() { return Err("placement expired during validation".into()); }
                // Consume before writes, including undo: uncertain transport is never retried automatically.
                stored.state = PistonPlacementState::NeedsInspection;
            }
            let write = self.bridge.write_blocks(&plan.proof.writes(undo),&plan.dimension).await;
            let wait = self.bridge.wait_ticks(30,&plan.dimension).await;
            let observed = self.bridge.scan_region(bounds.min,bounds.max,&plan.dimension).await;
            let verification = observed.as_ref().map_err(|e|e.to_string()).and_then(|snapshot| if undo { plan.proof.validate_empty(snapshot,&status.version) } else { plan.proof.validate_built(snapshot,&status.version) });
            let ok = write.is_ok() && wait.is_ok() && verification.is_ok();
            if ok { self.plans.table::<StoredPistonPlacement>().lock().await.get_mut(&id).ok_or("placement not found")?.state = if undo { PistonPlacementState::Undone } else { PistonPlacementState::Applied }; }
            let response = json!({"ok":ok,"operation_id":id,"verified":verification.is_ok(),"status":if ok {"verified"} else {"needs_inspection"},"undo":undo,"write_error":write.err().map(|e|e.to_string()),"wait_error":wait.err().map(|e|e.to_string()),"verification_error":verification.err(),"retry_allowed":false,"automatic_rollback":false,"bounds":bounds_json(bounds)});
            self.operations.record_completed(id,if undo {OperationKind::PlacementUndo} else {OperationKind::PlacementApply},response.clone()).await;
            if !ok {self.operations.fail(id,"piston placement requires inspection").await;}
            Ok(response)
        }.await;
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }

    async fn observed_mechanisms(
        &self,
        snapshot: &dustroute_translate::snapshot::MinecraftSnapshot,
        complete: bool,
        dimension: &str,
    ) -> Value {
        if !snapshot.blocks.iter().any(|b| {
            matches!(
                b.name.as_str(),
                "minecraft:piston"
                    | "minecraft:sticky_piston"
                    | "minecraft:piston_head"
                    | "minecraft:moving_piston"
            )
        }) {
            return json!([]);
        }
        let observation = match self.bridge.status().await {
            Ok(status) if status.connected && status.dimension.as_deref() == Some(dimension) => {
                crate::piston_door::inspect(snapshot, &status.version, complete, None)
            }
            _ => dustroute_translate::piston_observation::PistonObservation::unresolved(
                dustroute_translate::piston_observation::PistonObservationState::ObservationIncomplete,
                "version_unavailable",
                "server version and dimension could not be verified",
            ),
        };
        let recognized = matches!(
            observation.state,
            dustroute_translate::piston_observation::PistonObservationState::Open
                | dustroute_translate::piston_observation::PistonObservationState::Closed
        );
        json!([{
            "kind": if recognized { "piston_door" } else { "unidentified_piston_mechanism" },
            "recognition": if recognized { "exact_contract_match" } else { "unidentified" },
            "contract": if recognized { Some("piston_door_v1") } else { None },
            "candidate_assessments": [{ "contract": "piston_door_v1", "observation": observation }],
            "state": if recognized { Some(observation.state) } else { None },
            "scope": "entire_observed_region",
            "mutation_authorized": false
        }])
    }

    #[tool(
        description = "Plan open/closed for an already built, exact Java 1.21.11 door v1 (1x2). Requires a complete selected circuit including its empty guard. Translation only; no building or rotation. Use show_operation then invoke_operation(confirm=true).",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn new_piston_door_operation(
        &self,
        Parameters(params): Parameters<NewPistonDoorParams>,
    ) -> String {
        let result: Result<Value,String> = async {
            let player=self.resolve_player(None)?;
            if let Some(error)=self.authorize_player(&player) { return Err(error); }
            let (_,circuit)=self.load_circuit(&params.circuit_id,&player).await?;

            let status=self.bridge.status().await.map_err(|e|e.to_string())?;
            if !status.connected || status.dimension.as_deref()!=Some(circuit.dimension.as_str()) { return Err("bot is disconnected or in another dimension".into()); }
            self.policy.authorize_dimension(&circuit.dimension).map_err(|e|e.to_string())?;
            let observation=crate::piston_door::inspect(&circuit.snapshot,&status.version,circuit.complete,None);
            if !matches!(observation.state,dustroute_translate::piston_observation::PistonObservationState::Open|dustroute_translate::piston_observation::PistonObservationState::Closed) {
                return Ok(json!({"ok":false,"error":"door is not a completely observed stable configuration","observation":observation}));
            }
            let door=crate::piston_door::verify(&circuit.snapshot,&status.version)?;
            self.policy.validate_region(door.bounds()).map_err(|e|e.to_string())?;
            let id=uuid::Uuid::new_v4();
            let result=json!({"ok":true,"operation_id":id,"contract":"piston_door_v1","observation":observation,"state":door.state(),"target":params.target,"bounds":bounds_json(door.bounds()),"lever":door.lever(),"expires_in_seconds":300,"next_step":"show_operation, then invoke_operation(confirm=true) after confirmation"});
            let mut plans=self.plans.table::<StoredDoorPlan>().lock().await;
            plans.retain(|_,p|p.expires_at>Instant::now());
            if plans.len()>=256 { return Err("too many door plans; wait for expiry".into()); }
            plans.insert(id,StoredDoorPlan{player,dimension:circuit.dimension,door,target:params.target,lifecycle:InvocationState::Draft,expires_at:Instant::now()+std::time::Duration::from_secs(300)});
            drop(plans);
            self.operations.record_completed(id,OperationKind::PistonDoorProposal,result.clone()).await;
            Ok(result)
        }.await;
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }

    async fn show_piston_door(&self, id: uuid::Uuid, player: Option<&str>) -> String {
        let result:Result<Value,String>=async {
            let player=self.resolve_player(player)?;
            if let Some(error)=self.authorize_player(&player) { return Err(error); }
            let plan=self.plans.table::<StoredDoorPlan>().lock().await.get(&id).cloned().ok_or("door plan not found")?;
            if plan.player!=player || plan.lifecycle.attempted() || plan.expires_at<=Instant::now() { return Err("door plan is expired, consumed or owned by another player".into()); }
            self.policy.authorize_dimension(&plan.dimension).map_err(|e|e.to_string())?;
            let bounds=plan.door.bounds();
            let preview=self.bridge.preview_region(&player,bounds.min,bounds.max,&plan.dimension).await.map_err(|e|e.to_string())?;
            if let Some(p)=self.plans.table::<StoredDoorPlan>().lock().await.get_mut(&id) { p.lifecycle.preview()?; }
            Ok(json!({"ok":true,"operation_id":id,"state":plan.door.state(),"target":plan.target,"preview":preview,"warning":"Normal lever activation changes this door and leaves it in the requested state. Failure requires fresh inspection; no automatic retry or rollback."}))
        }.await;
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }

    async fn invoke_piston_door(&self, id: uuid::Uuid, confirm: bool) -> String {
        let result:Result<Value,String>=async {
            if !confirm { return Err("confirm=true is required".into()); }
            self.policy.authorize_mutation().map_err(|e|e.to_string())?;
            let player=self.resolve_player(None)?;
            if let Some(error)=self.authorize_player(&player) { return Err(error); }
            let _guard=self.mutation_lock.lock().await;
            let plan=self.plans.table::<StoredDoorPlan>().lock().await.get(&id).cloned().ok_or("door plan not found")?;
            if plan.player!=player || !plan.lifecycle.is_previewed() || plan.lifecycle.attempted() || plan.expires_at<=Instant::now() { return Err("door plan requires preview, matching owner, unexpired and unused token".into()); }
            let status=self.bridge.status().await.map_err(|e|e.to_string())?;
            if !status.connected || status.dimension.as_deref()!=Some(plan.dimension.as_str()) { return Err("bot is disconnected or in another dimension".into()); }
            self.policy.authorize_dimension(&plan.dimension).map_err(|e|e.to_string())?;
            let bounds=plan.door.bounds();
            self.policy.validate_region(bounds).map_err(|e|e.to_string())?;
            // Approach first, then scan immediately before the lever write.
            self.bridge.approach_lever(plan.door.lever(),&plan.dimension).await.map_err(|e|e.to_string())?;
            let snapshot=self.bridge.scan_region(bounds.min,bounds.max,&plan.dimension).await.map_err(|e|e.to_string())?;
            let observation=crate::piston_door::inspect(&snapshot,&status.version,true,Some(&plan.door));
            let current=match crate::piston_door::verify(&snapshot,&status.version) {
                Ok(door)=>door,
                Err(error)=>return Ok(json!({"ok":false,"error":error,"observation":observation})),
            };
            if current.lever()!=plan.door.lever() || current.state()!=plan.door.state() { return Ok(json!({"ok":false,"error":"door changed since proposal; inspect and create a new plan","observation":observation})); }
            // Consume before any write: an uncertain bridge response must never
            // make an old plan eligible for an automatic second toggle.
            {
                let mut plans = self.plans.table::<StoredDoorPlan>().lock().await;
                let stored = plans.get_mut(&id).ok_or("door plan expired")?;
                if stored.lifecycle.attempted() || stored.expires_at <= Instant::now() {
                    return Err("door plan expired or was already consumed".into());
                }
                stored.lifecycle.begin(true)?;
            }
            if current.state()==plan.target {
                self.plans.table::<StoredDoorPlan>().lock().await.get_mut(&id).ok_or("door plan missing")?.lifecycle.confirm(true);
                let result=json!({"ok":true,"operation_id":id,"state":plan.target,"observed_state":plan.target,"changed":false,"verified":true,"observation":observation});
                self.operations.record_completed(id,OperationKind::PistonDoorRun,result.clone()).await;
                return Ok(result);
            }
            let activation=self.bridge.activate_lever(plan.door.lever(),&plan.dimension).await;
            let wait=self.bridge.wait_ticks(30,&plan.dimension).await;
            let observed=self.bridge.scan_region(bounds.min,bounds.max,&plan.dimension).await;
            let observation=match &observed {
                Ok(snapshot)=>crate::piston_door::inspect(snapshot,&status.version,true,Some(&plan.door)),
                Err(_)=>dustroute_translate::piston_observation::PistonObservation::unresolved(dustroute_translate::piston_observation::PistonObservationState::ObservationIncomplete,"scan_failed","post-operation observation was not obtained"),
            };
            let verification = observed.as_ref().map_err(|e|e.to_string())
                .and_then(|snapshot|crate::piston_door::verify(snapshot,&status.version));
            let verified = verification.as_ref().ok();
            let matches=verified.as_ref().is_some_and(|d|d.lever()==plan.door.lever() && d.state()==plan.target);
            let ok=activation.is_ok() && wait.is_ok() && matches;
            self.plans.table::<StoredDoorPlan>().lock().await.get_mut(&id).ok_or("door plan missing")?.lifecycle.confirm(ok);
            let result=json!({"ok":ok,"operation_id":id,"target":plan.target,"observed_state":verified.as_ref().map(|d|d.state()),"observation":observation,"verified":matches,"verification_error":verification.as_ref().err(),"activation_error":activation.err().map(|e|e.to_string()),"wait_error":wait.err().map(|e|e.to_string()),"scan_error":observed.err().map(|e|e.to_string()),"status":if ok {"verified"} else {"needs_inspection"},"retry_allowed":false,"automatic_rollback":false});
            self.operations.record_completed(id,OperationKind::PistonDoorRun,result.clone()).await;
            if !ok { self.operations.fail(id, "door result requires fresh inspection").await; }
            Ok(result)
        }.await;
        json_text(result.unwrap_or_else(|error| json!({"ok":false,"error":error})))
    }
    #[tool(
        description = "Discover single-lever transition scenarios in the supplied immutable circuit_id without changing the world",
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
        let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
            Ok(circuit) => circuit,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let bounds = circuit.bounds;
        let dimension = circuit.dimension;
        if let Err(error) = self.policy.validate_region(bounds) {
            return json_text(json!({ "ok": false, "error": error.to_string() }));
        }
        let snapshot = circuit.snapshot;
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
            "circuit_id": circuit_id,
            "bounds": bounds_json(bounds),
            "dimension": dimension,
            "proposals": proposals,
            "next_step": "show_operation, then invoke_operation(confirm=true) only for a ready proposal"
        }))
    }

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
        let plans = self.plans.table::<StoredTransitionPlan>().lock().await;
        let Some(plan) = plans.get(&operation_id).cloned() else {
            return error_text(
                McpErrorCode::NotFound,
                "transition scenario not found",
                false,
            );
        };
        drop(plans);
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
        if let Some(stored) = self
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .get_mut(&operation_id)
        {
            if let Err(error) = stored.lifecycle.preview() {
                return json_text(json!({"ok":false,"error":error}));
            }
        }
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
            .plans
            .table::<StoredTransitionPlan>()
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
        if self.policy.preview_required && !plan.lifecycle.is_previewed() {
            return error_text(
                McpErrorCode::InvalidState,
                "show_operation is required first",
                false,
            );
        }
        if plan.lifecycle.attempted() {
            return json_text(json!({
                "ok": false,
                "error": "scenario was already executed; create and preview a new scenario"
            }));
        }
        if plan.safety.safety != TransitionSafety::Ready {
            let mut response = match serde_json::to_value(ErrorResponse::new(
                McpErrorCode::InvalidState,
                "scenario is preview-only because the region contains temporal or unsupported devices",
                false,
            )) {
                Ok(value) => value,
                Err(error) => {
                    return error_text(McpErrorCode::Internal, error.to_string(), false);
                }
            };
            let safety = match serde_json::to_value(&plan.safety) {
                Ok(value) => value,
                Err(error) => return error_text(McpErrorCode::Internal, error.to_string(), false),
            };
            if let Some(object) = response.as_object_mut() {
                object.insert("safety".to_owned(), safety);
            }
            return json_text(response);
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
        let current_snapshot = match self
            .bridge
            .scan_region(plan.bounds.min, plan.bounds.max, &plan.dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return json_text(json!({ "ok": false, "error": error.to_string() })),
        };
        if current_snapshot != plan.initial_snapshot {
            return json_text(
                json!({ "ok": false, "error": "transition region changed since preview; create a new scenario" }),
            );
        }
        let world = match world_from_snapshot_for_service(&current_snapshot) {
            Ok(world) => world,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        let world = match dustroute_translate::world::ValidatedWorld::try_from(world) {
            Ok(world) => world,
            Err(error) => {
                return json_text(
                    json!({ "ok": false, "error": error.to_string(), "validation": error }),
                );
            }
        };
        let mut analysis =
            dustroute_translate::world_reverse::analyze_world_region(&world, plan.bounds);
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
        {
            let mut plans = self.plans.table::<StoredTransitionPlan>().lock().await;
            let Some(stored) = plans.get_mut(&operation_id) else {
                return error_text(McpErrorCode::NotFound, "scenario missing", false);
            };
            if let Err(error) = stored.lifecycle.begin(self.policy.preview_required) {
                return error_text(McpErrorCode::InvalidState, error, false);
            }
        }
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
        if let Some(stored) = self
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .get_mut(&operation_id)
        {
            stored.lifecycle.confirm(restoration_verified);
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
            dustroute_translate::analysis::compare_live_trace(
                &simulated.trace,
                &live_scenario_trace,
            )
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
            "transition_trace": transition_trace,
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
            .plans
            .table::<StoredTransitionPlan>()
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
        if !plan.lifecycle.attempted() {
            return error_text(
                McpErrorCode::InvalidState,
                "scenario has not been attempted",
                false,
            );
        }
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
        let naturally_verified = activation_error.is_none()
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
        let mut forced_restore = None;
        let verified = if naturally_verified {
            true
        } else {
            let writes = plan
                .initial_snapshot
                .blocks
                .iter()
                .map(|block| {
                    observed_java_block_state(block)
                        .parse()
                        .map(|state| CommandWrite {
                            pos: block.pos,
                            state,
                        })
                })
                .collect::<Result<Vec<_>, _>>();
            forced_restore = Some(
                match self
                    .policy
                    .validate_placement_size(plan.initial_snapshot.blocks.len())
                {
                    Ok(()) => match writes {
                        Ok(writes) => self
                            .bridge
                            .write_blocks(&writes, &plan.dimension)
                            .await
                            .map_err(|error| error.to_string()),
                        Err(error) => Err(error),
                    },
                    Err(error) => Err(error.to_string()),
                },
            );
            let _ = self
                .bridge
                .wait_ticks(plan.observation_ticks, &plan.dimension)
                .await;
            forced_restore.as_ref().is_some_and(Result::is_ok)
                && self
                    .bridge
                    .scan_region(plan.bounds.min, plan.bounds.max, &plan.dimension)
                    .await
                    .is_ok_and(|snapshot| snapshot == plan.initial_snapshot)
        };
        if let Some(stored) = self
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .get_mut(&operation_id)
        {
            stored.lifecycle.confirm(verified);
        }
        let result = json!({
            "schema_version": TRANSITION_SCHEMA_V1,
            "ok": verified,
            "operation_id": operation_id,
            "restoration_verified": verified,
            "activation_error": activation_error,
            "natural_restore_verified": naturally_verified,
            "snapshot_restore_attempted": forced_restore.is_some(),
            "snapshot_restore_error": forced_restore.and_then(Result::err),
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
        description = "Start cancellable reverse analysis of the selected region and return an operation ID for progress polling. Set include_truth_table=true for explicitly bounded exhaustive functional inference, including large regions."
    )]
    async fn start_selected_region_conversion(
        &self,
        Parameters(params): Parameters<StartSelectedRegionConversionParams>,
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
        let request = match reverse_request_for_truth_table(
            bounds,
            TruthTableRequestOptions {
                include_truth_table: params.include_truth_table.unwrap_or(false),
                max_inputs: params.truth_table_max_inputs,
                settle_ticks: params.truth_table_settle_ticks,
                max_rows: params.truth_table_max_rows,
                max_work_units: params.truth_table_max_work_units,
                max_solver_iterations: params.truth_table_max_solver_iterations,
                max_elapsed_millis: params.truth_table_max_elapsed_millis,
            },
        ) {
            Ok(request) => request,
            Err(error) => return error_text(McpErrorCode::InvalidArgument, error, false),
        }
        .with_observation_complete(true);
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
            let world = match world_from_snapshot_for_service(&snapshot) {
                Ok(world) => world,
                Err(error) => {
                    operations.fail(operation_id, error).await;
                    return;
                }
            };
            operations
                .update(
                    operation_id,
                    OperationStatus::Running,
                    50,
                    if redstone_components > MAX_FLAT_ANALYSIS_COMPONENTS
                        && !request.infer_truth_table
                    {
                        "deriving hierarchical circuit views"
                    } else {
                        "analyzing selected circuit"
                    },
                )
                .await;
            let result = match tokio::task::spawn_blocking(move || {
                if redstone_components > MAX_FLAT_ANALYSIS_COMPONENTS && !request.infer_truth_table
                {
                    let mut analysis =
                        dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
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
                    let mut staged = app.analyze_physical(&world, request);
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
        description = "Manage durable placed custom piston Assemblies: list/get history, observe fresh complete samples, diagnose coordinate-level missing/extra/block/property differences with shared repair assessment and pinned Blueprint references, even when reconstruction is blocked, plan_removal for a matching applied instance, or plan_reconstruction to tear down an observed supported layout and rebuild its declared initial state. Reconstruction requires review of all affected blocks and no competing inputs/edits; matching client samples do not prove empty server queues. Rejects moving/incomplete observations and extra material. Fresh pinned source/target review required. Preview with show_operation; apply with invoke_operation(confirm=true). Never writes world blocks itself.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn manage_assembly(
        &self,
        Parameters(params): Parameters<assembly_placement::ManageAssemblyParams>,
    ) -> String {
        json_text(self.assembly_service().manage_placed_assembly(params).await)
    }

    #[tool(
        description = "Show an operation. Blueprint updates return the full diff and a fresh independent parent/child/shared review; this does not adopt them or contact Minecraft. World placement, repair, piston-door and transition-test operations retain their preview path.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn show_operation(&self, Parameters(params): Parameters<ShowOperationParams>) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let plan_kind = self.plans.kind(&operation_id).await;
        if plan_kind == Some(PlanKind::Assembly) {
            return json_text(
                self.assembly_service()
                    .show_assembly_construction(operation_id, params.player.as_deref())
                    .await,
            );
        }
        if plan_kind == Some(PlanKind::Piston) {
            return self
                .show_piston_placement(operation_id, params.player.as_deref())
                .await;
        }
        if plan_kind == Some(PlanKind::Door) {
            return self
                .show_piston_door(operation_id, params.player.as_deref())
                .await;
        }
        let revision_context = self
            .plans
            .placements()
            .lock()
            .await
            .revision(&operation_id)
            .cloned();
        if let Some(context) = revision_context {
            let player = match self.resolve_player(params.player.as_deref()) {
                Ok(player) => player,
                Err(error) => return json_text(json!({"ok":false,"error":error})),
            };
            if let Some(error) = self.authorize_player(&player) {
                return error;
            }
            if context.player != player
                || !context.lifecycle.can_begin(false)
                || context.expires_at <= Instant::now()
            {
                return json_text(
                    json!({"ok":false,"error":"revision placement expired, consumed or owned by another player"}),
                );
            }
            match self
                .bridge
                .preview_region(
                    &player,
                    context.before.min,
                    context.before.max,
                    &context.dimension,
                )
                .await
            {
                Ok(_) => {
                    if let Some(plan) = self.plans.placements().lock().await.get_mut(&operation_id)
                    {
                        plan.previewed = true;
                    }
                }
                Err(error) => return json_text(json!({"ok":false,"error":error.to_string()})),
            }
            return self
                .get_circuit_placement(Parameters(OperationParams {
                    operation_id: params.operation_id,
                }))
                .await;
        }
        if plan_kind == Some(PlanKind::Placement) {
            if let Some(plan) = self.plans.placements().lock().await.get_mut(&operation_id) {
                plan.previewed = true;
            }
            return match self.placement_view(operation_id).await {
                Ok(plan) => {
                    json_text(json!({"ok": true, "read_only": self.policy.read_only, "plan": plan}))
                }
                Err(error) => {
                    if let Some(plan) = self.plans.placements().lock().await.get_mut(&operation_id)
                    {
                        plan.previewed = false;
                    }
                    json_text(json!({"ok": false, "error": error}))
                }
            };
        }
        match self.repair_plan(operation_id).await {
            Ok(Some(_)) => {
                return self
                    .show_repair_plan(Parameters(PreviewRepairParams {
                        operation_id: params.operation_id,
                        player: params.player,
                    }))
                    .await;
            }
            Ok(None) => {}
            Err(error) => return error_text(McpErrorCode::Internal, error, false),
        }
        if plan_kind == Some(PlanKind::Transition) {
            return self
                .show_transition_test(Parameters(PreviewTransitionParams {
                    operation_id: params.operation_id,
                    player: params.player,
                }))
                .await;
        }
        if let Some(result) = self
            .blueprint_command(
                crate::blueprint_mcp::Command::Show(operation_id),
                params.player.as_deref(),
                false,
            )
            .await
        {
            return result;
        }
        error_text(McpErrorCode::NotFound, "operation not found", false)
    }

    #[tool(
        description = "For a Blueprint update, supply blueprint_decision action adopt or reject (with reason) and confirm=true. Adoption freshly revalidates and appends immutable records locally; rejection retains history. Neither writes Minecraft. Other operation kinds execute their existing previewed world action with confirm=true.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn invoke_operation(
        &self,
        Parameters(params): Parameters<InvokeOperationParams>,
    ) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        if let Some(decision) = params.blueprint_decision {
            if params.contracts.is_some() {
                return json_text(crate::blueprint_mcp::failure(
                    "transition contracts are not Blueprint decision parameters",
                ));
            }
            return self
                .blueprint_command(
                    crate::blueprint_mcp::Command::Decide(
                        operation_id,
                        Some(decision),
                        params.confirm,
                    ),
                    None,
                    true,
                )
                .await
                .expect("explicit blueprint response");
        }
        let plan_kind = self.plans.kind(&operation_id).await;
        if plan_kind == Some(PlanKind::Assembly) {
            return json_text(
                self.assembly_service()
                    .mutate_assembly_construction(operation_id, params.confirm, false)
                    .await,
            );
        }
        if plan_kind == Some(PlanKind::Piston) {
            return self
                .mutate_piston_placement(operation_id, params.confirm, false)
                .await;
        }
        if plan_kind == Some(PlanKind::Door) {
            return self.invoke_piston_door(operation_id, params.confirm).await;
        }
        if plan_kind == Some(PlanKind::Placement) {
            return self
                .mutate_placement(
                    ConfirmedOperationParams {
                        operation_id: params.operation_id,
                        confirm: params.confirm,
                    },
                    false,
                )
                .await;
        }
        match self.repair_plan(operation_id).await {
            Ok(Some(_)) => {
                return self
                    .mutate_repair(
                        ConfirmedOperationParams {
                            operation_id: params.operation_id,
                            confirm: params.confirm,
                        },
                        false,
                    )
                    .await;
            }
            Ok(None) => {}
            Err(error) => return error_text(McpErrorCode::Internal, error, false),
        }
        if plan_kind == Some(PlanKind::Transition) {
            return self
                .invoke_transition_test(Parameters(RunTransitionParams {
                    operation_id: params.operation_id,
                    confirm: params.confirm,
                    contracts: params.contracts,
                }))
                .await;
        }
        if let Some(result) = self
            .blueprint_command(
                crate::blueprint_mcp::Command::Decide(operation_id, None, params.confirm),
                None,
                false,
            )
            .await
        {
            return result;
        }
        error_text(McpErrorCode::NotFound, "operation not found", false)
    }

    #[tool(
        description = "Undo an applied placement or repair, or restore a transition-test operation. Requires confirm=true.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn undo_operation(
        &self,
        Parameters(params): Parameters<ConfirmedOperationParams>,
    ) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let plan_kind = self.plans.kind(&operation_id).await;
        if plan_kind == Some(PlanKind::Assembly) {
            return json_text(
                self.assembly_service()
                    .mutate_assembly_construction(operation_id, params.confirm, true)
                    .await,
            );
        }
        if plan_kind == Some(PlanKind::Piston) {
            return self
                .mutate_piston_placement(operation_id, params.confirm, true)
                .await;
        }
        if plan_kind == Some(PlanKind::Door) {
            return error_text(
                McpErrorCode::InvalidState,
                "door operations require fresh observation and a new target-state plan; automatic undo is unsupported",
                false,
            );
        }
        if plan_kind == Some(PlanKind::Placement) {
            return self.mutate_placement(params, true).await;
        }
        match self.repair_plan(operation_id).await {
            Ok(Some(_)) => return self.mutate_repair(params, true).await,
            Ok(None) => {}
            Err(error) => return error_text(McpErrorCode::Internal, error, false),
        }
        if plan_kind == Some(PlanKind::Transition) {
            return self
                .restore_transition_test(Parameters(RunTransitionParams {
                    operation_id: params.operation_id,
                    confirm: params.confirm,
                    contracts: None,
                }))
                .await;
        }
        if let Some(result) = self
            .blueprint_command(
                crate::blueprint_mcp::Command::Undo(operation_id),
                None,
                false,
            )
            .await
        {
            return result;
        }
        error_text(McpErrorCode::NotFound, "operation not found", false)
    }

    #[tool(
        description = "Read operation status/results, including persisted Blueprint update requests and decision history. Saved validation events are historical diagnostics, never fresh proof.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_operation(&self, Parameters(params): Parameters<OperationParams>) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        match self.operations.get(operation_id).await {
            Some(operation) => json_text(json!({ "ok": true, "operation": operation })),
            None => self
                .blueprint_command(
                    crate::blueprint_mcp::Command::Get(operation_id),
                    None,
                    false,
                )
                .await
                .unwrap_or_else(|| json_text(json!({"ok":false,"error":"unknown operation ID"}))),
        }
    }

    #[tool(description = "Cancel a queued or running DustRoute operation")]
    async fn stop_operation(&self, Parameters(params): Parameters<OperationParams>) -> String {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_text(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let cancelled = self.operations.cancel(operation_id).await;
        json_text(json!({ "ok": cancelled, "operation_id": operation_id }))
    }

    #[tool(description = "Clear a player's pending gaze-based region selection")]
    async fn clear_region(&self, Parameters(params): Parameters<PlayerParams>) -> String {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return json_text(json!({ "ok": false, "error": error })),
        };
        if let Some(error) = self.authorize_player(&player) {
            return error;
        }
        if let Some(session) = self.selections.lock().await.get_mut(&player) {
            session.session.clear();
            session.dimension = None;
        }
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
            "Work with the player on a Minecraft redstone circuit using the PowerShell-style Verb-Noun contract expressed as snake_case. For offline Blueprint work, start with get_circuit_revision(blueprint.kind=catalog); no live observation is required. Read exact source/type/classification/Assembly records through blueprint.kind, and use test_circuit_change(blueprint.action=import, capture_revision, optimize, enumerate_layouts, or propose_update). Import saves unverified data. Blueprint optimize searches the supplied Assembly under a selected behavioral type and pinned execution context. Its explicit component scope separates body positions and fixed external equipment, with body and complete counts reported separately. enumerate_layouts tests each torch/support placement; only passed candidates satisfy the selected type in that context. Device outputs observe torches directly. Levers are independent realizations, not a NOT input-type requirement. It may move ports and replace internal interpretations, returns standalone candidate data, and neither proves global minimality nor publishes or adopts anything. Review the changed decomposition and prepare an explicit parent update using the candidate definitions, state and context. Proposals must supply new immutable parent definitions and explicit candidate state; classification labels do not verify behavior, and connection types check signal compatibility only. Review through show_operation, including every child and shared occurrence, then invoke_operation with confirm=true and blueprint_decision action adopt or reject (with reason). Adoption revalidates and appends records locally; it never writes Minecraft or automatically updates other parents. Failed or undetermined checks block adoption even if the parent passes. Persisted history is not validation proof. Blueprint IDs are not live placement arguments. An Assembly ID needs unique adoption. With assembly_target, the unified electrical context supports custom piston construction in a completely observed empty target region, after fresh target review and construction simulation; every write is read back and a mismatch stops the sequence. This requires Java 1.21.11 with observed vanilla feature flags. Undo requires the exact constructed settled state. After restart, use manage_assembly to list saved instances, diagnose damage against the design without writing blocks, and then plan_removal or plan_reconstruction; preview and confirm that new operation. Saved instance records do not authorize writes, and uncertain attempts require inspection. Without assembly_target, grounded Assembly reflection retains its original-coordinate requirement. Neither route reuses stored validation as authority. For live-world tasks, use get_world for literal visibility, then test_circuit to capture an immutable circuit snapshot and compact health. Reuse its circuit_id for get_circuit_ir, convert_from_circuit, test_circuit_change, new_repair, get_repair_context, new_optimization, new_macro_optimization, new_transition_test, and new_piston_door_operation; never silently switch back to the current gaze during one task. After new_repair, use get_repair_context when physical fragments admit competing repair and external-input hypotheses; present supporting and contradictory evidence and ask the returned questions before choosing. For observed optimization, require an explicit focus, keep everything outside fixed, and explain verification limits. Only pass a macro component_id returned for that same circuit_id. For mixed IR, pass circuit_id and first request the summary, then pass its analysis_id and a node_id to expand only that node. Treat intrinsic sources, controllable inputs, event inputs, and observation boundaries as distinct. For hypothetical edits, test_circuit_change creates an immutable revision_id from either circuit_id or parent revision_id. Use get_circuit_revision to read saved edits and validation; revision IDs are never live circuit IDs or placement permissions. New operations only create plans. Always call show_operation, explain the preview, and obtain explicit confirmation before invoke_operation(confirm=true). Use undo_operation for supported recovery. For the existing 1x2 piston door, show_region rescans the selected region and returns mechanisms plus a fresh circuit_id; convert_from_circuit interprets that immutable snapshot. Use a fresh ID for a new_piston_door_operation only when the observed mechanism matches its contract. Piston door operations have no automatic retry or undo. For an explicitly selected region, call set_region twice and show_region; reuse the circuit_id returned by show_region. Never infer coordinates from prose when gaze tools can ground them, and never mutate the world without preview and explicit confirmation.".to_owned(),
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
            "Use the collaborate-on-redstone-circuit prompt. For offline Blueprint work, use get_circuit_revision with blueprint.kind and test_circuit_change with blueprint.action. Blueprint update decisions require show_operation and an explicit blueprint_decision on invoke_operation; adoption revalidates and saves locally, never writing Minecraft. For live-world tasks, use get_world for raw visibility, test_circuit to capture a stable circuit_id and compact health, then reuse that circuit_id for analysis, hypotheses, repair, and transition planning. Use get_repair_context to compare repair evidence against intentional-boundary alternatives. New creates a plan, show_operation previews it, invoke_operation requires explicit confirmation, and undo_operation recovers only supported operation kinds. After restart, use manage_assembly for saved custom piston instances, cause-independent diagnosis and explicit removal/reconstruction planning. Diagnosis accounts for observed input levels where modeled; differences alone do not prove damage or authorize writes."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::requests::{LogicalContractParam, TimingContractParam};
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, ClientInfo, ContentBlock},
    };
    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn truth_table_request_is_explicit_and_budgeted_for_large_circuits() {
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(
            Pos::new(0, 0, 0),
            Pos::new(4, 4, 4),
        );
        let request = reverse_request_for_truth_table(
            bounds,
            TruthTableRequestOptions {
                include_truth_table: true,
                ..TruthTableRequestOptions::default()
            },
        )
        .expect("default truth-table request");
        assert!(request.infer_truth_table);
        assert_eq!(request.max_inputs, 16);
        assert_eq!(request.settle_ticks, 60);
        assert_eq!(request.truth_table_budget, TruthTableBudget::DEFAULT);

        let request = reverse_request_for_truth_table(
            bounds,
            TruthTableRequestOptions {
                include_truth_table: true,
                max_solver_iterations: Some(2_000_000),
                max_elapsed_millis: Some(30_000),
                ..TruthTableRequestOptions::default()
            },
        )
        .expect("runtime budgets within the protocol bound");
        assert_eq!(request.truth_table_budget.max_solver_iterations, 2_000_000);
        assert_eq!(request.truth_table_budget.max_elapsed_millis, Some(30_000));

        let error = reverse_request_for_truth_table(
            bounds,
            TruthTableRequestOptions {
                include_truth_table: true,
                max_inputs: Some(17),
                ..TruthTableRequestOptions::default()
            },
        )
        .expect_err("input count above the protocol bound must fail");
        assert!(error.contains("truth_table_max_inputs"));

        let error = reverse_request_for_truth_table(
            bounds,
            TruthTableRequestOptions {
                include_truth_table: true,
                max_solver_iterations: Some(MAX_TRUTH_TABLE_SOLVER_ITERATIONS + 1),
                ..TruthTableRequestOptions::default()
            },
        )
        .expect_err("solver iteration count above the protocol bound must fail");
        assert!(error.contains("truth_table_max_solver_iterations"));
    }

    #[test]
    fn truth_table_budget_failure_is_structured_in_reverse_json() {
        let compiled = dustroute_translate::compiler::BaselineCompiler::new(
            dustroute_translate::compiler::BaselineCompileConfig::default(),
        )
        .compile(&dustroute_translate::circuits::half_adder())
        .expect("half-adder compiles");
        let (min, max) = compiled.world.bounds().expect("compiled bounds");
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(min, max);
        let translated = dustroute_translate::api::Translator.reverse(
            &compiled.world,
            ReverseRequest::new(bounds)
                .with_truth_table(16)
                .with_truth_table_budget(TruthTableBudget::new(1, u128::MAX)),
        );
        let value = reverse_result_json(bounds, &translated);
        assert_eq!(value["truth_table_status"], "budget_exceeded");
        assert_eq!(
            value["truth_table_error_details"]["code"],
            "budget_exceeded"
        );
        assert_eq!(value["truth_table_error_details"]["rows"], 4);
        assert_eq!(value["truth_table_error_details"]["max_rows"], 1);
    }

    #[test]
    fn truth_table_runtime_budget_failure_is_structured_in_reverse_json() {
        let compiled = dustroute_translate::compiler::BaselineCompiler::new(
            dustroute_translate::compiler::BaselineCompileConfig::default(),
        )
        .compile(&dustroute_translate::circuits::half_adder())
        .expect("half-adder compiles");
        let (min, max) = compiled.world.bounds().expect("compiled bounds");
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(min, max);
        let translated = dustroute_translate::api::Translator.reverse(
            &compiled.world,
            ReverseRequest::new(bounds)
                .with_truth_table(16)
                .with_truth_table_budget(
                    TruthTableBudget::new(usize::MAX, u128::MAX)
                        .with_max_solver_iterations(0)
                        .with_max_elapsed_millis(None),
                ),
        );
        let value = reverse_result_json(bounds, &translated);
        assert_eq!(value["truth_table_status"], "budget_exceeded");
        assert_eq!(
            value["truth_table_error_details"]["code"],
            "runtime_budget_exceeded"
        );
        assert_eq!(value["truth_table_error_details"]["completed_rows"], 0);
        assert_eq!(value["truth_table"], Value::Null);
    }

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
            optimization_contract_json(contract)["timing"]["mode"],
            "exact_transitions"
        );
    }

    #[test]
    fn boundary_record_keeps_static_identity_and_ignores_live_power() {
        let record =
            boundary_block_record(&dustroute_translate::snapshot::MinecraftSnapshotBlock {
                pos: Pos::new(1, 64, 2),
                name: "minecraft:repeater".to_owned(),
                properties: BTreeMap::from([
                    ("facing".to_owned(), "west".to_owned()),
                    ("delay".to_owned(), "2".to_owned()),
                    ("powered".to_owned(), "true".to_owned()),
                    ("locked".to_owned(), "false".to_owned()),
                ]),
            });
        assert_eq!(record.static_properties["facing"], "west");
        assert_eq!(record.static_properties["delay"], "2");
        assert!(!record.static_properties.contains_key("powered"));
        assert!(!record.static_properties.contains_key("locked"));
    }

    #[test]
    fn placement_baseline_rejects_dynamic_state_changes_between_preview_and_apply() {
        let mut expected = dustroute_physical::Block::new(BlockKind::Lever);
        expected.facing = Some(dustroute_physical::Facing::North);
        expected.support_offset = Some(Pos::new(0, -1, 0));
        expected.powered = Some(false);
        let mut actual = expected.clone();
        actual.powered = Some(true);
        assert!(!placement_baseline_matches(Some(&actual), &expected));
        assert!(placement_baseline_matches(Some(&expected), &expected));
        assert!(placement_baseline_matches(
            None,
            &dustroute_physical::Block::new(BlockKind::Air)
        ));
    }

    #[test]
    fn placement_baseline_accepts_observed_equivalent_synthetic_blocks() {
        let mut expected = dustroute_physical::Block::new(BlockKind::RedstoneTorch);
        expected.facing = Some(dustroute_physical::Facing::East);
        expected.support_offset = Some(Pos::new(-1, 0, 0));
        let mut actual = expected.clone();
        actual.observed_name = Some("minecraft:redstone_wall_torch".to_owned());
        actual.observed_properties = BTreeMap::from([
            ("facing".to_owned(), "east".to_owned()),
            ("lit".to_owned(), "true".to_owned()),
        ]);
        actual.observation_classification = dustroute_physical::ObservationClassification::Exact;
        actual.facing = None;
        actual.powered = Some(true);
        assert!(placement_baseline_matches(Some(&actual), &expected));
    }

    #[test]
    fn transition_trace_json_uses_arrays_for_coordinate_keyed_state() {
        let position = Pos::new(1, 64, -2);
        let trace = dustroute_translate::scenario::ScenarioTrace {
            duration_redstone_ticks: 2,
            duration_game_ticks: None,
            time_unit: dustroute_ir::TraceTimeUnit::RedstoneTick,
            events: Vec::new(),
            final_strengths: BTreeMap::from([(position, 15)]),
            final_powered: BTreeMap::from([(position, true)]),
            status: dustroute_ir::TraceStatus::Complete,
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
    async fn unified_operation_tools_reject_invalid_ids_with_argument_error() {
        let service = DustRouteMcp::new("127.0.0.1:1");
        let result: Value = serde_json::from_str(
            &service
                .invoke_operation(Parameters(InvokeOperationParams {
                    blueprint_decision: None,
                    operation_id: "not-a-uuid".into(),
                    confirm: true,
                    contracts: None,
                }))
                .await,
        )
        .unwrap();

        assert_eq!(result["ok"], false);
        assert_eq!(result["error_code"], "invalid_argument");
        assert_eq!(result["retryable"], false);

        let get_result: Value = serde_json::from_str(
            &service
                .get_operation(Parameters(OperationParams {
                    operation_id: "not-a-uuid".into(),
                }))
                .await,
        )
        .unwrap();
        assert_eq!(get_result["error_code"], "invalid_argument");

        let show_result: Value = serde_json::from_str(
            &service
                .show_operation(Parameters(ShowOperationParams {
                    operation_id: "not-a-uuid".into(),
                    player: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(show_result["error_code"], "invalid_argument");

        let undo_result: Value = serde_json::from_str(
            &service
                .undo_operation(Parameters(ConfirmedOperationParams {
                    operation_id: "not-a-uuid".into(),
                    confirm: true,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(undo_result["error_code"], "invalid_argument");

        let stop_result: Value = serde_json::from_str(
            &service
                .stop_operation(Parameters(OperationParams {
                    operation_id: "not-a-uuid".into(),
                }))
                .await,
        )
        .unwrap();
        assert_eq!(stop_result["error_code"], "invalid_argument");
    }

    #[tokio::test]
    async fn tampered_placement_is_revalidated_before_any_bridge_write() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move {
            // Baseline verification followed by placement-context validation.
            // Any write request is a regression, even if live verification
            // would later discover the broken placement.
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], "scan_region");
                let response = json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,json!({
                    "min": request["params"]["min"], "max": request["params"]["max"], "blocks": []
                }))});
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let service = DustRouteMcp::with_policy(
            address,
            McpPolicy {
                read_only: false,
                preview_required: false,
                ..McpPolicy::default()
            },
        );
        let mut proposed = dustroute_translate::world::World::new();
        proposed.place(BlockKind::Solid, Pos::new(0, 1, 0));
        let mut plan = plan_world_overlay(
            &dustroute_translate::world::World::new(),
            &dustroute_translate::world::ValidatedWorld::try_from(proposed).unwrap(),
            Pos::new(0, 0, 0),
            10,
        )
        .unwrap();
        plan.changes[0].after = dustroute_translate::world::Block::new(BlockKind::Repeater);
        let id = plan.operation_id;
        service
            .plans
            .placements()
            .lock()
            .await
            .insert(plan, "minecraft:overworld".into(), None);
        let response: Value = serde_json::from_str(
            &service
                .mutate_placement(
                    ConfirmedOperationParams {
                        operation_id: id.to_string(),
                        confirm: true,
                    },
                    false,
                )
                .await,
        )
        .unwrap();
        assert_eq!(response["ok"], false);
        assert!(response["error"].as_str().unwrap().contains("validation"));
        server.await.unwrap();
        assert!(!service.plans.placements().lock().await.is_applied(&id));
    }

    #[tokio::test]
    async fn placement_requires_show_before_invoke_when_preview_is_required() {
        let policy = McpPolicy {
            read_only: false,
            preview_required: true,
            ..McpPolicy::default()
        };
        let service = DustRouteMcp::with_policy("127.0.0.1:1", policy);
        let plan = plan_world_overlay(
            &dustroute_physical::World::new(),
            &dustroute_translate::world::ValidatedWorld::try_from(dustroute_physical::World::new())
                .unwrap(),
            Pos::new(0, 0, 0),
            1,
        )
        .unwrap();
        let operation_id = plan.operation_id;
        service
            .plans
            .placements()
            .lock()
            .await
            .insert(plan, "minecraft:overworld".into(), None);

        let rejected: Value = serde_json::from_str(
            &service
                .invoke_operation(Parameters(InvokeOperationParams {
                    blueprint_decision: None,
                    operation_id: operation_id.to_string(),
                    confirm: true,
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(rejected["error_code"], "invalid_state");
        assert!(
            !service
                .plans
                .placements()
                .lock()
                .await
                .get(&operation_id)
                .is_some_and(|plan| plan.previewed)
        );

        let shown: Value = serde_json::from_str(
            &service
                .show_operation(Parameters(ShowOperationParams {
                    operation_id: operation_id.to_string(),
                    player: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(shown["ok"], true);
        assert_eq!(shown["plan"]["previewed"], true);
        assert!(
            service
                .plans
                .placements()
                .lock()
                .await
                .get(&operation_id)
                .is_some_and(|plan| plan.previewed)
        );
    }

    #[tokio::test]
    async fn collaboration_prompt_requires_gaze_grounding_and_preview() {
        let prompt = DustRouteMcp::new("127.0.0.1:1")
            .collaboration_prompt()
            .await;
        let ContentBlock::Text(text) = &prompt.messages[0].content else {
            panic!("expected text prompt");
        };
        assert!(text.text.contains("get_world"));
        assert!(text.text.contains("test_circuit"));
        assert!(text.text.contains("get_circuit_ir"));
        assert!(text.text.contains("get_repair_context"));
        assert!(text.text.contains("new_optimization"));
        assert!(text.text.contains("show_operation"));
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

        assert_eq!(default_names.len(), 22);
        assert!(default_names.contains("manage_assembly"));
        assert!(!default_names.contains("get_piston_door_state"));
        assert!(!debug_names.contains("get_piston_door_state"));
        assert_eq!(debug_names.len(), 29);
        assert!(default_names.contains("test_circuit"));
        assert!(default_names.contains("get_circuit_ir"));
        assert!(default_names.contains("test_circuit_change"));
        assert!(default_names.contains("get_circuit_revision"));
        assert!(default_names.contains("get_repair_context"));
        assert!(default_names.contains("new_optimization"));
        assert!(default_names.contains("new_macro_optimization"));
        assert!(default_names.contains("invoke_operation"));
        assert!(!default_names.contains("invoke_repair"));
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
                .any(|tool| tool.name == "convert_from_circuit")
        );
        assert!(tools.tools.iter().any(|tool| tool.name == "test_circuit"));
        assert!(tools.tools.iter().any(|tool| tool.name == "get_world"));
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
            for _ in 0..5 {
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
                let response = json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,result) });
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
                .call_tool(CallToolRequestParams::new("set_region").with_arguments(arguments))
                .await
                .unwrap();
            let ContentBlock::Text(text) = &result.content[0] else {
                panic!("expected text tool result");
            };
            assert!(text.text.contains("\"ok\": true"));
        }
        let shown = client
            .call_tool(
                CallToolRequestParams::new("show_region").with_arguments(serde_json::Map::new()),
            )
            .await
            .unwrap();
        let ContentBlock::Text(shown_text) = &shown.content[0] else {
            panic!("expected text tool result");
        };
        let shown_value: Value = serde_json::from_str(&shown_text.text).unwrap();
        let circuit_id = shown_value["circuit_id"].as_str().unwrap();
        let ir = client
            .call_tool(
                CallToolRequestParams::new("get_circuit_ir").with_arguments(
                    serde_json::from_value::<serde_json::Map<String, Value>>(json!({
                        "circuit_id": circuit_id
                    }))
                    .unwrap(),
                ),
            )
            .await
            .unwrap();
        let ContentBlock::Text(ir_text) = &ir.content[0] else {
            panic!("expected text tool result");
        };
        assert!(ir_text.text.contains("\"ok\": true"));
        assert!(ir_text.text.contains(circuit_id));
        let converted = client
            .call_tool(
                CallToolRequestParams::new("convert_from_circuit").with_arguments(
                    serde_json::from_value::<serde_json::Map<String, Value>>(json!({
                        "scope": "selected_region"
                    }))
                    .unwrap(),
                ),
            )
            .await
            .unwrap();
        let ContentBlock::Text(converted_text) = &converted.content[0] else {
            panic!("expected text tool result");
        };
        assert!(converted_text.text.contains("\"ok\": true"));
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
                        format!("{}\n", json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,crate::bridge::test_scan_world(&request,result)) }))
                            .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let service =
            DustRouteMcp::with_policy_and_player(address, McpPolicy::default(), "builder");
        let result = service
            .convert_from_circuit(Parameters(AnalyzeLookedAtParams {
                player: None,
                max_components: Some(64),
                fragment_gap: Some(2),
                include_truth_table: Some(false),
                truth_table_max_inputs: None,
                truth_table_settle_ticks: None,
                truth_table_max_rows: None,
                truth_table_max_work_units: None,
                truth_table_max_solver_iterations: None,
                truth_table_max_elapsed_millis: None,
                scope: None,
                circuit_id: None,
            }))
            .await;
        let value: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(value["ok"], true, "{result}");
        assert_eq!(value["mechanisms"], json!([]));
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
        assert_eq!(
            value["circuit_identity"]["classification_level"],
            "local_gate_network"
        );
        assert_eq!(value["circuit_identity"]["local_gate_counts"]["buffer"], 1);
        assert_eq!(value["circuit_identity"]["local_gate_count"], 1);
        assert_eq!(
            value["circuit_identity"]["local_gate_samples"][0]["kind"],
            "buffer"
        );
        assert!(value["circuit_identity"].get("local_gates").is_none());
        assert_eq!(value["circuit_identity"]["analysis_complete"], true);
        assert!(value["circuit_identity"]["primary_candidate"].is_null());
        assert!(
            value["circuit_identity"]["uncertainty_reasons"]
                .as_array()
                .unwrap()
                .contains(&Value::String(
                    "no_registered_higher_level_pattern_matched".to_owned()
                ))
        );
        assert!(value["physical"]["observation"].is_object());
        assert!(value["physical"]["block_capabilities"]["groups"].is_array());
        assert!(value["stages"]["physical_scene"].is_object());
        assert_eq!(value["next_tools"]["repair_planning"], "new_repair");
        assert!(value.get("transition_scenarios").is_none());
        assert!(value.get("repair_proposals").is_none());
        assert_eq!(value["diagnostic"]["observation_complete"], true);
        assert!(value["diagnostic"]["counts"].is_object());
        assert!(value["diagnostic"]["recommended_next_action"].is_object());
        assert!(value["focused_explanation"]["role"].is_object());
        assert!(value["focused_explanation"]["incoming"].is_array());
        assert!(value["focused_explanation"]["paths_to_outputs"].is_array());
        let circuit_id = value["circuit_id"].as_str().unwrap().to_owned();
        let ir: Value = serde_json::from_str(
            &service
                .get_circuit_ir(Parameters(GetLookedAtCircuitIrParams {
                    player: None,
                    max_components: None,
                    fragment_gap: None,
                    node_id: None,
                    analysis_id: None,
                    circuit_id: Some(circuit_id.clone()),
                }))
                .await,
        )
        .unwrap();
        assert_eq!(ir["ok"], true);
        assert_eq!(ir["circuit_id"], circuit_id);
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
                        format!("{}\n", json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,crate::bridge::test_scan_world(&request,result)) }))
                            .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let service =
            DustRouteMcp::with_policy_and_player(address, McpPolicy::default(), "builder");
        let result = service
            .test_circuit(Parameters(DiagnoseLookedAtParams {
                player: None,
                max_components: Some(64),
                fragment_gap: Some(2),
                circuit_id: None,
            }))
            .await;
        bridge.abort();

        let value: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(value["ok"], true, "{result}");
        assert_eq!(value["schema_version"], "dustroute.diagnostic.v1");
        assert_eq!(value["analysis_mode"], "focused_fast");
        let shared: dustroute_translate::diagnostic::report::Diagnosis =
            serde_json::from_value(value["diagnostic"].clone()).unwrap();
        assert_eq!(
            shared.method,
            dustroute_translate::diagnostic::report::DiagnosticMethod::Connectivity
        );
        assert!(!shared.world_writes && !shared.repair.permission_granted);
        assert_eq!(
            shared.repair.status,
            dustroute_translate::diagnostic::report::RepairStatus::NotAssessed
        );
        assert_eq!(value["mutation_performed"], false);
        assert!(value["diagnostic"]["counts"].is_object());
        assert!(value["diagnostic"]["findings"].is_array());
        assert!(value["diagnostic"]["recommended_next_action"].is_object());
        assert!(value["focused_explanation"]["role"].is_object());
        assert!(value["focused_explanation"]["caveats"].is_array());
    }

    #[tokio::test]
    async fn repairs_and_undoes_a_broken_wire_through_the_mcp_workflow() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move {
            let mut repaired = false;
            for _ in 0..10 {
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
                    "submit_physical_batch" => {
                        repaired = request["params"]["changes"][0]["action"] == "place";
                        json!({
                            "protocol": crate::bridge_protocol::MUTATION_PROTOCOL, "placed_changes": 1,
                            "placement_mode": "mineflayer_player",
                            "retreat": { "x": 2.5, "y": 18.0, "z": 0.5 }
                        })
                    }
                    method => panic!("unexpected fake bridge method {method}"),
                };
                let response = json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,crate::bridge::test_scan_world(&request,result)) });
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
        let root = std::env::temp_dir().join(format!(
            "dustroute-repair-workflow-{}",
            uuid::Uuid::new_v4()
        ));
        let make_service = || {
            let mut service =
                DustRouteMcp::with_policy_and_player(address.clone(), policy.clone(), "builder");
            service.state_store = PlanStateStore::new(root.clone(), 3600);
            service
        };
        let service = make_service();
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(
            Pos::new(0, 0, 0),
            Pos::new(2, 2, 0),
        );
        service.selections.lock().await.insert(
            "builder".into(),
            LocatedSelection::with_bounds("builder", bounds, "minecraft:overworld".into()),
        );

        let shown: Value = serde_json::from_str(
            &service
                .show_region(Parameters(PlayerParams { player: None }))
                .await,
        )
        .unwrap();
        let circuit_id = shown["circuit_id"].as_str().unwrap().to_owned();
        let proposed: Value = serde_json::from_str(
            &service
                .new_repair(Parameters(ProposeRepairsParams {
                    player: None,
                    max_gap: Some(2),
                    circuit_id: circuit_id.clone(),
                }))
                .await,
        )
        .unwrap();
        assert_eq!(proposed["proposal_count"], 3);
        let shared: dustroute_translate::diagnostic::report::Diagnosis =
            serde_json::from_value(proposed["diagnostic"].clone()).unwrap();
        assert_eq!(
            shared.method,
            dustroute_translate::diagnostic::report::DiagnosticMethod::Connectivity
        );
        assert_eq!(
            shared.repair.status,
            dustroute_translate::diagnostic::report::RepairStatus::PlanAvailable
        );
        assert_eq!(
            shared.repair.strategy,
            dustroute_translate::diagnostic::report::RepairStrategy::PartialPatch
        );
        assert!(!shared.repair.permission_granted);
        let context: Value = serde_json::from_str(
            &service
                .get_repair_context(Parameters(GetRepairContextParams {
                    player: None,
                    circuit_id,
                    operation_id: None,
                    max_gap: Some(2),
                }))
                .await,
        )
        .unwrap();
        let context_diagnosis: dustroute_translate::diagnostic::report::Diagnosis =
            serde_json::from_value(context["facts"]["diagnostic"].clone()).unwrap();
        assert_eq!(context_diagnosis, shared);
        let finding_ids: std::collections::BTreeSet<_> = shared
            .findings
            .iter()
            .map(|f| f.finding_id.as_str())
            .collect();
        for candidate in proposed["proposals"].as_array().unwrap() {
            for id in candidate["diagnostic_finding_ids"].as_array().unwrap() {
                assert!(finding_ids.contains(id.as_str().unwrap()));
            }
        }

        let operation_id = proposed["proposals"][0]["operation_id"]
            .as_str()
            .unwrap()
            .to_owned();

        let preview = service
            .show_operation(Parameters(ShowOperationParams {
                operation_id: operation_id.clone(),
                player: None,
            }))
            .await;
        assert!(preview.contains("\"ok\": true"));
        // Preview and applied status must survive independent service instances.
        let restarted = make_service();
        let applied = restarted
            .invoke_operation(Parameters(InvokeOperationParams {
                blueprint_decision: None,
                operation_id: operation_id.clone(),
                confirm: true,
                contracts: None,
            }))
            .await;
        assert!(applied.contains("\"verified\": true"), "{applied}");
        let applied_value: Value = serde_json::from_str(&applied).unwrap();
        assert!(applied_value["resulting_logic"].is_object());
        assert_eq!(applied_value["semantic_verification"]["available"], false);
        let id = uuid::Uuid::parse_str(&operation_id).unwrap();
        assert_eq!(
            service.repair_plan(id).await.unwrap().unwrap().lifecycle,
            RepairLifecycle::Applied
        );
        let restarted = make_service();
        let undone = restarted
            .undo_operation(Parameters(ConfirmedOperationParams {
                operation_id,
                confirm: true,
            }))
            .await;
        assert!(undone.contains("\"verified\": true"), "{undone}");
        assert_eq!(
            service.repair_plan(id).await.unwrap().unwrap().lifecycle,
            RepairLifecycle::Undone
        );
        server.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn repair_operations_reject_unavailable_saved_state_before_and_after_restart() {
        for invalidation in ["expired", "deleted", "corrupt"] {
            for applied in [false, true] {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap().to_string();
                let root = std::env::temp_dir().join(format!(
                    "dustroute-repair-retention-{}",
                    uuid::Uuid::new_v4()
                ));
                let make_service = || {
                    let mut service = DustRouteMcp::with_policy_and_player(
                        address.clone(),
                        McpPolicy {
                            read_only: false,
                            ..McpPolicy::default()
                        },
                        "builder",
                    );
                    service.state_store = PlanStateStore::new(root.clone(), 3600);
                    service
                };
                let service = make_service();
                let id = uuid::Uuid::new_v4();
                let pos = Pos::new(1, 1, 0);
                service
                    .store_repair_plan(
                        id,
                        StoredRepairPlan {
                            patch: PhysicalPatch {
                                reason: dustroute_physical::PhysicalPatchReason::ConnectMissingWire,
                                affected_fragments: vec![],
                                confidence_percent: 100,
                                explanation: "restore missing wire".into(),
                                changes: vec![PhysicalBlockChange {
                                    pos,
                                    before: dustroute_physical::Block::new(BlockKind::Air),
                                    after: dustroute_physical::Block::new(BlockKind::RedstoneWire),
                                }],
                            },
                            dimension: "minecraft:overworld".into(),
                            analysis_bounds: dustroute_translate::world_reverse::RegionBounds::new(
                                Pos::new(0, 0, 0),
                                Pos::new(2, 2, 0),
                            ),
                            fragments_before: 2,
                            baseline_truth_table: None,
                            lifecycle: if applied {
                                RepairLifecycle::Applied
                            } else {
                                RepairLifecycle::Previewed
                            },
                            contract_satisfied: true,
                            preserved_boundary: vec![],
                        },
                    )
                    .await
                    .unwrap();
                // Exercise the formerly cached read before invalidating its disk record.
                assert!(service.repair_plan(id).await.unwrap().is_some());
                let path = root.join("repairs").join(format!("{id}.json"));
                match invalidation {
                    "expired" => {
                        let mut envelope: Value =
                            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                        envelope["saved_at_unix_seconds"] = json!(1);
                        std::fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
                    }
                    "deleted" => std::fs::remove_file(&path).unwrap(),
                    "corrupt" => std::fs::write(&path, b"not json").unwrap(),
                    _ => unreachable!(),
                }
                let restarted = make_service();
                let expected_code = if invalidation == "corrupt" {
                    "internal"
                } else {
                    "not_found"
                };
                let check = async {
                    for instance in [&service, &restarted] {
                        for action in ["show", "invoke", "undo"] {
                            let response = match action {
                                "show" => {
                                    instance
                                        .show_operation(Parameters(ShowOperationParams {
                                            operation_id: id.to_string(),
                                            player: None,
                                        }))
                                        .await
                                }
                                "invoke" => {
                                    instance
                                        .invoke_operation(Parameters(InvokeOperationParams {
                                            operation_id: id.to_string(),
                                            confirm: true,
                                            blueprint_decision: None,
                                            contracts: None,
                                        }))
                                        .await
                                }
                                "undo" => {
                                    instance
                                        .undo_operation(Parameters(ConfirmedOperationParams {
                                            operation_id: id.to_string(),
                                            confirm: true,
                                        }))
                                        .await
                                }
                                _ => unreachable!(),
                            };
                            let value: Value = serde_json::from_str(&response).unwrap();
                            assert_eq!(value["ok"], false, "{invalidation}/{action}: {value}");
                            assert_eq!(
                                value["error_code"], expected_code,
                                "{invalidation}/{action}: {value}"
                            );
                        }
                    }
                };
                tokio::select! {
                    () = check => {},
                    connection = listener.accept() => panic!(
                        "unavailable repair state must reject before any bridge call: {connection:?}"
                    ),
                }
                // Also reject a connection queued while a handler returned without awaiting it.
                assert!(
                    tokio::time::timeout(Duration::from_millis(10), listener.accept())
                        .await
                        .is_err()
                );
                std::fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn raw_inspection_preserves_states_and_reports_scan_boundaries() {
        let snapshot: dustroute_translate::snapshot::MinecraftSnapshot =
            serde_json::from_value(json!({
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

    #[tokio::test]
    async fn adopted_grounded_assembly_enters_existing_revision_placement_path() {
        let (records, proposal) = super::blueprint_tests::placement_fixture();
        let base_record: dustroute_library::assembly::AssemblyRevision =
            serde_json::from_value(records["blueprint"]["records"]["assemblies"][0].clone())
                .unwrap();
        let candidate_id: dustroute_library::blueprint::AssemblyRevisionId =
            serde_json::from_value(
                proposal["blueprint"]["request"]["candidate_state"]["id"].clone(),
            )
            .unwrap();
        let known = base_record.assembly.known_regions[0];
        let base = snapshot_from_grounded_assembly(&base_record, known.min, known.max).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let bridge_base = base.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                let result = match request["method"].as_str().unwrap() {
                    "status" => {
                        json!({"connected":true,"username":"DustRouteBot","host":"localhost","port":25565,"version":"1.21.11","dimension":"minecraft:overworld"})
                    }
                    "scan_region" => {
                        let min: Pos =
                            serde_json::from_value(request["params"]["min"].clone()).unwrap();
                        let max: Pos =
                            serde_json::from_value(request["params"]["max"].clone()).unwrap();
                        let blocks = bridge_base
                            .blocks
                            .iter()
                            .filter(|block| {
                                let p = block.pos;
                                p.x >= min.x
                                    && p.y >= min.y
                                    && p.z >= min.z
                                    && p.x <= max.x
                                    && p.y <= max.y
                                    && p.z <= max.z
                            })
                            .collect::<Vec<_>>();
                        json!({"min":min,"max":max,"blocks":blocks})
                    }
                    other => panic!("unexpected bridge request {other}"),
                };
                stream
                    .write_all(
                        format!("{}\n", json!({"id":request["id"],"result":crate::bridge::test_readback_response(&request,result)})).as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let root = std::env::temp_dir().join(format!(
            "dustroute-adopted-placement-{}",
            uuid::Uuid::new_v4()
        ));
        let mut service =
            DustRouteMcp::with_policy_and_player(address, McpPolicy::default(), "builder");
        service.state_store = PlanStateStore::new(root.clone(), 3600);
        let imported: Value = serde_json::from_str(
            &service
                .test_circuit_change(Parameters(serde_json::from_value(records).unwrap()))
                .await,
        )
        .unwrap();
        assert_eq!(imported["ok"], true, "{imported}");
        let proposed: Value = serde_json::from_str(
            &service
                .test_circuit_change(Parameters(serde_json::from_value(proposal).unwrap()))
                .await,
        )
        .unwrap();
        assert_eq!(proposed["ok"], true, "{proposed}");
        let unadopted =
            crate::blueprint_mcp::grounded_source(&service.state_store, "builder", &candidate_id)
                .unwrap_err();
        assert!(
            unadopted.contains("exactly one adopted update"),
            "{unadopted}"
        );
        let adopted: Value = serde_json::from_str(
            &service
                .invoke_operation(Parameters(InvokeOperationParams {
                    blueprint_decision: Some(crate::blueprint_mcp::BlueprintDecision::Adopt),
                    operation_id: proposed["operation_id"].as_str().unwrap().into(),
                    confirm: true,
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(adopted["ok"], true, "{adopted}");
        let ungrounded =
            crate::blueprint_mcp::grounded_source(&service.state_store, "builder", &candidate_id)
                .unwrap_err();
        assert!(
            ungrounded.contains("captured Circuit Revision ancestor"),
            "{ungrounded}"
        );
        let grounding_revision = uuid::Uuid::new_v4();
        let grounding = crate::blueprint_mcp::AssemblyGrounding {
            assembly_revision_id: base_record.id.clone(),
            circuit_revision_id: grounding_revision,
            base_observation_id: uuid::Uuid::new_v4(),
            dimension: "minecraft:overworld".into(),
            complete: true,
            base_snapshot: base,
        };
        let mut incomplete = grounding.clone();
        incomplete.complete = false;
        let rejected = crate::blueprint_mcp::execute(
            &service.state_store,
            "builder",
            crate::blueprint_mcp::Command::Capture {
                record: Box::new(base_record.clone()),
                grounding: incomplete,
            },
        )
        .unwrap_err();
        assert!(
            rejected.contains("complete Assembly Revision"),
            "{rejected}"
        );
        crate::blueprint_mcp::execute(
            &service.state_store,
            "builder",
            crate::blueprint_mcp::Command::Capture {
                record: Box::new(base_record.clone()),
                grounding,
            },
        )
        .unwrap();
        let plan: Value = serde_json::from_str(
            &service
                .new_placement(Parameters(
                    serde_json::from_value(json!({"assembly_revision_id":candidate_id})).unwrap(),
                ))
                .await,
        )
        .unwrap();
        assert_eq!(plan["ok"], true, "{plan}");
        assert_eq!(plan["source"]["kind"], "adopted_assembly_revision");
        assert_eq!(plan["source"]["fresh_review"]["status"], "passed");
        assert_eq!(
            plan["source"]["literal_observation"],
            "grounding.base_snapshot"
        );
        assert!(!plan["plan"]["changes"].as_array().unwrap().is_empty());
        server.abort();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn revision_placement_revalidates_cumulative_diff_context_and_undo() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        for mode in [
            "ok",
            "stale",
            "uncertain",
            "post_mismatch",
            "unpreviewed",
            "read_only",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let writes = Arc::new(AtomicUsize::new(0));
            let count = writes.clone();
            let drift = Arc::new(AtomicBool::new(false));
            let changed = drift.clone();
            let server = tokio::spawn(async move {
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut line = String::new();
                    BufReader::new(&mut stream)
                        .read_line(&mut line)
                        .await
                        .unwrap();
                    let req: Value = serde_json::from_str(&line).unwrap();
                    let mut error = None;
                    let result = match req["method"].as_str().unwrap() {
                        "status" => {
                            json!({"connected":true,"username":"DustRouteBot","host":"localhost","port":25565,"version":"1.21.11","dimension":"minecraft:overworld"})
                        }
                        "preview_region" => json!({}),
                        "scan_region" => {
                            let min: Pos =
                                serde_json::from_value(req["params"]["min"].clone()).unwrap();
                            let max: Pos =
                                serde_json::from_value(req["params"]["max"].clone()).unwrap();
                            let x = if count.load(Ordering::SeqCst) == 1 && mode != "post_mismatch"
                            {
                                1
                            } else {
                                0
                            };
                            let mut positions = vec![Pos::new(x, 0, 0)];
                            if changed.load(Ordering::SeqCst) {
                                positions.push(Pos::new(2, 0, 0));
                            }
                            let blocks = positions
                                .into_iter()
                                .filter(|p| {
                                    p.x >= min.x
                                        && p.y >= min.y
                                        && p.z >= min.z
                                        && p.x <= max.x
                                        && p.y <= max.y
                                        && p.z <= max.z
                                })
                                .map(|p| json!({"pos":p,"name":"minecraft:stone","properties":{}}))
                                .collect::<Vec<_>>();
                            json!({"min":min,"max":max,"blocks":blocks})
                        }
                        "submit_command_batch" => {
                            count.fetch_add(1, Ordering::SeqCst);
                            if mode == "uncertain" {
                                error = Some("write reply lost");
                            }
                            json!({"protocol":crate::bridge_protocol::MUTATION_PROTOCOL,"submitted_changes":req["params"]["changes"].as_array().unwrap().len()})
                        }
                        other => panic!("unexpected bridge request {other}"),
                    };
                    let reply = if let Some(error) = error {
                        json!({"id":req["id"],"error":error})
                    } else {
                        json!({"id":req["id"],"result":crate::bridge::test_readback_response(&req,result)})
                    };
                    stream
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .unwrap();
                }
            });
            let root = std::env::temp_dir().join(format!(
                "dustroute-revision-placement-{}",
                uuid::Uuid::new_v4()
            ));
            let mut service = DustRouteMcp::with_policy_and_player(
                address,
                McpPolicy {
                    read_only: mode == "read_only",
                    ..McpPolicy::default()
                },
                "builder",
            );
            service.state_store = PlanStateStore::new(root.clone(), 3600);
            let snapshot:dustroute_translate::snapshot::MinecraftSnapshot=serde_json::from_value(json!({"min":{"x":0,"y":0,"z":0},"max":{"x":1,"y":1,"z":1},"blocks":[{"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone","properties":{}}]})).unwrap();
            let id = service
                .store_circuit(StoredCircuit {
                    player: "builder".into(),
                    dimension: "minecraft:overworld".into(),
                    bounds: dustroute_translate::world_reverse::RegionBounds::new(
                        snapshot.min,
                        snapshot.max,
                    ),
                    target: None,
                    snapshot,
                    expansion: json!({}),
                    complete: true,
                    expires_at: Instant::now() + Duration::from_secs(300),
                })
                .await;
            let first:Value=serde_json::from_str(&service.test_circuit_change(Parameters(serde_json::from_value(json!({"circuit_id":id,"changes":[{"position":{"x":1,"y":0,"z":0},"block":"minecraft:stone"}]})).unwrap())).await).unwrap();
            let revision:Value=serde_json::from_str(&service.test_circuit_change(Parameters(serde_json::from_value(json!({"revision_id":first["revision_id"],"changes":[{"position":{"x":0,"y":0,"z":0},"block":"minecraft:air"}]})).unwrap())).await).unwrap();
            if mode == "ok" {
                let mut legacy = service
                    .load_revision(revision["revision_id"].as_str().unwrap(), "builder")
                    .unwrap();
                legacy.revision_id = uuid::Uuid::new_v4();
                legacy.base_snapshot = None;
                legacy.assembly = None;
                service
                    .state_store
                    .save(
                        PlanRecordKind::CircuitRevisions,
                        legacy.revision_id,
                        &legacy,
                    )
                    .unwrap();
                let denied: Value = serde_json::from_str(
                    &service
                        .new_placement(Parameters(
                            serde_json::from_value(json!({"revision_id":legacy.revision_id}))
                                .unwrap(),
                        ))
                        .await,
                )
                .unwrap();
                assert_eq!(denied["ok"], false);
                assert_eq!(writes.load(Ordering::SeqCst), 0);
            }
            service.circuits.lock().await.clear(); // persisted base survives original observation expiry.
            let proposal: Value = serde_json::from_str(
                &service
                    .new_placement(Parameters(
                        serde_json::from_value(json!({"revision_id":revision["revision_id"]}))
                            .unwrap(),
                    ))
                    .await,
            )
            .unwrap();
            assert_eq!(proposal["ok"], true, "{mode}: {proposal}");
            assert_eq!(proposal["plan"]["changes"].as_array().unwrap().len(), 2);
            let op = proposal["operation_id"].as_str().unwrap().to_owned();
            if mode != "unpreviewed" {
                let shown: Value = serde_json::from_str(
                    &service
                        .show_operation(Parameters(ShowOperationParams {
                            operation_id: op.clone(),
                            player: None,
                        }))
                        .await,
                )
                .unwrap();
                assert_eq!(shown["ok"], true);
            }
            if mode == "stale" {
                drift.store(true, Ordering::SeqCst);
            }
            let result: Value = serde_json::from_str(
                &service
                    .invoke_operation(Parameters(InvokeOperationParams {
                        blueprint_decision: None,
                        operation_id: op.clone(),
                        confirm: true,
                        contracts: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(result["ok"], mode == "ok", "{mode}: {result}");
            let expected = usize::from(matches!(mode, "ok" | "uncertain" | "post_mismatch"));
            assert_eq!(writes.load(Ordering::SeqCst), expected);
            if mode == "uncertain" || mode == "post_mismatch" {
                let retry: Value = serde_json::from_str(
                    &service
                        .invoke_operation(Parameters(InvokeOperationParams {
                            blueprint_decision: None,
                            operation_id: op.clone(),
                            confirm: true,
                            contracts: None,
                        }))
                        .await,
                )
                .unwrap();
                assert_eq!(retry["ok"], false);
                assert_eq!(writes.load(Ordering::SeqCst), 1);
            }
            let undo: Value = serde_json::from_str(
                &service
                    .undo_operation(Parameters(ConfirmedOperationParams {
                        operation_id: op,
                        confirm: true,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(undo["ok"], mode == "ok", "{mode}: {undo}");
            assert_eq!(
                writes.load(Ordering::SeqCst),
                if mode == "ok" { 2 } else { expected }
            );
            server.abort();
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn revisions_branch_persist_validate_and_never_become_live_circuits() {
        let root =
            std::env::temp_dir().join(format!("dustroute-revisions-{}", uuid::Uuid::new_v4()));
        let mut service =
            DustRouteMcp::with_policy_and_player("127.0.0.1:1", McpPolicy::default(), "builder");
        service.state_store = PlanStateStore::new(root.clone(), 3600);
        let snapshot:dustroute_translate::snapshot::MinecraftSnapshot=serde_json::from_value(json!({
            "min":{"x":-1,"y":-1,"z":-1},"max":{"x":3,"y":3,"z":3},
            "blocks":[{"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone","properties":{}},
            {"pos":{"x":0,"y":1,"z":0},"name":"minecraft:repeater","properties":{"facing":"north","delay":"1","powered":"false","locked":"false"}}]
        })).unwrap();
        let id = service
            .store_circuit(StoredCircuit {
                player: "builder".into(),
                dimension: "minecraft:overworld".into(),
                bounds: dustroute_translate::world_reverse::RegionBounds::new(
                    snapshot.min,
                    snapshot.max,
                ),
                target: None,
                snapshot: snapshot.clone(),
                expansion: json!({}),
                complete: true,
                expires_at: Instant::now() + Duration::from_secs(300),
            })
            .await;
        async fn edit(service: &DustRouteMcp, args: Value) -> Value {
            serde_json::from_str(
                &service
                    .test_circuit_change(Parameters(serde_json::from_value(args).unwrap()))
                    .await,
            )
            .unwrap()
        }
        let base = edit(&service, json!({"circuit_id":id,"changes":[]})).await;
        assert_eq!(base["ok"], true, "{base}");
        assert_eq!(base["parent_revision_ids"], json!([]));
        assert_eq!(base["base_observation_id"], id.to_string());
        assert_eq!(base["validation"]["after"]["status"], "structurally_valid");
        assert_eq!(base["assembly_state"]["status"], "available");
        assert_eq!(base["assembly_state"]["source_instances"], 0);
        let removed=edit(&service,json!({"revision_id":base["revision_id"],"changes":[{"position":{"x":0,"y":0,"z":0},"block":"minecraft:air"}]})).await;
        assert_eq!(removed["ok"], true, "{removed}");
        assert_eq!(
            removed["validation"]["after"]["status"],
            "invalid_or_unsupported"
        );
        assert_eq!(
            removed["validation"]["after"]["simulation"]["status"],
            "not_run"
        );
        let branch=edit(&service,json!({"revision_id":base["revision_id"],"changes":[{"position":{"x":0,"y":1,"z":0},"block":"minecraft:repeater","properties":{"facing":"east","delay":"2","powered":"false","locked":"false"}}]})).await;
        assert_eq!(branch["ok"], true, "{branch}");
        assert_eq!(branch["parent_revision_ids"], json!([base["revision_id"]]));
        assert_eq!(
            removed["parent_revision_ids"],
            branch["parent_revision_ids"]
        );
        assert_ne!(removed["revision_id"], branch["revision_id"]);
        assert_ne!(
            removed["assembly_state"]["assembly_revision_id"],
            branch["assembly_state"]["assembly_revision_id"]
        );
        assert_eq!(
            branch["assembly_state"]["parent_assembly_revision_ids"],
            json!([base["assembly_state"]["assembly_revision_id"]])
        );
        let repaired=edit(&service,json!({"revision_id":removed["revision_id"],"changes":[{"position":{"x":0,"y":0,"z":0},"block":"minecraft:stone"}]})).await;
        assert_eq!(
            repaired["validation"]["after"]["status"],
            "structurally_valid"
        );
        assert_eq!(
            repaired["parent_revision_ids"],
            json!([removed["revision_id"]])
        );
        assert_eq!(repaired["base_observation_id"], id.to_string());
        assert!(
            service
                .load_circuit(branch["revision_id"].as_str().unwrap(), "builder")
                .await
                .is_err()
        );
        assert_eq!(
            service
                .load_circuit(&id.to_string(), "builder")
                .await
                .unwrap()
                .1
                .snapshot,
            snapshot
        );
        assert!(service.plans.placements().lock().await.is_empty());
        assert!(
            service
                .plans
                .table::<StoredDoorPlan>()
                .lock()
                .await
                .is_empty()
        );
        assert!(
            service
                .plans
                .table::<StoredPistonPlacement>()
                .lock()
                .await
                .is_empty()
        );
        // A fresh service can read/edit revisions without the original observation or bridge.
        let mut restarted =
            DustRouteMcp::with_policy_and_player("127.0.0.1:1", McpPolicy::default(), "builder");
        restarted.state_store = PlanStateStore::new(root.clone(), 3600);
        let persisted: Value = serde_json::from_str(
            &restarted
                .get_circuit_revision(Parameters(GetCircuitRevisionParams {
                    blueprint: None,
                    revision_id: base["revision_id"].as_str().unwrap().into(),
                    include_snapshot: Some(true),
                }))
                .await,
        )
        .unwrap();
        assert_eq!(persisted["ok"], true);
        assert_eq!(persisted["snapshot"], json!(snapshot));
        assert_eq!(
            persisted["assembly_revision"]["id"],
            base["assembly_state"]["assembly_revision_id"]
        );
        assert!(
            persisted["assembly_revision"]["assembly"]["instances"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(persisted["validation"], base["validation"]);
        let child = edit(
            &restarted,
            json!({"revision_id":branch["revision_id"],"changes":[]}),
        )
        .await;
        assert_eq!(child["ok"], true);
        assert_eq!(child["base_observation_id"], id.to_string());
        assert!(
            restarted
                .load_revision(base["revision_id"].as_str().unwrap(), "someone_else")
                .is_err()
        );
        for args in [
            json!({"changes":[]}),
            json!({"circuit_id":id,"revision_id":base["revision_id"],"changes":[]}),
            json!({"revision_id":id,"changes":[]}),
            json!({"revision_id":base["revision_id"],"changes":[],"simulation_ticks":257}),
            json!({"revision_id":base["revision_id"],"changes":[{"position":{"x":99,"y":0,"z":0},"block":"minecraft:stone"}]}),
        ] {
            assert_eq!(edit(&service, args).await["ok"], false);
        }
        // A revision ID is not an operation capability either.
        let invoked: Value = serde_json::from_str(
            &service
                .invoke_operation(Parameters(InvokeOperationParams {
                    blueprint_decision: None,
                    operation_id: base["revision_id"].as_str().unwrap().into(),
                    confirm: true,
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(invoked["ok"], false);
        let invalid_state=edit(&service,json!({"revision_id":base["revision_id"],"changes":[{"position":{"x":0,"y":1,"z":0},"block":"minecraft:repeater","properties":{"facing":"sideways","delay":"2"}}]})).await;
        assert_eq!(invalid_state["ok"], true);
        assert_eq!(
            invalid_state["assembly_state"]["status"],
            "unavailable_or_legacy"
        );
        assert_eq!(
            invalid_state["validation"]["after"]["status"],
            "unavailable"
        );
        // Expiry of an ancestor does not invalidate a self-contained descendant.
        let path = root
            .join("circuit_revisions")
            .join(format!("{}.json", base["revision_id"].as_str().unwrap()));
        let mut envelope: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        envelope["saved_at_unix_seconds"] = json!(0);
        std::fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(
            service
                .load_revision(base["revision_id"].as_str().unwrap(), "builder")
                .is_err()
        );
        assert!(
            service
                .load_revision(branch["revision_id"].as_str().unwrap(), "builder")
                .is_ok()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn compiled_placement_keeps_pinned_sources_and_composed_state_in_existing_api() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let origin = Pos::new(10, 64, -20);
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let result = match req["method"].as_str().unwrap() {
                    "observe_player" => {
                        json!({"player":"builder","eye_position":{"x":10.0,"y":67.0,"z":-20.0},"yaw":0.0,"pitch":0.0,"targeted_block":origin,"dimension":"minecraft:overworld"})
                    }
                    "scan_region" => {
                        json!({"min":req["params"]["min"],"max":req["params"]["max"],"blocks":[]})
                    }
                    other => panic!("unexpected request (planning must not write): {other}"),
                };
                let reply = json!({"id":req["id"],"result":crate::bridge::test_readback_response(&req,result)});
                stream
                    .write_all(format!("{reply}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let service =
            DustRouteMcp::with_policy_and_player(address, McpPolicy::default(), "builder");
        for optimize in [false, true] {
            let proposed: Value = serde_json::from_str(
                &service
                    .new_placement(Parameters(
                        serde_json::from_value(json!({"circuit":"half-adder","optimize":optimize}))
                            .unwrap(),
                    ))
                    .await,
            )
            .unwrap();
            assert_eq!(proposed["ok"], true, "{proposed}");
            let full: Value = serde_json::from_str(
                &service
                    .get_circuit_placement(Parameters(OperationParams {
                        operation_id: proposed["operation_id"].as_str().unwrap().into(),
                    }))
                    .await,
            )
            .unwrap();
            let plan: PlacementPlan = serde_json::from_value(full["plan"].clone()).unwrap();
            let saved = plan.assembly.as_ref().unwrap();
            assert_eq!(saved.coordinate_origin, origin);
            assert_eq!(
                json!(saved.revision.id),
                proposed["assembly_state"]["assembly_revision_id"]
            );
            let view = saved
                .revision
                .assembly
                .inspect(dustroute_library::builtin_blueprints::builtin_blueprints())
                .unwrap();
            assert!(!view.source_differences().is_empty());
            assert!(
                view.occurrences
                    .values()
                    .any(|occurrence| occurrence.revision.as_str()
                        == dustroute_library::builtin_blueprints::NOT_TOP_REVISION)
            );
            let world = view.proposed_world();
            assert_eq!(world.iter().count(), plan.changes.len());
            for change in &plan.changes {
                let local = change.pos.offset(-origin.x, -origin.y, -origin.z);
                assert_eq!(world.get(local), Some(&change.after));
            }
            assert!(!plan.previewed);
            let shown: Value = serde_json::from_str(
                &service
                    .show_operation(Parameters(ShowOperationParams {
                        operation_id: proposed["operation_id"].as_str().unwrap().into(),
                        player: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(shown["ok"], true);
            assert_eq!(shown["plan"]["assembly"], full["plan"]["assembly"]);
            assert_eq!(shown["plan"]["previewed"], true);
            let mut legacy = full["plan"].clone();
            legacy.as_object_mut().unwrap().remove("assembly");
            assert!(
                serde_json::from_value::<PlacementPlan>(legacy)
                    .unwrap()
                    .assembly
                    .is_none()
            );
        }
        server.abort();
    }

    #[tokio::test]
    async fn piston_placement_uses_common_tools_and_fails_closed() {
        use crate::piston_door::{DoorState, sample};
        use std::sync::atomic::{AtomicUsize, Ordering};
        for mode in [
            "ok",
            "stale",
            "uncertain",
            "post_mismatch",
            "unpreviewed",
            "read_only",
            "expired",
            "unconfirmed",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let writes = Arc::new(AtomicUsize::new(0));
            let count = writes.clone();
            let server = tokio::spawn(async move {
                let mut scans = 0;
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut line = String::new();
                    BufReader::new(&mut stream)
                        .read_line(&mut line)
                        .await
                        .unwrap();
                    let req: Value = serde_json::from_str(&line).unwrap();
                    let mut error = None;
                    let result = match req["method"].as_str().unwrap() {
                        "status" => {
                            json!({"connected":true,"username":"DustRouteBot","host":"localhost","port":25565,"version":"1.21.11","dimension":"minecraft:overworld"})
                        }
                        "observe_player" => {
                            json!({"player":"builder","eye_position":{"x":0.0,"y":0.0,"z":0.0},"yaw":0.0,"pitch":0.0,"targeted_block":{"x":0,"y":-3,"z":0},"targeted_face":"up","distance":3.0,"dimension":"minecraft:overworld"})
                        }
                        "preview_region" | "wait_ticks" => json!({}),
                        "scan_region" => {
                            let mut snapshot = sample(DoorState::Open);
                            if count.load(Ordering::SeqCst) != 1 || mode == "post_mismatch" {
                                snapshot.blocks.clear();
                            }
                            if mode == "stale" && scans > 0 {
                                snapshot
                                    .blocks
                                    .push(sample(DoorState::Open).blocks[0].clone());
                            }
                            scans += 1;
                            serde_json::to_value(snapshot).unwrap()
                        }
                        "submit_command_batch" => {
                            count.fetch_add(1, Ordering::SeqCst);
                            if mode == "uncertain" {
                                error = Some("write response lost");
                            }
                            json!({"protocol":crate::bridge_protocol::MUTATION_PROTOCOL,"submitted_changes":req["params"]["changes"].as_array().unwrap().len()})
                        }
                        other => panic!("unexpected request {other}"),
                    };
                    let reply = if let Some(error) = error {
                        json!({"id":req["id"],"error":error})
                    } else {
                        json!({"id":req["id"],"result":crate::bridge::test_readback_response(&req,result)})
                    };
                    stream
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .unwrap();
                }
            });
            let service = DustRouteMcp::with_policy_and_player(
                address,
                McpPolicy {
                    read_only: mode == "read_only",
                    ..McpPolicy::default()
                },
                "builder",
            );
            let proposal: Value = serde_json::from_str(
                &service
                    .new_placement(Parameters(PreviewPlacementParams {
                        player: None,
                        circuit: "piston-door-1x2".into(),
                        revision_id: None,
                        assembly_revision_id: None,
                        assembly_target: None,
                        max_blocks: None,
                        optimize: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(proposal["ok"], true, "{mode}: {proposal}");
            let id = proposal["operation_id"].as_str().unwrap().to_owned();
            let detail: Value = serde_json::from_str(
                &service
                    .get_circuit_placement(Parameters(OperationParams {
                        operation_id: id.clone(),
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(detail["ok"], true);
            assert_eq!(detail["plan"]["changes"], proposal["changes"]);
            if mode != "unpreviewed" {
                let shown: Value = serde_json::from_str(
                    &service
                        .show_operation(Parameters(ShowOperationParams {
                            operation_id: id.clone(),
                            player: None,
                        }))
                        .await,
                )
                .unwrap();
                assert_eq!(shown["ok"], true);
            }
            if mode == "expired" {
                service
                    .plans
                    .table::<StoredPistonPlacement>()
                    .lock()
                    .await
                    .get_mut(&uuid::Uuid::parse_str(&id).unwrap())
                    .unwrap()
                    .expires_at = Instant::now();
            }
            let result: Value = serde_json::from_str(
                &service
                    .invoke_operation(Parameters(InvokeOperationParams {
                        blueprint_decision: None,
                        operation_id: id.clone(),
                        confirm: mode != "unconfirmed",
                        contracts: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(result["ok"], mode == "ok", "{mode}: {result}");
            let expected = usize::from(matches!(mode, "ok" | "uncertain" | "post_mismatch"));
            assert_eq!(writes.load(Ordering::SeqCst), expected, "{mode}");
            if expected == 1 {
                let retry: Value = serde_json::from_str(
                    &service
                        .invoke_operation(Parameters(InvokeOperationParams {
                            blueprint_decision: None,
                            operation_id: id.clone(),
                            confirm: true,
                            contracts: None,
                        }))
                        .await,
                )
                .unwrap();
                assert_eq!(retry["ok"], false);
                assert_eq!(writes.load(Ordering::SeqCst), 1);
            }
            let undo: Value = serde_json::from_str(
                &service
                    .undo_operation(Parameters(ConfirmedOperationParams {
                        operation_id: id,
                        confirm: true,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(undo["ok"], mode == "ok", "{mode}: {undo}");
            assert_eq!(
                writes.load(Ordering::SeqCst),
                if mode == "ok" { 2 } else { expected }
            );
            server.abort();
        }
    }

    #[tokio::test]
    async fn piston_door_operations_revalidate_consume_and_verify() {
        use crate::piston_door::{DoorState, sample};
        use std::sync::atomic::{AtomicUsize, Ordering};
        for mode in [
            "ok",
            "changed",
            "post_mismatch",
            "uncertain",
            "noop",
            "expired",
            "unpreviewed",
            "unconfirmed",
            "read_only",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let activations = Arc::new(AtomicUsize::new(0));
            let count = activations.clone();
            let server = tokio::spawn(async move {
                let mut scans = 0;
                loop {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut line = String::new();
                    BufReader::new(&mut stream)
                        .read_line(&mut line)
                        .await
                        .unwrap();
                    let req: Value = serde_json::from_str(&line).unwrap();
                    let mut error = None;
                    let result = match req["method"].as_str().unwrap() {
                        "status" => {
                            json!({"connected":true,"username":"DustRouteBot","host":"localhost","port":25565,"version":"1.21.11","dimension":"minecraft:overworld"})
                        }
                        "preview_region" => json!({}),
                        "approach_lever" => {
                            json!({"pos":{"x":2,"y":0,"z":4},"moved":false,"distance":2.0})
                        }
                        "scan_region" => {
                            let closed = scans > 0 && mode != "post_mismatch";
                            let mut s = sample(if closed {
                                DoorState::Closed
                            } else {
                                DoorState::Open
                            });
                            if mode == "changed" {
                                s.blocks.remove(0);
                            }
                            scans += 1;
                            serde_json::to_value(s).unwrap()
                        }
                        "activate_lever" => {
                            count.fetch_add(1, Ordering::SeqCst);
                            if mode == "uncertain" {
                                error = Some("activation reply lost");
                            }
                            json!({"pos":{"x":2,"y":0,"z":4},"before_powered":false,"after_powered":true})
                        }
                        "wait_ticks" => json!({}),
                        other => panic!("unexpected write or request: {other}"),
                    };
                    let reply = if let Some(error) = error {
                        json!({"id":req["id"],"error":error})
                    } else {
                        json!({"id":req["id"],"result":crate::bridge::test_readback_response(&req,result)})
                    };
                    stream
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .unwrap();
                }
            });
            let service = DustRouteMcp::with_policy_and_player(
                address,
                McpPolicy {
                    read_only: mode == "read_only",
                    ..McpPolicy::default()
                },
                "builder",
            );
            let snapshot = sample(DoorState::Open);
            let bounds =
                dustroute_translate::world_reverse::RegionBounds::new(snapshot.min, snapshot.max);
            let circuit = service
                .store_circuit(StoredCircuit {
                    player: "builder".into(),
                    dimension: "minecraft:overworld".into(),
                    bounds,
                    target: None,
                    snapshot,
                    expansion: json!({}),
                    complete: true,
                    expires_at: Instant::now() + std::time::Duration::from_secs(300),
                })
                .await;
            let proposed: Value = serde_json::from_str(
                &service
                    .new_piston_door_operation(Parameters(NewPistonDoorParams {
                        circuit_id: circuit.to_string(),
                        target: if mode == "noop" {
                            DoorState::Open
                        } else {
                            DoorState::Closed
                        },
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(proposed["ok"], true, "{mode}: {proposed}");
            let id = proposed["operation_id"].as_str().unwrap().to_owned();
            if mode != "unpreviewed" {
                let preview: Value = serde_json::from_str(
                    &service
                        .show_operation(Parameters(ShowOperationParams {
                            operation_id: id.clone(),
                            player: None,
                        }))
                        .await,
                )
                .unwrap();
                assert_eq!(preview["ok"], true);
            }
            if mode == "expired" {
                service
                    .plans
                    .table::<StoredDoorPlan>()
                    .lock()
                    .await
                    .get_mut(&uuid::Uuid::parse_str(&id).unwrap())
                    .unwrap()
                    .expires_at = Instant::now();
            }
            let result: Value = serde_json::from_str(
                &service
                    .invoke_operation(Parameters(InvokeOperationParams {
                        blueprint_decision: None,
                        operation_id: id.clone(),
                        confirm: mode != "unconfirmed",
                        contracts: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(
                result["ok"],
                mode == "ok" || mode == "noop",
                "{mode}: {result}"
            );
            let expected = usize::from(matches!(mode, "ok" | "post_mismatch" | "uncertain"));
            assert_eq!(activations.load(Ordering::SeqCst), expected, "{mode}");
            if matches!(mode, "ok" | "post_mismatch" | "uncertain" | "noop") {
                let retry: Value = serde_json::from_str(
                    &service
                        .invoke_operation(Parameters(InvokeOperationParams {
                            blueprint_decision: None,
                            operation_id: id,
                            confirm: true,
                            contracts: None,
                        }))
                        .await,
                )
                .unwrap();
                assert_eq!(retry["ok"], false);
                assert_eq!(activations.load(Ordering::SeqCst), expected);
            }
            if matches!(mode, "post_mismatch" | "uncertain") {
                assert_eq!(result["status"], "needs_inspection");
            }
            if matches!(mode, "ok" | "post_mismatch" | "uncertain") {
                assert_eq!(
                    result["observation"]["state"],
                    if mode == "post_mismatch" {
                        "open"
                    } else {
                        "closed"
                    }
                );
            }
            service.selections.lock().await.insert(
                "builder".into(),
                LocatedSelection::with_bounds("builder", bounds, "minecraft:overworld".into()),
            );
            let fresh: Value = serde_json::from_str(
                &service
                    .show_region(Parameters(PlayerParams { player: None }))
                    .await,
            )
            .unwrap();
            assert_eq!(fresh["ok"], true, "{mode}: {fresh}");
            assert_eq!(fresh["source"], "fresh_scan");
            assert_ne!(fresh["circuit_id"], circuit.to_string());
            if mode == "ok" || mode == "uncertain" {
                assert_eq!(fresh["mechanisms"][0]["kind"], "piston_door");
                assert_eq!(fresh["mechanisms"][0]["state"], "closed");
            }
            if mode == "ok" {
                let converted: Value = serde_json::from_str(&service.convert_from_circuit(Parameters(
                    serde_json::from_value(json!({"circuit_id": fresh["circuit_id"], "include_truth_table": false})).unwrap()
                )).await).unwrap();
                assert_eq!(converted["ok"], true, "{converted}");
                assert_eq!(converted["mechanisms"], fresh["mechanisms"]);
                let original: Value = serde_json::from_str(&service.convert_from_circuit(Parameters(
                    serde_json::from_value(json!({"circuit_id": circuit.to_string(), "include_truth_table": false})).unwrap()
                )).await).unwrap();
                assert_eq!(original["mechanisms"][0]["state"], "open");
            }
            if mode == "changed" {
                assert_eq!(
                    fresh["mechanisms"][0]["kind"],
                    "unidentified_piston_mechanism"
                );
                assert!(fresh["mechanisms"][0]["state"].is_null());
            }
            assert_eq!(activations.load(Ordering::SeqCst), expected);
            server.abort();
        }
    }
}
