//! MCP input schema only. Domain state, validated evidence and runtime handles
//! do not belong in these deserializable request records.
use super::assembly_placement;
use rmcp::schemars;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct NewPistonDoorParams {
    pub(super) circuit_id: String,
    pub(super) target: crate::piston_door::DoorState,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PlayerParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ObserveParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Ray-cast limit in blocks. Defaults to 64.
    pub(super) max_distance: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct InspectLookedAtWorldParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Explicit work region including intended additions and surrounding air.
    /// Must contain the current gaze target. Returns an immutable circuit_id.
    /// Cannot be combined with component_gap or max_components.
    pub(super) region: Option<RegionParam>,
    /// Maximum redstone components followed from the gaze target, from 1 through 32768. Defaults to 8192.
    pub(super) max_components: Option<usize>,
    /// Maximum Manhattan gap followed between nearby components, from 1 through 16. Defaults to 2 so a one-block break remains visible.
    pub(super) component_gap: Option<u32>,
    /// Ray-cast limit in blocks. Defaults to 64.
    pub(super) max_distance: Option<f64>,
    /// Include a raw non-air block list in addition to the redstone list. Defaults to false.
    pub(super) include_block_list: Option<bool>,
    /// Maximum entries returned in each block list, from 1 through 2048. Defaults to 256.
    pub(super) max_listed_blocks: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct MarkCornerParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Either `first` or `second`.
    pub(super) corner: String,
    /// Ray-cast limit in blocks. Defaults to 64.
    pub(super) max_distance: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct DiscoverCircuitParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Maximum redstone components followed from the gaze target. Defaults to 8192.
    pub(super) max_components: Option<usize>,
    /// Extra blocks around the discovered circuit. Defaults to 1.
    pub(super) padding: Option<i32>,
    /// Maximum Manhattan distance used to discover a nearby disconnected fragment, from 1 through 16. Defaults to 2.
    pub(super) fragment_gap: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct AnalyzeLookedAtParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Maximum redstone components followed from the gaze target. Defaults to 8192.
    pub(super) max_components: Option<usize>,
    /// Maximum Manhattan gap considered for broken connections, from 1 through 16. Defaults to 2.
    pub(super) fragment_gap: Option<u32>,
    /// Explicitly enumerate a bounded truth table. Defaults to false.
    pub(super) include_truth_table: Option<bool>,
    /// Maximum inferred input count for truth-table enumeration. Defaults to 16;
    /// the row and work budgets may reject a smaller practical budget.
    pub(super) truth_table_max_inputs: Option<usize>,
    /// Settle ticks used by truth-table rows. Defaults to 60, maximum 256.
    pub(super) truth_table_settle_ticks: Option<usize>,
    /// Maximum truth-table rows. Defaults to 256, maximum 65536.
    pub(super) truth_table_max_rows: Option<usize>,
    /// Maximum estimated full-world work units. Defaults to 2,000,000.
    pub(super) truth_table_max_work_units: Option<u64>,
    /// Maximum cumulative instantaneous solver iterations. Defaults to 1,000,000.
    pub(super) truth_table_max_solver_iterations: Option<usize>,
    /// Maximum elapsed time for exhaustive inference in milliseconds. Defaults to 120,000.
    pub(super) truth_table_max_elapsed_millis: Option<u64>,
    /// Observation source: gaze (default) or selected_region.
    pub(super) scope: Option<String>,
    /// Immutable circuit snapshot ID returned by an earlier circuit read. When supplied, gaze is not read again.
    pub(super) circuit_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct StartSelectedRegionConversionParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Explicitly enumerate a bounded truth table. Defaults to false.
    pub(super) include_truth_table: Option<bool>,
    /// Maximum inferred input count for truth-table enumeration. Defaults to 16.
    pub(super) truth_table_max_inputs: Option<usize>,
    /// Settle ticks used by truth-table rows. Defaults to 60, maximum 256.
    pub(super) truth_table_settle_ticks: Option<usize>,
    /// Maximum truth-table rows. Defaults to 256, maximum 65536.
    pub(super) truth_table_max_rows: Option<usize>,
    /// Maximum estimated full-world work units. Defaults to 2,000,000.
    pub(super) truth_table_max_work_units: Option<u64>,
    /// Maximum cumulative instantaneous solver iterations. Defaults to 1,000,000.
    pub(super) truth_table_max_solver_iterations: Option<usize>,
    /// Maximum elapsed time for exhaustive inference in milliseconds. Defaults to 120,000.
    pub(super) truth_table_max_elapsed_millis: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct DiagnoseLookedAtParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Maximum redstone components followed from the gaze target. Defaults to 8192.
    pub(super) max_components: Option<usize>,
    /// Maximum Manhattan gap considered when discovering broken fragments, from 1 through 16. Defaults to 2.
    pub(super) fragment_gap: Option<u32>,
    /// Immutable circuit snapshot ID returned by an earlier circuit read. Omit to capture the circuit at the current gaze.
    pub(super) circuit_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct GetLookedAtCircuitIrParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Maximum redstone components followed from the gaze target. Defaults to 8192.
    pub(super) max_components: Option<usize>,
    /// Maximum Manhattan gap considered when discovering broken fragments. Defaults to 2.
    pub(super) fragment_gap: Option<u32>,
    /// Expand one mixed-IR node from the returned summary into physical block details.
    pub(super) node_id: Option<usize>,
    /// Analysis ID returned by the summary call. Required with node_id to prevent stale expansion.
    pub(super) analysis_id: Option<String>,
    /// Immutable circuit snapshot ID returned by an earlier circuit read. Omit to capture the circuit at the current gaze.
    pub(super) circuit_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct TestCircuitChangeParams {
    /// Alternative to block edits: append Blueprint drafts, capture an Assembly,
    /// or create an explicit child-update proposal. Does not write Minecraft.
    pub(super) blueprint: Option<crate::blueprint_mcp::BlueprintWrite>,
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Virtual full-state replacements (up to 64). Air deletes; empty creates an unchanged revision.
    #[serde(default)]
    pub(super) changes: Vec<VirtualBlockChangeParam>,
    /// Number of simulator ticks after applying the virtual changes. Defaults to 64, maximum 256.
    pub(super) simulation_ticks: Option<usize>,
    /// Exactly one of circuit_id (observed snapshot) or revision_id (hypothetical parent).
    pub(super) circuit_id: Option<String>,
    pub(super) revision_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct VirtualBlockChangeParam {
    pub(super) position: CoordinateParam,
    /// Replacement block name such as minecraft:stone. Block-state properties default to empty.
    pub(super) block: String,
    /// Full replacement properties; omitted means an empty property map.
    #[serde(default)]
    pub(super) properties: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct GetCircuitRevisionParams {
    /// Alternative to revision_id: read the local Blueprint catalog or an exact
    /// source/type/classification/Assembly record. Saved records are not proof.
    pub(super) blueprint: Option<crate::blueprint_mcp::BlueprintRead>,
    #[serde(default)]
    pub(super) revision_id: String,
    /// Include the literal snapshot and modeled Assembly Revision, if available.
    pub(super) include_snapshot: Option<bool>,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct CoordinateParam {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) z: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RegionParam {
    pub(super) min: CoordinateParam,
    pub(super) max: CoordinateParam,
}
impl RegionParam {
    pub(super) fn bounds(self) -> Result<dustroute_translate::world_reverse::RegionBounds, String> {
        if self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z {
            return Err("work region has reversed bounds".into());
        }
        Ok(dustroute_translate::world_reverse::RegionBounds::new(
            dustroute_physical::Pos::new(self.min.x, self.min.y, self.min.z),
            dustroute_physical::Pos::new(self.max.x, self.max.y, self.max.z),
        ))
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PreviewPlacementParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Built-in circuit: half-adder, half-subtractor, mux2, decoder1to2, full-adder, or piston-door-1x2.
    #[serde(default)]
    pub(super) circuit: String,
    /// Alternative to a built-in name: plan the cumulative revision diff at its original coordinates.
    pub(super) revision_id: Option<String>,
    /// Alternative to circuit/revision: an adopted Assembly whose ancestry is grounded in a complete captured Circuit Revision. Reflected only at the original dimension and coordinates.
    pub(super) assembly_revision_id: Option<dustroute_library::blueprint::AssemblyRevisionId>,
    /// Construct the adopted Assembly at an explicit new target, after a fresh
    /// review there. Requires the unified electrical execution context.
    pub(super) assembly_target: Option<assembly_placement::AssemblyPlacementTarget>,
    /// Maximum number of blocks allowed in one placement plan. Defaults to 32768.
    pub(super) max_blocks: Option<usize>,
    /// Run directional compression followed by global compaction before creating the placement plan.
    pub(super) optimize: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct OperationParams {
    /// Operation UUID returned by a start or preview tool.
    pub(super) operation_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ConfirmedOperationParams {
    /// Operation UUID returned by new_placement.
    pub(super) operation_id: String,
    /// Must be true to acknowledge that this call changes the test world.
    pub(super) confirm: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ShowOperationParams {
    /// Operation UUID returned by a new_* tool or a Blueprint update proposal.
    pub(super) operation_id: String,
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct InvokeOperationParams {
    /// Required for Blueprint update operations; never used for world changes.
    /// Adoption revalidates the candidate and appends immutable records locally.
    pub(super) blueprint_decision: Option<crate::blueprint_mcp::BlueprintDecision>,
    /// Operation UUID returned by a new_* tool or a Blueprint update proposal.
    pub(super) operation_id: String,
    /// Acknowledge the previewed action: a local Blueprint decision or a world operation.
    pub(super) confirm: bool,
    /// Optional signal contracts used only by transition-test operations.
    pub(super) contracts: Option<Vec<TransitionContractParam>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ProposeTransitionParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Number of game ticks observed after activation, from 1 through 200. Defaults to 20.
    pub(super) observation_ticks: Option<u16>,
    /// Maximum block update events recorded, from 1 through 65536. Defaults to 16384.
    pub(super) max_events: Option<usize>,
    /// Circuit snapshot to use when creating transition plans.
    pub(super) circuit_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PreviewTransitionParams {
    pub(super) operation_id: String,
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct RunTransitionParams {
    pub(super) operation_id: String,
    /// Must be true because this normally activates a lever in the test world.
    pub(super) confirm: bool,
    /// Optional output contracts used to distinguish candidates, confirmed hazards, and intended pulses.
    pub(super) contracts: Option<Vec<TransitionContractParam>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct TransitionContractParam {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) z: i32,
    /// steady_state_only, stable_high, stable_low, intentional_high_pulse, intentional_low_pulse, maximum_high_pulse, or maximum_low_pulse.
    pub(super) intent: String,
    /// Widths for live scenarios are measured in game ticks.
    pub(super) minimum_width_ticks: Option<u64>,
    /// Required for intentional and maximum-width pulse contracts.
    pub(super) maximum_width_ticks: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct ProposeRepairsParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Maximum Manhattan gap between disconnected fragments. Defaults to 2.
    pub(super) max_gap: Option<u32>,
    /// Circuit snapshot to use when creating repair plans.
    pub(super) circuit_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct GetRepairContextParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Immutable circuit snapshot used to ground every fact and hypothesis.
    pub(super) circuit_id: String,
    /// Optional repair operation returned by new_repair. When omitted, the highest-ranked current hypothesis is explained.
    pub(super) operation_id: Option<String>,
    /// Maximum Manhattan gap considered for repair evidence. Defaults to 2.
    pub(super) max_gap: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct OptimizationFocusParam {
    pub(super) min: CoordinateParam,
    pub(super) max: CoordinateParam,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct NewOptimizationParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Immutable observed circuit to optimize.
    pub(super) circuit_id: String,
    /// Inclusive physical region that may change. Every block outside remains fixed.
    pub(super) focus: OptimizationFocusParam,
    /// Currently wire_length. Future objectives will be added explicitly.
    pub(super) objective: String,
    /// Explicit preservation contract. Omitted fields use the documented safe defaults.
    pub(super) contract: Option<OptimizationContractParam>,
    /// Optional bounded-search limits. Omitted fields use safe defaults.
    pub(super) search: Option<OptimizationSearchParam>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub(super) struct OptimizationSearchParam {
    pub(super) max_expansions: Option<usize>,
    pub(super) max_candidates: Option<usize>,
    pub(super) max_millis: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct NewMacroOptimizationParams {
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
    /// Immutable observed circuit containing the proposed functional cell.
    pub(super) circuit_id: String,
    /// Candidate component_id returned by convert_from_circuit.
    pub(super) component_id: String,
    /// Explicit preservation contract. Omitted fields use the documented safe defaults.
    pub(super) contract: Option<OptimizationContractParam>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub(super) struct OptimizationContractParam {
    pub(super) logical: Option<LogicalContractParam>,
    pub(super) timing: Option<TimingContractParam>,
    pub(super) pulse: Option<PulseContractParam>,
    pub(super) analog: Option<AnalogContractParam>,
    pub(super) boundary: Option<BoundaryContractParam>,
    pub(super) mutation: Option<MutationContractParam>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub(super) struct LogicalContractParam {
    /// Currently exact_truth_table.
    pub(super) mode: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub(super) struct TimingContractParam {
    /// exact_trace, exact_transitions, bounded_delay, settled_value_only, or preserve_order.
    pub(super) mode: Option<String>,
    pub(super) maximum_added_redstone_ticks: Option<usize>,
    pub(super) settle_deadline_redstone_ticks: Option<usize>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub(super) struct PulseContractParam {
    pub(super) allow_new_pulses: Option<bool>,
    pub(super) allow_removed_pulses: Option<bool>,
    pub(super) maximum_width_delta_redstone_ticks: Option<usize>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub(super) struct AnalogContractParam {
    pub(super) preserve_strength: Option<bool>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub(super) struct BoundaryContractParam {
    pub(super) preserve_blocks: Option<bool>,
    pub(super) preserve_facing: Option<bool>,
    pub(super) preserve_driver_positions: Option<bool>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub(super) struct MutationContractParam {
    pub(super) focus_only: Option<bool>,
    pub(super) allow_temporary_expansion: Option<bool>,
    pub(super) maximum_changed_blocks: Option<usize>,
    pub(super) automatic_apply: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct PreviewRepairParams {
    /// Repair operation UUID returned by new_repair.
    pub(super) operation_id: String,
    /// Optional override; normally omitted so DUSTROUTE_ASSIST_PLAYER is used.
    #[schemars(skip)]
    pub(super) player: Option<String>,
}
