//! Source-backed motion-time rules for Java 1.21.11. These are new immutable
//! programs; the historical bounded piston laws keep their original meaning.
use std::sync::OnceLock;

use serde::Serialize;

use crate::Pos;
use crate::law::{LawProgram, finite::FiniteLaw};
use crate::time::runtime::{HalfProgress, MotionHistory, TickSection};

pub const LAW_IDS: [&str; 4] = [
    "dustroute.law.piston.control.java-1-21-11.v1",
    "dustroute.law.piston.carrier.java-1-21-11.v1",
    "dustroute.law.piston.geometry.java-1-21-11.v2",
    "dustroute.law.piston.head.java-1-21-11.v1",
];

pub fn builtin_programs() -> &'static [LawProgram; 4] {
    static PROGRAMS: OnceLock<[LawProgram; 4]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        [
            crate::law::builtins::piston_motion_law::PISTON_CONTROL_JAVA_1_21_11_V1,
            crate::law::builtins::piston_motion_law::PISTON_CARRIER_JAVA_1_21_11_V1,
            crate::law::builtins::piston_motion_law::PISTON_GEOMETRY_JAVA_1_21_11_V2,
            crate::law::builtins::piston_motion_law::PISTON_HEAD_JAVA_1_21_11_V1,
        ]
        .map(|definition| definition.program())
    })
}

