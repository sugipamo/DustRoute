//! Bounded authoring of flying machines through ordinary Blueprint contracts.
//! Recipes supply data. Every candidate uses the shared physical verifier and
//! construction/operating-reference/removal paths; no flight-specific runtime.
mod definitions;
mod recipe;
mod sources;

use crate::behavior_type::BehaviorBudget;
use crate::blueprint_update::{BlueprintUpdateRequest, BlueprintUpdates, RecordedReview};
use crate::piston_construction::ElectricalConstruction;
use crate::promotion::CheckStatus;
use crate::{MinecraftSnapshot, Pos};
use dustroute_library::assembly::AssemblyRevision;
use dustroute_library::blueprint::{BlueprintCatalog, BlueprintRevision, TypeRevision};
use dustroute_library::flying_machine::FlyingMachineRequest;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct FlyingMachineRecords {
    pub types: Vec<TypeRevision>,
    pub revisions: Vec<BlueprintRevision>,
    pub assemblies: Vec<AssemblyRevision>,
}
impl FlyingMachineRecords {
    pub fn catalog(&self) -> Result<BlueprintCatalog, String> {
        let mut catalog = BlueprintCatalog::default();
        for definition in &self.types {
            catalog
                .insert_type(definition.clone())
                .map_err(|e| e.to_string())?;
        }
        catalog
            .insert_revisions(self.revisions.clone())
            .map_err(|e| e.to_string())?;
        for assembly in &self.assemblies {
            catalog
                .insert_assembly(assembly.clone())
                .map_err(|e| e.to_string())?;
        }
        Ok(catalog)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FlyingMachineVerification {
    pub status: CheckStatus,
    pub detail: String,
    pub review: Option<RecordedReview>,
    pub construction_steps: usize,
    pub removal_steps: usize,
    pub live_world_verified: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct GeneratedFlyingMachine {
    pub specification: FlyingMachineRequest,
    /// Import these unadopted records, then propose request through the normal API.
    pub records: FlyingMachineRecords,
    pub request: BlueprintUpdateRequest,
    pub context: RuntimeBehaviorContext,
    pub initial: MinecraftSnapshot,
    pub expected_arrival: MinecraftSnapshot,
    pub moving_positions: Vec<Pos>,
    /// Declared crops must disappear; any final occupant is defined by expected_arrival.
    pub destroyed_positions: Vec<Pos>,
    pub displacement: Pos,
    pub verification: FlyingMachineVerification,
}

/// Returns candidate data and fresh checks without publishing, adopting or
/// modifying a world. A failed/undetermined candidate is never a usable pass.
pub fn generate_flying_machine(
    specification: FlyingMachineRequest,
    budget: BehaviorBudget,
) -> Result<GeneratedFlyingMachine, String> {
    let recipe = recipe::expand(&specification)?;
    let (records, request) = sources::build(&specification, &recipe)?;
    let mut result = GeneratedFlyingMachine {
        specification,
        records,
        request,
        context: recipe.context,
        initial: recipe.initial,
        expected_arrival: recipe.arrival,
        moving_positions: recipe.moving,
        destroyed_positions: recipe.destroyed,
        displacement: recipe.delta,
        verification: FlyingMachineVerification {
            status: CheckStatus::Undetermined,
            detail: String::new(),
            review: None,
            construction_steps: 0,
            removal_steps: 0,
            live_world_verified: false,
        },
    };
    result.verification = verify(&result, budget)?;
    Ok(result)
}

fn verify(
    candidate: &GeneratedFlyingMachine,
    budget: BehaviorBudget,
) -> Result<FlyingMachineVerification, String> {
    let mut updates = BlueprintUpdates::new(candidate.records.catalog()?);
    updates
        .create(candidate.request.clone())
        .map_err(|e| e.to_string())?;
    // Use the caller's bounded review directly. Adoption independently repeats
    // all requirements against the concrete candidate after import/proposal.
    let mut catalog = candidate.records.catalog()?;
    catalog
        .insert_revisions(candidate.request.revisions.clone())
        .map_err(|e| e.to_string())?;
    let report = crate::promotion::review_assembly_with_context(
        &catalog,
        &candidate.request.candidate_state.assembly,
        Some(&candidate.context.clone().into()),
        budget,
    )
    .map_err(|e| e.to_string())?;
    let mut result = FlyingMachineVerification {
        status: report.status(),
        detail: "fresh declared single-operation review".into(),
        review: Some(RecordedReview::from(&report)),
        construction_steps: 0,
        removal_steps: 0,
        live_world_verified: false,
    };
    if result.status != CheckStatus::Passed {
        return Ok(result);
    }
    let checked = (|| -> Result<bool, String> {
        let world = candidate
            .request
            .candidate_state
            .assembly
            .inspect(&catalog)
            .map_err(|e| e.to_string())?
            .proposed_world();
        let construction = ElectricalConstruction::new(
            &world,
            candidate.context.known_region,
            candidate.context.root_limits,
        )?;
        if construction.settled() != &candidate.initial {
            return Ok(false);
        }
        result.construction_steps = construction.build_steps().len();
        let (arrival, remove) = construction.operating_removal(
            &[(candidate.context.input_levers[0], true)],
            candidate.context.root_limits,
        )?;
        if arrival != candidate.expected_arrival {
            return Ok(false);
        }
        result.removal_steps = remove.len();
        Ok(remove.last().is_some_and(|s| s.expected.blocks.is_empty()))
    })();
    match checked {
        Ok(true)=>result.detail="declared single-operation contract, exact native arrival, staged construction and arrival removal passed in the model; import/proposal/adoption and target revalidation are still required".into(),
        Ok(false)=>{result.status=CheckStatus::Failed;result.detail="constructed state, exact native arrival or removal differs from the declared geometry".into();},
        Err(error)=>{result.status=CheckStatus::Undetermined;result.detail=format!("construction/arrival/removal could not be established: {error}");},
    }
    Ok(result)
}
