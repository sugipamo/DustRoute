//! Placement and semantic circuit optimizers for DustRoute.

mod phased;
mod placement;
mod reverse;
mod semantic;

pub use phased::{
    AnchorPolicy, CompressionAxis, CompressionDirection, DirectionalWeights, OptimizationPhase,
    OptimizationPlan, PhaseOptimizationResult, PhaseScore, StagedOptimizationResult,
    optimize_staged,
};
pub use placement::{
    MutationKind, PlacementMutation, PlacementOptimizationResult, PlacementScore, PlacementWeights,
    apply_mutation, candidate_mutations, optimize_placement, placement_score,
    refresh_route_endpoints,
};
pub use reverse::{
    PhysicalRegion, RewriteReport, SemanticFragment, SemanticRewrite, eliminate_double_not,
    extract_and_then_not, extract_linear_not_chain, optimize_once_via_reverse,
    realize_identity_rewrite, realize_nand_rewrite, simplify_fragment,
};
pub use semantic::{
    CombinedPlacementScore, SemanticScore, SemanticWeights, combined_placement_score,
    evaluate_placement_with_semantics, semantic_score,
};
