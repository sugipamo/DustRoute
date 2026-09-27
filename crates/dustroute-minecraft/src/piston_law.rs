//! Immutable rules for the retained horizontal piston model. Adapters supply
//! actual block facts and materialize coordinate effects through WorldDelta.
//! No Blueprint occurrence, terminal, or interpretation is moved by these laws.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::law::{ExecutableLaw, Instruction, LawProgram, finite::FiniteLaw};
use crate::{
    Block, Facing, ObservationClassification, PistonAction, PistonMotionProfile, PistonState,
    PistonVariant, WireConnection,
};

pub const PISTON_LAW_IDS: [&str; 4] = [
    "dustroute.law.piston.state.bounded-v1",
    "dustroute.law.piston.motion.bounded-v1",
    "dustroute.law.piston.connection.bounded-v1",
    "dustroute.law.piston.payload.bounded-v1",
];

pub fn builtin_programs() -> &'static [LawProgram; 4] {
    static PROGRAMS: OnceLock<[LawProgram; 4]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        [
            include_str!("../laws/piston-state-v1.json"),
            include_str!("../laws/piston-motion-v1.json"),
            include_str!("../laws/piston-connection-v1.json"),
            include_str!("../laws/piston-payload-v1.json"),
        ]
        .map(|data| serde_json::from_str(data).expect("embedded piston law"))
    })
}

pub fn builtin_piston_laws() -> &'static PistonLaws {
    static LAWS: OnceLock<PistonLaws> = OnceLock::new();
    LAWS.get_or_init(|| PistonLaws::compile(builtin_programs()).expect("pinned piston laws"))
}

pub const ELECTRICAL_PAYLOAD_LAW: &str = "dustroute.law.piston.electrical-payload.java-1-21-11.v3";

pub fn electrical_payload_program() -> &'static LawProgram {
    static PROGRAM: OnceLock<LawProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../laws/piston-electrical-payload-java-1-21-11-v3.json"
        ))
        .expect("embedded electrical payload law")
    })
}

/// Payload contract for the electrical world, including retracted ordinary
/// and sticky bodies and native observers in all directions. The separate bounded-event law data
/// retains its original revision and admission rules.
pub fn electrical_payload_laws() -> &'static PistonLaws {
    static LAWS: OnceLock<PistonLaws> = OnceLock::new();
    LAWS.get_or_init(|| {
        let mut programs = builtin_programs().clone();
        programs[3] = electrical_payload_program().clone();
        PistonLaws::compile(&programs).expect("electrical payload ABI")
    })
}

#[derive(Clone, Debug)]
pub struct PistonLaws {
    state: FiniteLaw,
    motion: FiniteLaw,
    connection: FiniteLaw,
    payload: ExecutableLaw,
    name_inputs: Vec<String>,
}

/// Coordinate distances are measured along the actual piston facing. Carrier
/// choices describe physical block effects, not logical gate/handler names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MotionEffects {
    pub head: i32,
    pub push_step: i32,
    pub pull_from: i32,
    pub pull_to: i32,
    pub dirty_radius: i32,
    pub source_carrier: bool,
    pub destination_carrier: bool,
    pub head_carrier: bool,
    pub extending: bool,
    pub progress: u8,
    pub head_is_payload: bool,
}

const PAYLOAD_FACTS: &[(&str, u16)] = &[
    ("kind", 15),
    ("retracted", 1),
    ("normal", 1),
    ("horizontal", 1),
    ("powered", 1),
    ("head", 1),
    ("entity", 1),
    ("name_missing", 1),
    ("extended_false_or_absent", 1),
    ("coarse", 1),
    ("live", 1),
];

