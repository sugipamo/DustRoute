use super::*;

#[must_use]
pub fn derive_local_logic(result: &ReverseResult) -> LogicalRole {
    let Some(table) = &result.truth_table else {
        return LogicalRole {
            classification: FunctionalClassification::Unknown,
            output_functions: Vec::new(),
            input_count: result.analysis.inputs.len(),
            output_count: result.analysis.outputs.len(),
            basis: "physical_evidence_only".to_owned(),
            reason: Some(
                result
                    .truth_table_error
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "truth-table inference was not requested".to_owned()),
            ),
        };
    };
    classify_truth_table(table)
}

#[must_use]
pub fn classify_focused_role(result: &ReverseResult, target: Pos) -> FocusedRole {
    let physical_component = result
        .analysis
        .scene
        .component_at(target)
        .map(|component| component.id);
    let component = result
        .analysis
        .components
        .iter()
        .find(|component| component.positions.contains(&target));
    let Some(component) = component else {
        return FocusedRole {
            position: target,
            physical_component,
            signal_component: None,
            incoming_components: BTreeSet::new(),
            outgoing_components: BTreeSet::new(),
            role: LocalSignalRole::SupportOrUnresolved,
        };
    };
    let is_input = result
        .analysis
        .inputs
        .iter()
        .any(|item| item.component == component.id);
    let is_output = result
        .analysis
        .outputs
        .iter()
        .any(|item| item.component == component.id);
    let role = if is_input {
        LocalSignalRole::InputBoundary
    } else if is_output {
        LocalSignalRole::OutputBoundary
    } else if component.incoming.len() > 1 {
        LocalSignalRole::SignalMerge
    } else if component.outgoing.len() > 1 {
        LocalSignalRole::SignalBranch
    } else if component.incoming.contains(&component.id)
        || component.outgoing.contains(&component.id)
    {
        LocalSignalRole::FeedbackPath
    } else {
        LocalSignalRole::IntermediatePath
    };
    FocusedRole {
        position: target,
        physical_component,
        signal_component: Some(component.id),
        incoming_components: component.incoming.clone(),
        outgoing_components: component.outgoing.clone(),
        role,
    }
}

#[must_use]
pub fn classify_truth_table(table: &InferredTruthTable) -> LogicalRole {
    let columns = (0..table.outputs.len())
        .map(|index| {
            table
                .rows
                .iter()
                .map(|row| row.outputs[index])
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let functions = columns
        .iter()
        .map(|column| classify_boolean_column(column))
        .collect::<Vec<_>>();
    let classification = match (table.inputs.len(), functions.as_slice()) {
        (
            2,
            [BooleanFunction::Xor, BooleanFunction::And]
            | [BooleanFunction::And, BooleanFunction::Xor],
        ) => FunctionalClassification::HalfAdder,
        (3, functions) if functions.len() == 2 => {
            let parity = columns
                .iter()
                .any(|column| column == &[false, true, true, false, true, false, false, true]);
            let majority = columns
                .iter()
                .any(|column| column == &[false, false, false, true, false, true, true, true]);
            if parity && majority {
                FunctionalClassification::FullAdder
            } else {
                FunctionalClassification::Unclassified
            }
        }
        (_, [function]) => function_to_classification(*function),
        _ => FunctionalClassification::Unclassified,
    };
    LogicalRole {
        classification,
        output_functions: functions,
        input_count: table.inputs.len(),
        output_count: table.outputs.len(),
        basis: "inferred_truth_table".to_owned(),
        reason: None,
    }
}

fn classify_boolean_column(values: &[bool]) -> BooleanFunction {
    match values {
        [false, true] => BooleanFunction::Buffer,
        [true, false] => BooleanFunction::Not,
        [false, false, false, true] => BooleanFunction::And,
        [false, true, true, true] => BooleanFunction::Or,
        [false, true, true, false] => BooleanFunction::Xor,
        [true, true, true, false] => BooleanFunction::Nand,
        [true, false, false, false] => BooleanFunction::Nor,
        [true, false, false, true] => BooleanFunction::Xnor,
        _ => BooleanFunction::Unclassified,
    }
}

const fn function_to_classification(function: BooleanFunction) -> FunctionalClassification {
    match function {
        BooleanFunction::Buffer => FunctionalClassification::Buffer,
        BooleanFunction::Not => FunctionalClassification::Not,
        BooleanFunction::And => FunctionalClassification::And,
        BooleanFunction::Or => FunctionalClassification::Or,
        BooleanFunction::Xor => FunctionalClassification::Xor,
        BooleanFunction::Nand => FunctionalClassification::Nand,
        BooleanFunction::Nor => FunctionalClassification::Nor,
        BooleanFunction::Xnor => FunctionalClassification::Xnor,
        BooleanFunction::Unclassified => FunctionalClassification::Unclassified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_primitive_boolean_columns() {
        assert_eq!(
            classify_boolean_column(&[false, false, false, true]),
            BooleanFunction::And
        );
        assert_eq!(
            classify_boolean_column(&[false, true, true, true]),
            BooleanFunction::Or
        );
        assert_eq!(
            classify_boolean_column(&[false, true, true, false]),
            BooleanFunction::Xor
        );
        assert_eq!(
            classify_boolean_column(&[true, false]),
            BooleanFunction::Not
        );
        assert_eq!(
            classify_boolean_column(&[false, false, true, false]),
            BooleanFunction::Unclassified
        );
    }
}
