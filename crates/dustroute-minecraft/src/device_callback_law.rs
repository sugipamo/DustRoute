//! Target Java callback effects. The shared runtime owns queue identity/time.
use std::sync::OnceLock;

use crate::device_program::DEVICE_COUNT;
use crate::law::LawProgram;

pub const LAW_IDS: [&str; DEVICE_COUNT] = {
    let devices = crate::device_program::BUILTIN_DEVICES;
    let mut ids = [""; DEVICE_COUNT];
    let mut i = 0;
    while i < devices.len() {
        ids[i] = devices[i].spec().law_id;
        i += 1;
    }
    ids
};

pub fn builtin_programs() -> &'static [LawProgram; DEVICE_COUNT] {
    static PROGRAMS: OnceLock<[LawProgram; DEVICE_COUNT]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        crate::device_program::BUILTIN_DEVICES.map(|device| device.spec().law.program())
    })
}

/// Circuit-input aggregation is independently pinned in the execution context.
pub const COMPARATOR_SIGNAL_ID: &str = "dustroute.law.comparator.signal.java-1-21-11.v1";
pub fn comparator_signal_program() -> &'static LawProgram {
    static PROGRAM: OnceLock<LawProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| crate::device_program::builtin_laws::COMPARATOR_SIGNAL.program())
}

pub fn comparator_signal(rear: u8, side: u8, subtract: bool) -> Option<u8> {
    static LAW: OnceLock<crate::law::finite::FiniteLaw> = OnceLock::new();
    let law = LAW.get_or_init(|| {
        crate::law::finite::FiniteLaw::compile(
            comparator_signal_program(),
            &[("rear", 15), ("side", 15), ("subtract", 1)],
            &[("level", 15)],
        )
        .expect("checked comparator signal law")
    });
    law.evaluate(&[rear.into(), side.into(), subtract.into()])
        .map(|row| row[0] as u8)
}