impl PistonLaws {
    pub fn compile(programs: &[LawProgram; 4]) -> Result<Self, String> {
        let state = FiniteLaw::compile(
            &programs[0],
            &[("state", 3), ("action", 1), ("powered", 1), ("sticky", 1)],
            &[
                ("request", 2),
                ("valid", 1),
                ("stable", 3),
                ("moving", 3),
                ("pull", 1),
            ],
        )?;
        let motion = FiniteLaw::compile(
            &programs[1],
            &[("action", 1), ("count", 12), ("piston_payload", 1)],
            &[
                ("admit_push", 1),
                ("admit_chain", 1),
                ("limit", 12),
                ("head", 4),
                ("push_step", 4),
                ("pull_from", 4),
                ("pull_to", 4),
                ("dirty_radius", 4),
                ("source_carrier", 1),
                ("destination_carrier", 1),
                ("head_carrier", 1),
                ("extending", 1),
                ("progress", 1),
                ("head_is_payload", 1),
                ("activation_min", u16::MAX),
                ("activation_max", u16::MAX),
                ("completion", u16::MAX),
            ],
        )?;
        let connection = FiniteLaw::compile(
            &programs[2],
            &[
                ("kind", 15),
                ("observed", 1),
                ("shape", 1),
                ("arm", 1),
                ("facing", 1),
                ("aligned", 1),
            ],
            &[("connected", 1), ("error", 2)],
        )?;
        let program = &programs[3];
        let payload = program.compile()?;
        fn memoryless(body: &[Instruction]) -> bool {
            body.iter().all(|op| match op {
                Instruction::Set { .. } => true,
                Instruction::If {
                    then, otherwise, ..
                } => memoryless(then) && memoryless(otherwise),
                _ => false,
            })
        }
        let base: BTreeMap<_, _> = PAYLOAD_FACTS
            .iter()
            .map(|(n, m)| (String::from(*n), *m))
            .collect();
        let mut name_inputs = Vec::new();
        for (name, maximum) in &program.inputs {
            if base.get(name) == Some(maximum) {
                continue;
            }
            // Literal block-name predicates come from immutable rule data.
            // The adapter only compares the declared string with the actual
            // identifier after the legacy single-prefix normalization.
            if *maximum != 1 || !name.strip_prefix("name:").is_some_and(|n| !n.is_empty()) {
                return Err(
                    "piston payload inputs require exact block facts or literal name predicates"
                        .into(),
                );
            }
            name_inputs.push(name.clone());
        }
        if !base.iter().all(|(n, m)| program.inputs.get(n) == Some(m))
            || program
                .registers
                .iter()
                .map(|(n, r)| (n.as_str(), r.maximum))
                .collect::<BTreeMap<_, _>>()
                != BTreeMap::from([("immovable", 1), ("rejection", 4)])
            || !program.histories.is_empty()
            || program.handlers.len() != 1
            || !program
                .handlers
                .get("evaluate")
                .is_some_and(|body| memoryless(body))
        {
            return Err("piston payload law requires its exact memoryless effects ABI".into());
        }
        let result = Self {
            state,
            motion,
            connection,
            payload,
            name_inputs,
        };
        result
            .default_motion_profile()
            .validate()
            .map_err(|e| e.to_string())?;
        Ok(result)
    }

    fn state_row(
        &self,
        state: PistonState,
        action: PistonAction,
        powered: bool,
        sticky: bool,
    ) -> &[u16] {
        self.state
            .evaluate(&[
                state_tag(state),
                action_tag(action),
                u16::from(powered),
                u16::from(sticky),
            ])
            .expect("typed piston facts")
    }

    pub fn requested_action(&self, state: PistonState, powered: bool) -> Option<PistonAction> {
        match self.state_row(state, PistonAction::Extend, powered, false)[0] {
            0 => None,
            1 => Some(PistonAction::Extend),
            2 => Some(PistonAction::Retract),
            _ => unreachable!("compiled request bounds"),
        }
    }

    pub fn permits(&self, state: PistonState, action: PistonAction) -> bool {
        self.state_row(state, action, false, false)[1] == 1
    }

    pub fn stable_state(&self, action: PistonAction) -> PistonState {
        from_state_tag(self.state_row(PistonState::Retracted, action, false, false)[2])
    }

    pub fn moving_state(&self, action: PistonAction) -> PistonState {
        from_state_tag(self.state_row(PistonState::Retracted, action, false, false)[3])
    }

    pub fn pulls(&self, action: PistonAction, variant: PistonVariant) -> bool {
        self.state_row(
            PistonState::Retracted,
            action,
            false,
            variant == PistonVariant::Sticky,
        )[4] == 1
    }

    fn motion_row(
        &self,
        action: PistonAction,
        count: usize,
        contains_piston: bool,
    ) -> Option<&[u16]> {
        self.motion.evaluate(&[
            action_tag(action),
            count.try_into().ok()?,
            u16::from(contains_piston),
        ])
    }

    pub fn can_push_next(&self, count: usize) -> bool {
        self.motion_row(PistonAction::Extend, count, false)
            .is_some_and(|row| row[0] == 1)
    }

    pub fn chain_supported(&self, count: usize, contains_piston: bool) -> bool {
        self.motion_row(PistonAction::Extend, count, contains_piston)
            .is_some_and(|row| row[1] == 1)
    }

    pub fn push_limit(&self) -> usize {
        usize::from(
            self.motion_row(PistonAction::Extend, 0, false)
                .expect("valid facts")[2],
        )
    }

