//! Target Java callback effects. The shared runtime owns queue identity/time.
use std::sync::OnceLock;

use crate::law::LawProgram;

pub const LAW_IDS: [&str; 3] = [
    "dustroute.law.lamp.callback.java-1-21-11.v1",
    "dustroute.law.observer.callback.java-1-21-11.v1",
    "dustroute.law.stone-button.callback.java-1-21-11.v1",
];

pub fn builtin_programs() -> &'static [LawProgram; 3] {
    static PROGRAMS: OnceLock<[LawProgram; 3]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        [
            include_str!("../laws/lamp-callback-java-1-21-11-v1.json"),
            include_str!("../laws/observer-callback-java-1-21-11-v1.json"),
            include_str!("../laws/stone-button-callback-java-1-21-11-v1.json"),
        ]
        .map(|json| serde_json::from_str(json).expect("embedded target device law"))
    })
}
