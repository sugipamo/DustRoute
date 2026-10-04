//! Owned report data. None of these records restore a model, observation, or execution proof.
mod common;
mod flat;
mod hierarchical;
mod mechanisms;
mod truth;
pub(crate) use common::{FocusedComponent, focused_component, focused_hierarchy};
pub(crate) use flat::{ReverseAnalysisReport, reverse_report};
pub(crate) use hierarchical::{HierarchicalAnalysisReport, hierarchical_report};
pub(crate) use mechanisms::ObservedMechanism;
pub(crate) const MAX_FLAT_ANALYSIS_COMPONENTS: usize = 512;

#[cfg(test)]
pub(crate) mod tests;
