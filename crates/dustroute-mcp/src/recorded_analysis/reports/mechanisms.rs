use dustroute_translate::piston_observation::{PistonObservation, PistonObservationState};
use serde::Serialize;
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ObservedMechanism {
    kind: MechanismKind,
    recognition: Recognition,
    contract: Option<&'static str>,
    candidate_assessments: [CandidateAssessment; 1],
    state: Option<PistonObservationState>,
    scope: &'static str,
    mutation_authorized: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MechanismKind {
    PistonDoor,
    UnidentifiedPistonMechanism,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Recognition {
    ExactContractMatch,
    Unidentified,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct CandidateAssessment {
    contract: &'static str,
    observation: PistonObservation,
}
impl ObservedMechanism {
    pub(crate) fn piston(observation: PistonObservation) -> Self {
        let recognized = matches!(
            observation.state,
            PistonObservationState::Open | PistonObservationState::Closed
        );
        Self {
            kind: if recognized {
                MechanismKind::PistonDoor
            } else {
                MechanismKind::UnidentifiedPistonMechanism
            },
            recognition: if recognized {
                Recognition::ExactContractMatch
            } else {
                Recognition::Unidentified
            },
            contract: recognized.then_some("piston_door_v1"),
            state: recognized.then_some(observation.state),
            candidate_assessments: [CandidateAssessment {
                contract: "piston_door_v1",
                observation,
            }],
            scope: "entire_observed_region",
            mutation_authorized: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed_mechanism_preserves_each_state_without_authorizing_mutation() {
        use PistonObservationState::*;
        for state in [
            Open,
            Closed,
            Moving,
            Indeterminate,
            ConfigurationMismatch,
            ObservationIncomplete,
            UnsupportedVersion,
        ] {
            let observation = PistonObservation::unresolved(state, "fixture", "recorded issue");
            let report = ObservedMechanism::piston(observation.clone());
            assert_eq!(report.candidate_assessments[0].observation, observation);
            assert!(!report.mutation_authorized);
            let recognized = matches!(state, Open | Closed);
            assert_eq!(report.state, recognized.then_some(state));
            assert_eq!(report.contract, recognized.then_some("piston_door_v1"));
            let expected = serde_json::json!({
                "kind":if recognized {"piston_door"}else{"unidentified_piston_mechanism"},
                "recognition":if recognized {"exact_contract_match"}else{"unidentified"},
                "contract":recognized.then_some("piston_door_v1"),
                "candidate_assessments":[{"contract":"piston_door_v1","observation":observation}],
                "state":recognized.then_some(state),"scope":"entire_observed_region","mutation_authorized":false,
            });
            assert_eq!(serde_json::to_value(report).unwrap(), expected);
        }
    }
}