    pub fn motion_effects(&self, action: PistonAction) -> MotionEffects {
        let row = self.motion_row(action, 0, false).expect("valid facts");
        MotionEffects {
            head: i32::from(row[3]),
            push_step: i32::from(row[4]),
            pull_from: i32::from(row[5]),
            pull_to: i32::from(row[6]),
            dirty_radius: i32::from(row[7]),
            source_carrier: row[8] == 1,
            destination_carrier: row[9] == 1,
            head_carrier: row[10] == 1,
            extending: row[11] == 1,
            progress: row[12] as u8,
            head_is_payload: row[13] == 1,
        }
    }

    pub fn default_motion_profile(&self) -> PistonMotionProfile {
        let row = self
            .motion_row(PistonAction::Extend, 0, false)
            .expect("valid facts");
        PistonMotionProfile {
            initial_delay_min_game_ticks: u64::from(row[14]),
            initial_delay_max_game_ticks: u64::from(row[15]),
            movement_game_ticks: u64::from(row[16]),
        }
    }

    /// Error 1: missing observed wire shape; 2: missing observed device facing.
    pub fn input_connected(&self, source: &Block, direction: Facing) -> Result<bool, u16> {
        let row = self
            .connection
            .evaluate(&[
                crate::spatial::kind_tag(source.kind),
                u16::from(source.observed_name.is_some()),
                u16::from(source.wire_connections.is_some()),
                u16::from(
                    source
                        .wire_connections
                        .as_ref()
                        .and_then(|c| c.get(&direction))
                        .is_some_and(|c| *c != WireConnection::None),
                ),
                u16::from(source.facing.is_some()),
                u16::from(source.facing == Some(direction)),
            ])
            .expect("typed connection facts");
        if row[1] != 0 {
            Err(row[1])
        } else {
            Ok(row[0] == 1)
        }
    }

    fn payload_facts(&self, name: Option<&str>) -> BTreeMap<String, u16> {
        let mut facts = PAYLOAD_FACTS
            .iter()
            .map(|(name, _)| (String::from(*name), 0))
            .collect::<BTreeMap<_, _>>();
        let normalized = name.map(|n| n.strip_prefix("minecraft:").unwrap_or(n));
        for input in &self.name_inputs {
            facts.insert(
                input.clone(),
                u16::from(normalized == input.strip_prefix("name:")),
            );
        }
        facts
    }

    pub fn immovable_name(&self, name: &str) -> Result<bool, String> {
        let state = self.payload.event(
            &self.payload.initial_state(),
            "evaluate",
            &self.payload_facts(Some(name)),
        )?;
        Ok(state.register("immovable") == Some(1))
    }

    /// Rejection codes retain the existing error precedence: unsupported kind
    /// or piston state, coarse identity, live behavior, then immovable name.
    pub fn payload_rejection(&self, block: &Block) -> Result<u16, String> {
        let mut facts = self.payload_facts(block.observed_name.as_deref());
        facts.insert("kind".into(), crate::spatial::kind_tag(block.kind));
        for (name, value) in [
            (
                "retracted",
                block.piston_state == Some(PistonState::Retracted),
            ),
            (
                "normal",
                block.piston_variant == Some(PistonVariant::Normal),
            ),
            (
                "horizontal",
                block
                    .facing
                    .is_some_and(|f| f.horizontal_offset().is_some()),
            ),
            ("powered", block.powered == Some(true)),
            ("head", block.piston_head.is_some()),
            ("entity", block.piston_entity.is_some()),
            ("name_missing", block.observed_name.is_none()),
            (
                "extended_false_or_absent",
                block
                    .observed_properties
                    .get("extended")
                    .is_none_or(|v| v == "false"),
            ),
            (
                "coarse",
                block.observation_classification == ObservationClassification::Coarse,
            ),
            ("live", block.requires_live_observation()),
        ] {
            facts.insert(name.into(), u16::from(value));
        }
        let state = self
            .payload
            .event(&self.payload.initial_state(), "evaluate", &facts)?;
        Ok(state.register("rejection").expect("compiled payload ABI"))
    }
}

fn action_tag(action: PistonAction) -> u16 {
    match action {
        PistonAction::Extend => 0,
        PistonAction::Retract => 1,
    }
}
fn state_tag(state: PistonState) -> u16 {
    match state {
        PistonState::Retracted => 0,
        PistonState::Extending => 1,
        PistonState::Extended => 2,
        PistonState::Retracting => 3,
    }
}
fn from_state_tag(tag: u16) -> PistonState {
    [
        PistonState::Retracted,
        PistonState::Extending,
        PistonState::Extended,
        PistonState::Retracting,
    ][usize::from(tag)]
}
