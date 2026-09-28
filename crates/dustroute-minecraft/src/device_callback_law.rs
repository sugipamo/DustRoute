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
