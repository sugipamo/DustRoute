//! Executable laws use the same immutable Blueprint records as layouts. This
//! catalog is separate only as an asset; callers can import it into any catalog.
use std::sync::OnceLock;

use crate::Provenance;
use crate::blueprint::{BlueprintCatalog, BlueprintRevision, BlueprintRevisionId};
use dustroute_minecraft::law::LawProgram;

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
        let mut catalog = BlueprintCatalog::default();
        let template = revision(
            DUST_LAW_REVISION,
            "Dust signal combination and attenuation",
            "DustRoute: existing electrical model and differential fixtures",
            dustroute_minecraft::dust_law::builtin_program().clone(),
        );
        catalog
            .insert_revision(template.clone())
            .expect("fixed dust law metadata");
        catalog
            .insert_revision(revision(
                TORCH_LAW_REVISION,
                "Torch scheduled inversion, burnout and recovery (Java 1.21.11)",
                "DustRoute: frozen server block-state observations and pinned 1.21.11 runtime inspection",
                dustroute_minecraft::law::builtins::torch::TORCH.program(),
            ))
            .expect("fixed torch law metadata");
        // Programs live below the catalog; immutable metadata belongs here.
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

fn revision(id: &str, name: &str, author: &str, law: LawProgram) -> BlueprintRevision {
    BlueprintRevision {
        id: BlueprintRevisionId::new(id).expect("fixed law ID"),
        parents: vec![],
        name: name.into(),
        classifications: vec![],
        behavior_bindings: vec![],
        static_type_bindings: vec![],
        required_laws: vec![],
        blocks: vec![],
        initial_layout: None,
        law: Some(law),
        inclusions: vec![],
        ports: vec![],
        connections: vec![],
        port_bindings: vec![],
        provenance: Provenance {
            author: author.into(),
            source_url: None,
            license: None,
            retrieved_on: Some("2026-09-20".into()),
        },
    }
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
