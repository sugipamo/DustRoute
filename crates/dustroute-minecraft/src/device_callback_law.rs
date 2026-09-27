//! Target Java callback effects. The shared runtime owns queue identity/time.
use std::sync::OnceLock;

use crate::law::LawProgram;

pub const LAW_IDS: [&str; 3] = {
    let devices = crate::device_program::BUILTIN_DEVICES;
    [
        devices[0].spec().law_id,
        devices[1].spec().law_id,
        devices[2].spec().law_id,
    ]
};

pub fn builtin_programs() -> &'static [LawProgram; 3] {
    static PROGRAMS: OnceLock<[LawProgram; 3]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        crate::device_program::BUILTIN_DEVICES.map(|device| device.spec().law.program())
    })
}
