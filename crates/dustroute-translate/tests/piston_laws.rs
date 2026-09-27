#[path = "support/piston_models.rs"]
mod piston_models;

#[test]
fn piston_plans_moving_carriers_completion_and_events_match_the_pre_migration_model() {
    let expected: Vec<serde_json::Value> = include_str!("fixtures/piston_model_v1.jsonl")
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    let actual = piston_models::capture();
    assert_eq!(actual.len(), 40);
    for (actual, expected) in actual.iter().zip(&expected) {
        assert_eq!(
            actual, expected,
            "{} {} {}",
            actual["facing"], actual["variant"], actual["payload"]
        );
    }
    assert_eq!(actual.len(), expected.len());
}
