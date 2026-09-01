//! Placement and semantic circuit optimizers for DustRoute.

mod macro_realize;
mod macro_search;
mod phased;
mod physical;
mod placement;
mod realize;
mod reverse;
mod safety;
mod semantic;

pub use macro_realize::{
    ContextualVerificationState, MacroBoundaryDirection, MacroBoundaryPort, MacroPortRoute,
    MacroRealizationError, MacroRealizationVerification, MacroReplacementPlan,
    extract_cell_boundary, extract_model_boundary, plan_macro_replacement, resolve_builtin_layout,
};
pub use macro_search::{
    MacroReplacementCandidate, ObservedMacroMetrics, find_builtin_verified_macro_replacements,
    find_verified_macro_replacements,
};
pub use phased::{
    AnchorPolicy, CompressionAxis, CompressionDirection, DirectionalWeights, OptimizationPhase,
    OptimizationPlan, PhaseOptimizationResult, PhaseScore, StagedOptimizationResult,
    optimize_staged,
};
pub use physical::{
    PhasedPhysicalScore, PhasedPhysicalSelection, PhysicalOptimizationPhase,
    PhysicalWireOptimization, PhysicalWireOptimizationError, optimize_physical_wire_path,
    select_phased_physical_scores,
};
pub use placement::{
    MutationKind, PlacementMutation, PlacementOptimizationResult, PlacementScore, PlacementWeights,
    apply_mutation, candidate_mutations, optimize_placement, placement_score,
    refresh_route_endpoints,
};
pub use realize::{
    BehavioralEquivalence, BehavioralVerificationConfig, OptimizationRealizationError,
    OptimizationRoutingConfig, OptimizationVerification, RealizedOptimization, optimization_patch,
    realize_staged_optimization_against, verify_realized_optimization,
};
pub use reverse::{
    PhysicalRegion, RewriteReport, SemanticFragment, SemanticRewrite, eliminate_double_not,
    extract_and_then_not, extract_linear_not_chain, optimize_once_via_reverse,
    realize_identity_rewrite, realize_nand_rewrite, simplify_fragment,
};
pub use safety::{
    GuardedOptimizationCandidate, OptimizationSafety, OptimizationSafetyReason,
    TemporalCapabilities, assess_optimization_safety, prepare_guarded_optimization,
};
pub use semantic::{
    CombinedPlacementScore, SemanticScore, SemanticWeights, combined_placement_score,
    evaluate_placement_with_semantics, semantic_score,
};
