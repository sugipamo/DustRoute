//! Rust authoring for complete event laws, including history and scheduled handlers.
//! References, defaults and interpreter limits are checked in constant evaluation.
//! Runtime compilation still checks adapter contracts and runtime assignments.
//!
//! ```compile_fail,E0080
//! use dustroute_minecraft::law::{definition::*, static_program::Step};
//! const BAD: Definition = Definition::new(&[], &[], &[],
//!     &[Handler { name: "update", body: &[Step::Remember("missing")] }]);
//! ```
//! ```compile_fail,E0080
//! use dustroute_minecraft::law::{definition::*, static_program::Step};
//! const BAD: Definition = Definition::new(&[], &[], &[],
//!     &[Handler { name: "update", body: &[Step::ScheduleIfAbsent("missing", 2)] }]);
//! ```
//! ```compile_fail,E0080
//! use dustroute_minecraft::law::definition::*;
//! const BAD: Definition = Definition::new(&[], &[("lit", 2, 1)], &[],
//!     &[Handler { name: "update", body: &[] }]);
//! ```
//! ```compile_fail,E0080
//! use dustroute_minecraft::law::definition::*;
//! const BAD: Definition = Definition::new(&[("power", 1), ("power", 1)], &[], &[],
//!     &[Handler { name: "update", body: &[] }]);
//! ```
//! ```compile_fail,E0080
//! use dustroute_minecraft::law::{definition::*, static_program::Step};
//! const BAD: Definition = Definition::new(&[], &[], &[],
//!     &[Handler { name: "update", body: &[Step::ScheduleIfAbsent("update", 0)] }]);
//! ```
//! ```compile_fail,E0080
//! use dustroute_minecraft::law::{definition::*, static_program::{Step, Expression}};
//! const BAD: Definition = Definition::new(&[], &[("out", 0, 1)], &[],
//!     &[Handler { name: "update", body: &[Step::Set("out", Expression::Input("missing"))] }]);
//! ```
use super::static_program::{Expression, Step, body, same};
use super::{History, LawProgram, Register};

#[derive(Clone, Copy, Debug)]
pub struct Handler {
    pub name: &'static str,
    pub body: &'static [Step],
}

#[derive(Clone, Copy, Debug)]
pub struct Definition {
    inputs: &'static [(&'static str, u16)],
    /// Name, initial value, maximum value.
    registers: &'static [(&'static str, u16, u16)],
    /// Name, age window, capacity.
    histories: &'static [(&'static str, u16, u16)],
    handlers: &'static [Handler],
}

const fn name_valid(name: &str) -> bool {
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            return true;
        }
        i += 1;
    }
    false
}

#[derive(Clone, Copy)]
enum Namespace {
    Input,
    Register,
    History,
    Handler,
}

