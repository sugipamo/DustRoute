//! Compile a memoryless law over its entire finite input domain. Geometry
//! adapters supply facts; all output values come from the selected program.
use std::collections::BTreeMap;

use super::{Instruction, LawProgram};

#[derive(Clone, Debug)]
pub struct FiniteLaw {
    strides: Vec<usize>,
    maxima: Vec<u16>,
    rows: Vec<Vec<u16>>,
}

impl FiniteLaw {
    pub fn compile(
        program: &LawProgram,
        inputs: &[(&str, u16)],
        outputs: &[(&str, u16)],
    ) -> Result<Self, String> {
        fn memoryless(body: &[Instruction]) -> bool {
            body.iter().all(|instruction| match instruction {
                Instruction::Set { .. } => true,
                Instruction::If {
                    then, otherwise, ..
                } => memoryless(then) && memoryless(otherwise),
                _ => false,
            })
        }
        let input_bounds = inputs
            .iter()
            .map(|(n, v)| (String::from(*n), *v))
            .collect::<BTreeMap<_, _>>();
        let output_bounds = outputs
            .iter()
            .map(|(n, v)| (String::from(*n), *v))
            .collect::<BTreeMap<_, _>>();
        let compiled = program.compile()?;
        if input_bounds.len() != inputs.len()
            || output_bounds.len() != outputs.len()
            || program.inputs != input_bounds
            || program
                .registers
                .iter()
                .map(|(n, r)| (n.clone(), r.maximum))
                .collect::<BTreeMap<_, _>>()
                != output_bounds
            || !program.histories.is_empty()
            || program.handlers.len() != 1
            || !program
                .handlers
                .get("evaluate")
                .is_some_and(|body| memoryless(body))
        {
            return Err(
                "finite law requires the exact adapter ABI and a memoryless evaluate handler"
                    .into(),
            );
        }
        let mut count = 1usize;
        let mut strides = Vec::new();
        for (_, max) in inputs {
            strides.push(count);
            count = count
                .checked_mul(usize::from(*max) + 1)
                .filter(|n| *n <= 8192)
                .ok_or("finite law input domain exceeds 8192 rows")?;
        }
        let mut rows = Vec::with_capacity(count);
        for index in 0..count {
            let values = inputs
                .iter()
                .zip(&strides)
                .map(|((name, max), stride)| {
                    (
                        String::from(*name),
                        ((index / stride) % (usize::from(*max) + 1)) as u16,
                    )
                })
                .collect();
            // Each physical query starts from the declared initial registers;
            // no event queue or hidden history is created per interpretation.
            let state = compiled.event(&compiled.initial_state(), "evaluate", &values)?;
            rows.push(
                outputs
                    .iter()
                    .map(|(name, _)| state.register(name).expect("validated output"))
                    .collect(),
            );
        }
        Ok(Self {
            strides,
            maxima: inputs.iter().map(|(_, max)| *max).collect(),
            rows,
        })
    }

    pub fn evaluate(&self, inputs: &[u16]) -> Option<&[u16]> {
        if inputs.len() != self.maxima.len()
            || inputs.iter().zip(&self.maxima).any(|(v, max)| v > max)
        {
            return None;
        }
        let index = inputs
            .iter()
            .zip(&self.strides)
            .map(|(v, s)| usize::from(*v) * s)
            .sum::<usize>();
        Some(&self.rows[index])
    }
}
