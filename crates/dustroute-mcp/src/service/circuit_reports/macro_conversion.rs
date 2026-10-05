//! Diagnostic views of native macro proposals. A preview patch is report data;
//! the native plan, world and source catalog are never serialized or recovered.
use crate::operations::preview::optimization::{
    OptimizationAssessmentView, OptimizationContractView, VerificationState,
};
use dustroute_optimize::{
    MacroBoundaryDirection, MacroPortRoute, MacroRealizationError, MacroReplacementCandidate,
    MacroReplacementPlan, MacroSteadyStateReport, MacroStructuralReport, MacroTransitionCase,
    MacroTransitionEdge, MacroTransitionReport, MaterializedMacroReplacement, OptimizationContract,
    OptimizationContractAssessment,
};
use dustroute_physical::Pos;
use dustroute_translate::cells::RotationY;
use serde::{Serialize, Serializer, ser::SerializeStruct};

pub(in super::super) struct MacroProposals {
    pub candidates: Vec<MacroReplacementCandidate>,
    pub placement_plans: Vec<MacroPlanReport>,
}
impl Serialize for MacroProposals {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct("MacroProposals", 4)?;
        record.serialize_field("status", "proposal_only")?;
        record.serialize_field("realization", "contextual placement and transition verification are required before a mutation plan can be created")?;
        record.serialize_field(
            "candidates",
            &self.candidates.iter().map(Candidate).collect::<Vec<_>>(),
        )?;
        record.serialize_field("placement_plans", &self.placement_plans)?;
        record.end()
    }
}
struct Candidate<'a>(&'a MacroReplacementCandidate);
impl Serialize for Candidate<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let c = self.0;
        let mut record = serializer.serialize_struct("MacroCandidate", 10)?;
        record.serialize_field("component_id", c.component_id.as_str())?;
        record.serialize_field("name", &c.name)?;
        record.serialize_field("kind", &c.kind)?;
        record.serialize_field("layout_reference", &c.layout_reference)?;
        record.serialize_field("input_ports", &c.input_ports)?;
        record.serialize_field("output_ports", &c.output_ports)?;
        record.serialize_field("physical", &c.physical)?;
        record.serialize_field("saved_blocks", &c.saved_blocks)?;
        record.serialize_field("saved_volume", &c.saved_volume)?;
        record.serialize_field(
            "requires_contextual_transition_verification",
            &c.requires_contextual_transition_verification,
        )?;
        record.end()
    }
}
pub(in super::super) struct MacroPlanReport {
    pub plan: MacroReplacementPlan,
    pub structural: MacroStructuralReport,
    pub materialized: Result<MaterializedMacroReplacement, MacroRealizationError>,
    pub steady_state: Option<MacroSteadyStateReport>,
    pub transitions: Option<MacroTransitionReport>,
    pub contract: OptimizationContract,
    pub contract_assessment: Option<OptimizationContractAssessment>,
}
impl Serialize for MacroPlanReport {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let p = &self.plan;
        let mut record = serializer.serialize_struct("MacroPlanReport", 13)?;
        record.serialize_field("component_id", &p.component_id)?;
        record.serialize_field("origin", &p.placed.origin)?;
        record.serialize_field("rotation_y", &Rotation(p.placed.rotation))?;
        record.serialize_field("total_route_length", &p.total_route_length)?;
        record.serialize_field("automatic_apply_allowed", &p.automatic_apply_allowed)?;
        record.serialize_field("contract", &OptimizationContractView(self.contract))?;
        record.serialize_field(
            "contract_assessment",
            &self
                .contract_assessment
                .as_ref()
                .map(|a| OptimizationAssessmentView(a.clone())),
        )?;
        record.serialize_field("structural_report", &Structure(&self.structural))?;
        record.serialize_field("materialization", &Materialization(&self.materialized))?;
        record.serialize_field(
            "steady_state_report",
            &self.steady_state.as_ref().map(Steady),
        )?;
        record.serialize_field(
            "transition_report",
            &self.transitions.as_ref().map(Transitions),
        )?;
        record.serialize_field("verification", &Verification(&p.verification))?;
        record.serialize_field("routes", &p.routes.iter().map(Route).collect::<Vec<_>>())?;
        record.end()
    }
}
struct Rotation(RotationY);
impl Serialize for Rotation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self.0 {
            RotationY::R0 => "r0",
            RotationY::R90 => "r90",
            RotationY::R180 => "r180",
            RotationY::R270 => "r270",
        })
    }
}
struct Structure<'a>(&'a MacroStructuralReport);
impl Serialize for Structure<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Contact {
            first_route: usize,
            second_route: usize,
            first_position: Pos,
            second_position: Pos,
        }
        let r = self.0;
        let mut record = serializer.serialize_struct("MacroStructure", 7)?;
        record.serialize_field("valid", &r.valid())?;
        record.serialize_field("candidate_collisions", &r.candidate_collisions)?;
        record.serialize_field("route_collisions", &r.route_collisions)?;
        record.serialize_field(
            "route_cross_net_contacts",
            &r.route_cross_net_contacts
                .iter()
                .map(
                    |&(first_route, second_route, first_position, second_position)| Contact {
                        first_route,
                        second_route,
                        first_position,
                        second_position,
                    },
                )
                .collect::<Vec<_>>(),
        )?;
        record.serialize_field("candidate_support_issues", &r.candidate_support_issues)?;
        record.serialize_field("required_route_supports", &r.required_route_supports)?;
        record.serialize_field("blocked_route_supports", &r.blocked_route_supports)?;
        record.end()
    }
}
struct Materialization<'a>(&'a Result<MaterializedMacroReplacement, MacroRealizationError>);
impl Serialize for Materialization<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Ok(r) => {
                let mut record = serializer.serialize_struct("Materialization", 5)?;
                record.serialize_field("status", "preview_ready")?;
                record.serialize_field("change_count", &r.patch.changes.len())?;
                record.serialize_field("added_supports", &r.added_supports)?;
                record.serialize_field("inserted_repeaters", &r.inserted_repeaters)?;
                record.serialize_field("patch", &r.patch)?;
                record.end()
            }
            Err(error) => {
                let mut record = serializer.serialize_struct("Materialization", 2)?;
                record.serialize_field("status", "unavailable")?;
                record.serialize_field("reason", &format!("{error:?}"))?;
                record.end()
            }
        }
    }
}
struct Steady<'a>(&'a MacroSteadyStateReport);
impl Serialize for Steady<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let r = self.0;
        let mut record = serializer.serialize_struct("MacroSteadyState", 6)?;
        record.serialize_field("state", &VerificationState(r.state))?;
        record.serialize_field("comparison", &r.comparison)?;
        record.serialize_field("input_mapping", &r.input_mapping)?;
        record.serialize_field("output_mapping", &r.output_mapping)?;
        record.serialize_field("differing_assignments", &r.differing_assignments)?;
        record.serialize_field("reason", &r.reason)?;
        record.end()
    }
}
struct Transitions<'a>(&'a MacroTransitionReport);
impl Serialize for Transitions<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let r = self.0;
        let mut record = serializer.serialize_struct("MacroTransitions", 6)?;
        record.serialize_field("state", &VerificationState(r.state))?;
        record.serialize_field("reason_code", &r.unavailable_reason)?;
        record.serialize_field("case_count", &r.cases.len())?;
        record.serialize_field("differing_cases", &r.differing_cases)?;
        record.serialize_field("reason", &r.reason)?;
        record.serialize_field("cases", &r.cases.iter().map(Case).collect::<Vec<_>>())?;
        record.end()
    }
}
struct Case<'a>(&'a MacroTransitionCase);
impl Serialize for Case<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let c = self.0;
        let mut record = serializer.serialize_struct("MacroTransitionCase", 8)?;
        record.serialize_field("from", &c.from)?;
        record.serialize_field("to", &c.to)?;
        record.serialize_field("equivalent", &c.equivalent)?;
        record.serialize_field("first_difference_tick", &c.first_difference_tick)?;
        record.serialize_field(
            "original_transitions",
            &c.original_transition_edges()
                .iter()
                .map(Edge)
                .collect::<Vec<_>>(),
        )?;
        record.serialize_field(
            "candidate_transitions",
            &c.candidate_transition_edges()
                .iter()
                .map(Edge)
                .collect::<Vec<_>>(),
        )?;
        record.serialize_field("original_outputs", &c.original_outputs)?;
        record.serialize_field("candidate_outputs", &c.candidate_outputs)?;
        record.end()
    }
}
struct Edge<'a>(&'a MacroTransitionEdge);
impl Serialize for Edge<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let e = self.0;
        let mut record = serializer.serialize_struct("MacroTransitionEdge", 4)?;
        record.serialize_field("at_tick", &e.at_tick)?;
        record.serialize_field("from", &e.from)?;
        record.serialize_field("to", &e.to)?;
        record.serialize_field("elapsed_from_previous", &e.elapsed_from_previous)?;
        record.end()
    }
}
struct Verification<'a>(&'a dustroute_optimize::MacroRealizationVerification);
impl Serialize for Verification<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let v = self.0;
        let mut record = serializer.serialize_struct("MacroVerification", 3)?;
        record.serialize_field("structural", &VerificationState(v.structural))?;
        record.serialize_field("steady_state", &VerificationState(v.steady_state))?;
        record.serialize_field("transitions", &VerificationState(v.transitions))?;
        record.end()
    }
}
struct Route<'a>(&'a MacroPortRoute);
impl Serialize for Route<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let r = self.0;
        let mut record = serializer.serialize_struct("MacroRoute", 8)?;
        record.serialize_field(
            "direction",
            match r.boundary.direction {
                MacroBoundaryDirection::Input => "input",
                MacroBoundaryDirection::Output => "output",
            },
        )?;
        record.serialize_field("observed_index", &r.boundary.observed_index)?;
        record.serialize_field("boundary_position", &r.boundary.position)?;
        record.serialize_field("boundary_facing", &r.boundary.facing)?;
        record.serialize_field("driver_position", &r.boundary.driver_position)?;
        record.serialize_field("candidate_port", &r.candidate_port)?;
        record.serialize_field("candidate_position", &r.candidate_position)?;
        record.serialize_field("path", &r.path)?;
        record.end()
    }
}