impl Definition {
    pub const fn new(
        inputs: &'static [(&'static str, u16)],
        registers: &'static [(&'static str, u16, u16)],
        histories: &'static [(&'static str, u16, u16)],
        handlers: &'static [Handler],
    ) -> Self {
        let law = Self {
            inputs,
            registers,
            histories,
            handlers,
        };
        assert!(!handlers.is_empty(), "law needs a handler");
        let namespaces = [
            Namespace::Input,
            Namespace::Register,
            Namespace::History,
            Namespace::Handler,
        ];
        let mut n = 0;
        while n < namespaces.len() {
            let namespace = namespaces[n];
            let count = law.count(namespace);
            assert!(count <= 64, "too many law entries");
            let mut i = 0;
            while i < count {
                let name = law.name(namespace, i);
                assert!(name_valid(name), "empty law name");
                let mut j = 0;
                while j < i {
                    assert!(!same(name, law.name(namespace, j)), "duplicate law name");
                    j += 1;
                }
                i += 1;
            }
            n += 1;
        }
        let mut i = 0;
        while i < registers.len() {
            assert!(registers[i].1 <= registers[i].2, "invalid register default");
            i += 1;
        }
        i = 0;
        while i < histories.len() {
            assert!(
                histories[i].2 > 0 && histories[i].2 <= 1024,
                "invalid history capacity"
            );
            i += 1;
        }
        let mut budget = 4096;
        i = 0;
        while i < handlers.len() {
            law.check_body(handlers[i].body, 0, &mut budget);
            i += 1;
        }
        law
    }

    const fn count(&self, namespace: Namespace) -> usize {
        match namespace {
            Namespace::Input => self.inputs.len(),
            Namespace::Register => self.registers.len(),
            Namespace::History => self.histories.len(),
            Namespace::Handler => self.handlers.len(),
        }
    }
    const fn name(&self, namespace: Namespace, i: usize) -> &str {
        match namespace {
            Namespace::Input => self.inputs[i].0,
            Namespace::Register => self.registers[i].0,
            Namespace::History => self.histories[i].0,
            Namespace::Handler => self.handlers[i].name,
        }
    }
    const fn require(&self, namespace: Namespace, name: &str) {
        let mut i = 0;
        while i < self.count(namespace) {
            if same(self.name(namespace, i), name) {
                return;
            }
            i += 1;
        }
        panic!("unknown name in law definition")
    }
    const fn charge(depth: usize, budget: &mut usize) {
        assert!(depth <= 32 && *budget > 0, "law structural limit exceeded");
        *budget -= 1;
    }
    const fn check_expr(&self, expr: &Expression, depth: usize, budget: &mut usize) {
        Self::charge(depth, budget);
        match expr {
            Expression::Constant(_) => {}
            Expression::Input(name) => self.require(Namespace::Input, name),
            Expression::Register(name) => self.require(Namespace::Register, name),
            Expression::Count(name) => self.require(Namespace::History, name),
            Expression::Not(value) => self.check_expr(value, depth + 1, budget),
            Expression::Equal(a, b)
            | Expression::AtLeast(a, b)
            | Expression::And(a, b)
            | Expression::Maximum(a, b)
            | Expression::SaturatingSubtract(a, b) => {
                self.check_expr(a, depth + 1, budget);
                self.check_expr(b, depth + 1, budget);
            }
        }
    }
    const fn check_body(&self, steps: &[Step], depth: usize, budget: &mut usize) {
        let mut i = 0;
        while i < steps.len() {
            Self::charge(depth, budget);
            match &steps[i] {
                Step::Set(name, expr) => {
                    self.require(Namespace::Register, name);
                    self.check_expr(expr, depth + 1, budget);
                }
                Step::Remember(name) => self.require(Namespace::History, name),
                Step::ScheduleIfAbsent(name, after) => {
                    assert!(*after > 0, "scheduled event needs positive delay");
                    self.require(Namespace::Handler, name);
                }
                Step::If(expr, yes, no) => {
                    self.check_expr(expr, depth + 1, budget);
                    self.check_body(yes, depth + 1, budget);
                    self.check_body(no, depth + 1, budget);
                }
            }
            i += 1;
        }
    }

    /// Direct typed conversion, independent of any serialization codec.
    pub fn program(&self) -> LawProgram {
        LawProgram {
            inputs: self
                .inputs
                .iter()
                .map(|(n, max)| ((*n).into(), *max))
                .collect(),
            registers: self
                .registers
                .iter()
                .map(|(n, initial, maximum)| {
                    (
                        (*n).into(),
                        Register {
                            initial: *initial,
                            maximum: *maximum,
                        },
                    )
                })
                .collect(),
            histories: self
                .histories
                .iter()
                .map(|(n, window, capacity)| {
                    (
                        (*n).into(),
                        History {
                            window: *window,
                            capacity: *capacity,
                        },
                    )
                })
                .collect(),
            handlers: self
                .handlers
                .iter()
                .map(|h| (h.name.into(), body(h.body)))
                .collect(),
        }
    }
}
