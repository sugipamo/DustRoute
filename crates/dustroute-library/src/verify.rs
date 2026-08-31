use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fmt::{Display, Formatter};

use dustroute_ir::{DagBuilder, GateKind, LogicDag};

use crate::{ComponentKind, LogicalSpec};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalVerification {
    pub exhaustive: bool,
    pub cases_checked: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationError {
    StatefulUnsupported,
    InvalidPort(String),
    InvalidRowWidth { expected: usize, actual: usize },
    IncompleteTruthTable { expected: usize, actual: usize },
    DuplicateInputCase,
    BehaviorMismatch { row: usize },
    InvalidDag(String),
}

impl Display for VerificationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for VerificationError {}

pub fn verify_logical_spec(
    kind: ComponentKind,
    spec: &LogicalSpec,
) -> Result<LogicalVerification, VerificationError> {
    if spec.stateful {
        return Err(VerificationError::StatefulUnsupported);
    }
    if spec
        .ports
        .iter()
        .any(|port| port.name.is_empty() || port.bit_width != 1)
    {
        return Err(VerificationError::InvalidPort(
            "ports must have a name and bit width one".into(),
        ));
    }
    let input_names = spec.input_names();
    let output_names = spec.output_names();
    let unique_names = spec
        .ports
        .iter()
        .map(|port| port.name.as_str())
        .collect::<BTreeSet<_>>();
    if unique_names.len() != spec.ports.len() {
        return Err(VerificationError::InvalidPort(
            "port names must be unique".into(),
        ));
    }
    let row_width = input_names.len() + output_names.len();
    let expected_cases = 1_usize
        .checked_shl(input_names.len() as u32)
        .ok_or_else(|| VerificationError::InvalidPort("too many inputs".into()))?;
    if spec.truth_table.len() != expected_cases {
        return Err(VerificationError::IncompleteTruthTable {
            expected: expected_cases,
            actual: spec.truth_table.len(),
        });
    }
    let dag = dag_for(kind, &input_names, &output_names)?;
    let mut seen = BTreeSet::new();
    for (index, row) in spec.truth_table.iter().enumerate() {
        if row.len() != row_width {
            return Err(VerificationError::InvalidRowWidth {
                expected: row_width,
                actual: row.len(),
            });
        }
        let input_values = row[..input_names.len()].to_vec();
        if !seen.insert(input_values.clone()) {
            return Err(VerificationError::DuplicateInputCase);
        }
        let inputs = input_names
            .iter()
            .copied()
            .zip(input_values)
            .map(|(name, value)| (name.to_owned(), value))
            .collect::<HashMap<_, _>>();
        let actual = dag
            .evaluate(&inputs)
            .map_err(|error| VerificationError::InvalidDag(error.to_string()))?;
        if output_names.iter().enumerate().any(|(offset, name)| {
            actual.get(*name).copied() != Some(row[input_names.len() + offset])
        }) {
            return Err(VerificationError::BehaviorMismatch { row: index });
        }
    }
    Ok(LogicalVerification {
        exhaustive: true,
        cases_checked: expected_cases,
    })
}

fn dag_for(
    kind: ComponentKind,
    input_names: &[&str],
    output_names: &[&str],
) -> Result<LogicDag, VerificationError> {
    let expected_shape = match kind {
        ComponentKind::Not => (1, 1),
        ComponentKind::And | ComponentKind::Or | ComponentKind::Xor => (2, 1),
        ComponentKind::HalfAdder => (2, 2),
    };
    if (input_names.len(), output_names.len()) != expected_shape {
        return Err(VerificationError::InvalidPort(format!(
            "{kind:?} requires {} inputs and {} outputs",
            expected_shape.0, expected_shape.1
        )));
    }
    let mut builder = DagBuilder::new();
    let a = builder.input(input_names[0]);
    let dag = match kind {
        ComponentKind::Not => {
            let out = builder.gate(GateKind::Not, &[a], Some(output_names[0]));
            builder.finish([(output_names[0].into(), out)])
        }
        ComponentKind::And | ComponentKind::Or | ComponentKind::Xor => {
            let b = builder.input(input_names[1]);
            let gate = match kind {
                ComponentKind::And => GateKind::And,
                ComponentKind::Or => GateKind::Or,
                ComponentKind::Xor => GateKind::Xor,
                _ => unreachable!(),
            };
            let out = builder.gate(gate, &[a, b], Some(output_names[0]));
            builder.finish([(output_names[0].into(), out)])
        }
        ComponentKind::HalfAdder => {
            let b = builder.input(input_names[1]);
            let sum = builder.gate(GateKind::Xor, &[a, b], Some(output_names[0]));
            let carry = builder.gate(GateKind::And, &[a, b], Some(output_names[1]));
            builder.finish([
                (output_names[0].into(), sum),
                (output_names[1].into(), carry),
            ])
        }
    };
    dag.map_err(|error| VerificationError::InvalidDag(error.to_string()))
}
