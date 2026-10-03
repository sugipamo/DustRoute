//! Retained observer rules. World adapters provide field differences and timer
//! facts; the immutable program decides notification, pulse output and deadline
//! effects. The world's existing observations and event queue own all history.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::law::{ExecutableLaw, Instruction, LawProgram};

pub const OBSERVER_LAW_REVISION: &str = "dustroute.law.observer.compatibility-boundary.v1";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ObserverChanges {
    pub block_record: bool,
    pub signal: bool,
    pub weak: bool,
    pub strong: bool,
    pub repeater: bool,
    pub torch: bool,
    pub comparator: bool,
    pub observer: bool,
    pub lamp: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserverDeadline {
    Keep,
    SetAfter(u16),
    Clear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObserverPulseAction {
    pub powered: bool,
    pub deadline: ObserverDeadline,
}

pub fn builtin_program() -> &'static LawProgram {
    static PROGRAM: OnceLock<LawProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| crate::law::builtins::observer_law::OBSERVER_COMPATIBILITY_V1.program())
}

pub fn builtin_observer_law() -> &'static CompatibilityObserverLaw {
    static LAW: OnceLock<CompatibilityObserverLaw> = OnceLock::new();
    LAW.get_or_init(|| {
        CompatibilityObserverLaw::compile(builtin_program()).expect("pinned observer law")
    })
}

#[derive(Clone, Debug)]
pub struct CompatibilityObserverLaw(ExecutableLaw);

impl CompatibilityObserverLaw {
    pub fn compile(program: &LawProgram) -> Result<Self, String> {
        fn memoryless(body: &[Instruction]) -> bool {
            body.iter().all(|instruction| match instruction {
                Instruction::Set { .. } => true,
                Instruction::If {
                    then, otherwise, ..
                } => memoryless(then) && memoryless(otherwise),
                _ => false,
            })
        }
        let law = program.compile()?;
        if program.inputs != inputs(1)
            || program
                .registers
                .iter()
                .map(|(n, r)| (n.as_str(), r.maximum))
                .collect::<BTreeMap<_, _>>()
                != BTreeMap::from([
                    ("notify", 1),
                    ("apply", 1),
                    ("powered", 1),
                    ("deadline", 2),
                    ("after", u16::MAX),
                ])
            || !program.histories.is_empty()
            || program
                .handlers
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != ["end", "observe", "start"]
            || !program.handlers.values().all(|body| memoryless(body))
        {
            return Err(
                "compatibility observer law requires its exact memoryless observation/pulse ABI"
                    .into(),
            );
        }
        Ok(Self(law))
    }

    pub fn should_notify(&self, known: bool, changes: ObserverChanges) -> Result<bool, String> {
        let mut facts = inputs(0);
        for (name, value) in [
            ("known", known),
            ("block_record", changes.block_record),
            ("signal", changes.signal),
            ("weak", changes.weak),
            ("strong", changes.strong),
            ("repeater", changes.repeater),
            ("torch", changes.torch),
            ("comparator", changes.comparator),
            ("observer", changes.observer),
            ("lamp", changes.lamp),
        ] {
            facts.insert(name.into(), u16::from(value));
        }
        let next = self.0.event(&self.0.initial_state(), "observe", &facts)?;
        Ok(next.register("notify").expect("checked observer ABI") != 0)
    }

    pub fn start(&self, present: bool) -> Result<Option<ObserverPulseAction>, String> {
        self.pulse("start", present, false)
    }

    pub fn end(
        &self,
        present: bool,
        deadline_due: bool,
    ) -> Result<Option<ObserverPulseAction>, String> {
        self.pulse("end", present, deadline_due)
    }

    fn pulse(
        &self,
        event: &str,
        present: bool,
        due: bool,
    ) -> Result<Option<ObserverPulseAction>, String> {
        let mut facts = inputs(0);
        facts.insert("present".into(), u16::from(present));
        facts.insert("due".into(), u16::from(due));
        let next = self.0.event(&self.0.initial_state(), event, &facts)?;
        if next.register("apply") == Some(0) {
            return Ok(None);
        }
        let deadline = match next.register("deadline").expect("checked observer ABI") {
            0 => ObserverDeadline::Keep,
            1 => ObserverDeadline::SetAfter(next.register("after").expect("checked observer ABI")),
            2 => ObserverDeadline::Clear,
            _ => unreachable!("validated deadline bounds"),
        };
        Ok(Some(ObserverPulseAction {
            powered: next.register("powered") == Some(1),
            deadline,
        }))
    }
}

// Each handler receives its named facts; unrelated handler inputs are zero in
// this fixed adapter ABI. No absent observation is reconstructed from defaults.
fn inputs(value: u16) -> BTreeMap<String, u16> {
    [
        "known",
        "block_record",
        "signal",
        "weak",
        "strong",
        "repeater",
        "torch",
        "comparator",
        "observer",
        "lamp",
        "present",
        "due",
    ]
    .into_iter()
    .map(|name| (name.into(), value))
    .collect()
}
