//! Const-checked authoring for finite laws. Serialization remains `LawProgram`.
//!
//! Invalid references fail while compiling a constant, before a runtime exists.
//! ```compile_fail,E0080
//! use dustroute_minecraft::law::static_program::*;
//! const BAD: StaticLaw = StaticLaw::new(
//!     &[("input", 1)], &[("out", 1)],
//!     &[Step::Set("missing", Expression::Constant(1))]);
//! ```
//! ```compile_fail,E0080
//! use dustroute_minecraft::law::static_program::*;
//! const BAD: StaticLaw = StaticLaw::new(
//!     &[("input", 15)], &[("out", 1)],
//!     &[Step::Set("out", Expression::Input("input"))]);
//! ```
use super::{Expr, Instruction, LawProgram, Register};

#[derive(Clone, Copy, Debug)]
pub enum Expression {
    Constant(u16),
    Input(&'static str),
    Register(&'static str),
    Equal(&'static Self, &'static Self),
    AtLeast(&'static Self, &'static Self),
    Not(&'static Self),
    And(&'static Self, &'static Self),
    Maximum(&'static Self, &'static Self),
    SaturatingSubtract(&'static Self, &'static Self),
}

#[derive(Clone, Copy, Debug)]
pub enum Step {
    Set(&'static str, Expression),
    If(Expression, &'static [Self], &'static [Self]),
}

#[derive(Clone, Copy, Debug)]
pub struct StaticLaw {
    inputs: &'static [(&'static str, u16)],
    outputs: &'static [(&'static str, u16)],
    body: &'static [Step],
}

pub(crate) const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn names(columns: &[(&str, u16)]) {
    assert!(columns.len() <= 64, "too many law columns");
    let mut i = 0;
    while i < columns.len() {
        assert!(!columns[i].0.is_empty(), "empty law column");
        let mut j = 0;
        while j < i {
            assert!(!same(columns[i].0, columns[j].0), "duplicate law column");
            j += 1;
        }
        i += 1;
    }
}

const fn bound(columns: &[(&str, u16)], name: &str) -> u16 {
    let mut i = 0;
    while i < columns.len() {
        if same(columns[i].0, name) {
            return columns[i].1;
        }
        i += 1;
    }
    panic!("unknown law column")
}

impl StaticLaw {
    pub const fn new(
        inputs: &'static [(&'static str, u16)],
        outputs: &'static [(&'static str, u16)],
        body: &'static [Step],
    ) -> Self {
        names(inputs);
        names(outputs);
        let mut rows = 1usize;
        let mut i = 0;
        while i < inputs.len() {
            rows *= inputs[i].1 as usize + 1;
            assert!(rows <= 8192, "finite law exceeds 8192 rows");
            i += 1;
        }
        let law = Self {
            inputs,
            outputs,
            body,
        };
        law.check_body(body, 0, &mut 4096);
        law
    }

    pub const fn inputs(&self) -> &'static [(&'static str, u16)] {
        self.inputs
    }
    pub const fn outputs(&self) -> &'static [(&'static str, u16)] {
        self.outputs
    }
    pub const fn input_bound(&self, name: &str) -> u16 {
        bound(self.inputs, name)
    }
    pub const fn output_bound(&self, name: &str) -> u16 {
        bound(self.outputs, name)
    }

    const fn check_expr(&self, expr: &Expression, depth: usize, budget: &mut usize) -> u16 {
        assert!(depth <= 32 && *budget > 0, "law structural limit exceeded");
        *budget -= 1;
        match expr {
            Expression::Constant(v) => *v,
            Expression::Input(n) => self.input_bound(n),
            Expression::Register(n) => self.output_bound(n),
            Expression::Not(v) => {
                assert!(
                    self.check_expr(v, depth + 1, budget) <= 1,
                    "not needs Boolean"
                );
                1
            }
            Expression::Equal(a, b) | Expression::AtLeast(a, b) => {
                self.check_expr(a, depth + 1, budget);
                self.check_expr(b, depth + 1, budget);
                1
            }
            Expression::And(a, b) => {
                assert!(
                    self.check_expr(a, depth + 1, budget) <= 1,
                    "and needs Boolean"
                );
                assert!(
                    self.check_expr(b, depth + 1, budget) <= 1,
                    "and needs Boolean"
                );
                1
            }
            Expression::Maximum(a, b) | Expression::SaturatingSubtract(a, b) => {
                let a = self.check_expr(a, depth + 1, budget);
                let b = self.check_expr(b, depth + 1, budget);
                if matches!(expr, Expression::SaturatingSubtract(..)) || a >= b {
                    a
                } else {
                    b
                }
            }
        }
    }

    const fn check_body(&self, body: &[Step], depth: usize, budget: &mut usize) {
        let mut i = 0;
        while i < body.len() {
            assert!(depth <= 32 && *budget > 0, "law structural limit exceeded");
            *budget -= 1;
            match &body[i] {
                Step::Set(name, expr) => assert!(
                    self.check_expr(expr, depth + 1, budget) <= self.output_bound(name),
                    "law assignment exceeds output range"
                ),
                Step::If(expr, yes, no) => {
                    assert!(
                        self.check_expr(expr, depth + 1, budget) <= 1,
                        "if needs Boolean"
                    );
                    self.check_body(yes, depth + 1, budget);
                    self.check_body(no, depth + 1, budget);
                }
            }
            i += 1;
        }
    }

    /// Export the same public law format used by catalog revisions and proofs.
    pub fn program(&self) -> LawProgram {
        fn expression(e: &Expression) -> Expr {
            match e {
                Expression::Constant(value) => Expr::Constant { value: *value },
                Expression::Input(name) => Expr::Input {
                    name: (*name).into(),
                },
                Expression::Register(name) => Expr::Register {
                    name: (*name).into(),
                },
                Expression::Not(v) => Expr::Not {
                    value: Box::new(expression(v)),
                },
                Expression::Equal(a, b) => Expr::Equal {
                    left: Box::new(expression(a)),
                    right: Box::new(expression(b)),
                },
                Expression::AtLeast(a, b) => Expr::AtLeast {
                    left: Box::new(expression(a)),
                    right: Box::new(expression(b)),
                },
                Expression::And(a, b) => Expr::And {
                    left: Box::new(expression(a)),
                    right: Box::new(expression(b)),
                },
                Expression::Maximum(a, b) => Expr::Maximum {
                    left: Box::new(expression(a)),
                    right: Box::new(expression(b)),
                },
                Expression::SaturatingSubtract(a, b) => Expr::SaturatingSubtract {
                    left: Box::new(expression(a)),
                    right: Box::new(expression(b)),
                },
            }
        }
        fn body(steps: &[Step]) -> Vec<Instruction> {
            steps
                .iter()
                .map(|s| match s {
                    Step::Set(name, e) => Instruction::Set {
                        register: (*name).into(),
                        value: expression(e),
                    },
                    Step::If(e, yes, no) => Instruction::If {
                        condition: expression(e),
                        then: body(yes),
                        otherwise: body(no),
                    },
                })
                .collect()
        }
        LawProgram {
            inputs: self
                .inputs
                .iter()
                .map(|(n, max)| ((*n).into(), *max))
                .collect(),
            registers: self
                .outputs
                .iter()
                .map(|(n, max)| {
                    (
                        (*n).into(),
                        Register {
                            initial: 0,
                            maximum: *max,
                        },
                    )
                })
                .collect(),
            histories: Default::default(),
            handlers: [("evaluate".into(), body(self.body))].into(),
        }
    }
}
