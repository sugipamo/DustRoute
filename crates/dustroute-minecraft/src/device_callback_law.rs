//! Target Java callback effects. The shared runtime owns queue identity/time.
use std::sync::OnceLock;

use crate::law::{LawProgram, finite::FiniteLaw};

pub const LAW_IDS: [&str; 2] = [
    "dustroute.law.lamp.callback.java-1-21-11.v1",
    "dustroute.law.observer.callback.java-1-21-11.v1",
];

pub fn builtin_programs() -> &'static [LawProgram; 2] {
    static PROGRAMS: OnceLock<[LawProgram; 2]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        [
            include_str!("../laws/lamp-callback-java-1-21-11-v1.json"),
            include_str!("../laws/observer-callback-java-1-21-11-v1.json"),
        ]
        .map(|json| serde_json::from_str(json).expect("embedded target device law"))
    })
}

pub fn builtin_laws() -> &'static DeviceCallbackLaws {
    static LAWS: OnceLock<DeviceCallbackLaws> = OnceLock::new();
    LAWS.get_or_init(|| DeviceCallbackLaws::compile(builtin_programs()).expect("target device ABI"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserverCallback {
    Shape = 0,
    Tick = 1,
    Added = 2,
    Removed = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceEffect {
    pub write: bool,
    pub powered: bool,
    pub delay: u64,
    pub notify: bool,
}

#[derive(Clone, Debug)]
pub struct DeviceCallbackLaws {
    lamp: FiniteLaw,
    observer: FiniteLaw,
}

impl DeviceCallbackLaws {
    pub fn compile(programs: &[LawProgram; 2]) -> Result<Self, String> {
        Ok(Self {
            lamp: FiniteLaw::compile(
                &programs[0],
                &[("lit", 1), ("input", 1), ("scheduled", 1)],
                &[("write", 1), ("lit", 1), ("delay", 4)],
            )?,
            observer: FiniteLaw::compile(
                &programs[1],
                &[("event", 3), ("powered", 1), ("queued", 1), ("front", 1)],
                &[
                    ("write", 1),
                    ("powered", 1),
                    ("delay", 2),
                    ("notify", 1),
                    ("signal", 15),
                ],
            )?,
        })
    }

    pub fn lamp(&self, lit: bool, input: bool, scheduled: bool) -> DeviceEffect {
        let r = self
            .lamp
            .evaluate(&[lit.into(), input.into(), scheduled.into()])
            .expect("Boolean lamp facts");
        DeviceEffect {
            write: r[0] != 0,
            powered: r[1] != 0,
            delay: r[2].into(),
            notify: false,
        }
    }

    pub fn observer(
        &self,
        event: ObserverCallback,
        powered: bool,
        queued: bool,
        front: bool,
    ) -> DeviceEffect {
        let r = self
            .observer
            .evaluate(&[event as u16, powered.into(), queued.into(), front.into()])
            .expect("bounded observer facts");
        DeviceEffect {
            write: r[0] != 0,
            powered: r[1] != 0,
            delay: r[2].into(),
            notify: r[3] != 0,
        }
    }

    pub fn observer_signal(&self, powered: bool) -> u8 {
        self.observer
            .evaluate(&[0, powered.into(), 0, 0])
            .expect("Boolean observer power")[4] as u8
    }
}
