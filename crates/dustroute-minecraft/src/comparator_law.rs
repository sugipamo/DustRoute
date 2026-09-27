//! The retained comparator calculation as immutable executable data. The world
//! supplies actual input levels and delivers the compatibility boundary; its
//! existing queue and ComparatorUpdate event own sampling and output commitment.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::law::{ExecutableLaw, Instruction, LawProgram};

pub const COMPARATOR_LAW_REVISION: &str = "dustroute.law.comparator.compatibility-boundary.v1";

pub fn builtin_program() -> &'static LawProgram {
    static PROGRAM: OnceLock<LawProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        serde_json::from_str(include_str!("../laws/comparator-compatibility-v1.json"))
            .expect("embedded comparator law")
    })
}

pub fn builtin_comparator_law() -> &'static CompatibilityComparatorLaw {
    static LAW: OnceLock<CompatibilityComparatorLaw> = OnceLock::new();
    LAW.get_or_init(|| {
        CompatibilityComparatorLaw::compile(builtin_program()).expect("pinned comparator law")
    })
}

#[derive(Clone, Debug)]
pub struct CompatibilityComparatorLaw(ExecutableLaw);

impl CompatibilityComparatorLaw {
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
        if program.inputs
            != BTreeMap::from([
                ("rear".into(), 255),
                ("side_a".into(), 255),
                ("side_b".into(), 255),
                ("subtract".into(), 1),
            ])
            || program.registers.len() != 1
            || program
                .registers
                .get("strength")
                .is_none_or(|r| r.maximum != 255)
            || !program.histories.is_empty()
            || program.handlers.len() != 1
            || !program
                .handlers
                .get("evaluate")
                .is_some_and(|body| memoryless(body))
        {
            return Err(
                "compatibility comparator law requires its exact memoryless u8-level ABI".into(),
            );
        }
        Ok(Self(law))
    }

    /// Minecraft signal cases use 0..=15. The raw legacy simulator also accepts
    /// u8 levels from synthetic comparator states; retaining those values avoids
    /// silently clamping or promoting them into valid physical observations.
    pub fn evaluate(&self, rear: u8, sides: [u8; 2], subtract: bool) -> Result<u8, String> {
        let result = self.0.event(
            &self.0.initial_state(),
            "evaluate",
            &BTreeMap::from([
                ("rear".into(), u16::from(rear)),
                ("side_a".into(), u16::from(sides[0])),
                ("side_b".into(), u16::from(sides[1])),
                ("subtract".into(), u16::from(subtract)),
            ]),
        )?;
        Ok(result.register("strength").expect("checked u8 output ABI") as u8)
    }
}
