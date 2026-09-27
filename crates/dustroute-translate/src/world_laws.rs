//! Resolve the physical world's immutable laws independently of interpretation
//! nesting. Requirements are declarations, not requests to instantiate devices.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use dustroute_library::behavior_type::{PhysicalBehaviorContext, PhysicalBehaviorProfile};
use dustroute_library::blueprint::{BlueprintCatalog, BlueprintRevision, BlueprintRevisionId};
use dustroute_library::execution_context::{check_law_requirements, resolve_law_references};
use dustroute_minecraft::execution_context::{
    LawRole, WorldExecutionContext, WorldExecutionProfile,
};
use dustroute_minecraft::law::{ExecutableLaw, Instruction, LawProgram};

use crate::dust_law::DustLaw;

/// The legacy simulator and electrical solver use the same compiled default
/// world laws. This selects their existing profile; it does not upgrade it.
pub(crate) fn builtin_world_laws() -> &'static WorldLaws {
    use dustroute_library::behavior_type::BehaviorInitialCondition;
    use dustroute_library::builtin_laws::{DUST_LAW_REVISION, TORCH_LAW_REVISION, builtin_laws};
    static LAWS: OnceLock<WorldLaws> = OnceLock::new();
    LAWS.get_or_init(|| {
        WorldLaws::resolve(
            builtin_laws(),
            &PhysicalBehaviorContext {
                profile: PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
                initial_condition: BehaviorInitialCondition::FreshConstruction,
                dust_law: BlueprintRevisionId::new(DUST_LAW_REVISION).expect("built-in ID"),
                torch_law: BlueprintRevisionId::new(TORCH_LAW_REVISION).expect("built-in ID"),
                max_electrical_iterations: 128,
                input_drivers: vec![],
            },
        )
        .expect("built-in world laws")
    })
}
/// Adapters selected by the current dust/torch behavioral proof contexts. The
/// legacy simulator and bounded runner have separate fixed device law pins;
/// those do not make device laws available in these proof contexts. Further
/// families need explicit context support, not merely a catalog entry.
#[derive(Debug)]
pub(crate) struct WorldLaws {
    pub dust: DustLaw,
    pub torch_revision: BlueprintRevision,
    pub torch: ExecutableLaw,
}

/// The compatibility simulator uses a distinct complete world contract. It
/// shares stateless adapters, while its device queues/history remain per world.
pub(crate) struct CompatibilityWorldLaws {
    pub context: WorldExecutionContext,
    pub repeater: &'static dustroute_minecraft::repeater_law::CompatibilityRepeaterLaw,
    pub comparator: &'static dustroute_minecraft::comparator_law::CompatibilityComparatorLaw,
    pub observer: &'static dustroute_minecraft::observer_law::CompatibilityObserverLaw,
    pub lamp: &'static dustroute_minecraft::lamp_law::CompatibilityLampLaw,
}

pub(crate) fn builtin_compatibility_world_laws() -> &'static CompatibilityWorldLaws {
    static LAWS: OnceLock<CompatibilityWorldLaws> = OnceLock::new();
    LAWS.get_or_init(|| {
        let context = WorldExecutionContext::for_profile(
            WorldExecutionProfile::RedstoneCompatibilityBoundaryV1,
        );
        resolve_law_references(dustroute_library::builtin_laws::builtin_laws(), &context)
            .expect("pinned compatibility world laws");
        // Dust/torch helpers compile these same fixed source programs. No
        // extra history or additional physical callback is created here.
        builtin_world_laws();
        CompatibilityWorldLaws {
            context,
            repeater: dustroute_minecraft::repeater_law::builtin_compatibility_law(),
            comparator: dustroute_minecraft::comparator_law::builtin_comparator_law(),
            observer: dustroute_minecraft::observer_law::builtin_observer_law(),
            lamp: dustroute_minecraft::lamp_law::builtin_compatibility_law(),
        }
    })
}

/// Check exact law dependencies, including requirements on selected law sources.
/// Cyclic declarations do not create recursive execution or duplicate state.
pub(crate) fn check_requirements(
    catalog: &BlueprintCatalog,
    context: &PhysicalBehaviorContext,
    required: &[BlueprintRevisionId],
) -> Result<(), String> {
    check_law_requirements(catalog, &context.execution_context(), required)
}

impl WorldLaws {
    pub fn resolve(
        catalog: &BlueprintCatalog,
        context: &PhysicalBehaviorContext,
    ) -> Result<Self, String> {
        let execution_context = context.execution_context();
        let selected = resolve_law_references(catalog, &execution_context)?;
        dustroute_minecraft::spatial::builtin_spatial_laws();
        let dust = DustLaw::from_revision(selected[&LawRole::DustStrength])?;
        let torch_revision = selected[&LawRole::Torch].clone();
        let program = torch_revision
            .law
            .as_ref()
            .ok_or("torch revision has no law")?;
        if program.inputs != BTreeMap::from([("powered".into(), 1)])
            || program.registers.get("lit").is_none_or(|r| r.maximum != 1)
            || !program.handlers.contains_key("neighbor_update")
        {
            return Err("torch adapter needs a Boolean powered input, lit register and neighbor_update handler".into());
        }
        let torch = program.compile()?;
        if context.profile == PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1 {
            validate_effect_law(program)?;
        }
        Ok(Self {
            dust,
            torch_revision,
            torch,
        })
    }
}

/// This profile has one block callback slot. Neighbor handlers may request it,
/// but may not themselves change the visible block; that would require nested
/// physical effect delivery, which this bounded adapter does not implement.
fn validate_effect_law(program: &LawProgram) -> Result<(), String> {
    fn body_supported(body: &[Instruction], neighbor: bool) -> bool {
        body.iter().all(|instruction| match instruction {
            Instruction::Set { register, .. } => !neighbor || register != "lit",
            Instruction::Remember { .. } => true,
            Instruction::ScheduleIfAbsent { event, .. } => event == "scheduled_tick",
            Instruction::If {
                then, otherwise, ..
            } => body_supported(then, neighbor) && body_supported(otherwise, neighbor),
        })
    }
    if program.handlers.len() != 2
        || !program.handlers.contains_key("scheduled_tick")
        || !program
            .handlers
            .iter()
            .all(|(name, body)| body_supported(body, name == "neighbor_update"))
    {
        return Err("block-effects profile requires neighbor_update/scheduled_tick handlers, one scheduled_tick slot, and no lit writes in neighbor_update".into());
    }
    Ok(())
}
