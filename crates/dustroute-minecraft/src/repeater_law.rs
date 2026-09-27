//! Two retained repeater models, expressed as immutable executable data.
//! These are model revisions, not claims of complete Vanilla behavior. World
//! adapters own geometry, input sampling, event delivery and before-state checks.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::law::finite::FiniteLaw;
use crate::law::{ExecutableLaw, Instruction, LawProgram, LawState};

pub const COMPATIBILITY_REVISION: &str = "dustroute.law.repeater.compatibility-boundary.v1";
pub const BOUNDED_REVISION: &str = "dustroute.law.repeater.bounded-event.v1";
pub const REPEATER_LAW_IDS: [&str; 2] = [COMPATIBILITY_REVISION, BOUNDED_REVISION];

pub fn builtin_programs() -> &'static [LawProgram; 2] {
    static PROGRAMS: OnceLock<[LawProgram; 2]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        [
            include_str!("../laws/repeater-compatibility-v1.json"),
            include_str!("../laws/repeater-bounded-v1.json"),
        ]
        .map(|source| serde_json::from_str(source).expect("embedded repeater law"))
    })
}

pub fn builtin_compatibility_law() -> &'static CompatibilityRepeaterLaw {
    static LAW: OnceLock<CompatibilityRepeaterLaw> = OnceLock::new();
    LAW.get_or_init(|| {
        CompatibilityRepeaterLaw::compile(&builtin_programs()[0])
            .expect("pinned compatibility repeater law")
    })
}

pub fn builtin_bounded_law() -> &'static BoundedRepeaterLaw {
    static LAW: OnceLock<BoundedRepeaterLaw> = OnceLock::new();
    LAW.get_or_init(|| {
        BoundedRepeaterLaw::compile(&builtin_programs()[1]).expect("pinned bounded repeater law")
    })
}

/// One state per physical position, owned by the compatibility simulator. The
/// seed is its historical initialization policy, not reconstructed live history.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
pub struct CompatibilityRepeaterState {
    delay: u8,
    state: LawState,
}

impl CompatibilityRepeaterState {
    pub fn powered(&self) -> bool {
        self.state.register("powered").expect("checked adapter ABI") != 0
    }

    pub fn has_pending_output(&self) -> bool {
        self.state.register("pending").expect("checked adapter ABI") != 0
    }
}

#[derive(Clone, Debug)]
pub struct CompatibilityRepeaterLaw(ExecutableLaw);

impl CompatibilityRepeaterLaw {
    pub fn compile(program: &LawProgram) -> Result<Self, String> {
        fn register_only(body: &[Instruction]) -> bool {
            body.iter().all(|instruction| match instruction {
                Instruction::Set { .. } => true,
                Instruction::If {
                    then, otherwise, ..
                } => register_only(then) && register_only(otherwise),
                _ => false,
            })
        }
        let law = program.compile()?;
        if program.inputs != inputs(4, true, true)
            || program
                .registers
                .iter()
                .map(|(name, r)| (name.as_str(), r.maximum))
                .collect::<BTreeMap<_, _>>()
                != BTreeMap::from([
                    ("powered", 1),
                    ("pending", 1),
                    ("q0", 1),
                    ("q1", 1),
                    ("q2", 1),
                    ("q3", 1),
                ])
            || !program.histories.is_empty()
            || program.handlers.len() != 2
            || !program.handlers.contains_key("initialize")
            || !program.handlers.contains_key("boundary")
            || !program.handlers.values().all(|body| register_only(body))
        {
            return Err(
                "compatibility repeater law requires its exact register-only boundary ABI".into(),
            );
        }
        Ok(Self(law))
    }

    pub fn initial(&self, delay: u8, powered: bool) -> Result<CompatibilityRepeaterState, String> {
        if !(1..=4).contains(&delay) {
            return Err("compatibility repeater delay must be 1..=4".into());
        }
        Ok(CompatibilityRepeaterState {
            delay,
            state: self.0.event(
                &self.0.initial_state(),
                "initialize",
                &inputs(delay, powered, false),
            )?,
        })
    }

    /// One legacy boundary (two game ticks), not one game tick. The world
    /// prepares this value before committing it at the existing RepeaterUpdate.
    pub fn boundary(
        &self,
        state: &CompatibilityRepeaterState,
        rear_powered: bool,
        locked: bool,
    ) -> Result<CompatibilityRepeaterState, String> {
        Ok(CompatibilityRepeaterState {
            delay: state.delay,
            state: self.0.event(
                &state.state,
                "boundary",
                &inputs(state.delay, rear_powered, locked),
            )?,
        })
    }
}

fn inputs(delay: u8, rear_powered: bool, locked: bool) -> BTreeMap<String, u16> {
    BTreeMap::from([
        ("delay".into(), u16::from(delay)),
        ("rear_powered".into(), u16::from(rear_powered)),
        ("locked".into(), u16::from(locked)),
    ])
}

/// The bounded runner already stores output in World and pending callbacks in
/// its event queue. This law decides requests and delivered callback outcomes;
/// it adds no second output state, timer or assumed history.
#[derive(Clone, Debug)]
pub struct BoundedRepeaterLaw(FiniteLaw);

impl BoundedRepeaterLaw {
    pub fn compile(program: &LawProgram) -> Result<Self, String> {
        Ok(Self(FiniteLaw::compile(
            program,
            &[
                ("powered", 1),
                ("rear_powered", 1),
                ("expected_powered", 1),
                ("delay", 4),
            ],
            &[
                ("needs_update", 1),
                ("apply_tick", 1),
                ("next_powered", 1),
                ("delay_game_ticks", 8),
            ],
        )?))
    }

    pub fn needs_update(&self, powered: bool, rear_powered: bool) -> bool {
        self.0
            .evaluate(&[u16::from(powered), u16::from(rear_powered), 0, 0])
            .expect("Boolean bounded inputs")[0]
            != 0
    }

    pub fn delay_game_ticks(&self, delay: u8) -> Option<u64> {
        if !(1..=4).contains(&delay) {
            return None;
        }
        Some(u64::from(self.0.evaluate(&[0, 0, 0, u16::from(delay)])?[3]))
    }

    /// None retains a delivered stale event as a no-op. Some supplies the output
    /// to the world's existing validated, atomic before/after delta adapter.
    pub fn scheduled_output(&self, rear_powered: bool, expected_powered: bool) -> Option<bool> {
        let outputs = self
            .0
            .evaluate(&[0, u16::from(rear_powered), u16::from(expected_powered), 0])
            .expect("Boolean bounded inputs");
        (outputs[1] != 0).then_some(outputs[2] != 0)
    }
}
