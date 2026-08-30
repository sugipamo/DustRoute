//! Application services shared by MCP, CLI, and future frontends.

mod planning;

use dustroute_ir::LogicDag;
use dustroute_physical::World;
use dustroute_translate::{
    ForwardOptions, ForwardResult, PhysicalAnalysis, RegionBounds, ReverseRequest, ReverseResult,
    TranslateError, Translator, analyze_physical_region, decoder_1_to_2, full_adder, half_adder,
    half_subtractor, mux_2_to_1,
};

pub use planning::{
    BlockChange, PlacementPlan, PlanningError, UndoPlan, plan_world_overlay, relocate_world,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct DustRouteService {
    translator: Translator,
}

impl DustRouteService {
    #[must_use]
    pub fn built_in_circuit(name: &str) -> Option<LogicDag> {
        match name {
            "half-adder" => Some(half_adder()),
            "half-subtractor" => Some(half_subtractor()),
            "mux2" => Some(mux_2_to_1()),
            "decoder1to2" => Some(decoder_1_to_2()),
            "full-adder" => Some(full_adder()),
            _ => None,
        }
    }

    pub fn compile_builtin(
        &self,
        name: &str,
        options: ForwardOptions,
    ) -> Result<Option<ForwardResult>, TranslateError> {
        Self::built_in_circuit(name)
            .map(|circuit| self.translator.forward(&circuit, options))
            .transpose()
    }

    #[must_use]
    pub fn analyze_world(&self, world: &World, request: ReverseRequest) -> ReverseResult {
        self.translator.reverse(world, request)
    }

    /// Preferred physical-first entry point for frontends that need evidence,
    /// completeness, timing, and higher interpretations together.
    #[must_use]
    pub fn analyze_physical(&self, world: &World, request: ReverseRequest) -> PhysicalAnalysis {
        analyze_physical_region(world, request)
    }

    #[must_use]
    pub const fn default_reverse_request(bounds: RegionBounds) -> ReverseRequest {
        ReverseRequest::new(bounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_the_same_builtin_catalog_to_all_frontends() {
        for name in [
            "half-adder",
            "half-subtractor",
            "mux2",
            "decoder1to2",
            "full-adder",
        ] {
            assert!(DustRouteService::built_in_circuit(name).is_some());
        }
        assert!(DustRouteService::built_in_circuit("unknown").is_none());
    }
}
