//! Shared dust-strength execution. The immutable Blueprint asset lives below
//! the catalog so both world runners execute its exact program without a cycle.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::law::{Instruction, LawProgram};

pub const DUST_LAW_REVISION: &str = "dustroute.law.dust-strength.v1";
pub const BLUEPRINT_JSON: &str = include_str!("../laws/dust-blueprint-v1.json");

pub fn builtin_program() -> &'static LawProgram {
    static PROGRAM: OnceLock<LawProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        // The low layer reads executable data only. Catalog identity, metadata
        // and immutable storage are validated by the library layer.
        let asset: serde_json::Value = serde_json::from_str(BLUEPRINT_JSON).expect("dust asset");
        serde_json::from_value(asset["revisions"][0]["law"].clone()).expect("dust program")
    })
}

pub fn builtin_dust_law() -> &'static DustStrengthLaw {
    static LAW: OnceLock<DustStrengthLaw> = OnceLock::new();
    LAW.get_or_init(|| DustStrengthLaw::compile(builtin_program()).expect("pinned dust law"))
}

#[derive(Clone, Debug)]
pub struct DustStrengthLaw {
    levels: [[u8; 16]; 16],
}

impl DustStrengthLaw {
    pub fn compile(program: &LawProgram) -> Result<Self, String> {
        // Retain the original adapter's accepted program ABI, including its
        // allowance for additional bounded registers in a memoryless handler.
        if program.inputs != BTreeMap::from([("direct".into(), 15), ("neighbor".into(), 15)])
            || !program.histories.is_empty()
            || program.handlers.len() != 1
            || !program.handlers.contains_key("evaluate")
            || program
                .registers
                .get("strength")
                .is_none_or(|r| r.maximum != 15)
        {
            return Err(
                "dust adapter requires bounded levels and a memoryless evaluate handler".into(),
            );
        }
        fn memoryless(body: &[Instruction]) -> bool {
            body.iter().all(|instruction| match instruction {
                Instruction::Set { .. } => true,
                Instruction::If {
                    then, otherwise, ..
                } => memoryless(then) && memoryless(otherwise),
                _ => false,
            })
        }
        let compiled = program.compile()?;
        if !memoryless(&program.handlers["evaluate"]) {
            return Err("instantaneous dust law cannot schedule events or retain history".into());
        }
        let mut levels = [[0; 16]; 16];
        for direct in 0..=15 {
            for neighbor in 0..=15 {
                let evaluated = compiled.event(
                    &compiled.initial_state(),
                    "evaluate",
                    &BTreeMap::from([("direct".into(), direct), ("neighbor".into(), neighbor)]),
                )?;
                levels[usize::from(direct)][usize::from(neighbor)] = evaluated
                    .register("strength")
                    .ok_or("missing dust output")?
                    as u8;
            }
        }
        Ok(Self { levels })
    }

    /// Out-of-domain facts cannot be evaluated or silently normalized.
    pub fn strength(&self, direct: u8, neighbor: u8) -> Option<u8> {
        self.levels
            .get(usize::from(direct))?
            .get(usize::from(neighbor))
            .copied()
    }
}
