//! Boolean expression IR and rewrite search.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::{DagBuilder, GateKind, LogicDag, LogicError, NodeId};

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", content = "values", rename_all = "snake_case")]
pub enum Expr {
    Var(String),
    Const(bool),
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Xor(Vec<Expr>),
    Nand(Vec<Expr>),
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Var(name) => f.write_str(name),
            Self::Const(value) => f.write_str(if *value { "1" } else { "0" }),
            Self::Not(value) => write!(f, "!({value})"),
            Self::And(values) => joined(f, values, " & "),
            Self::Or(values) => joined(f, values, " | "),
            Self::Xor(values) => joined(f, values, " ^ "),
            Self::Nand(values) => {
                f.write_str("![(")?;
                joined(f, values, " & ")?;
                f.write_str(")]")
            }
        }
    }
}

fn joined(f: &mut Formatter<'_>, values: &[Expr], separator: &str) -> std::fmt::Result {
    f.write_str("(")?;
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            f.write_str(separator)?;
        }
        Display::fmt(value, f)?;
    }
    f.write_str(")")
}

impl Expr {
    #[must_use]
    pub fn evaluate(&self, env: &BTreeMap<String, bool>) -> bool {
        match self {
            Self::Var(n) => env[n],
            Self::Const(v) => *v,
            Self::Not(v) => !v.evaluate(env),
            Self::And(v) => v.iter().all(|x| x.evaluate(env)),
            Self::Or(v) => v.iter().any(|x| x.evaluate(env)),
            Self::Xor(v) => v.iter().filter(|x| x.evaluate(env)).count() % 2 == 1,
            Self::Nand(v) => !v.iter().all(|x| x.evaluate(env)),
        }
    }
    #[must_use]
    pub fn size(&self) -> usize {
        match self {
            Self::Var(_) | Self::Const(_) => 1,
            Self::Not(v) => 1 + v.size(),
            Self::And(v) | Self::Or(v) | Self::Xor(v) | Self::Nand(v) => {
                1 + v.iter().map(Self::size).sum::<usize>()
            }
        }
    }
}

#[must_use]
pub fn rewrites_once(expr: &Expr) -> BTreeSet<Expr> {
    let mut out = BTreeSet::new();
    if let Expr::Not(v) = expr {
        if let Expr::Not(inner) = &**v {
            out.insert((**inner).clone());
        }
        if let Expr::And(xs) = &**v {
            if xs.len() == 2 {
                out.insert(Expr::Nand(xs.clone()));
            }
        }
    }
    if let Expr::Nand(xs) = expr {
        if xs.len() == 2 {
            out.insert(Expr::Not(Box::new(Expr::And(xs.clone()))));
        }
    }
    match expr {
        Expr::Not(v) => {
            for child in rewrites_once(v) {
                out.insert(Expr::Not(Box::new(child)));
            }
        }
        Expr::And(xs) | Expr::Or(xs) | Expr::Xor(xs) | Expr::Nand(xs) => {
            for (i, _) in xs.iter().enumerate() {
                for child in rewrites_once(&xs[i]) {
                    let mut ys = xs.clone();
                    ys[i] = child;
                    out.insert(match expr {
                        Expr::And(_) => Expr::And(ys),
                        Expr::Or(_) => Expr::Or(ys),
                        Expr::Xor(_) => Expr::Xor(ys),
                        _ => Expr::Nand(ys),
                    });
                }
            }
        }
        _ => {}
    }
    out.remove(expr);
    out
}

