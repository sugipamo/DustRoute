//! Executable laws use the same immutable Blueprint records as layouts. This
//! catalog is separate only as an asset; callers can import it into any catalog.
use std::sync::OnceLock;

use crate::blueprint::{BlueprintCatalog, BlueprintRevision, BlueprintRevisionId};

pub use dustroute_minecraft::dust_law::DUST_LAW_REVISION;
pub use dustroute_minecraft::execution_context::TORCH_LAW_REVISION;

/// Spatial programs are fixed by the existing world/placement profiles. They
/// are implicit in old serialized contexts; no historical record is rewritten.
pub fn spatial_law_revisions() -> &'static [BlueprintRevisionId; 4] {
    static IDS: OnceLock<[BlueprintRevisionId; 4]> = OnceLock::new();
    IDS.get_or_init(|| {
        dustroute_minecraft::spatial::SPATIAL_LAW_IDS
            .map(|id| BlueprintRevisionId::new(id).expect("fixed spatial law ID"))
    })
}

pub fn builtin_laws() -> &'static BlueprintCatalog {
    static CATALOG: OnceLock<BlueprintCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut catalog =
            BlueprintCatalog::from_json(include_str!("../blueprints/torch-law-v1.json"))
                .expect("embedded physical laws are structurally valid");
        let dust = BlueprintCatalog::from_json(dustroute_minecraft::dust_law::BLUEPRINT_JSON)
            .expect("embedded dust law is structurally valid");
        catalog
            .insert_revisions(dust.revisions().cloned().collect())
            .expect("distinct immutable laws");
        // The programs live below the catalog so placement and the bounded
        // world runner can execute the exact same data without a crate cycle.
        let template = dust_law_template(&dust);
        for (id, program) in dustroute_minecraft::spatial::SPATIAL_LAW_IDS
            .into_iter()
            .zip(dustroute_minecraft::spatial::builtin_programs())
            .chain(
                dustroute_minecraft::repeater_law::REPEATER_LAW_IDS
                    .into_iter()
                    .zip(dustroute_minecraft::repeater_law::builtin_programs()),
            )
            .chain(std::iter::once((
                dustroute_minecraft::comparator_law::COMPARATOR_LAW_REVISION,
                dustroute_minecraft::comparator_law::builtin_program(),
            )))
            .chain(std::iter::once((
                dustroute_minecraft::observer_law::OBSERVER_LAW_REVISION,
                dustroute_minecraft::observer_law::builtin_program(),
            )))
            .chain(
                dustroute_minecraft::lamp_law::LAMP_LAW_IDS
                    .into_iter()
                    .zip(dustroute_minecraft::lamp_law::builtin_programs()),
            )
            .chain(std::iter::once((
                dustroute_minecraft::device_callback_law::COMPARATOR_SIGNAL_ID,
                dustroute_minecraft::device_callback_law::comparator_signal_program(),
            )))
            .chain(
                dustroute_minecraft::device_callback_law::LAW_IDS
                    .into_iter()
                    .zip(dustroute_minecraft::device_callback_law::builtin_programs()),
            )
            .chain(
                dustroute_minecraft::piston_law::PISTON_LAW_IDS
                    .into_iter()
                    .zip(dustroute_minecraft::piston_law::builtin_programs()),
            )
            .chain(
                dustroute_minecraft::piston_motion_law::LAW_IDS
                    .into_iter()
                    .zip(dustroute_minecraft::piston_motion_law::builtin_programs()),
            )
            .chain(
                dustroute_minecraft::piston_electrical_law::LAW_IDS
                    .into_iter()
                    .zip(dustroute_minecraft::piston_electrical_law::builtin_programs()),
            )
            .chain(std::iter::once((
                dustroute_minecraft::piston_law::ELECTRICAL_PAYLOAD_LAW,
                dustroute_minecraft::piston_law::electrical_payload_program(),
            )))
        {
            let mut source = template.clone();
            source.id = BlueprintRevisionId::new(id).expect("fixed world law ID");
            source.name = id.into();
            source.law = Some(program.clone());
            catalog
                .insert_revision(source)
                .expect("distinct immutable world law");
        }
        catalog
    })
}

fn dust_law_template(catalog: &BlueprintCatalog) -> &BlueprintRevision {
    catalog
        .revision(&BlueprintRevisionId::new(DUST_LAW_REVISION).expect("fixed ID"))
        .expect("embedded memoryless law metadata")
}

pub fn dust_law_revision() -> &'static BlueprintRevision {
    builtin_laws()
        .revision(&BlueprintRevisionId::new(DUST_LAW_REVISION).expect("fixed ID"))
        .expect("embedded dust law")
}

pub fn torch_law_revision() -> &'static BlueprintRevision {
    builtin_laws()
        .revision(&BlueprintRevisionId::new(TORCH_LAW_REVISION).expect("fixed ID"))
        .expect("embedded torch law")
}
