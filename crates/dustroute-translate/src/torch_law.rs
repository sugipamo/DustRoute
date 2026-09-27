//! Physical adapter for the pinned, executable torch Blueprint Revision.
//! The law owns inversion, history and scheduling; the caller supplies actual
//! support power and delivers neighbor events. No gate interpretation is used.
use std::collections::BTreeMap;

use dustroute_minecraft::law::{ExecutableLaw, LawState};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
pub struct TorchState(LawState);

impl TorchState {
    pub fn pending_game_ticks(&self) -> Option<u16> {
        self.0.pending("scheduled_tick")
    }
}

pub fn law() -> &'static ExecutableLaw {
    &crate::world_laws::builtin_world_laws().torch
}

pub fn initial(lit: bool) -> TorchState {
    TorchState(
        law()
            .initialize_register(&law().initial_state(), "lit", u16::from(lit))
            .expect("Boolean torch output"),
    )
}

pub fn notify(state: &TorchState, support_powered: bool) -> TorchState {
    TorchState(
        law()
            .event(
                &state.0,
                "neighbor_update",
                &BTreeMap::from([("powered".into(), u16::from(support_powered))]),
            )
            .expect("pinned torch law accepts a Boolean support input"),
    )
}

pub fn advance(state: &TorchState) -> TorchState {
    TorchState(
        law()
            .advance(&state.0)
            .expect("pinned torch law has bounded, validated state"),
    )
}

pub fn lit(state: &TorchState) -> bool {
    state.0.register("lit").expect("pinned torch output") != 0
}

pub fn burnout_seen(state: &TorchState) -> bool {
    state
        .0
        .register("burnout_seen")
        .expect("pinned diagnostic register")
        != 0
}
