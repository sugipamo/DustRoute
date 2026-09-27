//! The finite dust-level law is evaluated from its Blueprint program. Caching
//! its complete 16 x 16 domain avoids interpreting JSON-level names on every
//! electrical iteration; no attenuation formula is duplicated in this adapter.
use dustroute_library::blueprint::BlueprintRevision;
use dustroute_minecraft::dust_law::DustStrengthLaw;

#[derive(Clone, Debug)]
pub struct DustLaw {
    revision: BlueprintRevision,
    law: DustStrengthLaw,
}

impl DustLaw {
    pub fn from_revision(revision: &BlueprintRevision) -> Result<Self, String> {
        let program = revision
            .law
            .as_ref()
            .ok_or("selected dust revision has no law")?;
        Ok(Self {
            revision: revision.clone(),
            law: DustStrengthLaw::compile(program)?,
        })
    }

    pub fn revision(&self) -> &BlueprintRevision {
        &self.revision
    }

    /// Inputs are maxima of already connected sources, clamped to the existing
    /// electrical model's 0..=15 domain at its physical observation boundary.
    pub fn strength(&self, direct: u8, neighbor: u8) -> u8 {
        self.law
            .strength(direct.min(15), neighbor.min(15))
            .expect("legacy clamped dust facts")
    }
}

pub fn builtin_dust_law() -> &'static DustLaw {
    &crate::world_laws::builtin_world_laws().dust
}
