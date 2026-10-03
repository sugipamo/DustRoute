//! Target-version electrical rules for the forthcoming expanded piston profile.
//! These do not change the retained directional or isolated-direct profiles.
//! Geometry supplies source identity, query direction, rendered wire arms and
//! solid-block evidence. Notifications and pending ticks belong to the runtime.
use std::sync::OnceLock;

use crate::law::{LawProgram, finite::FiniteLaw};

pub const LAW_IDS: [&str; 4] = [
    "dustroute.law.piston.signal-emission.java-1-21-11.v1",
    "dustroute.law.piston.conductor.java-1-21-11.v1",
    "dustroute.law.piston.power-query.java-1-21-11.v1",
    "dustroute.law.repeater.callback.java-1-21-11.v1",
];

pub fn builtin_programs() -> &'static [LawProgram; 4] {
    static PROGRAMS: OnceLock<[LawProgram; 4]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        [
            crate::law::builtins::piston_electrical_law::PISTON_SIGNAL_EMISSION_JAVA_1_21_11_V1,
            crate::law::builtins::piston_electrical_law::PISTON_CONDUCTOR_JAVA_1_21_11_V1,
            crate::law::builtins::piston_electrical_law::PISTON_POWER_QUERY_JAVA_1_21_11_V1,
            crate::law::builtins::piston_electrical_law::REPEATER_CALLBACK_JAVA_1_21_11_V1,
        ]
        .map(|definition| definition.program())
    })
}

pub fn builtin_laws() -> &'static ElectricalLaws {
    static LAWS: OnceLock<ElectricalLaws> = OnceLock::new();
    LAWS.get_or_init(|| ElectricalLaws::compile(builtin_programs()).expect("pinned electrical ABI"))
}

#[derive(Clone, Copy, Debug)]
#[repr(u16)]
pub enum SignalSource {
    Passive,
    Lever,
    RedstoneBlock,
    Repeater,
    Wire,
}

/// Java's receiver-to-source direction; this is not the emitted signal vector.
#[derive(Clone, Copy, Debug)]
#[repr(u16)]
pub enum QueryDirection {
    Down,
    Up,
    Horizontal,
}

#[derive(Clone, Copy, Debug)]
pub struct EmissionFacts {
    pub source: SignalSource,
    /// Explicit wire strength, or 0/15 for a discrete powered device.
    pub level: u8,
    pub query: QueryDirection,
    /// Lever: opposite of support offset. Repeater: opposite of internal output.
    pub direction_match: bool,
    /// The recomputed wire arm opposite the query is connected.
    pub wire_connected: bool,
    /// False only during a wire's non-wire-source input calculation.
    pub wires_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Emission {
    pub weak: u8,
    pub strong: u8,
}

#[derive(Clone, Copy, Debug)]
#[repr(u16)]
pub enum PistonPowerQuery {
    Adjacent,
    OwnPosition,
    Quasi,
}

#[derive(Clone, Copy, Debug)]
pub struct RepeaterFacts {
    pub powered: bool,
    pub input: bool,
    pub locked: bool,
    pub target_misaligned: bool,
    pub delay: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepeaterDecision {
    pub request: bool,
    /// 0 = extremely high, 1 = very high, 2 = high (ascending delivery order).
    pub priority: u8,
    pub apply: bool,
    pub next_powered: bool,
    /// A short input still produces an ON pulse, then requests an OFF callback.
    pub schedule_off: bool,
    pub delay_game_ticks: u64,
}

#[derive(Clone, Debug)]
pub struct ElectricalLaws {
    emission: FiniteLaw,
    conductor: FiniteLaw,
    query: FiniteLaw,
    repeater: FiniteLaw,
}

impl ElectricalLaws {
    pub fn compile(programs: &[LawProgram; 4]) -> Result<Self, String> {
        Ok(Self {
            emission: FiniteLaw::compile(
                &programs[0],
                &[
                    ("source", 4),
                    ("level", 15),
                    ("query", 2),
                    ("direction_match", 1),
                    ("wire_connected", 1),
                    ("wires_enabled", 1),
                ],
                &[("weak", 15), ("strong", 15)],
            )?,
            conductor: FiniteLaw::compile(
                &programs[1],
                &[("weak", 15), ("received_strong", 15), ("conducts", 1)],
                &[("emitted", 15)],
            )?,
            query: FiniteLaw::compile(
                &programs[2],
                &[("query", 2), ("front", 1), ("powered", 1)],
                &[("accept", 1)],
            )?,
            repeater: FiniteLaw::compile(
                &programs[3],
                &[
                    ("powered", 1),
                    ("input", 1),
                    ("locked", 1),
                    ("target_misaligned", 1),
                    ("delay", 4),
                ],
                &[
                    ("request", 1),
                    ("priority", 2),
                    ("apply", 1),
                    ("next_powered", 1),
                    ("schedule_off", 1),
                    ("delay_game_ticks", 8),
                ],
            )?,
        })
    }

    pub fn emission(&self, facts: EmissionFacts) -> Option<Emission> {
        let row = self.emission.evaluate(&[
            facts.source as u16,
            facts.level.into(),
            facts.query as u16,
            facts.direction_match.into(),
            facts.wire_connected.into(),
            facts.wires_enabled.into(),
        ])?;
        Some(Emission {
            weak: row[0] as u8,
            strong: row[1] as u8,
        })
    }

    pub fn emitted(&self, weak: u8, received_strong: u8, conducts: bool) -> Option<u8> {
        Some(
            self.conductor
                .evaluate(&[weak.into(), received_strong.into(), conducts.into()])?[0]
                as u8,
        )
    }

    pub fn accepts(&self, query: PistonPowerQuery, front: bool, powered: bool) -> bool {
        self.query
            .evaluate(&[query as u16, front.into(), powered.into()])
            .expect("bounded query facts")[0]
            != 0
    }

    pub fn repeater(&self, facts: RepeaterFacts) -> Option<RepeaterDecision> {
        if !(1..=4).contains(&facts.delay) {
            return None;
        }
        let row = self.repeater.evaluate(&[
            facts.powered.into(),
            facts.input.into(),
            facts.locked.into(),
            facts.target_misaligned.into(),
            facts.delay.into(),
        ])?;
        Some(RepeaterDecision {
            request: row[0] != 0,
            priority: row[1] as u8,
            apply: row[2] != 0,
            next_powered: row[3] != 0,
            schedule_off: row[4] != 0,
            delay_game_ticks: row[5].into(),
        })
    }
}
