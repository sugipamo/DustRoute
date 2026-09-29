use super::*;

#[must_use]
pub fn verify_semantic_equivalence(
    expected: &PhysicalAnalysis,
    actual: &PhysicalAnalysis,
) -> SemanticEquivalence {
    match (&expected.reverse.truth_table, &actual.reverse.truth_table) {
        (Some(expected), Some(actual)) => {
            let comparison = compare_truth_tables(expected, actual);
            SemanticEquivalence {
                equivalent: comparison.comparable && comparison.fitness_penalty == 0,
                reason: if comparison.comparable && comparison.fitness_penalty == 0 {
                    "truth tables are identical".to_owned()
                } else {
                    "truth tables differ or have incompatible terminals".to_owned()
                },
                comparison: Some(comparison),
            }
        }
        _ => SemanticEquivalence {
            equivalent: false,
            comparison: None,
            reason: "both analyses require inferred truth tables".to_owned(),
        },
    }
}
