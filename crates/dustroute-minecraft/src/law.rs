//! Bounded, data-defined local event laws. Geometry and event delivery belong
//! to adapters; no block kind or gate name has meaning in this interpreter.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub mod abstract_history;
pub mod finite;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LawProgram {
    pub inputs: BTreeMap<String, u16>,
    pub registers: BTreeMap<String, Register>,
    pub histories: BTreeMap<String, History>,
    pub handlers: BTreeMap<String, Vec<Instruction>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Register {
    pub initial: u16,
    pub maximum: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct History {
    /// Inclusive age window, in game ticks. Older entries cannot affect a law.
    pub window: u16,
    pub capacity: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expr {
    Constant { value: u16 },
    Input { name: String },
    Register { name: String },
    Count { history: String },
    Equal { left: Box<Expr>, right: Box<Expr> },
    AtLeast { left: Box<Expr>, right: Box<Expr> },
    Not { value: Box<Expr> },
    And { left: Box<Expr>, right: Box<Expr> },
    Maximum { left: Box<Expr>, right: Box<Expr> },
    SaturatingSubtract { left: Box<Expr>, right: Box<Expr> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Instruction {
    Set {
        register: String,
        value: Expr,
    },
    Remember {
        history: String,
    },
    /// One pending event per handler. A later request never replaces the first.
    ScheduleIfAbsent {
        event: String,
        after: u16,
    },
    If {
        condition: Expr,
        then: Vec<Instruction>,
        otherwise: Vec<Instruction>,
    },
}

/// Contains all future-relevant local state, using relative ages/deadlines.
/// Not deserializable: saved diagnostic state cannot silently become execution
/// evidence. Interpreter methods return a new value and leave the input intact.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LawState {
    inputs: BTreeMap<String, u16>,
    registers: BTreeMap<String, u16>,
    histories: BTreeMap<String, Vec<u16>>,
    pending: BTreeMap<String, u16>,
}

impl LawState {
    pub fn register(&self, name: &str) -> Option<u16> {
        self.registers.get(name).copied()
    }

    pub fn pending(&self, event: &str) -> Option<u16> {
        self.pending.get(event).copied()
    }
}

#[derive(Clone, Debug)]
pub struct ExecutableLaw {
    program: LawProgram,
}

impl LawProgram {
    pub fn compile(&self) -> Result<ExecutableLaw, String> {
        if self.handlers.is_empty() {
            return Err("a law needs an event handler".into());
        }
        for names in [
            self.inputs.keys().collect::<Vec<_>>(),
            self.registers.keys().collect(),
            self.histories.keys().collect(),
            self.handlers.keys().collect(),
        ] {
            if names.len() > 64 || names.iter().any(|s| s.trim().is_empty()) {
                return Err("law names must be nonempty; at most 64 entries per namespace".into());
            }
        }
        if self.registers.values().any(|r| r.initial > r.maximum)
            || self
                .histories
                .values()
                .any(|h| h.capacity == 0 || h.capacity > 1024)
        {
            return Err("invalid register or history bounds".into());
        }
        let mut budget = 4096;
        for body in self.handlers.values() {
            self.validate_body(body, 0, &mut budget)?;
        }
        Ok(ExecutableLaw {
            program: self.clone(),
        })
    }

    fn charge(depth: usize, budget: &mut usize) -> Result<(), String> {
        if depth > 32 || *budget == 0 {
            return Err("law program exceeds interpreter structural limits".into());
        }
        *budget -= 1;
        Ok(())
    }

    fn validate_expr(&self, value: &Expr, depth: usize, budget: &mut usize) -> Result<(), String> {
        Self::charge(depth, budget)?;
        let exists = match value {
            Expr::Constant { .. } => true,
            Expr::Input { name } => self.inputs.contains_key(name),
            Expr::Register { name } => self.registers.contains_key(name),
            Expr::Count { history } => self.histories.contains_key(history),
            Expr::Not { value } => {
                self.validate_expr(value, depth + 1, budget)?;
                true
            }
            Expr::Equal { left, right }
            | Expr::AtLeast { left, right }
            | Expr::And { left, right }
            | Expr::Maximum { left, right }
            | Expr::SaturatingSubtract { left, right } => {
                self.validate_expr(left, depth + 1, budget)?;
                self.validate_expr(right, depth + 1, budget)?;
                true
            }
        };
        if exists {
            Ok(())
        } else {
            Err("unknown name in law expression".into())
        }
    }

    fn validate_body(
        &self,
        body: &[Instruction],
        depth: usize,
        budget: &mut usize,
    ) -> Result<(), String> {
        for instruction in body {
            Self::charge(depth, budget)?;
            match instruction {
                Instruction::Set { register, value } => {
                    if !self.registers.contains_key(register) {
                        return Err("unknown law register".into());
                    }
                    self.validate_expr(value, depth + 1, budget)?;
                }
                Instruction::Remember { history } => {
                    if !self.histories.contains_key(history) {
                        return Err("unknown law history".into());
                    }
                }
                Instruction::ScheduleIfAbsent { event, after } => {
                    if *after == 0 || !self.handlers.contains_key(event) {
                        return Err("invalid scheduled law event".into());
                    }
                }
                Instruction::If {
                    condition,
                    then,
                    otherwise,
                } => {
                    self.validate_expr(condition, depth + 1, budget)?;
                    self.validate_body(then, depth + 1, budget)?;
                    self.validate_body(otherwise, depth + 1, budget)?;
                }
            }
        }
        Ok(())
    }
}

impl ExecutableLaw {
    fn check_state(&self, state: &LawState) -> Result<(), String> {
        let valid = state.inputs.len() == self.program.inputs.len()
            && state
                .inputs
                .iter()
                .all(|(n, v)| self.program.inputs.get(n).is_some_and(|max| v <= max))
            && state.registers.len() == self.program.registers.len()
            && state.registers.iter().all(|(n, v)| {
                self.program
                    .registers
                    .get(n)
                    .is_some_and(|r| *v <= r.maximum)
            })
            && state.histories.len() == self.program.histories.len()
            && state.histories.iter().all(|(n, ages)| {
                self.program.histories.get(n).is_some_and(|h| {
                    ages.len() <= usize::from(h.capacity) && ages.iter().all(|age| *age <= h.window)
                })
            })
            && state
                .pending
                .iter()
                .all(|(n, delay)| *delay > 0 && self.program.handlers.contains_key(n));
        if valid {
            Ok(())
        } else {
            Err("state does not match this law's schema".into())
        }
    }

    pub fn initial_state(&self) -> LawState {
        LawState {
            inputs: self.program.inputs.keys().map(|n| (n.clone(), 0)).collect(),
            registers: self
                .program
                .registers
                .iter()
                .map(|(n, r)| (n.clone(), r.initial))
                .collect(),
            histories: self
                .program
                .histories
                .keys()
                .map(|n| (n.clone(), vec![]))
                .collect(),
            pending: BTreeMap::new(),
        }
    }

    /// Adapter initialization may supply an observed register value, without
    /// pretending to reconstruct an unobserved history or pending event queue.
    pub fn initialize_register(
        &self,
        state: &LawState,
        name: &str,
        value: u16,
    ) -> Result<LawState, String> {
        self.check_state(state)?;
        let mut next = state.clone();
        self.assign(&mut next, name, value)?;
        Ok(next)
    }

    pub fn event(
        &self,
        state: &LawState,
        event: &str,
        inputs: &BTreeMap<String, u16>,
    ) -> Result<LawState, String> {
        self.check_state(state)?;
        if inputs.len() != self.program.inputs.len()
            || inputs
                .iter()
                .any(|(n, v)| self.program.inputs.get(n).is_none_or(|max| v > max))
        {
            return Err("law input names or ranges do not match".into());
        }
        let mut next = state.clone();
        next.inputs = inputs.clone();
        self.execute(
            &mut next,
            self.program
                .handlers
                .get(event)
                .ok_or("unknown law event")?,
        )?;
        Ok(next)
    }

    /// One game tick, with deterministic handler-name ordering for simultaneous
    /// local timers. This order is a model policy, not Vanilla order evidence.
    pub fn advance(&self, state: &LawState) -> Result<LawState, String> {
        self.advance_with_register_effects(state, &mut |_, _, _| Ok(()))
    }

    /// Run due handlers with a synchronous adapter effect after each changed
    /// register assignment, before the next law instruction. The adapter may
    /// deliver a local event here; its pending requests precede later requests
    /// in the interrupted handler. Unchanged assignments produce no effect.
    ///
    /// Register-to-world mapping belongs to the selected execution profile, not
    /// this interpreter. The ordinary `advance` remains atomic. Errors discard
    /// the tentative state; effects must therefore operate on tentative data too.
    pub fn advance_with_register_effects<F>(
        &self,
        state: &LawState,
        on_change: &mut F,
    ) -> Result<LawState, String>
    where
        F: FnMut(&Self, &mut LawState, &str) -> Result<(), String>,
    {
        self.check_state(state)?;
        let mut next = state.clone();
        for (name, ages) in &mut next.histories {
            let window = self.program.histories[name].window;
            ages.retain_mut(|age| {
                if *age == window {
                    false
                } else {
                    *age += 1;
                    true
                }
            });
        }
        self.advance_after_history_expiration(next, on_change)
    }

    // Shared by exact execution and the history abstraction. Only expiration
    // differs; countdown, callback ordering, instructions and effects are common.
    fn advance_after_history_expiration<F>(
        &self,
        mut next: LawState,
        on_change: &mut F,
    ) -> Result<LawState, String>
    where
        F: FnMut(&Self, &mut LawState, &str) -> Result<(), String>,
    {
        let mut due = vec![];
        for (event, delay) in &mut next.pending {
            *delay -= 1;
            if *delay == 0 {
                due.push(event.clone());
            }
        }
        for event in &due {
            next.pending.remove(event);
        }
        for event in due {
            self.execute_with_register_effects(
                &mut next,
                &self.program.handlers[&event],
                on_change,
            )?;
        }
        Ok(next)
    }

    fn value(state: &LawState, value: &Expr) -> u16 {
        match value {
            Expr::Constant { value } => *value,
            Expr::Input { name } => state.inputs[name],
            Expr::Register { name } => state.registers[name],
            Expr::Count { history } => state.histories[history].len() as u16,
            Expr::Equal { left, right } => {
                u16::from(Self::value(state, left) == Self::value(state, right))
            }
            Expr::AtLeast { left, right } => {
                u16::from(Self::value(state, left) >= Self::value(state, right))
            }
            Expr::And { left, right } => {
                u16::from(Self::value(state, left) != 0 && Self::value(state, right) != 0)
            }
            Expr::Not { value } => u16::from(Self::value(state, value) == 0),
            Expr::Maximum { left, right } => {
                Self::value(state, left).max(Self::value(state, right))
            }
            Expr::SaturatingSubtract { left, right } => {
                Self::value(state, left).saturating_sub(Self::value(state, right))
            }
        }
    }

    fn assign(&self, state: &mut LawState, name: &str, value: u16) -> Result<(), String> {
        if self
            .program
            .registers
            .get(name)
            .is_none_or(|r| value > r.maximum)
        {
            return Err("law register assignment outside declared bounds".into());
        }
        state.registers.insert(name.into(), value);
        Ok(())
    }

    fn execute(&self, state: &mut LawState, body: &[Instruction]) -> Result<(), String> {
        self.execute_with_register_effects(state, body, &mut |_, _, _| Ok(()))
    }

    fn execute_with_register_effects<F>(
        &self,
        state: &mut LawState,
        body: &[Instruction],
        on_change: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(&Self, &mut LawState, &str) -> Result<(), String>,
    {
        for instruction in body {
            match instruction {
                Instruction::Set { register, value } => {
                    let value = Self::value(state, value);
                    let changed = state.registers[register] != value;
                    self.assign(state, register, value)?;
                    if changed {
                        on_change(self, state, register)?;
                        self.check_state(state)?;
                    }
                }
                Instruction::Remember { history } => {
                    let ages = state.histories.get_mut(history).expect("compiled history");
                    if ages.len() >= usize::from(self.program.histories[history].capacity) {
                        return Err("law history capacity exhausted".into());
                    }
                    ages.push(0);
                }
                Instruction::ScheduleIfAbsent { event, after } => {
                    state.pending.entry(event.clone()).or_insert(*after);
                }
                Instruction::If {
                    condition,
                    then,
                    otherwise,
                } => self.execute_with_register_effects(
                    state,
                    if Self::value(state, condition) != 0 {
                        then
                    } else {
                        otherwise
                    },
                    on_change,
                )?,
            }
        }
        Ok(())
    }
}
