//! Produce an unadopted, freshly checked flight candidate from typed JSON data.
use dustroute_library::flying_machine::FlyingMachineRequest;
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::flying_machine::generate_flying_machine;
use dustroute_translate::promotion::CheckStatus;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: generate_flying_machine REQUEST.json")?;
    let request: FlyingMachineRequest = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let generated = generate_flying_machine(request, BehaviorBudget::default())?;
    let mut request = serde_json::to_value(&generated.request)?;
    request
        .as_object_mut()
        .ok_or("request object required")?
        .remove("id");
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "records":generated.records,"request":request,"generation_request":generated.specification,
            "generation_verification":generated.verification,"expected_arrival":generated.expected_arrival,
            "finite_flight":{"distance":generated.specification.distance,
                "displacement":generated.displacement,"moving_positions":generated.moving_positions,
                "destroyed_positions":generated.destroyed_positions,
                "wait_ticks":u64::from(generated.specification.distance)*12+40}
        }))?
    );
    if generated.verification.status != CheckStatus::Passed {
        return Err(generated.verification.detail.into());
    }
    Ok(())
}
