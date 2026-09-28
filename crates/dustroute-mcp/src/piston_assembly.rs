//! Private fresh placement capability for an adopted custom electrical Assembly.
//! No persisted report or deserialized value can construct this capability.
use dustroute_library::assembly::Assembly;
use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_translate::assembly_transform::AssemblyTransform;
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::piston_construction::{
    ElectricalConstruction, ElectricalConstructionStep,
};
use dustroute_translate::promotion::{CheckStatus, review_assembly_with_context};
use dustroute_translate::{MinecraftSnapshot, RegionBounds};

#[derive(Clone, Debug)]
pub(crate) struct ValidatedAssemblyPlacement {
    assembly: Assembly,
    context: RuntimeBehaviorContext,
    construction: ElectricalConstruction,
    review: serde_json::Value,
    design_index: dustroute_translate::diagnostic::report::DesignIndex,
}

impl ValidatedAssemblyPlacement {
    pub(crate) fn new(
        catalog: &BlueprintCatalog,
        assembly: &Assembly,
        context: &RuntimeBehaviorContext,
        transform: AssemblyTransform,
        baseline: &MinecraftSnapshot,
        version: &str,
    ) -> Result<Self, String> {
        let region = transform.region(context.known_region)?;
        Self::check_empty(baseline, version, RegionBounds::new(region.min, region.max))?;
        Self::review_target(catalog, assembly, context, transform)
    }

    /// Rebuild the model capability from pinned source data; this does not
    /// assert that the target is empty or that any live observation succeeded.
    pub(crate) fn review_target(
        catalog: &BlueprintCatalog,
        assembly: &Assembly,
        context: &RuntimeBehaviorContext,
        transform: AssemblyTransform,
    ) -> Result<Self, String> {
        let (assembly, context) = transform.apply(assembly, context)?;
        let review = review_assembly_with_context(
            catalog,
            &assembly,
            Some(&context.clone().into()),
            BehaviorBudget::default(),
        )
        .map_err(|e| e.to_string())?;
        if review.status() != CheckStatus::Passed {
            return Err(format!(
                "fresh review at the target coordinates did not pass: {:?}",
                review.status()
            ));
        }
        let view = assembly.inspect(catalog).map_err(|e| e.to_string())?;
        let world = view.proposed_world();
        let design_index = dustroute_translate::diagnostic::report::DesignIndex::from_view(&view);
        // The current command model includes pre-write scheduling and explicit
        // observer initialization. Rebuild it even for saved plans: historical
        // direct-install results never grant placement/removal authority.
        let construction =
            ElectricalConstruction::new(&world, context.known_region, context.root_limits)?;
        let mut review = crate::blueprint_mcp::report_json(&review, catalog);
        // These are model initial conditions, not facts recovered from the
        // live block-state baseline. Keep the accepted snapshot-based workflow
        // while making new hidden-state dependencies visible in its preview.
        let conditions: Vec<_> = world
            .iter()
            .filter_map(|(position, block)| {
                let definition =
                    dustroute_translate::world::device_program::program(block)?.definition();
                let output = definition.signal_level
                    == dustroute_translate::world::device_program::SignalLevel::StoredOutput;
                (output || definition.history.is_some()).then(|| serde_json::json!({
                "position":position,
                "block":definition.native_identity(block).ok(),
                "initial_output_signal":output.then_some(0),
                "position_history":definition.history.as_ref().map(|history| serde_json::json!({
                    "assumed":"empty", "observed":false, "survives_block_removal":true,
                    "window_game_ticks":history.policy.window
                })),
                "runtime_state_reconstructed_from_snapshot":false
            }))
            })
            .collect();
        if !conditions.is_empty() {
            review["device_initial_conditions"] = serde_json::json!(conditions);
        }
        Ok(Self {
            assembly,
            context,
            construction,
            review,
            design_index,
        })
    }
    pub(crate) fn bounds(&self) -> RegionBounds {
        RegionBounds::new(self.context.known_region.min, self.context.known_region.max)
    }
    pub(crate) fn assembly(&self) -> &Assembly {
        &self.assembly
    }
    pub(crate) fn context(&self) -> &RuntimeBehaviorContext {
        &self.context
    }
    pub(crate) fn review(&self) -> &serde_json::Value {
        &self.review
    }
    pub(crate) fn design_index(&self) -> &dustroute_translate::diagnostic::report::DesignIndex {
        &self.design_index
    }
    pub(crate) fn settled(&self) -> &MinecraftSnapshot {
        self.construction.settled()
    }
    pub(crate) fn steps(&self, undo: bool) -> &[ElectricalConstructionStep] {
        if undo {
            self.construction.remove_steps()
        } else {
            self.construction.build_steps()
        }
    }
    pub(crate) fn reconstruction_steps(
        &self,
        baseline: &MinecraftSnapshot,
    ) -> Result<Vec<ElectricalConstructionStep>, String> {
        self.construction
            .reconstruction_steps(baseline, self.context.root_limits)
    }
    pub(crate) fn operating_reference(
        &self,
        observed: &MinecraftSnapshot,
    ) -> Result<MinecraftSnapshot, String> {
        let blocks = crate::revision::blocks(observed)?;
        let inputs = self
            .context
            .input_levers
            .iter()
            .map(|position| {
                let level = blocks
                    .get(position)
                    .filter(|b| b.name == "minecraft:lever")
                    .and_then(|b| b.properties.get("powered"))
                    .and_then(|value| match value.as_str() {
                        "true" => Some(true),
                        "false" => Some(false),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        format!("declared input lever is missing or unreadable at {position:?}")
                    })?;
                Ok((*position, level))
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.construction
            .operating_reference(&inputs, self.context.root_limits)
    }
    pub(crate) fn validate_before(
        &self,
        snapshot: &MinecraftSnapshot,
        version: &str,
        undo: bool,
    ) -> Result<(), String> {
        if undo {
            Self::matches(snapshot, self.construction.settled(), version)
        } else {
            Self::check_empty(snapshot, version, self.bounds())
        }
    }
    pub(crate) fn validate_after(
        &self,
        snapshot: &MinecraftSnapshot,
        version: &str,
        undo: bool,
    ) -> Result<(), String> {
        self.validate_before(snapshot, version, !undo)
    }
    pub(crate) fn matches(
        actual: &MinecraftSnapshot,
        expected: &MinecraftSnapshot,
        version: &str,
    ) -> Result<(), String> {
        if version != "1.21.11" {
            return Err("custom piston placement requires Java 1.21.11".into());
        }
        if actual.min != expected.min || actual.max != expected.max {
            return Err("exact complete construction-region observation required".into());
        }
        if crate::revision::blocks(actual)? != crate::revision::blocks(expected)? {
            return Err("construction state or surrounding world differs from the verified plan; inspect before continuing".into());
        }
        Ok(())
    }
    fn check_empty(
        snapshot: &MinecraftSnapshot,
        version: &str,
        bounds: RegionBounds,
    ) -> Result<(), String> {
        Self::matches(
            snapshot,
            &MinecraftSnapshot {
                min: bounds.min,
                max: bounds.max,
                blocks: vec![],
            },
            version,
        )
    }
}
