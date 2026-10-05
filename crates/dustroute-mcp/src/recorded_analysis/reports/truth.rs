use dustroute_translate::world_reverse::TruthTableError;
use serde::{Serialize, Serializer, ser::SerializeStruct};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TruthDiagnostics {
    pub status: TruthStatus,
    pub error: Option<TruthTableError>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TruthStatus {
    Computed,
    BudgetExceeded,
    Unavailable,
    NotRequested,
}
impl TruthDiagnostics {
    pub fn new(translated: &dustroute_translate::api::ReverseResult) -> Self {
        let status = if translated.truth_table.is_some() {
            TruthStatus::Computed
        } else {
            match &translated.truth_table_error {
                Some(
                    TruthTableError::BudgetExceeded { .. }
                    | TruthTableError::RuntimeBudgetExceeded { .. }
                    | TruthTableError::ElapsedBudgetExceeded { .. },
                ) => TruthStatus::BudgetExceeded,
                Some(_) => TruthStatus::Unavailable,
                None => TruthStatus::NotRequested,
            }
        };
        Self {
            status,
            error: translated.truth_table_error.clone(),
        }
    }
}
impl Serialize for TruthDiagnostics {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct("TruthDiagnostics", 3)?;
        record.serialize_field("truth_table_status", &self.status)?;
        record.serialize_field("truth_table_error", &self.error.as_ref().map(ErrorMessage))?;
        record.serialize_field(
            "truth_table_error_details",
            &self.error.as_ref().map(ErrorDetails),
        )?;
        record.end()
    }
}
struct ErrorMessage<'a>(&'a TruthTableError);
impl Serialize for ErrorMessage<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self.0)
    }
}
/// Preserve u128 internally and the original number-or-decimal-text API boundary.
struct LargeCount(u128);
impl Serialize for LargeCount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if !serializer.is_human_readable() {
            return serializer.serialize_u128(self.0);
        }
        match u64::try_from(self.0) {
            Ok(n) => serializer.serialize_u64(n),
            Err(_) => serializer.collect_str(&self.0),
        }
    }
}
struct ErrorDetails<'a>(&'a TruthTableError);
impl Serialize for ErrorDetails<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use TruthTableError::*;
        let (code, fields) = match self.0 {
            BudgetExceeded { .. } => ("budget_exceeded", 6),
            RuntimeBudgetExceeded { .. } => ("runtime_budget_exceeded", 6),
            ElapsedBudgetExceeded { .. } => ("elapsed_budget_exceeded", 6),
            NonSettling { .. } => ("non_settling", 5),
            TooManyInputs(_) => ("too_many_inputs", 3),
            IncompleteObservation => ("incomplete_observation", 2),
            NoInputs => ("no_inputs", 2),
            NoOutputs => ("no_outputs", 2),
            UnmappedExternalInputs(_) => ("unmapped_external_inputs", 3),
            UnmappedObservableOutputs(_) => ("unmapped_observable_outputs", 3),
            AmbiguousInputMapping { .. } => ("ambiguous_input_mapping", 4),
            AmbiguousOutputMapping { .. } => ("ambiguous_output_mapping", 4),
            NoDriverPosition(_) => ("no_driver_position", 3),
            InvalidDriver { .. } => ("invalid_driver", 5),
            Simulation(_) => ("simulation_error", 2),
        };
        let mut record = serializer.serialize_struct("TruthTableErrorDetails", fields)?;
        record.serialize_field("code", code)?;
        match self.0 {
            Simulation(message) => record.serialize_field("message", message)?,
            _ => record.serialize_field("message", &ErrorMessage(self.0))?,
        }
        match self.0 {
            BudgetExceeded {
                rows,
                max_rows,
                estimated_work_units,
                max_work_units,
            } => {
                record.serialize_field("rows", rows)?;
                record.serialize_field("max_rows", max_rows)?;
                record
                    .serialize_field("estimated_work_units", &LargeCount(*estimated_work_units))?;
                record.serialize_field("max_work_units", &LargeCount(*max_work_units))?;
            }
            RuntimeBudgetExceeded {
                rows,
                completed_rows,
                solver_iterations,
                max_solver_iterations,
            } => {
                record.serialize_field("rows", rows)?;
                record.serialize_field("completed_rows", completed_rows)?;
                record.serialize_field("solver_iterations", solver_iterations)?;
                record.serialize_field("max_solver_iterations", max_solver_iterations)?;
            }
            ElapsedBudgetExceeded {
                rows,
                completed_rows,
                elapsed_millis,
                max_elapsed_millis,
            } => {
                record.serialize_field("rows", rows)?;
                record.serialize_field("completed_rows", completed_rows)?;
                record.serialize_field("elapsed_millis", &LargeCount(*elapsed_millis))?;
                record.serialize_field("max_elapsed_millis", max_elapsed_millis)?;
            }
            NonSettling {
                row,
                settle_ticks,
                pending_events,
            } => {
                record.serialize_field("row", row)?;
                record.serialize_field("settle_ticks", settle_ticks)?;
                record.serialize_field("pending_events", pending_events)?;
            }
            TooManyInputs(n) => record.serialize_field("input_count", n)?,
            UnmappedExternalInputs(p) | UnmappedObservableOutputs(p) => {
                record.serialize_field("positions", p)?
            }
            AmbiguousInputMapping {
                external_inputs,
                inferred_inputs,
            } => {
                record.serialize_field("external_input_count", external_inputs)?;
                record.serialize_field("inferred_input_count", inferred_inputs)?;
            }
            AmbiguousOutputMapping {
                observable_outputs,
                inferred_outputs,
            } => {
                record.serialize_field("observable_output_count", observable_outputs)?;
                record.serialize_field("inferred_output_count", inferred_outputs)?;
            }
            NoDriverPosition(p) => record.serialize_field("position", p)?,
            InvalidDriver {
                position,
                expected,
                actual,
            } => {
                record.serialize_field("position", position)?;
                record.serialize_field("expected", expected)?;
                record.serialize_field("actual", actual)?;
            }
            IncompleteObservation | NoInputs | NoOutputs | Simulation(_) => {}
        }
        record.end()
    }
}

/// Reuse the analysis failure projection for other model-only MCP reports.
/// The owned source enum retains its numeric and positional evidence.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TruthErrorView(pub TruthTableError);
impl Serialize for TruthErrorView {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ErrorDetails(&self.0).serialize(serializer)
    }
}
