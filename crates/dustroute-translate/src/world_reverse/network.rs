//! Boolean expressions and functional dependencies derived from complete tables.
use super::truth_table::{TruthTableBudget, TruthTableError, infer_truth_table_with_budget};
use super::{
    FunctionalNetworkModel, InferredOutputFunction, InferredTruthTable, PhysicalInfluence,
    RegionAnalysis,
};
use crate::ir::expr::Expr;
use crate::world::World;
use std::collections::BTreeSet;

#[must_use]
pub fn infer_output_expressions(table: &InferredTruthTable) -> Vec<Expr> {
    (0..table.outputs.len())
        .map(|output| infer_output_expression(table, output))
        .collect()
}

pub fn derive_functional_network(
    world: &World,
    analysis: &RegionAnalysis,
    max_inputs: usize,
    settle_ticks: usize,
) -> Result<FunctionalNetworkModel, TruthTableError> {
    derive_functional_network_with_budget(
        world,
        analysis,
        max_inputs,
        settle_ticks,
        TruthTableBudget::default(),
    )
}

pub fn derive_functional_network_with_budget(
    world: &World,
    analysis: &RegionAnalysis,
    max_inputs: usize,
    settle_ticks: usize,
    budget: TruthTableBudget,
) -> Result<FunctionalNetworkModel, TruthTableError> {
    let truth_table =
        infer_truth_table_with_budget(world, analysis, max_inputs, settle_ticks, budget)?;
    let expressions = infer_output_expressions(&truth_table);
    let output_functions = truth_table
        .outputs
        .iter()
        .cloned()
        .zip(expressions)
        .enumerate()
        .map(
            |(output_index, (terminal, expression))| InferredOutputFunction {
                output_index,
                terminal,
                expression,
                truth_column: truth_table
                    .rows
                    .iter()
                    .map(|row| row.outputs[output_index])
                    .collect(),
            },
        )
        .collect();
    let physical_influences = analysis
        .components
        .iter()
        .map(|component| {
            let input_dependencies = analysis
                .inputs
                .iter()
                .enumerate()
                .filter(|(_, terminal)| {
                    component_reaches(analysis, terminal.component, component.id)
                })
                .map(|(index, _)| index)
                .collect::<BTreeSet<_>>();
            let output_dependencies = analysis
                .outputs
                .iter()
                .enumerate()
                .filter(|(_, terminal)| {
                    component_reaches(analysis, component.id, terminal.component)
                })
                .map(|(index, _)| index)
                .collect::<BTreeSet<_>>();
            PhysicalInfluence {
                component: component.id,
                positions: component.positions.clone(),
                shared_role: input_dependencies.len() > 1 || output_dependencies.len() > 1,
                input_dependencies,
                output_dependencies,
            }
        })
        .collect();
    Ok(FunctionalNetworkModel {
        truth_table,
        output_functions,
        physical_influences,
    })
}

fn component_reaches(analysis: &RegionAnalysis, start: usize, target: usize) -> bool {
    let mut pending = vec![start];
    let mut visited = BTreeSet::new();
    while let Some(component) = pending.pop() {
        if component == target {
            return true;
        }
        if !visited.insert(component) {
            continue;
        }
        pending.extend(analysis.components[component].outgoing.iter().copied());
    }
    false
}

fn infer_output_expression(table: &InferredTruthTable, output: usize) -> Expr {
    let vars: Vec<_> = (0..table.inputs.len())
        .map(|index| Expr::Var(format!("in{index}")))
        .collect();
    let mut candidates = vec![Expr::Const(false), Expr::Const(true)];
    for var in &vars {
        candidates.push(var.clone());
        candidates.push(Expr::Not(Box::new(var.clone())));
    }
    for left in 0..vars.len() {
        for right in left + 1..vars.len() {
            let pair = vec![vars[left].clone(), vars[right].clone()];
            candidates.push(Expr::And(pair.clone()));
            candidates.push(Expr::Or(pair.clone()));
            candidates.push(Expr::Xor(pair.clone()));
            candidates.push(Expr::Nand(pair));
        }
    }
    if vars.len() > 2 {
        candidates.push(Expr::And(vars.clone()));
        candidates.push(Expr::Or(vars.clone()));
        candidates.push(Expr::Xor(vars.clone()));
        candidates.push(Expr::Nand(vars.clone()));
    }
    if vars.len() == 3 {
        candidates.push(Expr::Or(vec![
            Expr::And(vec![vars[0].clone(), vars[1].clone()]),
            Expr::And(vec![vars[0].clone(), vars[2].clone()]),
            Expr::And(vec![vars[1].clone(), vars[2].clone()]),
        ]));
    }
    candidates
        .into_iter()
        .filter(|candidate| expression_matches(table, output, candidate))
        .min_by_key(|candidate| (candidate.size(), candidate.clone()))
        .unwrap_or_else(|| canonical_sum_of_products(table, output, &vars))
}

fn expression_matches(table: &InferredTruthTable, output: usize, expression: &Expr) -> bool {
    table.rows.iter().all(|row| {
        let env = row
            .inputs
            .iter()
            .enumerate()
            .map(|(index, value)| (format!("in{index}"), *value))
            .collect();
        expression.evaluate(&env) == row.outputs[output]
    })
}

fn canonical_sum_of_products(table: &InferredTruthTable, output: usize, vars: &[Expr]) -> Expr {
    let terms: Vec<_> = table
        .rows
        .iter()
        .filter(|row| row.outputs[output])
        .map(|row| {
            Expr::And(
                vars.iter()
                    .cloned()
                    .zip(&row.inputs)
                    .map(|(var, value)| {
                        if *value {
                            var
                        } else {
                            Expr::Not(Box::new(var))
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    match terms.as_slice() {
        [] => Expr::Const(false),
        [single] => single.clone(),
        _ => Expr::Or(terms),
    }
}
