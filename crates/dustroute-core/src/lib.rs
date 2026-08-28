//! Core intermediate representations and compiler stages for DustRoute.

pub mod cell_library;
pub mod cells;
pub mod circuits;
pub mod compiler;
pub mod connectivity;
pub mod electrical;
pub mod expr;
pub mod logic;
pub mod minecraft_export;
pub mod minecraft_semantics;
pub mod multinet;
pub mod physical;
pub mod placement;
pub mod port_realization;
pub mod reverse;
pub mod routing;
pub mod routing_resources;
pub mod sim;
pub mod wire;
pub mod world;

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
    extract_connectivity, physical_step, physical_step_connected,
};
pub use electrical::{
    DeviceOutputState, InstantaneousElectricalState, MAX_SIGNAL, PoweredBlockState,
    solve_instantaneous,
};
pub use expr::{Expr, best_by_size, rewrites_once, search_equivalents};
pub use logic::{DagBuilder, GateKind, LogicDag, LogicError, LogicNode, NodeId};
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
pub use physical::{CellId, Endpoint, PhysicalCircuit, PhysicalError, Route, RouteId};
pub use placement::{
    MutationKind, PlacementMutation, PlacementOptimizationResult, PlacementScore, PlacementWeights,
    apply_mutation, candidate_mutations, optimize_placement, placement_score,
    refresh_route_endpoints,
};
pub use port_realization::{
    PortRealization, PortRealizationError, realize_sink_endpoint, realize_source_endpoint,
    terminal_for_endpoint,
};
pub use reverse::{
    PhysicalRegion, RewriteReport, SemanticFragment, SemanticRewrite, eliminate_double_not,
    extract_and_then_not, extract_linear_not_chain, optimize_once_via_reverse,
    realize_identity_rewrite, realize_nand_rewrite, simplify_fragment,
};
pub use routing::{
    RouteNotFound, RouteResult, RouterConfig, astar_route, insert_repeaters, materialize_route,
};
pub use routing_resources::{
    RoutingResources, branch_stair_clearances, electrical_keepout_for_wire, horizontal_neighbors,
};
pub use sim::{RedstoneTickSimulator, TickState};
pub use wire::{dust_connected, infer_wire_connection, update_wire_shapes, wire_has_arm};
pub use world::{Block, BlockKind, BlockProperties, Facing, Pos, WireConnection, World};
