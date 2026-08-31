//! Core intermediate representations and compiler stages for DustRoute.

pub mod analysis;
pub mod api;
pub mod behavior;
pub mod cell_library;
pub mod cells;
pub mod circuits;
pub mod compiler;
pub mod connectivity;
pub mod diagnostic;
pub mod electrical;
pub mod expr {
    pub use dustroute_ir::expr::*;
}
pub mod logic {
    pub use dustroute_ir::logic::*;
}
pub mod liveness;
pub mod minecraft_export;
pub mod minecraft_semantics;
pub mod multinet;
pub mod physical;
pub mod port_realization;
pub mod repair;
pub mod routing;
pub mod routing_resources;
pub mod scenario;
pub mod sim;
pub mod snapshot;
pub mod wire;
pub mod world {
    pub use dustroute_minecraft::*;
}
pub mod world_reverse;

pub use analysis::{
    BooleanFunction, FocusedRole, FunctionalClassification, LocalSignalRole, LogicalRole,
    PhysicalAnalysis, SemanticEquivalence, SignalPath, analyze_physical_region,
    classify_focused_role, classify_truth_table, compare_live_trace, derive_local_logic,
    explain_signal_path, propose_scenarios, simulate_scenario, verify_semantic_equivalence,
};
pub use api::{
    ForwardOptions, ForwardResult, ReverseRequest, ReverseResult, TranslateError, Translator,
};
pub use behavior::simulate_behavior_trace;
pub use cell_library::{CellLibrary, CellVerification, default_cell_library, verify_cell};
pub use cells::{
    InputPort, OutputPort, PhysicalCell, PlacedCell, PortKind, RotationY, and_cell,
    baseline_cell_for, buffered_boundary_cell, nand_cell, not_cell, not_top_cell, or_buffered_cell,
    terminal_cell,
};
pub use circuits::{decoder_1_to_2, full_adder, half_adder, half_subtractor, mux_2_to_1};
pub use compiler::{BaselineCompileConfig, BaselineCompileResult, BaselineCompiler, CompileError};
pub use connectivity::{
    ConnectivityEdge, EdgeKind, PhysicalConnectivityGraph, PhysicalStep, PhysicalStepKind,
    build_physical_circuit, extract_connectivity, physical_step, physical_step_connected,
};
pub use diagnostic::{
    CircuitDiagnosticReport, CircuitDiagnosticStatus, DiagnosticConfidence, DiagnosticCounts,
    DiagnosticFinding, DiagnosticHealth, RecommendedAction, RecommendedActionKind, diagnose_scene,
};
pub use dustroute_ir::{
    DagBuilder, Expr, GateKind, LogicDag, LogicError, LogicNode, NodeId, best_by_size,
    rewrites_once, search_equivalents,
};
pub use dustroute_minecraft::{
    Block, BlockKind, BlockProperties, BlockRedstoneTraits, Facing, OccupiedShape, Pos,
    WireConnection, World,
};
pub use electrical::{
    DeviceOutputState, InstantaneousElectricalState, MAX_SIGNAL, PoweredBlockState,
    solve_instantaneous,
};
pub use liveness::{
    DirectedSignalRegion, DriveFailure, RankedLivenessFinding, RequiredInputAssessment,
    RequiredInputStatus, SignalLivenessReport, SignalSource, SignalSourceKind, UndrivenInput,
    analyze_signal_liveness, rank_liveness_findings,
};
pub use minecraft_export::{
    DataPack, JavaExportConfig, MinecraftExportError, compiled_circuit_datapack,
    isolated_build_commands, java_block_state, world_setblock_commands,
};
pub use minecraft_semantics::{SemanticProbe, semantic_probes, semantics_datapack};
pub use multinet::{
    BrokenStep, LegalityReport, MultiNetRouting, NetId, RerouteEvent, RipupRoutingError,
    RipupRoutingResult, RoutedNet, RoutingJob, materialize_multinet, route_jobs_ripup,
    route_net_tree, validate_routing_legality,
};
pub use physical::{
    CellId, Endpoint, PhysicalError, PlacementCircuit, Route, RouteId, TerminalContract,
    TerminalDirection,
};
pub use port_realization::{
    PortRealization, PortRealizationError, realize_sink_endpoint, realize_source_endpoint,
    terminal_for_endpoint,
};
pub use repair::{
    propose_scene_component_removal, propose_scene_repairs, propose_scene_repairs_near,
};
pub use routing::{
    RouteNotFound, RouteResult, RouterConfig, astar_route, insert_repeaters, materialize_route,
};
pub use routing_resources::{
    RoutingResources, branch_stair_clearances, electrical_keepout_for_wire, horizontal_neighbors,
};
pub use scenario::{
    Scenario, ScenarioAction, ScenarioCapability, ScenarioDifference, ScenarioEvent,
    ScenarioExpectation, ScenarioPulseExpectation, ScenarioRun, ScenarioSafety, ScenarioTrace,
    compare_scenario_traces, run_scenario,
};
pub use sim::{RedstoneTickSimulator, TickState};
pub use snapshot::{
    MinecraftSnapshot, MinecraftSnapshotBlock, SnapshotError, world_from_snapshot,
    world_from_snapshot_json,
};
pub use wire::{
    DustTransfer, dust_connected, dust_transfer, dust_transmits, infer_wire_connection,
    update_wire_shapes, wire_has_arm,
};
pub use world_reverse::{
    InferredTerminal, InferredTruthTable, RegionAnalysis, RegionBounds, SignalComponent,
    SignalDiagnostics, TerminalConfidence, TruthTableComparison, TruthTableError, TruthTableRow,
    analyze_world_region, analyze_world_region_in_dimension, compare_truth_tables,
    infer_output_expressions, infer_truth_table,
};
