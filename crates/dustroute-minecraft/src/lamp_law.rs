//! Existing lamp models as executable immutable data. Worlds own lamp state,
//! deadlines, source sampling and event delivery; these rules own the effects.
use std::sync::OnceLock;

use crate::law::{LawProgram, finite::FiniteLaw};

pub const LAMP_LAW_IDS: [&str; 2] = [
    "dustroute.law.lamp.compatibility-boundary.v1",
    "dustroute.law.lamp.bounded-event.v1",
];

pub fn builtin_programs() -> &'static [LawProgram; 2] {
    static PROGRAMS: OnceLock<[LawProgram; 2]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        [
            include_str!("../laws/lamp-compatibility-v1.json"),
            include_str!("../laws/lamp-bounded-v1.json"),
        ]
        .map(|json| serde_json::from_str(json).expect("embedded lamp law"))
    })
}

pub fn builtin_compatibility_law() -> &'static CompatibilityLampLaw {
    static LAW: OnceLock<CompatibilityLampLaw> = OnceLock::new();
    LAW.get_or_init(|| {
        CompatibilityLampLaw::compile(&builtin_programs()[0]).expect("pinned lamp law")
    })
}

pub fn builtin_bounded_law() -> &'static BoundedLampLaw {
    static LAW: OnceLock<BoundedLampLaw> = OnceLock::new();
    LAW.get_or_init(|| BoundedLampLaw::compile(&builtin_programs()[1]).expect("pinned lamp law"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LampDeadline {
    Keep,
    SetAfter(u16),
    Clear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LampAction {
    /// None preserves an absent cache entry as well as an existing value.
    pub lit: Option<bool>,
    pub deadline: LampDeadline,
}

#[derive(Clone, Debug)]
pub struct CompatibilityLampLaw(FiniteLaw);

impl CompatibilityLampLaw {
    pub fn compile(program: &LawProgram) -> Result<Self, String> {
        Ok(Self(FiniteLaw::compile(
            program,
            &[
                ("initial", 1),
                ("powered", 1),
                ("lit", 1),
                ("has_deadline", 1),
                ("due", 1),
                ("last_tick", 1),
            ],
            &[
                ("write", 1),
                ("lit", 1),
                ("deadline", 2),
                ("after", u16::MAX),
            ],
        )?))
    }

    /// Retain the compatibility model's power-derived construction seed.
    pub fn initially_lit(&self, powered: bool) -> Option<bool> {
        self.evaluate(true, powered, false, None, false).lit
    }

    pub fn boundary(
        &self,
        powered: bool,
        lit: bool,
        deadline_due: Option<bool>,
        last_tick: bool,
    ) -> LampAction {
        self.evaluate(false, powered, lit, deadline_due, last_tick)
    }

    fn evaluate(
        &self,
        initial: bool,
        powered: bool,
        lit: bool,
        deadline_due: Option<bool>,
        last_tick: bool,
    ) -> LampAction {
        let row = self
            .0
            .evaluate(&[
                u16::from(initial),
                u16::from(powered),
                u16::from(lit),
                u16::from(deadline_due.is_some()),
                u16::from(deadline_due.unwrap_or(false)),
                u16::from(last_tick),
            ])
            .expect("Boolean lamp facts");
        LampAction {
            lit: (row[0] == 1).then_some(row[1] == 1),
            deadline: match row[2] {
                0 => LampDeadline::Keep,
                1 => LampDeadline::SetAfter(row[3]),
                2 => LampDeadline::Clear,
                _ => unreachable!("compiled effect bounds"),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct BoundedLampLaw(FiniteLaw);

impl BoundedLampLaw {
    pub fn compile(program: &LawProgram) -> Result<Self, String> {
        Ok(Self(FiniteLaw::compile(
            program,
            &[("powered", 1)],
            &[("lit", 1)],
        )?))
    }

    pub fn lit(&self, powered: bool) -> bool {
        self.0
            .evaluate(&[u16::from(powered)])
            .expect("Boolean lamp power")[0]
            == 1
    }
}
