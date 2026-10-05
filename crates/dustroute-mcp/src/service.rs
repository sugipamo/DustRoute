use crate::operations::mutation::{Success, UnrecordedFailure};
#[cfg(test)]
use crate::operations::preview::optimization::OptimizationContractView;
use crate::operations::preview::optimization::WireObjective;
use crate::operations::preview::{
    BuiltinOptimization, BuiltinPlacementPreview, BuiltinPlanningFailure, DoorPlanningFailure,
    DoorProposal, ExternalInputHypothesis, HypothesisConfidence, OptimizationPhase,
    PistonPlacementPreview, PistonPlanDisplay, PistonPlanState, PlacementPlanDisplay,
    RelatedComponent, RelatedConnection, RemovalCandidate, RepairCandidate, RepairCandidateEntry,
    RepairCandidates, RepairContextFacts, RepairContextReport, RepairHypothesis, ShownDoor,
    ShownPistonPlacement, ShownRepair,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

mod assembly_placement;
mod blueprints;
mod circuit_capture;
mod circuit_reports;
#[cfg(all(test, feature = "voxrig"))]
mod construction_batch_live_test;
mod construction_executor;
mod construction_job;
mod electrical_edit;
mod mcp_output;
mod optimization_workflow;
mod placement_workflow;
mod player_scope;
use mcp_output::{error_reply, json_reply, typed_reply};
mod repair_workflow;
mod session_reports;
mod transition_workflow;
use transition_workflow::{StoredTransitionPlan, TransitionWorkflow};
#[cfg(feature = "voxrig")]
mod survival;
mod world_editor;
#[cfg(test)]
use circuit_reports::reverse_result_json;
use circuit_reports::{
    Capture, Conversion, Discovery, GazeFlat, GazeHierarchy, MacroPlanReport, MacroProposals,
    NextTools, Selected,
};
use circuit_reports::{
    bounds_json, focused_component, focused_hierarchy, mixed_ir_report, raw_world_inspection,
    revision_display, revision_validation,
};
#[cfg(test)]
mod analysis_tests;
#[cfg(test)]
mod building_tests;
#[cfg(test)]
mod construction_job_tests;
#[cfg(test)]
mod electrical_edit_tests;
#[cfg(test)]
mod mcp_boundary_tests;
#[cfg(test)]
mod operation_diagnostics_tests;
#[cfg(test)]
mod optimization_contract_tests;
#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod piston_tests;
#[cfg(test)]
mod placement_tests;
#[cfg(test)]
mod repair_tests;
mod requests;
#[cfg(test)]
mod revision_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod transition_failure_tests;
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
use circuit_capture::{
    CircuitCapture, DiscoveryObservation, ExpansionEvidence, is_redstone_candidate_name,
};
mod operation_lifecycle;
use operation_lifecycle::{InvocationState, RepairLifecycle, RevisionLifecycle};
mod operation_plans;
use operation_plans::{OperationPlans, PlanKind};

use dustroute_app::DustRouteService;
use dustroute_optimize::{
    AnchorPolicy, BehavioralVerificationConfig, CompressionAxis, CompressionDirection,
    ContextualVerificationState, ObservedMacroMetrics, OptimizationContract, OptimizationPlan,
    OptimizationRoutingConfig, OptimizationSafety, PhysicalOptimizationSearchBudget,
    TemporalCapabilities, TimingContractMode, assess_macro_contract, assess_optimization_safety,
    extract_model_boundary_with_context, find_builtin_verified_macro_replacements,
    materialize_macro_replacement_in_known_regions, plan_macro_replacement_with_reserved,
    realize_staged_optimization_against, validate_macro_structure, verify_macro_steady_state,
    verify_macro_transitions, verify_realized_optimization,
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
    model::{
        CallToolResult, GetPromptResult, Implementation, PromptMessage, Role, ServerCapabilities,
        ServerInfo,
    },
    prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use crate::McpConfig;
use crate::api::{
    DIAGNOSTIC_SCHEMA_V1, ErrorResponse, McpErrorCode, PLACEMENT_SCHEMA_V1, TRANSITION_SCHEMA_V1,
};
use crate::failure::{
    CauseKind, ExecutionProgress, FailureCause, FailurePhase, FailureReport, PersistenceOutcome,
    WorldOutcome,
};
use crate::operations::analysis::{AnalysisResult, QueuedAnalysis};
use crate::recorded_analysis::{
    RecordedExpansion,
    reports::{ObservedMechanism, hierarchical_report, reverse_report},
};
use crate::state::{PlanRecordKind, PlanStateStore};
use crate::{
    BlockChange, BotBridge, McpPolicy, OperationKind, OperationRegistry, OperationStatus,
    PlacementPlan, SelectionSession, TransitionSafety, TransitionSafetyAssessment,
    assess_transition_safety, plan_world_overlay,
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
    #[cfg(feature = "voxrig")]
    survival: Arc<survival::Jobs>,
    #[cfg(feature = "voxrig")]
    survival_observer: Option<voxrig::Client>,
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
impl From<PistonPlacementState> for PistonPlanState {
    fn from(state: PistonPlacementState) -> Self {
        match state {
            PistonPlacementState::Planned => Self::Planned,
            PistonPlacementState::Applied => Self::Applied,
            PistonPlacementState::Undone => Self::Undone,
            PistonPlacementState::NeedsInspection => Self::NeedsInspection,
        }
    }
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
    snapshot: crate::snapshot_content::SharedSnapshot,
    expansion: ExpansionEvidence,
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

/// Convert an already decoded bridge snapshot directly into the simulator
/// world.  Bridge responses are typed before they reach the service, so
/// serializing them to JSON and parsing the same JSON again only adds copying
/// and allocation cost on every analysis path.
fn world_from_snapshot_for_service(
    snapshot: &dustroute_translate::snapshot::MinecraftSnapshot,
) -> Result<dustroute_translate::world::World, FailureCause> {
    world_from_snapshot(snapshot).map_err(FailureCause::from)
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

impl Default for DustRouteMcp {
    fn default() -> Self {
        Self::new()
    }
}

impl DustRouteMcp {
    #[must_use]
    pub fn new() -> Self {
        Self::with_policy(McpPolicy::default())
    }

    #[must_use]
    pub fn with_policy(policy: McpPolicy) -> Self {
        Self::with_policy_and_profile(policy, ToolProfile::from_environment())
    }

    #[must_use]
    pub fn with_policy_and_profile(policy: McpPolicy, profile: ToolProfile) -> Self {
        let mut tool_router = Self::tool_router();
        #[cfg(feature = "voxrig")]
        tool_router.merge(Self::survival_tool_router());
        if profile == ToolProfile::Default {
            for tool in DEBUG_ONLY_TOOLS {
                tool_router.disable_route(tool.to_owned());
            }
        }
        Self {
            bridge: BotBridge::disconnected(),
            #[cfg(feature = "voxrig")]
            survival: Arc::default(),
            #[cfg(feature = "voxrig")]
            survival_observer: None,
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
    pub fn with_policy_and_player(policy: McpPolicy, assist_player: impl Into<String>) -> Self {
        let mut service = Self::with_policy(policy);
        service.assist_player = Some(assist_player.into());
        service
    }

    #[must_use]
    pub fn with_config(config: McpConfig, policy: McpPolicy) -> Self {
        let state_scope = format!("{}\n{}", config.server_address, config.assist_player);
        let mut service = Self::with_policy_and_player(policy, config.assist_player);
        service.server_address = Some(config.server_address);
        service.state_store = PlanStateStore::from_environment(&state_scope);
        service
    }

    // Explicit workflow-fixture injection; no production TCP/JSON backend.
    #[cfg(test)]
    fn with_test_transport(address: impl Into<String>) -> Self {
        Self::with_test_transport_and_policy(address, McpPolicy::default())
    }
    #[cfg(test)]
    fn with_test_transport_and_policy(address: impl Into<String>, policy: McpPolicy) -> Self {
        Self::with_test_transport_and_profile(address, policy, ToolProfile::from_environment())
    }
    #[cfg(test)]
    fn with_test_transport_and_profile(
        address: impl Into<String>,
        policy: McpPolicy,
        profile: ToolProfile,
    ) -> Self {
        let mut service = Self::with_policy_and_profile(policy, profile);
        service.bridge = BotBridge::with_test_transport(address);
        service
    }
    #[cfg(test)]
    fn with_test_transport_and_player(
        address: impl Into<String>,
        policy: McpPolicy,
        player: impl Into<String>,
    ) -> Self {
        let mut service = Self::with_test_transport_and_policy(address, policy);
        service.assist_player = Some(player.into());
        service
    }

    /// Explicit native offline client. Connecting does not grant server confirmation.
    #[cfg(feature = "voxrig")]
    pub async fn connect_voxrig(
        config: McpConfig,
        policy: McpPolicy,
        username: &str,
    ) -> Result<Self, crate::bridge::BotBridgeError> {
        let (host, port) = config.server_address.rsplit_once(':').ok_or_else(|| {
            crate::bridge::BotBridgeError::Protocol("server endpoint unavailable".into())
        })?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let port = port
            .parse::<u16>()
            .map_err(|_| crate::bridge::BotBridgeError::Protocol("invalid server port".into()))?;
        let connection = voxrig::ConnectionConfig::offline(
            voxrig::Server::new(host, port),
            username,
            voxrig::MinecraftVersion::Java1_21_11,
        );
        let bridge = BotBridge::connect_voxrig(connection).await?;
        let mut service = Self::with_config(config, policy);
        service.bridge = bridge;
        if let Ok(observer) = std::env::var("DUSTROUTE_SURVIVAL_OBSERVER_USERNAME") {
            service = service.with_survival_observer(&observer).await?;
        }
        Ok(service)
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
    fn resolve_player(&self, requested: Option<&str>) -> Result<String, FailureCause> {
        self.player_scope().resolve(requested)
    }

    fn transition_workflow(&self) -> TransitionWorkflow<'_> {
        TransitionWorkflow {
            bridge: &self.bridge,
            policy: &self.policy,
            plans: &self.plans,
            operations: &self.operations,
            mutation_lock: &self.mutation_lock,
        }
    }

    fn authorize_player(&self, player: &str) -> Result<(), FailureCause> {
        self.player_scope()
            .authorize(player)
            .map_err(|cause| cause.at(FailurePhase::Admission))
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

    async fn mutate_placement(
        &self,
        params: ConfirmedOperationParams,
        undo: bool,
    ) -> CallToolResult {
        typed_reply(
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

    async fn mutate_repair(&self, params: ConfirmedOperationParams, undo: bool) -> CallToolResult {
        typed_reply(
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
    ) -> (
        TransitionSafetyAssessment,
        Vec<crate::operations::transition::TransitionProposal>,
    ) {
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
            let proposal = crate::operations::transition::TransitionProposal::new(
                operation_id,
                lever,
                original_powered,
                observation_ticks,
                max_events,
                safety.clone(),
            );
            self.operations
                .record_completed(
                    operation_id,
                    OperationKind::TransitionProposal,
                    proposal.clone().into(),
                )
                .await;
            proposals.push(proposal);
        }
        (safety, proposals)
    }

    async fn selected_region(
        &self,
        player: &str,
    ) -> Result<(dustroute_translate::world_reverse::RegionBounds, String), FailureCause> {
        let selections = self.selections.lock().await;
        let selected = selections.get(player).ok_or_else(|| {
            FailureCause::new(CauseKind::InvalidState, "no selection session for player")
        })?;
        let bounds = selected.session.bounds().map_err(FailureCause::from)?;
        let dimension = selected.dimension.clone().ok_or_else(|| {
            FailureCause::new(CauseKind::InvalidState, "selection has no dimension")
        })?;
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

    async fn capture_work_region(
        &self,
        player: &str,
        region: requests::RegionParam,
        params: &InspectLookedAtWorldParams,
    ) -> Result<circuit_reports::CapturedWorldInspection, FailureCause> {
        if params.component_gap.is_some()
            || params.max_components.is_some()
            || params.max_distance.is_some()
        {
            return Err(FailureCause::new(
                CauseKind::InvalidInput,
                "region cannot be combined with gaze/discovery options (component_gap, max_components, max_distance)",
            ));
        }
        let bounds = region
            .bounds()
            .map_err(|e| FailureCause::new(CauseKind::InvalidInput, e))?;
        self.policy
            .validate_region(bounds)
            .map_err(FailureCause::from)?;
        let limit = params.max_listed_blocks.unwrap_or(256);
        if !(1..=2048).contains(&limit) {
            return Err(FailureCause::input_range(
                "max_listed_blocks",
                limit as f64,
                1.0,
                2048.0,
            ));
        }
        let observation = self
            .bridge
            .observe_player_context(player)
            .await
            .map_err(FailureCause::from)?;
        let target: Option<Pos> = None;
        self.policy
            .authorize_dimension(&observation.dimension)
            .map_err(FailureCause::from)?;
        let record = self
            .bridge
            .scan_region_fresh(bounds.min, bounds.max, &observation.dimension)
            .await
            .map_err(FailureCause::from)?
            .into_stationary_record()?;
        let snapshot = record.snapshot;
        if snapshot.min != bounds.min || snapshot.max != bounds.max {
            return Err(FailureCause::new(
                CauseKind::ObservationIncomplete,
                "complete exact work region required",
            ));
        }
        dustroute_translate::snapshot::index_literal_snapshot(&snapshot)?;
        let inspection = raw_world_inspection(
            &snapshot,
            target,
            &observation.dimension,
            params.include_block_list.unwrap_or(false),
            limit,
        );
        let expansion = ExpansionEvidence::ExplicitWorkRegion {
            limit_reached: false,
            scope: "complete requested cuboid; circuit and movement may extend beyond it",
        };
        let content_id = snapshot.id();
        let circuit_id = self
            .store_circuit(StoredCircuit {
                player: player.into(),
                dimension: observation.dimension.clone(),
                bounds,
                target,
                snapshot,
                expansion: expansion.clone(),
                complete: true,
                expires_at: Instant::now() + CIRCUIT_SNAPSHOT_TTL,
            })
            .await;
        Ok(circuit_reports::CapturedWorldInspection {
            inspection,
            circuit_id,
            content_id,
            observation_id: record.observation_id,
            circuit_expires_in_seconds: CIRCUIT_SNAPSHOT_TTL.as_secs(),
            player_observation: observation,
            observation_capabilities: self.bridge.observation_capabilities(),
            readback: record.readback,
            expansion,
            mutation_authorized: false,
        })
    }

    async fn load_circuit(
        &self,
        circuit_id: &str,
        player: &str,
    ) -> Result<(uuid::Uuid, StoredCircuit), FailureCause> {
        let id = uuid::Uuid::parse_str(circuit_id).map_err(|error| {
            FailureCause::new(
                CauseKind::InvalidInput,
                format!("invalid circuit_id: {error}"),
            )
        })?;
        let now = Instant::now();
        let mut circuits = self.circuits.lock().await;
        circuits.retain(|_, stored| stored.expires_at > now);
        let circuit = circuits.get(&id).cloned().ok_or_else(|| {
            FailureCause::new(
                CauseKind::NotFound,
                "unknown or expired circuit_id; capture the circuit again",
            )
        })?;
        if circuit.player != player {
            return Err(FailureCause::new(
                CauseKind::PermissionDenied,
                "circuit_id belongs to a different assisted player",
            ));
        }
        Ok((id, circuit))
    }

    async fn discover_selection(
        &self,
        player: &str,
        max_components: Option<usize>,
        padding: i32,
        fragment_gap: u32,
    ) -> Result<DiscoveryObservation, FailureCause> {
        self.resolve_player(Some(player))?;
        self.policy
            .authorize_player(player)
            .map_err(FailureCause::from)?;
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
    ) -> Result<(uuid::Uuid, StoredCircuit), FailureCause> {
        let discovery = self
            .discover_selection(player, max_components, 1, fragment_gap)
            .await?;
        let target = discovery.candidate.seed;
        let (bounds, dimension) = self.selected_region(player).await?;
        let snapshot = self
            .bridge
            .scan_region_shared(bounds.min, bounds.max, &dimension)
            .await
            .map_err(FailureCause::from)?;
        let complete = !discovery.expansion.limit_reached();
        let circuit = StoredCircuit {
            player: player.to_owned(),
            dimension,
            bounds,
            target: Some(target),
            snapshot,
            expansion: discovery.expansion,
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
    ) -> Result<(uuid::Uuid, StoredCircuit), FailureCause> {
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
    async fn get_bot_status(&self) -> CallToolResult {
        match self.bridge.status().await {
            Ok(status) => json_reply(json!({
                "ok": true,
                "bot": status,
                "configured_server": self.server_address,
                "assist_player": self.assist_player,
                "policy": self.policy,
                "observation_capabilities": self.bridge.observation_capabilities()
            })),
            Err(error) => typed_reply(FailureCause::from(error).as_response()),
        }
    }

    #[tool(
        description = "List players visible to the Minecraft bot. If the configured assist player is outside tracking range, move only the bot to that player and retry.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_visible_player(&self) -> CallToolResult {
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
                        reacquire_error = Some(FailureCause::from(error));
                    } else {
                        match self.bridge.visible_players().await {
                            Ok(refreshed) => players = refreshed,
                            Err(error) => reacquire_error = Some(FailureCause::from(error)),
                        }
                    }
                }
                let players = players
                    .into_iter()
                    .filter(|player| {
                        self.policy.authorize_player(&player.player).is_ok()
                            && self.policy.authorize_dimension(&player.dimension).is_ok()
                    })
                    .collect::<Vec<_>>();
                typed_reply(session_reports::VisiblePlayers {
                    outcome: match reacquire_error.as_ref() {
                        None => session_reports::VisibilityOutcome::Visible { ok: Success },
                        Some(cause) => {
                            session_reports::VisibilityOutcome::Failed(cause.as_response())
                        }
                    },
                    players,
                    assist_player: &self.assist_player,
                    reacquire_error: reacquire_error
                        .as_ref()
                        .map(session_reports::DiagnosticText),
                })
            }
            Err(error) => typed_reply(FailureCause::from(error).as_response()),
        }
    }

    #[tool(
        description = "Observe a player's eye position, gaze direction, and targeted block",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_player_gaze(
        &self,
        Parameters(params): Parameters<ObserveParams>,
    ) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
        };
        if let Err(error) = self.authorize_player(&player) {
            return typed_reply(error.as_response());
        }
        match self
            .bridge
            .observe_player(&player, params.max_distance.unwrap_or(64.0))
            .await
        {
            Ok(observation) => match self.policy.authorize_dimension(&observation.dimension) {
                Ok(()) => json_reply(json!({ "ok": true, "observation": observation })),
                Err(error) => typed_reply(FailureCause::from(error).as_response()),
            },
            Err(error) => typed_reply(FailureCause::from(error).as_response()),
        }
    }

    #[tool(
        description = "Inspect raw Minecraft blocks near player gaze, following adjacent redstone components until their edge or max_components. Alternatively provide region={min,max} independently of gaze to capture a complete stationary work cuboid, including air for intended additions. Explicit region cannot be combined with component_gap or max_components. Returns an immutable circuit_id; observation does not authorize mutation.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_world(
        &self,
        Parameters(params): Parameters<InspectLookedAtWorldParams>,
    ) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(UnrecordedFailure::from(error)),
        };
        if let Err(error) = self.player_scope().authorize(&player) {
            return typed_reply(error.at(FailurePhase::Admission).as_response());
        }
        if let Some(region) = params.region {
            return match self.capture_work_region(&player, region, &params).await {
                Ok(response) => typed_reply(response),
                Err(error) => typed_reply(circuit_reports::InspectionFailure {
                    refusal: error.into(),
                    observation: None,
                    scan_complete: Some(false),
                }),
            };
        }
        let max_components = params.max_components.unwrap_or(8192);
        let component_gap = params.component_gap.unwrap_or(2);
        let max_distance = params.max_distance.unwrap_or(64.0);
        let max_listed_blocks = params.max_listed_blocks.unwrap_or(256);
        for (name, actual, min, max) in [
            ("max_components", max_components as f64, 1.0, 32768.0),
            ("component_gap", component_gap as f64, 1.0, 16.0),
            ("max_distance", max_distance, 1.0, 256.0),
            ("max_listed_blocks", max_listed_blocks as f64, 1.0, 2048.0),
        ] {
            if !(min..=max).contains(&actual) {
                return typed_reply(
                    FailureCause::input_range(name, actual, min, max).as_response(),
                );
            }
        }
        let observation = match self.bridge.observe_player(&player, max_distance).await {
            Ok(observation) => observation,
            Err(error) => {
                return typed_reply(FailureCause::from(error).as_response());
            }
        };
        let Some(target) = observation.targeted_block else {
            return typed_reply(circuit_reports::InspectionFailure {
                refusal: FailureCause::new(
                    CauseKind::NotFound,
                    "the player is not looking at a block",
                )
                .into(),
                observation: Some(observation),
                scan_complete: None,
            });
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return typed_reply(FailureCause::from(error).as_response());
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
                return typed_reply(circuit_reports::InspectionFailure {
                    refusal: error.into(),
                    observation: Some(observation),
                    scan_complete: Some(false),
                });
            }
        };
        let mut result = raw_world_inspection(
            &scan.snapshot,
            target,
            &observation.dimension,
            params.include_block_list.unwrap_or(false),
            max_listed_blocks,
        );
        result.scan.complete = !scan.limit_reached;
        result.scan.completeness_basis = if scan.limit_reached {
            "component limit reached before the adjacency frontier was exhausted"
        } else {
            "the adjacency frontier was exhausted without reaching the component limit"
        };
        result.boundary = circuit_reports::InspectionBoundary::ComponentFrontier {
            component_frontier_remaining: scan.limit_reached,
            redstone_touches_boundary: scan.limit_reached,
            guidance: scan
                .limit_reached
                .then_some("raise max_components or explicitly select a smaller functional area"),
        };
        typed_reply(circuit_reports::GazeWorldInspection {
            inspection: result,
            player_observation: observation,
            expansion: circuit_reports::GazeExpansion {
                strategy: "adjacent_component_flood_fill",
                component_gap,
                components_loaded: scan.component_count,
                component_limit: scan.component_limit,
                limit_reached: scan.limit_reached,
                complete: !scan.limit_reached,
                scanned_tiles: scan.scanned_tiles,
                scanned_block_positions: scan.scanned_block_positions,
                guidance: scan.limit_reached.then_some("the circuit is larger than the configured component limit; treat this inspection as incomplete"),
            },
        })
    }

    #[tool(
        description = "Mark the first or second region corner at the block a player is looking at"
    )]
    async fn set_region(&self, Parameters(params): Parameters<MarkCornerParams>) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
        };
        if let Err(error) = self.authorize_player(&player) {
            return typed_reply(error.as_response());
        }
        let observation = match self
            .bridge
            .observe_player(&player, params.max_distance.unwrap_or(64.0))
            .await
        {
            Ok(observation) => observation,
            Err(error) => {
                return typed_reply(FailureCause::from(error).as_response());
            }
        };
        let Some(target) = observation.targeted_block else {
            return json_reply(
                json!({ "ok": false, "error": "the player is not looking at a block" }),
            );
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return typed_reply(FailureCause::from(error).as_response());
        }
        let mut selections = self.selections.lock().await;
        let selected = selections
            .entry(player.clone())
            .or_insert_with(|| LocatedSelection::new(&player));
        use session_reports::{Corner, CornerResponse, MarkedCorner};
        let result = match params.corner.as_str() {
            "first" => {
                selected.session.mark_first(target);
                selected.dimension = Some(observation.dimension.clone());
                CornerResponse::Marked(MarkedCorner {
                    ok: Success,
                    corner: Corner::First,
                    position: target,
                    bounds: None,
                })
            }
            "second" => match selected
                .dimension
                .as_ref()
                .filter(|dimension| *dimension == &observation.dimension)
            {
                None => CornerResponse::Failed(UnrecordedFailure::message(
                    "player changed dimension between region corners",
                )),
                Some(_) => match selected.session.mark_second(target) {
                    Ok(bounds) => CornerResponse::Marked(MarkedCorner {
                        ok: Success,
                        corner: Corner::Second,
                        position: target,
                        bounds: Some(bounds),
                    }),
                    Err(error) => CornerResponse::Failed(FailureCause::from(error).into()),
                },
            },
            _ => {
                CornerResponse::Failed(UnrecordedFailure::message("corner must be first or second"))
            }
        };
        typed_reply(result)
    }

    #[tool(
        description = "Infer the bounds of 'this circuit' by progressively following adjacent redstone from the block a player is looking at, stopping at the circuit edge or max_components"
    )]
    async fn resolve_looked_at_circuit(
        &self,
        Parameters(params): Parameters<DiscoverCircuitParams>,
    ) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
        };
        if let Err(error) = self.authorize_player(&player) {
            return typed_reply(error.as_response());
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
            Ok(discovery) => typed_reply(discovery.response()),
            Err(error) => typed_reply(error.as_response()),
        }
    }

    #[tool(
        description = "Capture or reuse an immutable circuit snapshot and return a compact health summary with shared diagnostic findings and bounded focused_explanation (local role, directed edges, terminal candidates, paths, and timing caveats). Omit circuit_id to capture the current gaze; pass the returned circuit_id to keep later analysis on the same circuit",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn test_circuit(
        &self,
        Parameters(params): Parameters<DiagnoseLookedAtParams>,
    ) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
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
            Err(error) => return typed_reply(error.as_response()),
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
                return typed_reply(Conversion {
                    report: error.as_response(),
                    captured: Capture {
                        circuit_id,
                        mechanisms,
                    },
                    detail: Selected {},
                });
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
            dustroute_translate::analysis::explain_focused_component(&focused, target, complete)
        });
        typed_reply(circuit_reports::FocusedDiagnostic {
            ok: Success,
            schema_version: DIAGNOSTIC_SCHEMA_V1,
            analysis_mode: "focused_fast",
            captured: Capture {
                mechanisms,
                circuit_id,
            },
            content_id: snapshot.id(),
            circuit_expires_in_seconds: CIRCUIT_SNAPSHOT_TTL.as_secs(),
            mutation_performed: false,
            target,
            bounds,
            expansion: circuit.expansion.recorded(),
            diagnostic,
            focused_explanation,
            detail_tools: circuit_reports::DetailTools::default(),
        })
    }

    #[tool(
        description = "Get a bounded mixed-IR summary from an immutable circuit snapshot. Omit circuit_id to capture the current gaze, then reuse circuit_id with analysis_id and node_id for stable detail expansion.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_circuit_ir(
        &self,
        Parameters(params): Parameters<GetLookedAtCircuitIrParams>,
    ) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(UnrecordedFailure::from(error)),
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
            Err(error) => return typed_reply(UnrecordedFailure::from(error)),
        };
        let target = circuit.target;
        let bounds = circuit.bounds;
        let dimension = circuit.dimension;
        let snapshot = circuit.snapshot;
        let context = dustroute_translate::world::execution_context::WorldExecutionContext::for_profile(
            dustroute_translate::world::execution_context::WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V19,
        );
        let analysis_id = crate::snapshot_content::ValidationKey::new(
            &[snapshot.id()],
            &context,
            &crate::snapshot_content::StaticAnalysisConditions::new(
                &dimension,
                bounds,
                circuit.complete,
            ),
        );
        if params.node_id.is_some()
            && params.analysis_id.as_deref() != Some(analysis_id.to_string().as_str())
        {
            return typed_reply(circuit_reports::IrIdentityMismatch {
                refusal: UnrecordedFailure::message(if params.analysis_id.is_some() {
                    "the circuit, model, or analysis conditions no longer match the mixed-IR summary; request a new summary before expanding a node"
                } else {
                    "analysis_id from a mixed-IR summary is required when node_id is specified"
                }),
                current_analysis_id: analysis_id,
                analysis_id_schema: crate::snapshot_content::VALIDATION_KEY_SCHEMA,
                retryable: true,
            });
        }
        let world = match world_from_snapshot_for_service(&snapshot) {
            Ok(world) => world,
            Err(error) => return typed_reply(UnrecordedFailure::from(error)),
        };
        let mut analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
        analysis.scene.observation.dimension = dimension;
        let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
        let mixed_ir = match mixed_ir_report(&hierarchy, params.node_id) {
            Ok(mixed_ir) => mixed_ir,
            Err(error) => {
                return typed_reply(circuit_reports::IrNodeUnavailable {
                    refusal: UnrecordedFailure::message(error),
                    available_node_count: dustroute_ir::build_mixed_ir(&hierarchy).nodes.len(),
                });
            }
        };
        typed_reply(circuit_reports::CircuitIrResponse {
            ok: Success,
            analysis_mode: "mixed_ir",
            circuit_id,
            content_id: snapshot.id(),
            analysis_id,
            analysis_id_schema: crate::snapshot_content::VALIDATION_KEY_SCHEMA,
            mutation_performed: false,
            target,
            bounds,
            analysis_complete: circuit.complete,
            expansion: circuit.expansion,
            mixed_ir,
            guidance: if params.node_id.is_some() {
                "Use the physical block details and directed neighbors to explain or diagnose this node in context."
            } else {
                "Choose a node_id from this bounded graph and call this tool again to expand only that node."
            },
        })
    }

    #[tool(
        description = "Use blueprint.action=generate_grounded_building_design with design and ground_material to author a passive survival building on declared protected ground; the normal import/proposal/adoption flow applies. Use blueprint.action=generate_building_design with namespace, name, known_region, named parts (fill/shell/blocks shapes plus local cutouts), permanent air spaces and an optional pinned component Assembly to author explicit virtual building geometry. Identical materials are deduplicated; conflicting materials, occupied spaces and unsupported requests return structured errors with item/position. Review failures expose diagnostics with failed versus undetermined status, affected occurrence/type/coordinate, expected and actual state, available input/time evidence and verifier counterexamples. Components need unique adoption, source/target anchors, rotation and explicit source-frame reserved_space; optional exports alias exact terminals. Generation checks the combined world and shared construction without writes or adoption. Use blueprint.action=generate_building_design_update with base_assembly_revision_id, previous structured input and revised design under a new namespace to generate an immutable update against a uniquely adopted base. It checks all retained requirements, preserves unchanged child pins and returns the ordinary diff/proposal workflow; placed instances remain pinned. Use blueprint.action=generate_building with a namespace, width, depth, height, optional floor/wall/roof materials and entrance to author a bounded enclosure with exact structure and air-clearance contracts. It returns unadopted records and fresh shared-runtime construction checks; see result.next_step for import, proposal, adoption and placement. Use blueprint.action=generate_building_with_door with building and door requests to embed a uniquely adopted typed 3x3 door in the north wall; select its Assembly, instance, behavioral type, rotation and explicit source-frame motion space. The combined world is freshly checked and exports door_control and nine door_aperture ports. Or use blueprint.action=generate_flying_machine to author a bounded finite-flight candidate with typed engine, body, distance, rotation, reflection and attachments; generation returns unadopted records and fresh checks, never world writes. Or use blueprint.action to import unverified source/state records, capture_revision from a saved circuit revision, optimize the supplied Assembly or an explicit component body for fewer actual blocks preserving only an explicit behavioral type, enumerate_layouts for freshly checked torch/support placements, or propose_update with explicit new definitions and candidate state. Component scope separates body_positions from fixed external equipment and reports body/total counts. Device outputs can directly observe a torch. Blueprint optimize and enumerate_layouts return candidate data without publishing it; inspect moved ports and removed interpretations before proposing parent changes. Blueprint proposals continue through show_operation and invoke_operation. Otherwise create an immutable hypothetical circuit revision from exactly one circuit_id or revision_id. Apply up to 4096 full block-state edits (add, replace, or air to delete), save before/after diagnostics, bounded initial-state simulation and a separate modeled Assembly Revision when decodable. Blueprint source references stay pinned; observation alone does not infer them. Empty changes copies the source. Never changes Minecraft or authorizes placement.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn test_circuit_change(
        &self,
        Parameters(params): Parameters<TestCircuitChangeParams>,
    ) -> CallToolResult {
        if let Some(write) = params.blueprint {
            if !params.changes.is_empty()
                || params.simulation_ticks.is_some()
                || params.circuit_id.is_some()
                || params.revision_id.is_some()
            {
                return typed_reply(crate::blueprint_mcp::failure(
                    "blueprint operations cannot be combined with circuit block-edit parameters",
                ));
            }
            let command = if let crate::blueprint_mcp::BlueprintWrite::CaptureRevision {
                revision_id,
            } = write
            {
                let player = match self.resolve_player(params.player.as_deref()) {
                    Ok(p) => p,
                    Err(e) => return typed_reply(crate::blueprint_mcp::failure(e)),
                };
                if let Err(error) = self.player_scope().authorize(&player) {
                    return typed_reply(error.at(FailurePhase::Admission).as_response());
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
                    Err(e) => return typed_reply(crate::blueprint_mcp::failure(e)),
                };
                crate::blueprint_mcp::Command::Capture {
                    record: Box::new(record),
                    grounding,
                }
            } else {
                crate::blueprint_mcp::Command::Write(
                    crate::mcp_input::MeasuredBlueprintWrite::measure(write),
                )
            };
            return typed_reply(
                self.blueprint_command(command, params.player.as_deref(), true)
                    .await
                    .expect("explicit blueprint response"),
            );
        }
        let result: Result<circuit_reports::RevisionDisplay,UnrecordedFailure> = async {
            let player=self.resolve_player(params.player.as_deref()).map_err(|error| UnrecordedFailure::message(error.to_string()))?;
            self.player_scope().authorize(&player).map_err(|error| UnrecordedFailure::from(error.at(FailurePhase::Admission)))?;
            let ticks=params.simulation_ticks.unwrap_or(64);
            if !(1..=256).contains(&ticks) {return Err("simulation_ticks must be 1 through 256".into());}
            let (base_observation_id,parents,dimension,target,complete,baseline,base_snapshot,parent_assembly)=match (params.circuit_id.as_deref(),params.revision_id.as_deref()) {
                (Some(id),None)=>{
                    let (id,c)=self.load_circuit(id,&player).await.map_err(|error| UnrecordedFailure::message(error.to_string()))?;
                    (id,vec![],c.dimension,c.target,c.complete,c.snapshot.clone(),Some(crate::revision::normalize(&c.snapshot)?),None)
                },
                (None,Some(id))=>{
                    let r=self.load_revision(id,&player)?;
                    (r.base_observation_id,vec![r.revision_id],r.dimension,r.target,r.complete,self.bridge.share_snapshot(r.snapshot)?,r.base_snapshot,r.assembly)
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
            let modification=circuit_reports::electrical_modification_validation(&baseline,&snapshot,complete);
            let mut revision=crate::revision::CircuitRevision{
                schema_version:"dustroute.circuit-revision.v1".into(),revision_id:uuid::Uuid::new_v4(),parent_revision_ids:parents,base_observation_id,player,dimension,target,complete,snapshot,base_snapshot,changes,
                validation:crate::recorded_revision::RevisionValidation::StateReview(Box::new(crate::recorded_revision::StateRevisionReview {
                    before, after, simulation_ticks:ticks, scope:"initial_state_only; no functional equivalence or live-world guarantee".into(), electrical_modification:None, assembly:None })),
                assembly:None,
            };
            if let Some(modification)=modification {
                revision.validation.state_mut().ok_or("state review missing")?.electrical_modification=Some(modification);
            }
            match revision.capture_assembly(parent_assembly.as_ref()) {
                Ok(record)=>{
                    let checked=dustroute_translate::assembly::validate_assembly(dustroute_library::builtin_blueprints::builtin_blueprints(),&record.assembly);
                    revision.validation.state_mut().ok_or("state review missing")?.assembly=Some(match checked {
                        Ok(_)=>crate::recorded_revision::AssemblyValidation::PlacementAndDeclaredConnectionsValid { scope:"modeled state only; no behavioral or live-world proof".into() },
                        Err(error)=>crate::recorded_revision::AssemblyValidation::InvalidOrUnsupported { error:error.to_string() },
                    });
                    revision.assembly=Some(record);
                },
                Err(error)=>{
                    if parent_assembly.as_ref().is_some_and(|parent| !parent.assembly.instances.is_empty()) {
                        return Err(format!("cannot retain pinned blueprint interpretations for this state: {error}").into());
                    }
                    revision.validation.state_mut().ok_or("state review missing")?.assembly=Some(crate::recorded_revision::AssemblyValidation::Unavailable { error });
                },
            }
            dustroute_codec::storage::encode("dustroute.circuit-revision-record.v1", &revision, crate::revision::MAX_BYTES).map_err(|e|e.to_string())?;
            self.state_store.save(PlanRecordKind::CircuitRevisions,revision.revision_id,&revision)?;
            Ok(revision_display(revision,false))
        }.await;
        match result {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
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
    ) -> CallToolResult {
        if let Some(query) = params.blueprint {
            if !params.revision_id.is_empty() || params.include_snapshot.is_some() {
                return typed_reply(crate::blueprint_mcp::failure(
                    "blueprint queries cannot be combined with revision_id/include_snapshot",
                ));
            }
            return typed_reply(
                self.blueprint_command(crate::blueprint_mcp::Command::Read(query), None, true)
                    .await
                    .expect("explicit blueprint response"),
            );
        }
        let result: Result<circuit_reports::RevisionDisplay, UnrecordedFailure> = (|| {
            let player = self
                .resolve_player(None)
                .map_err(|error| UnrecordedFailure::message(error.to_string()))?;
            self.player_scope()
                .authorize(&player)
                .map_err(|error| UnrecordedFailure::from(error.at(FailurePhase::Admission)))?;
            let revision = self.load_revision(&params.revision_id, &player)?;
            Ok(revision_display(
                revision,
                params.include_snapshot.unwrap_or(false),
            ))
        })();
        match result {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    #[tool(
        description = "Convert a captured or selected physical circuit into bounded physical, mixed-IR, and higher-level identity summaries with focused_explanation for the gaze target. Reuse circuit_id to avoid following a moved gaze. Set include_truth_table=true for explicitly bounded exhaustive functional inference, including large circuits. Repair and transition planning are separate tools.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn convert_from_circuit(
        &self,
        Parameters(params): Parameters<AnalyzeLookedAtParams>,
    ) -> CallToolResult {
        match params.scope.as_deref() {
            None | Some("gaze") => {}
            Some("selected_region") => return self.convert_from_selected_region(params).await,
            Some(_) => {
                return error_reply(
                    McpErrorCode::InvalidArgument,
                    "scope must be gaze or selected_region",
                    false,
                );
            }
        }
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
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
            Err(error) => return typed_reply(error.as_response()),
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
                return typed_reply(Conversion {
                    report: error.as_response(),
                    captured: Capture {
                        circuit_id,
                        mechanisms,
                    },
                    detail: Selected {},
                });
            }
        };
        let discovered_components = circuit.expansion.components_loaded().unwrap_or(0);
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
            Err(error) => return error_reply(McpErrorCode::InvalidArgument, error, false),
        }
        .with_observation_complete(circuit.complete);
        if discovered_components > MAX_FLAT_ANALYSIS_COMPONENTS && !request.infer_truth_table {
            let mut analysis =
                dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
            analysis.scene.observation.dimension = dimension;
            let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
            let focused =
                target.map(|target| focused_hierarchy(&analysis.scene, &hierarchy, target));
            return typed_reply(Conversion {
                report: hierarchical_report(
                    bounds,
                    &hierarchy,
                    focused,
                    circuit.expansion.recorded(),
                    target,
                ),
                captured: Capture {
                    circuit_id,
                    mechanisms,
                },
                detail: GazeHierarchy {
                    circuit_identity: crate::recorded_analysis::circuit_identity(
                        &hierarchy,
                        None,
                        circuit.complete,
                        0,
                    ),
                    diagnostic: dustroute_translate::diagnostic::diagnose_scene(
                        &analysis.scene,
                        target,
                        circuit.complete,
                    ),
                    next_tools: NextTools::default(),
                },
            });
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
                        Some(MacroPlanReport {
                            plan,
                            structural,
                            materialized,
                            steady_state,
                            transitions,
                            contract,
                            contract_assessment,
                        })
                    })
                    .collect::<Vec<_>>()
            })
        });
        let focused = target.map(|target| focused_component(translated, target));
        let incomplete = !circuit.complete;
        typed_reply(Conversion {
            report: reverse_report(bounds, translated),
            captured: Capture {
                circuit_id,
                mechanisms,
            },
            detail: GazeFlat {
                circuit_identity: crate::recorded_analysis::circuit_identity(
                    &staged.hierarchy,
                    Some(&staged.logical_role),
                    !incomplete,
                    0,
                ),
                diagnostic: dustroute_translate::diagnostic::diagnose_scene(
                    &translated.analysis.scene,
                    target,
                    !incomplete,
                ),
                focused_component: focused,
                focused_explanation: target.map(|target| {
                    dustroute_translate::analysis::explain_focused_component(
                        &staged,
                        target,
                        !incomplete,
                    )
                }),
                discovery: Discovery {
                    seed: target,
                    bounds,
                },
                analysis_complete: !incomplete,
                macro_replacement_candidates: macro_candidates.map(|candidates| MacroProposals {
                    candidates,
                    placement_plans: macro_plans.unwrap_or_default(),
                }),
                next_tools: NextTools::default(),
                interpretation_guidance: if incomplete {
                    "Treat the logical classification as provisional because the connected circuit continues beyond the scan boundary. Explain the local role, then ask the user to select or isolate a larger functional region before applying a repair."
                } else {
                    "Explain the focused component in the context of the inferred logical function. Repairs are proposals only; preview one and obtain confirmation before mutation."
                },
            },
        })
    }

    #[tool(
        description = "Plan a built-in circuit, a grounded revision at its original coordinates, or an adopted electrical Assembly at a new assembly_target (source_anchor, target_anchor, rotation). work_regions creates a durable job for a literal revision, subdividing oversized regions and combining bounded support/watch cycles into at most 64 stages of 64 declared changes; intermediate outputs are derived from whole-context physics without changing inputs or the final target; manage_construction_job freshly plans each next region or reverse undo in the whole context, including after restart. edit_scope declares editable/protected regions for a captured revision or grounded Assembly at its original site. Scoped edits, including inert building blocks, use the common runtime and protect all other observed cells at every modeled microstep in application and undo. A grounded revision with supported stationary pistons uses the common physical runtime for differential construction and conditional undo; it does not imply verified flying/harvest behavior or adopt the revision. Custom construction freshly reviews the target and simulates every ordered command and groups immediately idle commands, with full-region readback before and after each model-reviewed batch. Returns a previewable operation without changing the world"
    )]
    async fn new_placement(
        &self,
        Parameters(params): Parameters<PreviewPlacementParams>,
    ) -> CallToolResult {
        if params.work_regions.is_some()
            && (params.revision_id.is_none()
                || params.assembly_revision_id.is_some()
                || params.assembly_target.is_some()
                || !params.circuit.is_empty()
                || params.optimize.unwrap_or(false))
        {
            return json_reply(
                json!({"ok":false,"error":"work_regions requires a literal revision_id at its captured site"}),
            );
        }
        if params.edit_scope.is_some()
            && (params.assembly_target.is_some()
                || (params.revision_id.is_none() && params.assembly_revision_id.is_none()))
        {
            return json_reply(
                json!({"ok":false,"error":"edit_scope requires a captured revision or grounded Assembly at its original site; fresh Assembly construction requires an empty target"}),
            );
        }
        if params.assembly_target.is_some() {
            return match self
                .assembly_service()
                .plan_assembly_construction(params)
                .await
            {
                Ok(response) => typed_reply(response),
                Err(error) => typed_reply(error.as_response()),
            };
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
        match self.plan_builtin_placement(params).await {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn plan_builtin_placement(
        &self,
        params: PreviewPlacementParams,
    ) -> Result<BuiltinPlacementPreview, BuiltinPlanningFailure> {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return Err(error.into()),
        };
        self.player_scope()
            .authorize(&player)
            .map_err(|error| error.at(FailurePhase::Admission))?;
        let observation = match self.bridge.observe_player(&player, 64.0).await {
            Ok(observation) => observation,
            Err(error) => {
                return Err(FailureCause::from(error).into());
            }
        };
        let Some(origin) = observation.targeted_block else {
            return Err("the player is not looking at a block".into());
        };
        if let Err(error) = self.policy.authorize_dimension(&observation.dimension) {
            return Err(FailureCause::from(error).into());
        }
        let translated = match self
            .app
            .compile_builtin(&params.circuit, ForwardOptions::default())
        {
            Ok(Some(result)) => result,
            Ok(None) => {
                return Err("unknown circuit; expected half-adder, half-subtractor, mux2, decoder1to2, full-adder, or piston-door-1x2".into());
            }
            Err(error) => return Err(error.to_string().into()),
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
                    return Err(format!("placement optimization failed: {error}").into());
                }
            };
            let verification = verify_realized_optimization(
                &translated.compiled.world,
                &translated.compiled.physical,
                &realized,
                BehavioralVerificationConfig::default(),
            );
            let safety = assess_optimization_safety(&verification, TemporalCapabilities::current());
            if matches!(&safety, OptimizationSafety::Rejected { .. }) {
                return Err(BuiltinPlanningFailure::OptimizationRejected {
                    safety,
                    topology_preserved: verification.topology_preserved,
                    behavior: Box::new(verification.behavior),
                });
            }
            let phases = realized
                .optimization
                .phases
                .iter()
                .map(|phase| OptimizationPhase {
                    accepted_mutations: phase.accepted.len(),
                    initial_score: phase.initial_score.total,
                    final_score: phase.final_score.total,
                })
                .collect::<Vec<_>>();
            let assembly = match realized.capture_assembly() {
                Ok(assembly) => assembly,
                Err(error) => {
                    return Err(format!("cannot capture optimized blueprint state: {error}").into());
                }
            };
            (
                realized.world,
                assembly,
                Some(BuiltinOptimization {
                    safety,
                    topology_preserved: verification.topology_preserved,
                    phases,
                }),
            )
        } else {
            let assembly = match translated.compiled.capture_assembly(&params.circuit) {
                Ok(assembly) => assembly,
                Err(error) => {
                    return Err(format!("cannot capture compiled blueprint state: {error}").into());
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
            return Err(format!("proposed assembly failed validation: {error}").into());
        }
        let Some((local_min, local_max)) = proposed_world.bounds() else {
            return Err("compiled circuit is empty".into());
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
            return Err(FailureCause::from(error).into());
        }
        let proposed_blocks = proposed_world.iter().count();
        if let Err(error) = self.policy.validate_placement_size(proposed_blocks) {
            return Err(FailureCause::from(error).into());
        }
        let snapshot = match self
            .bridge
            .scan_region(min, max, &observation.dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Err(FailureCause::from(error).into());
            }
        };
        let existing = match world_from_snapshot_for_service(&snapshot) {
            Ok(world) => world,
            Err(error) => return Err(error.into()),
        };
        let proposed_world =
            match dustroute_translate::world::ValidatedWorld::try_from(proposed_world) {
                Ok(world) => world,
                Err(error) => {
                    return Err(BuiltinPlanningFailure::InvalidWorld(error));
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
            Err(error) => {
                return Err(FailureCause::from(error).into());
            }
        };
        let operation_id = plan.operation_id;
        let assembly_id = crate::revision::assembly_id(uuid::Uuid::new_v4());
        let placement_assembly = dustroute_app::PlacementAssembly {
            coordinate_origin: origin,
            revision: dustroute_library::assembly::AssemblyRevision {
                id: assembly_id,
                parents: vec![],
                assembly,
            },
        };
        let response = BuiltinPlacementPreview::new(
            &plan,
            &placement_assembly,
            placement_bounds,
            optimization,
            self.policy.read_only,
        );
        plan.assembly = Some(placement_assembly);
        self.plans
            .placements()
            .lock()
            .await
            .insert(plan, observation.dimension, None);
        self.operations
            .record_completed(
                operation_id,
                OperationKind::PlacementPreview,
                response.clone().into(),
            )
            .await;
        Ok(response)
    }

    #[tool(
        description = "Retrieve the complete placement and exact undo plan by operation ID, including a separate proposed Assembly Revision and its coordinate origin when available. Blueprint references are pinned source definitions; proposed state is not live evidence.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_circuit_placement(
        &self,
        Parameters(params): Parameters<OperationParams>,
    ) -> CallToolResult {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_reply(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        if self.plans.kind(&operation_id).await == Some(PlanKind::ElectricalEdit) {
            return match self.electrical_edit_view(operation_id).await {
                Ok(response) => typed_reply(response),
                Err(error) => typed_reply(crate::operations::mutation::UnrecordedFailure::message(
                    error,
                )),
            };
        }
        if self.plans.kind(&operation_id).await == Some(PlanKind::Assembly) {
            return match self
                .assembly_service()
                .get_assembly_construction(operation_id)
                .await
            {
                Ok(response) => typed_reply(response),
                Err(error) => typed_reply(error.as_response()),
            };
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
                Err(error) => return typed_reply(UnrecordedFailure::message(error)),
            };
            if let Err(error) = self.player_scope().authorize(&player) {
                return typed_reply(error.at(FailurePhase::Admission).as_response());
            }
            if plan.player != player {
                return typed_reply(UnrecordedFailure::message(
                    "placement belongs to another player",
                ));
            }
            return typed_reply(PistonPlanDisplay::new(
                operation_id,
                &plan.proof,
                plan.previewed,
                plan.state.into(),
                self.policy.read_only,
            ));
        }
        match self.placement_view(operation_id).await {
            Ok(plan) => typed_reply(PlacementPlanDisplay {
                ok: Success,
                read_only: self.policy.read_only,
                plan,
            }),
            Err(error) => typed_reply(UnrecordedFailure::message(error)),
        }
    }

    #[tool(
        description = "Show the player's selected region in the Minecraft world before analysis or mutation"
    )]
    async fn show_region(&self, Parameters(params): Parameters<PlayerParams>) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
        };
        if let Err(error) = self.authorize_player(&player) {
            return typed_reply(error.as_response());
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return typed_reply(error.as_response()),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return typed_reply(FailureCause::from(error).as_response());
        }
        match self
            .bridge
            .preview_region(&player, bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(preview) => {
                let snapshot = match self
                    .bridge
                    .scan_region_shared(bounds.min, bounds.max, &dimension)
                    .await
                {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        return typed_reply(FailureCause::from(error).as_response());
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
                        expansion: ExpansionEvidence::ExplicitSelectedRegion {
                            components_loaded: None,
                            component_limit: None,
                            limit_reached: false,
                        },
                        complete: true,
                        expires_at: Instant::now() + CIRCUIT_SNAPSHOT_TTL,
                    })
                    .await;
                json_reply(json!({
                    "ok": true,
                    "circuit_id": circuit_id,
                    "circuit_expires_in_seconds": CIRCUIT_SNAPSHOT_TTL.as_secs(),
                    "bounds": bounds_json(bounds),
                    "preview": preview,
                    "source": "fresh_scan",
                    "mechanisms": mechanisms
                }))
            }
            Err(error) => typed_reply(FailureCause::from(error).as_response()),
        }
    }

    async fn convert_from_selected_region(&self, params: AnalyzeLookedAtParams) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
        };
        if let Err(error) = self.authorize_player(&player) {
            return typed_reply(error.as_response());
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return typed_reply(error.as_response()),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return typed_reply(FailureCause::from(error).as_response());
        }
        let snapshot = match self
            .bridge
            .scan_region_shared(bounds.min, bounds.max, &dimension)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return typed_reply(FailureCause::from(error).as_response());
            }
        };
        let circuit_id = self
            .store_circuit(StoredCircuit {
                player,
                dimension: dimension.clone(),
                bounds,
                target: None,
                snapshot: snapshot.clone(),
                expansion: ExpansionEvidence::ExplicitSelectedRegion {
                    components_loaded: None,
                    component_limit: None,
                    limit_reached: false,
                },
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
                return typed_reply(Conversion {
                    report: error.as_response(),
                    captured: Capture {
                        circuit_id,
                        mechanisms,
                    },
                    detail: Selected {},
                });
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
            Err(error) => return error_reply(McpErrorCode::InvalidArgument, error, false),
        }
        .with_observation_complete(true);
        if redstone_components > MAX_FLAT_ANALYSIS_COMPONENTS && !request.infer_truth_table {
            let mut analysis =
                dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
            analysis.scene.observation.dimension = dimension;
            let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
            let report = hierarchical_report(
                bounds,
                &hierarchy,
                None,
                ExpansionEvidence::ExplicitSelectedRegion {
                    components_loaded: Some(redstone_components),
                    component_limit: None,
                    limit_reached: false,
                }
                .recorded(),
                None,
            );
            return typed_reply(Conversion {
                report,
                captured: Capture {
                    circuit_id,
                    mechanisms,
                },
                detail: Selected {},
            });
        }
        let mut staged = self.app.analyze_physical(&world, request);
        staged.reverse.analysis.scene.observation.dimension = dimension;
        typed_reply(Conversion {
            report: reverse_report(bounds, &staged.reverse),
            captured: Capture {
                circuit_id,
                mechanisms,
            },
            detail: Selected {},
        })
    }

    #[tool(
        description = "Create ranked, non-mutating partial repair plans from the supplied immutable circuit_id, with shared diagnostic evidence and report-local finding references"
    )]
    async fn new_repair(
        &self,
        Parameters(params): Parameters<ProposeRepairsParams>,
    ) -> CallToolResult {
        let result: Result<RepairCandidates, UnrecordedFailure> = async {
            let player = match self.resolve_player(params.player.as_deref()) {
                Ok(player) => player,
                Err(error) => return Err(error.into()),
            };
            self.player_scope()
                .authorize(&player)
                .map_err(|error| error.at(FailurePhase::Admission))?;
            let max_gap = params.max_gap.unwrap_or(2);
            if !(1..=8).contains(&max_gap) {
                return Err("max_gap must be 1..8".into());
            }
            let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
                Ok(circuit) => circuit,
                Err(error) => return Err(error.into()),
            };
            let bounds = circuit.bounds;
            let dimension = circuit.dimension;
            let snapshot = circuit.snapshot;
            let world = match world_from_snapshot_for_service(&snapshot) {
                Ok(world) => world,
                Err(error) => return Err(error.into()),
            };
            let analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
            let fragments_before = analysis.scene.fragments.len();
            let mut diagnostic = dustroute_translate::diagnostic::diagnose_scene(
                &analysis.scene,
                circuit.target,
                circuit.complete,
            );
            let proposals = dustroute_translate::repair::propose_scene_repairs(
                &world,
                &analysis.scene,
                max_gap,
            );
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
                    return Err(error.into());
                }
                let diagnostic_finding_ids = diagnostic
                    .diagnosis
                    .findings_for_repair(&proposal, &analysis.scene);
                let candidate = RepairCandidate::new(proposal);
                self.operations
                    .record_completed(
                        operation_id,
                        OperationKind::RepairProposal,
                        candidate.clone().into(),
                    )
                    .await;
                response.push(RepairCandidateEntry {
                    operation_id,
                    diagnostic_finding_ids,
                    candidate,
                });
            }
            Ok(RepairCandidates::new(
                circuit_id,
                bounds,
                fragments_before,
                diagnostic,
                response,
            ))
        }
        .await;
        match result {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    #[tool(
        description = "Explain repair evidence for an immutable circuit as bounded facts, competing hypotheses, counterfactual impact, related physical components, and questions for the player",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_repair_context(
        &self,
        Parameters(params): Parameters<GetRepairContextParams>,
    ) -> CallToolResult {
        match self.repair_context_report(params).await {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn repair_context_report(
        &self,
        params: GetRepairContextParams,
    ) -> Result<RepairContextReport, UnrecordedFailure> {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return Err(error.into()),
        };
        self.player_scope()
            .authorize(&player)
            .map_err(|error| error.at(FailurePhase::Admission))?;
        let max_gap = params.max_gap.unwrap_or(2);
        if !(1..=8).contains(&max_gap) {
            return Err(UnrecordedFailure::coded(
                McpErrorCode::InvalidArgument,
                "max_gap must be 1..8".to_string(),
                false,
            ));
        }
        let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
            Ok(circuit) => circuit,
            Err(error) => {
                return Err(UnrecordedFailure::coded(
                    McpErrorCode::NotFound,
                    error.to_string(),
                    false,
                ));
            }
        };
        let world = match world_from_snapshot_for_service(&circuit.snapshot) {
            Ok(world) => world,
            Err(error) => {
                return Err(UnrecordedFailure::coded(
                    McpErrorCode::SerializationFailed,
                    error.to_string(),
                    false,
                ));
            }
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
                    return Err(UnrecordedFailure::coded(
                        McpErrorCode::InvalidArgument,
                        error.to_string(),
                        false,
                    ));
                }
            },
            None => None,
        };
        let selected_patch = if let Some(operation_id) = operation_id {
            let plan = match self.repair_plan(operation_id).await {
                Ok(Some(plan)) => plan,
                Ok(None) => {
                    return Err(UnrecordedFailure::coded(
                        McpErrorCode::NotFound,
                        "repair operation not found".to_string(),
                        false,
                    ));
                }
                Err(error) => {
                    return Err(UnrecordedFailure::coded(
                        McpErrorCode::Internal,
                        error.to_string(),
                        false,
                    ));
                }
            };
            if plan.dimension != circuit.dimension || plan.analysis_bounds != circuit.bounds {
                return Err(UnrecordedFailure::coded(
                    McpErrorCode::InvalidArgument,
                    "operation_id does not belong to this circuit snapshot".to_string(),
                    false,
                ));
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
                    .map(|connection| RelatedConnection {
                        component: connection.source.component,
                        transfer: connection.transfer,
                        confidence: connection.confidence,
                    })
                    .collect::<Vec<_>>();
                let outgoing = analysis
                    .scene
                    .connections
                    .iter()
                    .filter(|connection| connection.source.component == component.id)
                    .map(|connection| RelatedConnection {
                        component: connection.sink.component,
                        transfer: connection.transfer,
                        confidence: connection.confidence,
                    })
                    .collect::<Vec<_>>();
                RelatedComponent {
                    id: component.id,
                    position: component.pos,
                    block: component.block.kind,
                    facing: component.block.facing,
                    support: component.support,
                    incoming,
                    outgoing,
                }
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
            hypotheses.push(RepairHypothesis::Repair {
                kind: proposal.patch.reason,
                confidence_percent: proposal.patch.confidence_percent,
                supporting_evidence,
                contradictions,
                diagnostic_finding_ids: diagnostic
                    .diagnosis
                    .findings_for_repair(proposal, &analysis.scene),
                physical_evidence: proposal.evidence.clone(),
                counterfactual_impact: proposal.impact,
                operation_id,
            });
        }
        if diagnostic.counts.awaiting_external_input > 0 {
            hypotheses.push(RepairHypothesis::ExternalInputs {
                kind: ExternalInputHypothesis::IntentionalExternalInputs, confidence: HypothesisConfidence::Plausible,
                supporting_evidence: vec!["the disconnected paths satisfy the structural rules for inferred input boundaries"],
                contradictions: selected.map_or_else(Vec::new, |proposal| vec![format!(
                    "the highest-ranked virtual repair improves connectivity with {} block change(s)",
                    proposal.patch.changes.len()
                )]),
            });
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
        let facts = RepairContextFacts {
            observation_complete: diagnostic.observation_complete,
            components: analysis.scene.components.len(),
            fragments: analysis.scene.fragments.len(),
            gap_candidates: gaps.iter().take(16).cloned().collect(),
            gap_candidates_truncated: gaps.len() > 16,
            diagnostic,
            temporal: analysis.scene.temporal_assessment(),
        };
        Ok(RepairContextReport::new(
            circuit_id,
            operation_id,
            gaps.len(),
            facts,
            hypotheses,
            related_components,
            questions,
        ))
    }

    #[tool(
        description = "Create a non-mutating, reversible macro replacement operation from a candidate component_id returned by convert_from_circuit",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn new_macro_optimization(
        &self,
        Parameters(params): Parameters<NewMacroOptimizationParams>,
    ) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return error_reply(McpErrorCode::InvalidArgument, error, false),
        };
        if let Err(error) = self.player_scope().authorize(&player) {
            return typed_reply(error.at(FailurePhase::Admission).as_response());
        }
        let contract = match optimization_contract_from_param(params.contract) {
            Ok(contract) => contract,
            Err(error) => return error_reply(McpErrorCode::InvalidArgument, error, false),
        };
        let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
            Ok(circuit) => circuit,
            Err(error) => return error_reply(McpErrorCode::NotFound, error, false),
        };
        match self
            .optimization_workflow()
            .propose_macro(circuit_id, circuit, params.component_id, contract)
            .await
        {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    #[tool(
        description = "Create a non-mutating, reversible optimization plan for a simple physical dust path inside an explicit focus while fixing everything outside",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn new_optimization(
        &self,
        Parameters(params): Parameters<NewOptimizationParams>,
    ) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(UnrecordedFailure::message(error)),
        };
        if let Err(error) = self.player_scope().authorize(&player) {
            return typed_reply(error.at(FailurePhase::Admission).as_response());
        }
        let objective = match params.objective.as_str() {
            "wire_length" => WireObjective::WireLength,
            "density_then_wire_length" => WireObjective::DensityThenWireLength,
            _ => {
                return error_reply(
                    McpErrorCode::InvalidArgument,
                    "objective must be wire_length or density_then_wire_length",
                    false,
                );
            }
        };
        let contract = match optimization_contract_from_param(params.contract) {
            Ok(contract) => contract,
            Err(error) => return error_reply(McpErrorCode::InvalidArgument, error, false),
        };
        let search_budget = match optimization_search_budget(params.search) {
            Ok(budget) => budget,
            Err(error) => return error_reply(McpErrorCode::InvalidArgument, error, false),
        };
        let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
            Ok(circuit) => circuit,
            Err(error) => return error_reply(McpErrorCode::NotFound, error, false),
        };
        let focus = dustroute_translate::world_reverse::RegionBounds::new(
            Pos::new(params.focus.min.x, params.focus.min.y, params.focus.min.z),
            Pos::new(params.focus.max.x, params.focus.max.y, params.focus.max.z),
        );
        match self
            .optimization_workflow()
            .propose_wire(
                circuit_id,
                circuit,
                focus,
                objective,
                contract,
                search_budget,
            )
            .await
        {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    #[tool(
        description = "Create a low-confidence removal repair for the redstone component the player is looking at. Use only when the player explicitly identifies it as an unwanted connection."
    )]
    async fn new_component_removal_plan(
        &self,
        Parameters(params): Parameters<PlayerParams>,
    ) -> CallToolResult {
        let result: Result<RemovalCandidate, UnrecordedFailure> = async {
            let player = match self.resolve_player(params.player.as_deref()) {
                Ok(player) => player,
                Err(error) => return Err(error.into()),
            };
            let observation = match self.bridge.observe_player(&player, 64.0).await {
                Ok(observation) => observation,
                Err(error) => {
                    return Err(FailureCause::from(error).into());
                }
            };
            let Some(target) = observation.targeted_block else {
                return Err("player is not looking at a block".into());
            };
            let (bounds, dimension) = match self.selected_region(&player).await {
                Ok(region) => region,
                Err(error) => return Err(error.into()),
            };
            if !bounds.contains(target) {
                return Err("target is outside the selected region".into());
            }
            let snapshot = match self
                .bridge
                .scan_region(bounds.min, bounds.max, &dimension)
                .await
            {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    return Err(FailureCause::from(error).into());
                }
            };
            let world = match world_from_snapshot_for_service(&snapshot) {
                Ok(world) => world,
                Err(error) => return Err(error.into()),
            };
            let analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
            let Some(proposal) = dustroute_translate::repair::propose_scene_component_removal(
                &world,
                &analysis.scene,
                target,
            ) else {
                return Err("target is not a removable redstone component".into());
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
                return Err(error.into());
            }
            Ok(RemovalCandidate::new(operation_id, proposal))
        }
        .await;
        match result {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn show_repair_plan(
        &self,
        Parameters(params): Parameters<PreviewRepairParams>,
    ) -> CallToolResult {
        let result: Result<ShownRepair, UnrecordedFailure> = async {
            let player = match self.resolve_player(params.player.as_deref()) {
                Ok(player) => player,
                Err(error) => return Err(error.into()),
            };
            let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
                Ok(id) => id,
                Err(error) => {
                    return Err(UnrecordedFailure::coded(
                        McpErrorCode::InvalidArgument,
                        error.to_string(),
                        false,
                    ));
                }
            };
            // Serialize preview persistence with attempts, so a delayed preview
            // cannot overwrite a saved NeedsInspection state.
            let _mutation_guard = self.mutation_lock.lock().await;
            let plan = match self.repair_plan(operation_id).await {
                Ok(Some(plan)) => plan,
                Ok(None) => {
                    return Err("unknown or expired repair ID".into());
                }
                Err(error) => return Err(error.into()),
            };
            let Some(bounds) = bounds_for_changes(&plan.patch.changes) else {
                return Err("repair has no changes".into());
            };
            match self
                .bridge
                .preview_region(&player, bounds.min, bounds.max, &plan.dimension)
                .await
            {
                Ok(preview) => {
                    let mut previewed = plan.clone();
                    if let Err(error) = previewed.lifecycle.preview() {
                        return Err(UnrecordedFailure::coded(
                            McpErrorCode::InvalidState,
                            error.to_string(),
                            false,
                        ));
                    }
                    if let Err(error) = self.store_repair_plan(operation_id, previewed).await {
                        return Err(error.into());
                    }
                    Ok(ShownRepair::new(operation_id, bounds, plan.patch, preview))
                }
                Err(error) => Err(FailureCause::from(error).into()),
            }
        }
        .await;
        match result {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn plan_revision_placement(&self, params: PreviewPlacementParams) -> CallToolResult {
        use crate::operations::mutation::UnrecordedFailure;
        let result: Result<crate::operations::preview::GroundedPlacementPreview, UnrecordedFailure> = async {
            if !params.circuit.is_empty()
                || params.optimize.unwrap_or(false)
                || params.assembly_revision_id.is_some()
            {
                return Err("revision_id cannot be combined with a built-in circuit, Assembly Revision or optimization".into());
            }
            let player=self.resolve_player(params.player.as_deref())?;
            self.player_scope().authorize(&player).map_err(|error| error.at(FailurePhase::Admission))?;
            let revision=self.load_revision(params.revision_id.as_deref().ok_or("revision_id required")?,&player)?;
            self.plan_grounded_revision_placement(
                &params,
                player,
                revision.clone(),
                crate::placement_source::PlacementSource::CircuitRevision { revision_id:revision.revision_id },
            ).await.map_err(UnrecordedFailure::message)
        }.await;
        match result {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn plan_adopted_assembly_placement(
        &self,
        params: PreviewPlacementParams,
    ) -> CallToolResult {
        use crate::operations::mutation::UnrecordedFailure;
        let result: Result<crate::operations::preview::GroundedPlacementPreview, UnrecordedFailure> = async {
            if !params.circuit.is_empty()
                || params.revision_id.is_some()
                || params.optimize.unwrap_or(false)
            {
                return Err("assembly_revision_id cannot be combined with a built-in circuit, circuit revision or optimization".into());
            }
            let player = self.resolve_player(params.player.as_deref())?;
            self.player_scope().authorize(&player).map_err(|error| error.at(FailurePhase::Admission))?;
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
                validation: crate::recorded_revision::RevisionValidation::AdoptedReview { source:crate::recorded_revision::AdoptedReviewSource::FreshAdoptedAssemblyReview },
                assembly: Some(record.clone()),
            };
            self.plan_grounded_revision_placement(
                &params,
                player,
                revision,
                crate::placement_source::PlacementSource::AdoptedAssemblyRevision {
                    assembly_revision_id:record.id,
                    adopted_by:basis.adopted_by,
                    grounding_assembly_revision_id:basis.grounding_assembly_revision_id,
                    fresh_review:Box::new((&basis.fresh_review).into()),
                    literal_observation:crate::placement_source::LiteralObservation::GroundingBaseSnapshot,
                    candidate_interpretation:crate::placement_source::CandidateInterpretation::RecordAssembly,
                },
            )
            .await.map_err(UnrecordedFailure::message)
        }
        .await;
        match result {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn plan_grounded_revision_placement(
        &self,
        params: &PreviewPlacementParams,
        player: String,
        revision: crate::revision::CircuitRevision,
        source: crate::placement_source::PlacementSource,
    ) -> Result<crate::operations::preview::GroundedPlacementPreview, String> {
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
        if params.work_regions.is_some() {
            return self
                .create_construction_job(params, &revision, before, after, status, source)
                .await
                .map(|response| {
                    crate::operations::preview::GroundedPlacementPreview::Job(Box::new(response))
                });
        }
        let old = world_from_snapshot(&before).map_err(|e| e.to_string())?;
        let new = world_from_snapshot(&after).map_err(|e| e.to_string())?;
        if params.edit_scope.is_some()
            || self.bridge.observation_capabilities().backend
                == crate::bridge::ObservationBackend::Voxrig
            || old.iter().chain(new.iter()).any(|(_, block)| {
                dustroute_translate::world::physical::requires_callback_runtime(block)
            })
        {
            return self
                .plan_electrical_edit(
                    params,
                    before,
                    after,
                    status,
                    electrical_edit::EditOrigin {
                        revision_id: revision.revision_id,
                        source,
                        job: None,
                    },
                )
                .await
                .map(|response| {
                    crate::operations::preview::GroundedPlacementPreview::Electrical(Box::new(
                        response,
                    ))
                });
        }
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
        let response = crate::operations::preview::GroundedRevisionPreview::new(
            source,
            revision.revision_id,
            revision.base_observation_id,
            bounds,
            plan.clone(),
            self.policy.read_only,
        );
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
            .record_completed(id, OperationKind::PlacementPreview, response.clone().into())
            .await;
        Ok(crate::operations::preview::GroundedPlacementPreview::Ordinary(Box::new(response)))
    }

    async fn plan_piston_placement(&self, params: PreviewPlacementParams) -> CallToolResult {
        match self.piston_placement_proposal(params).await {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn piston_placement_proposal(
        &self,
        params: PreviewPlacementParams,
    ) -> Result<PistonPlacementPreview, UnrecordedFailure> {
        if params.optimize.unwrap_or(false) {
            return Err("the pinned piston-door layout cannot be optimized".into());
        }
        let player = self.resolve_player(params.player.as_deref())?;
        self.player_scope()
            .authorize(&player)
            .map_err(|error| error.at(FailurePhase::Admission))?;
        let observation = self
            .bridge
            .observe_player(&player, 64.0)
            .await
            .map_err(|error| error.to_string())?;
        let anchor = observation
            .targeted_block
            .ok_or("look at the ground below the placement")?;
        let origin = Pos::new(
            anchor.x,
            anchor.y.checked_add(3).ok_or("coordinate overflow")?,
            anchor.z,
        );
        if [origin.x, origin.y, origin.z]
            .iter()
            .any(|v| *v < i32::MIN + 16 || *v > i32::MAX - 16)
        {
            return Err("coordinate overflow".into());
        }
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(
            origin.offset(-3, -2, -4),
            origin.offset(7, 3, 6),
        );
        self.policy
            .authorize_dimension(&observation.dimension)
            .map_err(|error| error.to_string())?;
        self.policy
            .validate_region(bounds)
            .map_err(|error| error.to_string())?;
        let status = self
            .bridge
            .status()
            .await
            .map_err(|error| error.to_string())?;
        if !status.connected || status.dimension.as_deref() != Some(observation.dimension.as_str())
        {
            return Err("bot disconnected or dimension changed".into());
        }
        let baseline = self
            .bridge
            .scan_region(bounds.min, bounds.max, &observation.dimension)
            .await
            .map_err(|error| error.to_string())?;
        let proof =
            crate::piston_door::ValidatedDoorPlacement::new(origin, &baseline, &status.version)?;
        let count = proof.initial().blocks.len();
        self.policy
            .validate_placement_size(count)
            .map_err(|error| error.to_string())?;
        if params.max_blocks.is_some_and(|limit| count > limit) {
            return Err("placement exceeds max_blocks".into());
        }
        let mut materials = BTreeMap::<String, usize>::new();
        for block in &proof.initial().blocks {
            *materials.entry(block.name.clone()).or_default() += 1;
        }
        let id = uuid::Uuid::new_v4();
        let response =
            PistonPlacementPreview::new(id, anchor, &proof, materials, self.policy.read_only);
        let mut plans = self.plans.table::<StoredPistonPlacement>().lock().await;
        plans.retain(|_, plan| {
            plan.state != PistonPlacementState::Planned || plan.expires_at > Instant::now()
        });
        if plans.len() >= 256 {
            return Err("too many retained piston placements".into());
        }
        plans.insert(
            id,
            StoredPistonPlacement {
                player,
                dimension: observation.dimension,
                proof,
                previewed: false,
                state: PistonPlacementState::Planned,
                expires_at: Instant::now() + Duration::from_secs(300),
            },
        );
        drop(plans);
        self.operations
            .record_completed(id, OperationKind::PlacementPreview, response.clone().into())
            .await;
        Ok(response)
    }

    async fn show_piston_placement(&self, id: uuid::Uuid, player: Option<&str>) -> CallToolResult {
        let result: Result<ShownPistonPlacement, UnrecordedFailure> = async {
            let player = self.resolve_player(player)?;
            self.player_scope()
                .authorize(&player)
                .map_err(|error| error.at(FailurePhase::Admission))?;
            let plan = self
                .plans
                .table::<StoredPistonPlacement>()
                .lock()
                .await
                .get(&id)
                .cloned()
                .ok_or("placement not found")?;
            if plan.player != player
                || plan.state != PistonPlacementState::Planned
                || plan.expires_at <= Instant::now()
            {
                return Err("placement is expired, used, or owned by another player".into());
            }
            let bounds = plan.proof.bounds();
            self.policy
                .authorize_dimension(&plan.dimension)
                .map_err(|error| error.to_string())?;
            self.policy
                .validate_region(bounds)
                .map_err(|error| error.to_string())?;
            let preview = self
                .bridge
                .preview_region(&player, bounds.min, bounds.max, &plan.dimension)
                .await
                .map_err(|error| error.to_string())?;
            self.plans
                .table::<StoredPistonPlacement>()
                .lock()
                .await
                .get_mut(&id)
                .ok_or("placement not found")?
                .previewed = true;
            Ok(ShownPistonPlacement::new(id, &plan.proof, preview))
        }
        .await;
        match result {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn mutate_piston_placement(
        &self,
        id: uuid::Uuid,
        confirm: bool,
        undo: bool,
    ) -> CallToolResult {
        use crate::operations::piston::{ExecutionOutcome, PistonExecution, PistonPlacementResult};
        let mut progress = ExecutionProgress::default();
        let result: Result<PistonPlacementResult,FailureCause> = async {
            if !confirm { return Err("confirm=true is required".into()); }
            self.policy.authorize_mutation().map_err(FailureCause::from)?;
            let player = self.resolve_player(None)?;
            self.player_scope().authorize(&player)?;
            let _guard = self.mutation_lock.lock().await;
            let plan = self.plans.table::<StoredPistonPlacement>().lock().await.get(&id).cloned().ok_or("placement not found")?;
            if plan.player != player { return Err("placement belongs to another player".into()); }
            if (undo && plan.state != PistonPlacementState::Applied) || (!undo && (plan.state != PistonPlacementState::Planned || !plan.previewed || plan.expires_at <= Instant::now())) { return Err("placement requires an unused, unexpired preview; undo requires verified application".into()); }
            let bounds = plan.proof.bounds();
            self.policy.authorize_dimension(&plan.dimension).map_err(FailureCause::from)?;
            self.policy.validate_region(bounds).map_err(FailureCause::from)?;
            self.policy.validate_placement_size(plan.proof.initial().blocks.len()).map_err(FailureCause::from)?;
            let status = self.bridge.status().await.map_err(FailureCause::from)?;
            if !status.connected || status.dimension.as_deref()!=Some(plan.dimension.as_str()) { return Err("bot disconnected or dimension changed".into()); }
            progress.phase=FailurePhase::BeforeReadback;
            let baseline = self.bridge.scan_region(bounds.min,bounds.max,&plan.dimension).await.map_err(FailureCause::from)?;
            if undo { plan.proof.validate_built(&baseline,&status.version)?; }
            else { plan.proof.validate_empty(&baseline,&status.version)?; }
            {
                let mut plans = self.plans.table::<StoredPistonPlacement>().lock().await;
                let stored = plans.get_mut(&id).ok_or("placement not found")?;
                if !undo && stored.expires_at <= Instant::now() { return Err("placement expired during validation".into()); }
                // Consume before writes, including undo: uncertain transport is never retried automatically.
                stored.state = PistonPlacementState::NeedsInspection;
            }
            progress.operation_consumed=true;
            progress.total_changes=Some(plan.proof.writes(undo).len());
            progress.begin_submission();
            let write = self.bridge.write_blocks(&plan.proof.writes(undo),&plan.dimension).await;
            let wait = self.bridge.wait_ticks(30,&plan.dimension).await;
            let observed = self.bridge.scan_region(bounds.min,bounds.max,&plan.dimension).await;
            let verification = observed.as_ref().map_err(|e|e.to_string()).and_then(|snapshot| if undo { plan.proof.validate_empty(snapshot,&status.version) } else { plan.proof.validate_built(snapshot,&status.version) });
            let ok = write.is_ok() && wait.is_ok() && verification.is_ok();
            if ok { self.plans.table::<StoredPistonPlacement>().lock().await.get_mut(&id).ok_or("placement not found")?.state = if undo { PistonPlacementState::Undone } else { PistonPlacementState::Applied }; }
            let verified = verification.is_ok();
            let write_error = write.as_ref().err().map(ToString::to_string);
            let wait_error = wait.as_ref().err().map(ToString::to_string);
            let verification_error = verification.as_ref().err().cloned();
            let mut failure=None;
            match write {
                Ok(receipt)=>progress.submitted(receipt.submitted_changes),
                Err(error)=>{let report=progress.submission_error(error,WorldOutcome::NotAttempted);failure=Some(report);}
            }
            if let Err(error)=wait { FailureReport::append(&progress,&mut failure,error.cause().at(FailurePhase::Wait)); }
            match observed {
                Err(error)=>FailureReport::append(&progress,&mut failure,error.cause().at(FailurePhase::AfterReadback)),
                Ok(_)=>if let Err(error)=verification {FailureReport::append(&progress,&mut failure,FailureCause::new(CauseKind::VerificationMismatch,error).at(FailurePhase::Verification));},
            }
            progress.phase=if verification_error.is_none(){FailurePhase::Verification}else{FailurePhase::AfterReadback};
            if verified {progress.world=WorldOutcome::Verified;progress.verified_steps=progress.total_changes.unwrap_or(0);}
            let response = PistonPlacementResult::executed(id, PistonExecution {
                verified, undo, write_error, wait_error, verification_error, automatic_rollback: false,
                bounds: bounds.into(), outcome: ExecutionOutcome::from_attempt(progress.clone(), failure),
            });
            self.operations.record_completed(id,if undo {OperationKind::PlacementUndo} else {OperationKind::PlacementApply},response.clone().into()).await;
            if !ok {self.operations.fail(id,"piston placement requires inspection").await;}
            Ok(response)
        }.await;
        crate::performance::execution_progress(&progress);
        let response = result.unwrap_or_else(|error| {
            PistonPlacementResult::failed_attempt(id, progress.cause(error))
        });
        typed_reply(response)
    }

    async fn observed_mechanisms(
        &self,
        snapshot: &dustroute_translate::snapshot::MinecraftSnapshot,
        complete: bool,
        dimension: &str,
    ) -> Vec<ObservedMechanism> {
        if !snapshot.blocks.iter().any(|b| {
            matches!(
                b.name.as_str(),
                "minecraft:piston"
                    | "minecraft:sticky_piston"
                    | "minecraft:piston_head"
                    | "minecraft:moving_piston"
            )
        }) {
            return Vec::new();
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
        vec![ObservedMechanism::piston(observation)]
    }

    #[tool(
        description = "Plan open/closed for an already built, exact Java 1.21.11 door v1 (1x2). Requires a complete selected circuit including its empty guard. Translation only; no building or rotation. Use show_operation then invoke_operation(confirm=true).",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn new_piston_door_operation(
        &self,
        Parameters(params): Parameters<NewPistonDoorParams>,
    ) -> CallToolResult {
        match self.piston_door_proposal(params).await {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn piston_door_proposal(
        &self,
        params: NewPistonDoorParams,
    ) -> Result<DoorProposal, DoorPlanningFailure> {
        let player = self.resolve_player(None)?;
        self.player_scope()
            .authorize(&player)
            .map_err(|error| error.at(FailurePhase::Admission))?;
        let (_, circuit) = self.load_circuit(&params.circuit_id, &player).await?;
        let status = self
            .bridge
            .status()
            .await
            .map_err(|error| error.to_string())?;
        if !status.connected || status.dimension.as_deref() != Some(circuit.dimension.as_str()) {
            return Err("bot is disconnected or in another dimension".into());
        }
        self.policy
            .authorize_dimension(&circuit.dimension)
            .map_err(|error| error.to_string())?;
        let observation =
            crate::piston_door::inspect(&circuit.snapshot, &status.version, circuit.complete, None);
        if !matches!(
            observation.state,
            dustroute_translate::piston_observation::PistonObservationState::Open
                | dustroute_translate::piston_observation::PistonObservationState::Closed
        ) {
            return Err(DoorPlanningFailure::incomplete(observation));
        }
        let door = crate::piston_door::verify(&circuit.snapshot, &status.version)?;
        self.policy
            .validate_region(door.bounds())
            .map_err(|error| error.to_string())?;
        let id = uuid::Uuid::new_v4();
        let response = DoorProposal::new(id, observation, &door, params.target);
        let mut plans = self.plans.table::<StoredDoorPlan>().lock().await;
        plans.retain(|_, plan| plan.expires_at > Instant::now());
        if plans.len() >= 256 {
            return Err("too many door plans; wait for expiry".into());
        }
        plans.insert(
            id,
            StoredDoorPlan {
                player,
                dimension: circuit.dimension,
                door,
                target: params.target,
                lifecycle: InvocationState::Draft,
                expires_at: Instant::now() + Duration::from_secs(300),
            },
        );
        drop(plans);
        self.operations
            .record_completed(
                id,
                OperationKind::PistonDoorProposal,
                response.clone().into(),
            )
            .await;
        Ok(response)
    }

    async fn show_piston_door(&self, id: uuid::Uuid, player: Option<&str>) -> CallToolResult {
        let result: Result<ShownDoor, UnrecordedFailure> = async {
            let player = self.resolve_player(player)?;
            self.player_scope()
                .authorize(&player)
                .map_err(|error| error.at(FailurePhase::Admission))?;
            let plan = self
                .plans
                .table::<StoredDoorPlan>()
                .lock()
                .await
                .get(&id)
                .cloned()
                .ok_or("door plan not found")?;
            if plan.player != player
                || plan.lifecycle.attempted()
                || plan.expires_at <= Instant::now()
            {
                return Err("door plan is expired, consumed or owned by another player".into());
            }
            self.policy
                .authorize_dimension(&plan.dimension)
                .map_err(|error| error.to_string())?;
            let bounds = plan.door.bounds();
            let preview = self
                .bridge
                .preview_region(&player, bounds.min, bounds.max, &plan.dimension)
                .await
                .map_err(|error| error.to_string())?;
            if let Some(stored) = self
                .plans
                .table::<StoredDoorPlan>()
                .lock()
                .await
                .get_mut(&id)
            {
                stored.lifecycle.preview()?;
            }
            Ok(ShownDoor::new(id, plan.door.state(), plan.target, preview))
        }
        .await;
        match result {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error),
        }
    }

    async fn invoke_piston_door(&self, id: uuid::Uuid, confirm: bool) -> CallToolResult {
        use crate::operations::piston::{DoorExecution, DoorOperationResult, ExecutionOutcome};
        let mut progress = ExecutionProgress::default();
        let result:Result<DoorOperationResult,FailureCause>=async {
            if !confirm { return Err("confirm=true is required".into()); }
            self.policy.authorize_mutation().map_err(FailureCause::from)?;
            let player=self.resolve_player(None)?;
            self.player_scope().authorize(&player)?;
            let _guard=self.mutation_lock.lock().await;
            let plan=self.plans.table::<StoredDoorPlan>().lock().await.get(&id).cloned().ok_or("door plan not found")?;
            if plan.player!=player || !plan.lifecycle.is_previewed() || plan.lifecycle.attempted() || plan.expires_at<=Instant::now() { return Err("door plan requires preview, matching owner, unexpired and unused token".into()); }
            let status=self.bridge.status().await.map_err(FailureCause::from)?;
            if !status.connected || status.dimension.as_deref()!=Some(plan.dimension.as_str()) { return Err("bot is disconnected or in another dimension".into()); }
            self.policy.authorize_dimension(&plan.dimension).map_err(FailureCause::from)?;
            let bounds=plan.door.bounds();
            self.policy.validate_region(bounds).map_err(FailureCause::from)?;
            // Approach first, then scan immediately before the lever write.
            self.bridge.approach_lever(plan.door.lever(),&plan.dimension).await.map_err(FailureCause::from)?;
            progress.phase=FailurePhase::BeforeReadback;
            let snapshot=self.bridge.scan_region(bounds.min,bounds.max,&plan.dimension).await.map_err(FailureCause::from)?;
            let observation=crate::piston_door::inspect(&snapshot,&status.version,true,Some(&plan.door));
            let current=match crate::piston_door::verify(&snapshot,&status.version) {
                Ok(door)=>door,
                Err(error)=>return Ok(DoorOperationResult::refused(id, error, observation)),
            };
            if current.lever()!=plan.door.lever() || current.state()!=plan.door.state() { return Ok(DoorOperationResult::refused(id, "door changed since proposal; inspect and create a new plan", observation)); }
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
            progress.operation_consumed=true;
            progress.total_changes=Some(1);
            if current.state()==plan.target {
                self.plans.table::<StoredDoorPlan>().lock().await.get_mut(&id).ok_or("door plan missing")?.lifecycle.confirm(true);
                progress.world=WorldOutcome::Verified;progress.total_changes=Some(0);
                crate::performance::execution_progress(&progress);
                let result = DoorOperationResult::unchanged(id, plan.target, observation, progress.clone());
                self.operations.record_completed(id,OperationKind::PistonDoorRun,result.clone().into()).await;
                return Ok(result);
            }
            progress.begin_submission();
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
            let observed_state = verified.map(|d| d.state());
            let verification_error = verification.as_ref().err().cloned();
            let activation_error = activation.as_ref().err().map(ToString::to_string);
            let wait_error = wait.as_ref().err().map(ToString::to_string);
            let scan_error = observed.as_ref().err().map(ToString::to_string);
            let mut failure=None;
            match activation {Ok(_)=>progress.submitted(1),Err(error)=>{progress.submitted_changes=None;FailureReport::append(&progress,&mut failure,error.cause().at(FailurePhase::Submission));}}
            if let Err(error)=wait {FailureReport::append(&progress,&mut failure,error.cause().at(FailurePhase::Wait));}
            match observed {
                Err(error)=>FailureReport::append(&progress,&mut failure,error.cause().at(FailurePhase::AfterReadback)),
                Ok(_)=>if !matches {FailureReport::append(&progress,&mut failure,FailureCause::new(CauseKind::VerificationMismatch,"door did not match requested state").at(FailurePhase::Verification));},
            }
            progress.phase=if scan_error.is_none(){FailurePhase::Verification}else{FailurePhase::AfterReadback};
            if matches {progress.world=WorldOutcome::Verified;progress.verified_steps=1;}
            crate::performance::execution_progress(&progress);
            let result = DoorOperationResult::executed(id, DoorExecution {
                target: plan.target, observed_state, observation, verified: matches,
                verification_error, activation_error, wait_error, scan_error, automatic_rollback: false,
                outcome: ExecutionOutcome::from_attempt(progress.clone(), failure),
            });
            self.operations.record_completed(id,OperationKind::PistonDoorRun,result.clone().into()).await;
            if !ok { self.operations.fail(id, "door result requires fresh inspection").await; }
            Ok(result)
        }.await;
        let response = result.unwrap_or_else(|error| {
            crate::performance::execution_progress(&progress);
            DoorOperationResult::failed_attempt(id, progress.cause(error))
        });
        typed_reply(response)
    }
    #[tool(
        description = "Discover single-lever transition scenarios in the supplied immutable circuit_id without changing the world",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn new_transition_test(
        &self,
        Parameters(params): Parameters<ProposeTransitionParams>,
    ) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
        };
        if let Err(error) = self.player_scope().authorize(&player) {
            return typed_reply(error.at(FailurePhase::Admission).as_response());
        }
        let observation_ticks = params.observation_ticks.unwrap_or(20);
        let max_events = params.max_events.unwrap_or(16_384);
        if !(1..=200).contains(&observation_ticks) || !(1..=65_536).contains(&max_events) {
            return error_reply(
                McpErrorCode::InvalidArgument,
                "observation_ticks must be 1..200 and max_events must be 1..65536",
                false,
            );
        }
        let (circuit_id, circuit) = match self.load_circuit(&params.circuit_id, &player).await {
            Ok(circuit) => circuit,
            Err(error) => return typed_reply(error.as_response()),
        };
        let bounds = circuit.bounds;
        let dimension = circuit.dimension;
        if let Err(error) = self.policy.validate_region(bounds) {
            return typed_reply(FailureCause::from(error).as_response());
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
            #[derive(Serialize)]
            struct Rejected {
                #[serde(flatten)]
                error: crate::operations::mutation::UnrecordedFailure,
                safety: TransitionSafetyAssessment,
            }
            return typed_reply(Rejected {
                error: crate::operations::mutation::UnrecordedFailure::message(
                    "the observed region is not eligible for an automatic transition scenario",
                ),
                safety,
            });
        }
        #[derive(Serialize)]
        struct Proposals {
            schema_version: &'static str,
            ok: crate::operations::mutation::Success,
            circuit_id: uuid::Uuid,
            bounds: dustroute_translate::world_reverse::RegionBounds,
            dimension: String,
            proposals: Vec<crate::operations::transition::TransitionProposal>,
            next_step: &'static str,
        }
        typed_reply(Proposals {
            schema_version: TRANSITION_SCHEMA_V1,
            ok: crate::operations::mutation::Success,
            circuit_id,
            bounds,
            dimension,
            proposals,
            next_step: "show_operation, then invoke_operation(confirm=true) only for a ready proposal",
        })
    }

    async fn show_transition_test(
        &self,
        Parameters(params): Parameters<PreviewTransitionParams>,
    ) -> CallToolResult {
        use crate::operations::mutation::{Success, UnrecordedFailure};
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_reply(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let plans = self.plans.table::<StoredTransitionPlan>().lock().await;
        let Some(plan) = plans.get(&operation_id).cloned() else {
            return error_reply(
                McpErrorCode::NotFound,
                "transition scenario not found",
                false,
            );
        };
        drop(plans);
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
        };
        if player != plan.player {
            return typed_reply(UnrecordedFailure::message(
                "scenario belongs to another player",
            ));
        }
        let preview = match self
            .bridge
            .preview_region(&player, plan.bounds.min, plan.bounds.max, &plan.dimension)
            .await
        {
            Ok(preview) => preview,
            Err(error) => {
                return typed_reply(FailureCause::from(error).as_response());
            }
        };
        if let Some(stored) = self
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .get_mut(&operation_id)
        {
            if let Err(error) = stored.lifecycle.preview() {
                return typed_reply(UnrecordedFailure::message(error));
            }
        }
        #[derive(Serialize)]
        struct Preview {
            schema_version: &'static str,
            ok: Success,
            operation_id: uuid::Uuid,
            lever: Pos,
            original_powered: bool,
            safety: TransitionSafetyAssessment,
            preview: crate::bridge::PreviewSubmission,
            warning: &'static str,
        }
        typed_reply(Preview {
            schema_version: TRANSITION_SCHEMA_V1,
            ok: Success,
            operation_id,
            lever: plan.lever,
            original_powered: plan.original_powered,
            safety: plan.safety,
            preview,
            warning: "running moves the bot within reach when necessary, normally activates this lever once, and restores it after observation",
        })
    }

    async fn invoke_transition_test(
        &self,
        Parameters(params): Parameters<RunTransitionParams>,
    ) -> CallToolResult {
        let workflow = self.transition_workflow();
        let session = match workflow.admit(params.confirm).await {
            Ok(session) => session,
            Err(refusal) => return typed_reply(refusal),
        };
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_reply(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let prepared = match session.prepare_run(operation_id).await {
            Ok(prepared) => prepared,
            Err(refusal) => return typed_reply(refusal),
        };
        let contracts = match transition_contracts(params.contracts.as_deref(), prepared.scene()) {
            Ok(contracts) => contracts,
            Err(error) => return typed_reply(UnrecordedFailure::message(error)),
        };
        match prepared.run(contracts).await {
            Ok(result) => typed_reply(result),
            Err(refusal) => typed_reply(refusal),
        }
    }

    async fn restore_transition_test(
        &self,
        Parameters(params): Parameters<RunTransitionParams>,
    ) -> CallToolResult {
        let workflow = self.transition_workflow();
        let session = match workflow.admit(params.confirm).await {
            Ok(session) => session,
            Err(refusal) => return typed_reply(refusal),
        };
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_reply(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        match session.restore(operation_id).await {
            Ok(result) => typed_reply(result),
            Err(refusal) => typed_reply(refusal),
        }
    }

    #[tool(
        description = "Start cancellable reverse analysis of the selected region and return an operation ID for progress polling. Set include_truth_table=true for explicitly bounded exhaustive functional inference, including large regions."
    )]
    async fn start_selected_region_conversion(
        &self,
        Parameters(params): Parameters<StartSelectedRegionConversionParams>,
    ) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(UnrecordedFailure::from(error)),
        };
        if let Err(error) = self.player_scope().authorize(&player) {
            return typed_reply(error.at(FailurePhase::Admission).as_response());
        }
        let (bounds, dimension) = match self.selected_region(&player).await {
            Ok(region) => region,
            Err(error) => return typed_reply(UnrecordedFailure::from(error)),
        };
        if let Err(error) = self.policy.validate_region(bounds) {
            return typed_reply(FailureCause::from(error).as_response());
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
            Err(error) => return error_reply(McpErrorCode::InvalidArgument, error, false),
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
            let guard = operations
                .begin_activity(
                    operation_id,
                    crate::operations::ActivityAction::AnalyzeRegion,
                )
                .await;
            crate::performance::with_activity(guard.as_ref().map(|g| g.0.clone()), async {
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
                        operations
                            .complete(
                                operation_id,
                                AnalysisResult::from(
                                    FailureCause::from(error).at(FailurePhase::BeforeReadback),
                                )
                                .into(),
                            )
                            .await;
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
                let normalization =
                    crate::performance::span(crate::performance::Phase::Normalization);
                let world = match world_from_snapshot_for_service(&snapshot) {
                    Ok(world) => world,
                    Err(error) => {
                        operations
                            .complete(
                                operation_id,
                                AnalysisResult::from(error.at(FailurePhase::Normalization)).into(),
                            )
                            .await;
                        return;
                    }
                };
                drop(normalization);
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
                let capture = crate::performance::current();
                let result = match tokio::task::spawn_blocking(move || {
                    capture.in_blocking(|| {
                        let _phase = crate::performance::span(crate::performance::Phase::Analysis);
                        if redstone_components > MAX_FLAT_ANALYSIS_COMPONENTS
                            && !request.infer_truth_table
                        {
                            let mut analysis =
                                dustroute_translate::world_reverse::analyze_world_region(
                                    &world, bounds,
                                );
                            analysis.scene.observation.dimension = dimension;
                            let hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
                            AnalysisResult::from(hierarchical_report(
                                bounds,
                                &hierarchy,
                                None,
                                RecordedExpansion::ExplicitSelectedRegion {
                                    components_loaded: Some(redstone_components),
                                    component_limit: None,
                                    limit_reached: false,
                                },
                                None,
                            ))
                        } else {
                            let mut staged = app.analyze_physical(&world, request);
                            staged.reverse.analysis.scene.observation.dimension = dimension;
                            AnalysisResult::from(reverse_report(bounds, &staged.reverse))
                        }
                    })
                })
                .await
                {
                    Ok(result) => result,
                    Err(error) => {
                        operations
                            .complete(
                                operation_id,
                                AnalysisResult::from(
                                    FailureCause::new(CauseKind::Unknown, error.to_string())
                                        .at(FailurePhase::Analysis),
                                )
                                .into(),
                            )
                            .await;
                        return;
                    }
                };
                if operations.is_cancelled(operation_id).await {
                    return;
                }
                operations.complete(operation_id, result.into()).await;
            })
            .await;
        });
        typed_reply(QueuedAnalysis {
            ok: Success,
            operation_id,
            status: OperationStatus::Queued,
            next_step: "poll get_operation; call stop_operation if the conversion is no longer needed",
        })
    }

    #[tool(
        description = "Manage durable placed custom piston Assemblies: list/get history, observe fresh complete samples, diagnose coordinate-level missing/extra/block/property differences with shared repair assessment and pinned Blueprint references, even when reconstruction is blocked, plan_removal for a matching applied instance, or plan_reconstruction to tear down an observed supported layout and rebuild its declared initial state. Reconstruction requires review of all affected blocks and no competing inputs/edits; matching client samples do not prove empty server queues. Rejects moving/incomplete observations and extra material. Fresh pinned source/target review required. Preview with show_operation; apply with invoke_operation(confirm=true). Never writes world blocks itself.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn manage_assembly(
        &self,
        Parameters(params): Parameters<assembly_placement::ManageAssemblyParams>,
    ) -> CallToolResult {
        match self.assembly_service().manage_placed_assembly(params).await {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(error.as_response()),
        }
    }

    #[tool(
        description = "Read or observe a durable region construction job, freshly plan_next or plan_undo after restart, explicitly plan_recovery from an unchanged failed-stage baseline, or cancel pending work. Every stage uses the entire observed physical context and requires show_operation then invoke_operation(confirm=true). Saved progress never restores an executable plan; uncertain/partial writes require inspection and are never automatically retried or rolled back. Future regions are unverified until individually planned. No block writes occur in this tool.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn manage_construction_job(
        &self,
        Parameters(params): Parameters<construction_job::ManageConstructionJobParams>,
    ) -> CallToolResult {
        match self.manage_construction_job_inner(params).await {
            Ok(response) => typed_reply(response),
            Err(error) => typed_reply(crate::operations::mutation::UnrecordedFailure::message(
                error,
            )),
        }
    }

    #[tool(
        description = "Show an operation. Blueprint updates return the full diff and a fresh independent parent/child/shared review; this does not adopt them or contact Minecraft. World placement, repair, piston-door and transition-test operations retain their preview path.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn show_operation(
        &self,
        Parameters(params): Parameters<ShowOperationParams>,
    ) -> CallToolResult {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_reply(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let plan_kind = self.plans.kind(&operation_id).await;
        if plan_kind == Some(PlanKind::ElectricalEdit) {
            return match self
                .show_electrical_edit(operation_id, params.player.as_deref())
                .await
            {
                Ok(response) => typed_reply(response),
                Err(error) => typed_reply(error.as_response()),
            };
        }
        if plan_kind == Some(PlanKind::Assembly) {
            return match self
                .assembly_service()
                .show_assembly_construction(operation_id, params.player.as_deref())
                .await
            {
                Ok(response) => typed_reply(response),
                Err(error) => typed_reply(error.as_response()),
            };
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
                Err(error) => return typed_reply(error.as_response()),
            };
            if let Err(error) = self.authorize_player(&player) {
                return typed_reply(error.as_response());
            }
            if context.player != player
                || !context.lifecycle.can_begin(false)
                || context.expires_at <= Instant::now()
            {
                return json_reply(
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
                Err(error) => return json_reply(json!({"ok":false,"error":error.to_string()})),
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
                Ok(plan) => typed_reply(PlacementPlanDisplay {
                    ok: Success,
                    read_only: self.policy.read_only,
                    plan,
                }),
                Err(error) => {
                    if let Some(plan) = self.plans.placements().lock().await.get_mut(&operation_id)
                    {
                        plan.previewed = false;
                    }
                    typed_reply(UnrecordedFailure::message(error))
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
            Err(error) => return error_reply(McpErrorCode::Internal, error, false),
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
            return typed_reply(result);
        }
        error_reply(McpErrorCode::NotFound, "operation not found", false)
    }

    #[tool(
        description = "For a Blueprint update, supply blueprint_decision action adopt or reject (with reason) and confirm=true. Adoption freshly revalidates and appends immutable records locally; rejection retains history. Neither writes Minecraft. Other operation kinds execute their existing previewed world action with confirm=true.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn invoke_operation(
        &self,
        Parameters(params): Parameters<InvokeOperationParams>,
    ) -> CallToolResult {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_reply(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        if let Some(decision) = params.blueprint_decision {
            if params.contracts.is_some() {
                return typed_reply(crate::blueprint_mcp::failure(
                    "transition contracts are not Blueprint decision parameters",
                ));
            }
            return typed_reply(
                self.blueprint_command(
                    crate::blueprint_mcp::Command::Decide(
                        operation_id,
                        Some(decision),
                        params.confirm,
                    ),
                    None,
                    true,
                )
                .await
                .expect("explicit blueprint response"),
            );
        }
        let plan_kind = self.plans.kind(&operation_id).await;
        if plan_kind == Some(PlanKind::ElectricalEdit) {
            return typed_reply(
                self.mutate_electrical_edit(operation_id, params.confirm, false)
                    .await,
            );
        }
        if plan_kind == Some(PlanKind::Assembly) {
            return typed_reply(
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
            Err(error) => return error_reply(McpErrorCode::Internal, error, false),
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
            return typed_reply(result);
        }
        error_reply(McpErrorCode::NotFound, "operation not found", false)
    }

    #[tool(
        description = "Undo an applied placement or repair, or restore a transition-test operation. Requires confirm=true.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn undo_operation(
        &self,
        Parameters(params): Parameters<ConfirmedOperationParams>,
    ) -> CallToolResult {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_reply(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let plan_kind = self.plans.kind(&operation_id).await;
        if plan_kind == Some(PlanKind::ElectricalEdit) {
            return typed_reply(
                self.mutate_electrical_edit(operation_id, params.confirm, true)
                    .await,
            );
        }
        if plan_kind == Some(PlanKind::Assembly) {
            return typed_reply(
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
            return error_reply(
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
            Err(error) => return error_reply(McpErrorCode::Internal, error, false),
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
            return typed_reply(result);
        }
        error_reply(McpErrorCode::NotFound, "operation not found", false)
    }

    #[tool(
        description = "Read operation status/results, including persisted Blueprint decisions and existing-machine edit history. Edit attempts retain verified steps and readback evidence after restart without restoring executable plans. Saved records are historical diagnostics, never fresh proof. The separate activity field reports request-local active phase and elapsed time during invoke/undo and asynchronous analysis; execution_progress distinguishes submission, verification and durable checkpoints. Activity is not persisted and gives no ETA or mutation cancellation guarantee.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_operation(
        &self,
        Parameters(params): Parameters<OperationParams>,
    ) -> CallToolResult {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_reply(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let activity = self.operations.activity(operation_id).await;
        // Live activity is separate from archived facts, including a saved intent.
        #[derive(Serialize)]
        struct WithActivity<T> {
            #[serde(flatten)]
            response: T,
            #[serde(skip_serializing_if = "Option::is_none")]
            activity: Option<crate::operations::ActivitySnapshot>,
        }
        match self.electrical_edit_history(operation_id) {
            Ok(Some(record)) => {
                return typed_reply(WithActivity {
                    response: record,
                    activity,
                });
            }
            Err(error) => {
                return typed_reply(WithActivity {
                    response: crate::operations::mutation::UnrecordedFailure::message(error),
                    activity,
                });
            }
            Ok(None) => {}
        }
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Query {
            Record {
                ok: Success,
                operation: Box<crate::operations::OperationRecord>,
            },
            Active {
                ok: Success,
                operation_id: uuid::Uuid,
            },
            Missing(ErrorResponse),
        }
        let result = match self.operations.get(operation_id).await {
            Some(operation) => Query::Record {
                ok: Success,
                operation: Box::new(operation),
            },
            None if activity.is_some() && self.plans.kind(&operation_id).await.is_some() => {
                Query::Active {
                    ok: Success,
                    operation_id,
                }
            }
            None => {
                if let Some(response) = self
                    .blueprint_command(
                        crate::blueprint_mcp::Command::Get(operation_id),
                        None,
                        false,
                    )
                    .await
                {
                    return typed_reply(blueprints::ResponseWithActivity { response, activity });
                }
                if activity.is_some() {
                    Query::Active {
                        ok: Success,
                        operation_id,
                    }
                } else {
                    Query::Missing(ErrorResponse::new(
                        McpErrorCode::NotFound,
                        "unknown operation ID",
                        false,
                    ))
                }
            }
        };
        typed_reply(WithActivity {
            response: result,
            activity,
        })
    }

    #[tool(
        description = "Request cancellation of queued/running region analysis. In-flight observation or model work may finish; mutations cannot be cancelled through this tool."
    )]
    async fn stop_operation(
        &self,
        Parameters(params): Parameters<OperationParams>,
    ) -> CallToolResult {
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return error_reply(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        let cancelled = self.operations.cancel(operation_id).await;
        json_reply(json!({ "ok": cancelled, "operation_id": operation_id }))
    }

    #[tool(description = "Clear a player's pending gaze-based region selection")]
    async fn clear_region(&self, Parameters(params): Parameters<PlayerParams>) -> CallToolResult {
        let player = match self.resolve_player(params.player.as_deref()) {
            Ok(player) => player,
            Err(error) => return typed_reply(error.as_response()),
        };
        if let Err(error) = self.authorize_player(&player) {
            return typed_reply(error.as_response());
        }
        if let Some(session) = self.selections.lock().await.get_mut(&player) {
            session.session.clear();
            session.dimension = None;
        }
        json_reply(json!({ "ok": true, "player": player }))
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
            "Work with the player on a Minecraft redstone circuit using the PowerShell-style Verb-Noun contract expressed as snake_case. For offline Blueprint work, start with get_circuit_revision(blueprint.kind=catalog); no live observation is required. For caller-designed buildings, test_circuit_change(blueprint.action=generate_building_design) accepts typed explicit parts, fill/shell/blocks shapes, per-part cutouts and named permanent-air spaces. Translate the agreed design into this data; inspect structured errors/diagnostics and distinguish failed requirements from undetermined checks before correcting by item/coordinate. Use generate_building_design_update with the adopted base, previous input and revised design to propose changes while retaining unchanged pins; adoption does not upgrade placed instances. For region jobs, get_world(region) captures explicit coordinates independently of gaze. test_circuit_change accepts up to 4096 virtual edits; new_placement(revision_id,work_regions) creates a durable job from disjoint regions, automatically subdividing oversized regions and combining bounded support/watch cycles into at most 64 stages of 64 declared changes. Stage boundaries include natural physical changes; one temporary output initialization is allowed without changing controllable inputs or the immutable final target. Regions already satisfied by natural updates still require fresh observation, preview and confirmation with no block writes. Use manage_construction_job to observe, freshly plan_next or reverse plan_undo, including after restart; show and confirm every stage. Only the current stage is physically proved, in the entire context; later stages are unverified. No automatic retry or rollback; partial writes require inspection and a new repair design. For edits at a captured site, new_placement edit_scope declares editable/protected regions, with all other observed cells protected during modeled callbacks. It can wrap one uniquely adopted equipment Assembly, preserving all nested requirements and exposing selected terminal aliases. Geometry outside its explicit motion region stays fixed throughout combined-world review. Passed model checks permit the ordinary import/proposal/adoption/target-planning workflow; they never imply live-site clearance or world-write permission. For small building requests, test_circuit_change(blueprint.action=generate_building) authors a bounded enclosure with floor/walls/roof and a passage, exact air clearance and shared physical construction checks. For an enclosure with a door, use blueprint.action=generate_building_with_door and a uniquely adopted PistonDoor source: supply the selected Assembly, occurrence, type, rotation and explicit source-frame reserved_space. The first path supports a one-block-deep 3x3 door inside the north-wall frame, with combined structure and operation verification and aliased control/aperture ports. Generation does not publish or authorize writes: import result.records, propose_update with result.request after removing its id, review and adopt before new_placement with assembly_target. Building shell materials are limited to supported cubes, and the complete target must be observed empty. Use a fresh reconstruction plan after interruption, never replay an old uncertain attempt. Read exact source/type/classification/Assembly records through blueprint.kind, and use test_circuit_change(blueprint.action=import, capture_revision, optimize, enumerate_layouts, or propose_update). Import saves unverified data. Blueprint optimize searches the supplied Assembly under a selected behavioral type and pinned execution context. Its explicit component scope separates body positions and fixed external equipment, with body and complete counts reported separately. enumerate_layouts tests each torch/support placement; only passed candidates satisfy the selected type in that context. Device outputs observe torches directly. Levers are independent realizations, not a NOT input-type requirement. It may move ports and replace internal interpretations, returns standalone candidate data, and neither proves global minimality nor publishes or adopts anything. Review the changed decomposition and prepare an explicit parent update using the candidate definitions, state and context. Proposals must supply new immutable parent definitions and explicit candidate state; classification labels do not verify behavior, and connection types check signal compatibility only. Review through show_operation, including every child and shared occurrence, then invoke_operation with confirm=true and blueprint_decision action adopt or reject (with reason). Adoption revalidates and appends records locally; it never writes Minecraft or automatically updates other parents. Failed or undetermined checks block adoption even if the parent passes. Persisted history is not validation proof. Blueprint IDs are not live placement arguments. An Assembly ID needs unique adoption. With assembly_target, the unified electrical context supports custom Assembly construction, including passive buildings, in a completely observed empty target region, after fresh target review and construction simulation; commands preserve their reviewed order; each model-reviewed batch is read back over the entire region, and a mismatch stops the sequence. This requires Java 1.21.11 with observed vanilla feature flags. Undo requires the exact constructed settled state. After restart, use manage_assembly to list saved instances, diagnose damage against the design without writing blocks, and then plan_removal or plan_reconstruction; preview and confirm that new operation. Saved instance records do not authorize writes, and uncertain attempts require inspection. Without assembly_target, grounded Assembly reflection retains its original-coordinate requirement. Neither route reuses stored validation as authority. For live-world tasks, use get_world for literal visibility, then test_circuit to capture an immutable circuit snapshot and compact health. Reuse its circuit_id for get_circuit_ir, convert_from_circuit, test_circuit_change, new_repair, get_repair_context, new_optimization, new_macro_optimization, new_transition_test, and new_piston_door_operation; never silently switch back to the current gaze during one task. After new_repair, use get_repair_context when physical fragments admit competing repair and external-input hypotheses; present supporting and contradictory evidence and ask the returned questions before choosing. For observed optimization, require an explicit focus, keep everything outside fixed, and explain verification limits. Only pass a macro component_id returned for that same circuit_id. For mixed IR, pass circuit_id and first request the summary, then pass its analysis_id and a node_id to expand only that node. Treat intrinsic sources, controllable inputs, event inputs, and observation boundaries as distinct. For hypothetical edits, test_circuit_change creates an immutable revision_id from either circuit_id or parent revision_id. Use get_circuit_revision to read saved edits and validation; revision IDs are never live circuit IDs or placement permissions. New operations only create plans. Always call show_operation, explain the preview, and obtain explicit confirmation before invoke_operation(confirm=true). Use undo_operation for supported recovery. For the existing 1x2 piston door, show_region rescans the selected region and returns mechanisms plus a fresh circuit_id; convert_from_circuit interprets that immutable snapshot. Use a fresh ID for a new_piston_door_operation only when the observed mechanism matches its contract. Piston door operations have no automatic retry or undo. For an explicitly selected region, call set_region twice and show_region; reuse the circuit_id returned by show_region. Never infer coordinates from prose when gaze tools can ground them, and never mutate the world without preview and explicit confirmation.".to_owned(),
        )])
        .with_description("Safe gaze-grounded DustRoute collaboration workflow")
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for DustRouteMcp {
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, rmcp::ErrorData> {
        let name = request.name.clone();
        let activity = if matches!(name.as_ref(), "invoke_operation" | "undo_operation") {
            let id = request
                .arguments
                .as_ref()
                .and_then(|a| a.get("operation_id"))
                .and_then(Value::as_str)
                .and_then(|id| uuid::Uuid::parse_str(id).ok());
            match id {
                Some(id) => {
                    self.operations
                        .begin_activity(
                            id,
                            if name == "invoke_operation" {
                                crate::operations::ActivityAction::InvokeOperation
                            } else {
                                crate::operations::ActivityAction::UndoOperation
                            },
                        )
                        .await
                }
                None => None,
            }
        } else {
            None
        };
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        crate::performance::with_activity(activity.as_ref().map(|guard| guard.0.clone()), async {
            crate::performance::tool(&name, self.tool_router.call(call)).await
        })
        .await
    }

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
