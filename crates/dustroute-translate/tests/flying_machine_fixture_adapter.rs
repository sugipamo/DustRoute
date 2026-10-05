//! Test-only authoring data for independent public-workflow trials.
//! Generation results are unadopted data, never retained live evidence.
use dustroute_library::flying_machine::FlyingMachineRequest;
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::flying_machine::generate_flying_machine;
use dustroute_translate::promotion::CheckStatus;

#[test]
#[ignore = "explicit fixture export; requires absolute DUSTROUTE_FLIGHT_INPUT/OUTPUT paths"]
fn export_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::var("DUSTROUTE_FLIGHT_INPUT")?;
    let output = std::env::var("DUSTROUTE_FLIGHT_OUTPUT")?;
    let request: FlyingMachineRequest = serde_json::from_slice(&std::fs::read(input)?)?;
    let generated = generate_flying_machine(request, BehaviorBudget::default())?;
    // Only the test's final MCP request fixture omits its local proposal ID.
    let mut request = serde_json::to_value(&generated.request)?;
    request
        .as_object_mut()
        .ok_or("request object required")?
        .remove("id");
    let record = serde_json::json!({
        "records":generated.records,"request":request,"generation_request":generated.specification,
        "generation_verification":generated.verification,"expected_arrival":generated.expected_arrival,
        "finite_flight":{"distance":generated.specification.distance,
            "displacement":generated.displacement,"moving_positions":generated.moving_positions,
            "destroyed_positions":generated.destroyed_positions,
            "wait_ticks":u64::from(generated.specification.distance)*12+40}
    });
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    serde_json::to_writer_pretty(file, &record)?;
    if generated.verification.status != CheckStatus::Passed {
        return Err(generated.verification.detail.into());
    }
    Ok(())
}