#[must_use]
pub fn search_equivalents(start: &Expr, max_steps: usize, max_states: usize) -> BTreeSet<Expr> {
    let mut seen = BTreeSet::from([start.clone()]);
    let mut q = VecDeque::from([(start.clone(), 0)]);
    while let Some((e, d)) = q.pop_front() {
        if d == max_steps {
            continue;
        }
        for n in rewrites_once(&e) {
            if seen.len() >= max_states {
                return seen;
            }
            if seen.insert(n.clone()) {
                q.push_back((n, d + 1));
            }
        }
    }
    seen
}
#[must_use]
pub fn best_by_size(start: &Expr, max_steps: usize, max_states: usize) -> Expr {
    search_equivalents(start, max_steps, max_states)
        .into_iter()
        .min_by_key(|e| (e.size(), e.clone()))
        .unwrap()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExprToLogicError {
    ConstantUnsupported,
    EmptyOperator,
    Logic(LogicError),
}

impl Display for ExprToLogicError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConstantUnsupported => {
                formatter.write_str("constant expressions are not supported by LogicDag")
            }
            Self::EmptyOperator => formatter.write_str("empty expression operator"),
            Self::Logic(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ExprToLogicError {}

#[must_use = "expression conversion can fail"]
pub fn logic_from_expressions(
    outputs: impl IntoIterator<Item = (String, Expr)>,
) -> Result<LogicDag, ExprToLogicError> {
    let mut builder = DagBuilder::new();
    let outputs = outputs
        .into_iter()
        .map(|(name, expression)| {
            let value = lower_expression(&mut builder, &expression)?;
            let output = builder.gate(GateKind::Output, &[value], Some(&name));
            Ok((name, output))
        })
        .collect::<Result<Vec<_>, ExprToLogicError>>()?;
    builder.finish(outputs).map_err(ExprToLogicError::Logic)
}

fn lower_expression(
    builder: &mut DagBuilder,
    expression: &Expr,
) -> Result<NodeId, ExprToLogicError> {
    match expression {
        Expr::Var(name) => Ok(builder.input(name)),
        Expr::Const(_) => Err(ExprToLogicError::ConstantUnsupported),
        Expr::Not(value) => {
            let value = lower_expression(builder, value)?;
            Ok(builder.gate(GateKind::Not, &[value], None))
        }
        Expr::And(values) => lower_operator(builder, GateKind::And, values),
        Expr::Or(values) => lower_operator(builder, GateKind::Or, values),
        Expr::Xor(values) => lower_operator(builder, GateKind::Xor, values),
        Expr::Nand(values) if values.len() == 2 => {
            let left = lower_expression(builder, &values[0])?;
            let right = lower_expression(builder, &values[1])?;
            Ok(builder.gate(GateKind::Nand, &[left, right], None))
        }
        Expr::Nand(values) => {
            let and = lower_operator(builder, GateKind::And, values)?;
            Ok(builder.gate(GateKind::Not, &[and], None))
        }
    }
}

fn lower_operator(
    builder: &mut DagBuilder,
    kind: GateKind,
    values: &[Expr],
) -> Result<NodeId, ExprToLogicError> {
    let mut values = values.iter();
    let first = values.next().ok_or(ExprToLogicError::EmptyOperator)?;
    let mut current = lower_expression(builder, first)?;
    for value in values {
        let right = lower_expression(builder, value)?;
        current = builder.gate(kind, &[current, right], None);
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simplifies_double_not_and_nand() {
        let a = Expr::Var("a".into());
        assert_eq!(
            best_by_size(&Expr::Not(Box::new(Expr::Not(Box::new(a.clone())))), 4, 256),
            a
        );
        let n = Expr::Not(Box::new(Expr::And(vec![
            Expr::Var("a".into()),
            Expr::Var("b".into()),
        ])));
        assert!(matches!(best_by_size(&n, 4, 256), Expr::Nand(_)));
    }

    #[test]
    fn converts_expressions_back_to_logic_dag() {
        let dag = logic_from_expressions([(
            "out".into(),
            Expr::Xor(vec![Expr::Var("a".into()), Expr::Var("b".into())]),
        )])
        .unwrap();
        assert_eq!(dag.outputs().len(), 1);
        assert!(dag.nodes().iter().any(|node| node.kind == GateKind::Xor));
    }
}