pub fn builtin_laws() -> &'static PistonMotionLaws {
    static LAWS: OnceLock<PistonMotionLaws> = OnceLock::new();
    LAWS.get_or_init(|| {
        PistonMotionLaws::compile(builtin_programs()).expect("pinned motion-time laws")
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum PistonBlockEvent {
    Extend,
    Retract,
    RetractDrop,
}

impl PistonBlockEvent {
    fn code(self) -> u16 {
        match self {
            Self::Extend => 0,
            Self::Retract => 1,
            Self::RetractDrop => 2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ControlFacts {
    pub powered: bool,
    pub extended: bool,
    pub can_push: bool,
    pub matching_carrier: bool,
    pub last_progress: HalfProgress,
    pub same_tick: bool,
    pub in_block_tick: bool,
    pub sticky: bool,
    pub pullable: bool,
    pub event: PistonBlockEvent,
}

impl Default for ControlFacts {
    fn default() -> Self {
        Self {
            powered: false,
            extended: false,
            can_push: false,
            matching_carrier: false,
            last_progress: HalfProgress::Zero,
            same_tick: false,
            in_block_tick: false,
            sticky: false,
            pullable: false,
            event: PistonBlockEvent::Extend,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlDecision {
    pub request: Option<PistonBlockEvent>,
    pub execute: bool,
    pub finish_payload: bool,
    pub pull: bool,
    pub remove_head: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarrierStep {
    pub complete: bool,
    pub discard: bool,
    pub history: MotionHistory,
    pub again: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeometryEffects {
    pub head: i32,
    pub payload: i32,
    pub limit: usize,
    pub body_carrier: bool,
    pub head_carrier: bool,
    pub accept_input: bool,
    pub first_tick_delay: u64,
    pub next_tick_delay: u64,
    pub neighbor_offset: Pos,
    pub shape_offset: Pos,
}

#[derive(Clone, Debug)]
pub struct PistonMotionLaws {
    control: FiniteLaw,
    carrier: FiniteLaw,
    geometry: FiniteLaw,
    head: FiniteLaw,
}

fn progress_code(p: HalfProgress) -> u16 {
    match p {
        HalfProgress::Zero => 0,
        HalfProgress::Half => 1,
        HalfProgress::Full => 2,
    }
}

fn progress(value: u16) -> HalfProgress {
    match value {
        0 => HalfProgress::Zero,
        1 => HalfProgress::Half,
        2 => HalfProgress::Full,
        _ => unreachable!("compiled progress bound"),
    }
}

impl PistonMotionLaws {
    pub fn compile(programs: &[LawProgram; 4]) -> Result<Self, String> {
        Ok(Self {
            control: FiniteLaw::compile(
                &programs[0],
                &[
                    ("powered", 1),
                    ("extended", 1),
                    ("can_push", 1),
                    ("matching_carrier", 1),
                    ("last_progress", 2),
                    ("same_tick", 1),
                    ("in_block_tick", 1),
                    ("sticky", 1),
                    ("pullable", 1),
                    ("event", 2),
                ],
                &[
                    ("request", 3),
                    ("execute", 1),
                    ("finish_payload", 1),
                    ("pull", 1),
                    ("remove_head", 1),
                ],
            )?,
            carrier: FiniteLaw::compile(
                &programs[1],
                &[
                    ("progress", 2),
                    ("last_progress", 2),
                    ("forced", 1),
                    ("source", 1),
                ],
                &[
                    ("complete", 1),
                    ("discard", 1),
                    ("next_progress", 2),
                    ("next_last", 2),
                    ("save_time", 1),
                    ("again", 1),
                ],
            )?,
            geometry: FiniteLaw::compile(
                &programs[2],
                &[
                    ("retract", 1),
                    ("front", 1),
                    ("block_entity_section", 1),
                    ("neighbor_index", 5),
                    ("count", 12),
                    ("piston_payload", 1),
                    ("has_block_entity", 1),
                    ("extended_piston", 1),
                ],
                &[
                    ("head", 2),
                    ("payload", 2),
                    ("limit", 12),
                    ("body_carrier", 1),
                    ("head_carrier", 1),
                    ("accept_input", 1),
                    ("first_tick_delay", 1),
                    ("next_tick_delay", 1),
                    ("dx", 2),
                    ("dy", 2),
                    ("dz", 2),
                    ("admit_chain", 1),
                    ("immovable", 1),
                    ("shape_dx", 2),
                    ("shape_dy", 2),
                    ("shape_dz", 2),
                ],
            )?,
            head: FiniteLaw::compile(
                &programs[3],
                &[
                    ("body_piston", 1),
                    ("body_moving", 1),
                    ("aligned", 1),
                    ("same_variant", 1),
                    ("extended", 1),
                ],
                &[("attached", 1), ("supported", 1)],
            )?,
        })
    }

    pub fn control(&self, facts: ControlFacts) -> ControlDecision {
        let row = self
            .control
            .evaluate(&[
                facts.powered.into(),
                facts.extended.into(),
                facts.can_push.into(),
                facts.matching_carrier.into(),
                progress_code(facts.last_progress),
                facts.same_tick.into(),
                facts.in_block_tick.into(),
                facts.sticky.into(),
                facts.pullable.into(),
                facts.event.code(),
            ])
            .expect("bounded control facts");
        ControlDecision {
            request: match row[0] {
                0 => None,
                1 => Some(PistonBlockEvent::Extend),
                2 => Some(PistonBlockEvent::Retract),
                3 => Some(PistonBlockEvent::RetractDrop),
                _ => unreachable!("compiled event bound"),
            },
            execute: row[1] != 0,
            finish_payload: row[2] != 0,
            pull: row[3] != 0,
            remove_head: row[4] != 0,
        }
    }

    pub fn carrier(
        &self,
        history: MotionHistory,
        forced: bool,
        source: bool,
        game_tick: u64,
    ) -> CarrierStep {
        let row = self
            .carrier
            .evaluate(&[
                progress_code(history.progress),
                progress_code(history.last_progress),
                forced.into(),
                source.into(),
            ])
            .expect("bounded carrier facts");
        CarrierStep {
            complete: row[0] != 0,
            discard: row[1] != 0,
            again: row[5] != 0,
            history: MotionHistory {
                progress: progress(row[2]),
                last_progress: progress(row[3]),
                saved_world_time: if row[4] != 0 {
                    game_tick
                } else {
                    history.saved_world_time
                },
            },
        }
    }

    pub fn geometry(
        &self,
        retract: bool,
        front: bool,
        section: TickSection,
        neighbor_index: u8,
    ) -> Option<GeometryEffects> {
        let row = self.geometry.evaluate(&[
            retract.into(),
            front.into(),
            u16::from(section == TickSection::BlockEntities),
            neighbor_index.into(),
            0,
            0,
            0,
            0,
        ])?;
        Some(GeometryEffects {
            head: row[0].into(),
            payload: row[1].into(),
            limit: row[2].into(),
            body_carrier: row[3] != 0,
            head_carrier: row[4] != 0,
            accept_input: row[5] != 0,
            first_tick_delay: row[6].into(),
            next_tick_delay: row[7].into(),
            neighbor_offset: Pos::new(
                i32::from(row[8]) - 1,
                i32::from(row[9]) - 1,
                i32::from(row[10]) - 1,
            ),
            shape_offset: Pos::new(
                i32::from(row[13]) - 1,
                i32::from(row[14]) - 1,
                i32::from(row[15]) - 1,
            ),
        })
    }

    pub fn chain_supported(&self, count: usize, piston_payload: bool) -> bool {
        let Ok(count) = u16::try_from(count) else {
            return false;
        };
        self.geometry
            .evaluate(&[0, 0, 0, 0, count, piston_payload.into(), 0, 0])
            .is_some_and(|row| row[11] != 0)
    }

    pub fn immovable(&self, has_block_entity: bool, extended_piston: bool) -> bool {
        self.geometry
            .evaluate(&[
                0,
                0,
                0,
                0,
                0,
                0,
                has_block_entity.into(),
                extended_piston.into(),
            ])
            .expect("bounded geometry facts")[12]
            != 0
    }

    pub fn head_supported(
        &self,
        body_piston: bool,
        body_moving: bool,
        aligned: bool,
        same_variant: bool,
        extended: bool,
    ) -> bool {
        self.head
            .evaluate(&[
                body_piston.into(),
                body_moving.into(),
                aligned.into(),
                same_variant.into(),
                extended.into(),
            ])
            .expect("bounded head facts")[1]
            != 0
    }
}
