//! Provenance vocabulary shared by physical observations and simulated traces.
//!
//! A live Mineflayer bridge can observe packet-visible block updates, but it
//! cannot identify the vanilla scheduler queue that caused them.  These types
//! therefore make the evidence level explicit instead of making a packet order
//! look like a scheduler explanation.  The optional parent sequence is kept
//! for traces produced by a future causal event engine; current live and
//! steady-state traces leave it unset.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A state change whose semantic role has not been narrowed further.
    #[default]
    StateTransition,
    /// A change caused by an explicit input action such as a lever toggle.
    ExternalAction,
    /// A signal or device state propagated through the observed circuit.
    SignalPropagation,
    /// The rising edge of a pulse-producing device.
    PulseStart,
    /// The falling edge of a pulse-producing device.
    PulseEnd,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCause {
    /// The producer is not observable from the current evidence.
    #[default]
    Unknown,
    /// The event was seeded from a pre-action snapshot.
    InitialSnapshot,
    /// Mineflayer reported a packet-visible block update.  This does not
    /// identify the internal Minecraft scheduler cause.
    PacketObservation,
    /// A typed external input mutation was applied by the simulator or actor.
    ExternalInput,
    /// The simulator observed ordinary signal propagation.
    SimulatorPropagation,
    /// An Observer detected a front-face block/state transition.
    ObserverFrontStateChange,
    /// A delayed repeater output became visible.
    RepeaterDelay,
    /// A future scheduler implementation supplied a scheduled block tick.
    ScheduledTick,
    /// A future block-event implementation supplied a block event.
    BlockEvent,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    #[default]
    Unknown,
    LiveMineflayer,
    Simulator,
    InitialSnapshot,
}
