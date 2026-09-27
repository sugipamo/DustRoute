//! Versioned execution boundary for synchronous world callbacks.
//!
//! This is delivery/state infrastructure, not a new complete physics adapter.
//! Existing `PhysicsEngine` profiles and wire formats keep their meanings.
//! A stateless adapter supplies block laws and resumable operations; every
//! future-relevant value must live in payloads, the world, or carrier history.
mod executor;
mod motion;
mod observation;

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use serde::Serialize;

use crate::{Block, BlockKind, PistonVariant, Pos, Region, World, WorldDelta};

pub use executor::{
    PistonBehaviorState, RuntimeCheckpoint, RuntimeStateKey, SynchronousWorldRuntime,
};
pub use motion::{CarrierEffect, CarrierId, CarrierState, HalfProgress, MotionHistory};
pub use observation::LocationObservation;

/// Delivery contract only. Device laws must be selected separately by an
/// adapter; this identifier makes no complete Vanilla conformance claim.
pub const PROFILE: &str = "dustroute.synchronous-world-callbacks.v3";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TickSection {
    External,
    BlockTicks,
    BlockEvents,
    BlockEntities,
    /// Server-thread player input after the world's tick has returned. The
    /// world time is still this tick; resulting block events wait for the next.
    AfterWorldTick,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RuntimeTime {
    pub game_tick: u64,
    pub section: TickSection,
}

impl RuntimeTime {
    pub const fn in_block_tick(self) -> bool {
        matches!(
            self.section,
            TickSection::BlockTicks | TickSection::BlockEvents
        )
    }

    /// Events requested after the block-event pass wait for the next pass.
    /// Synchronous callbacks keep the enclosing section unchanged.
    pub fn next_block_event(self) -> Result<Self, RuntimeError> {
        Ok(Self {
            game_tick: if self.section > TickSection::BlockEvents {
                self.game_tick
                    .checked_add(1)
                    .ok_or(RuntimeError::ClockOverflow)?
            } else {
                self.game_tick
            },
            section: TickSection::BlockEvents,
        })
    }
}

/// Identity of the receiving block, independent from power/facing state.
/// Concrete observations retain their names; native piston variants are
/// separate block identities even though both use `BlockKind::Piston`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BlockIdentity {
    kind: BlockKind,
    name: Option<String>,
    piston_variant: Option<PistonVariant>,
}

impl BlockIdentity {
    pub fn of(block: &Block) -> Self {
        Self {
            kind: block.kind,
            name: block.observed_name.clone(),
            piston_variant: (block.kind == BlockKind::Piston).then(|| crate::piston_variant(block)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuntimeCall<P> {
    pub target: Pos,
    pub payload: P,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum QueueRequest<P> {
    External {
        game_tick: u64,
        call: RuntimeCall<P>,
    },
    AfterWorldTick {
        game_tick: u64,
        call: RuntimeCall<P>,
    },
    ScheduledTick {
        game_tick: u64,
        call: RuntimeCall<P>,
    },
    /// Java block ticks: lower priority first, stable insertion order at equal
    /// priority, one pending tick per concrete block identity and position.
    /// Historical ScheduledTick requests retain their FIFO/no-dedup semantics.
    PrioritizedBlockTick {
        game_tick: u64,
        priority: u8,
        block: BlockIdentity,
        call: RuntimeCall<P>,
    },
    BlockEvent {
        block: BlockIdentity,
        call: RuntimeCall<P>,
    },
    CarrierTick {
        game_tick: u64,
        carrier: CarrierId,
        call: RuntimeCall<P>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationKind {
    Input,
    External,
    ScheduledTick,
    BlockEvent,
    CarrierTick,
    Callback,
    Continuation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Invocation<P> {
    pub id: u64,
    pub root: u64,
    pub cause: Option<u64>,
    pub time: RuntimeTime,
    pub kind: InvocationKind,
    pub depth: usize,
    pub call: RuntimeCall<P>,
}

/// The mutation is committed before callbacks run. All callbacks (including
/// their descendants) finish before the continuation is evaluated against the
/// then-current world. This exposes intermediate writes without allowing an
/// external input to interrupt a synchronous operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeOutcome<P> {
    pub delta: Option<WorldDelta>,
    pub carriers: Vec<CarrierEffect>,
    pub callbacks: Vec<RuntimeCall<P>>,
    pub continuation: Option<P>,
    pub queued: Vec<QueueRequest<P>>,
}

impl<P> Default for RuntimeOutcome<P> {
    fn default() -> Self {
        Self {
            delta: None,
            carriers: Vec::new(),
            callbacks: Vec::new(),
            continuation: None,
            queued: Vec::new(),
        }
    }
}

/// Implementations must be pure with respect to this view and invocation.
/// No adapter instance is retained: continuations and local facts belong in
/// `Payload`, not an uncheckpointed Rust closure or mutable side cache.
pub trait RuntimeAdapter: 'static {
    type Payload: Clone + std::fmt::Debug + Eq;
    const REVISION: &'static str;
    /// Required model-specific initial placement/evidence gate. A new adapter
    /// must explicitly validate its own supported blocks and known boundary;
    /// it cannot reinterpret an old placement proof as mechanical evidence.
    fn validate_initial(view: RuntimeView<'_>) -> Result<(), RuntimeError>;
    fn handle(
        invocation: &Invocation<Self::Payload>,
        view: RuntimeView<'_>,
    ) -> Result<RuntimeOutcome<Self::Payload>, RuntimeError>;
}

#[derive(Clone, Copy)]
pub struct RuntimeView<'a> {
    pub(crate) adapter_revision: &'static str,
    pub(crate) world: &'a World,
    pub(crate) region: Region,
    pub(crate) time: RuntimeTime,
    pub(crate) carriers: &'a BTreeMap<Pos, CarrierState>,
    pub(crate) staged_carriers: &'a BTreeMap<Pos, Block>,
    pub(crate) limits: RuntimeLimits,
    pub(crate) block_ticks: &'a dyn BlockTickQuery,
}

pub(crate) trait BlockTickQuery {
    fn contains(
        &self,
        position: Pos,
        block: &BlockIdentity,
        time: RuntimeTime,
        ticking: bool,
    ) -> bool;
}

impl<'a> RuntimeView<'a> {
    /// Java isQueued excludes the batch already collected for this tick.
    pub fn block_tick_queued(self, position: Pos, block: &BlockIdentity) -> bool {
        self.block_ticks.contains(position, block, self.time, false)
    }

    /// Java isTicking includes only callbacks still waiting in the ready batch.
    pub fn block_tick_ticking(self, position: Pos, block: &BlockIdentity) -> bool {
        self.block_ticks.contains(position, block, self.time, true)
    }

    /// The single physical adapter selected for this entire world.
    pub const fn adapter_revision(self) -> &'static str {
        self.adapter_revision
    }
    /// Absence denotes Air only inside the explicitly known region.
    pub fn block(self, position: Pos) -> Result<Block, RuntimeError> {
        if !self.region.contains(position) {
            return Err(RuntimeError::UnknownSpace(position));
        }
        Ok(self
            .world
            .get(position)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air)))
    }

    /// For geometry adapters that also consume `known_region`. Missing cells
    /// outside that region must never be interpreted as known Air.
    pub const fn world(self) -> &'a World {
        self.world
    }
    pub const fn known_region(self) -> Region {
        self.region
    }
    pub const fn time(self) -> RuntimeTime {
        self.time
    }
    pub fn carrier(self, position: Pos) -> Option<&'a CarrierState> {
        self.carriers.get(&position)
    }
    /// True only for a write made by this runtime before entity registration.
    /// An imported moving block without entity evidence is never accepted.
    pub fn carrier_staged(self, position: Pos) -> bool {
        self.staged_carriers.contains_key(&position)
    }
    pub const fn limits(self) -> RuntimeLimits {
        self.limits
    }
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct RuntimeLimits {
    pub max_microsteps: usize,
    pub max_pending: usize,
    pub max_call_depth: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_microsteps: 100_000,
            max_pending: 10_000,
            max_call_depth: 512,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum DeliveryResult {
    Executed,
    BlockReplaced,
    CarrierRetired,
}

/// Includes metadata-only operations and no-ops, not just visible block deltas.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuntimeRecord<P> {
    pub invocation: Invocation<P>,
    pub result: DeliveryResult,
    pub delta: Option<WorldDelta>,
    pub carrier_changes: Vec<(Pos, Option<CarrierState>, Option<CarrierState>)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    Invalid(String),
    UnknownSpace(Pos),
    CarrierConflict(Pos),
    PastDelivery {
        current: RuntimeTime,
        requested: RuntimeTime,
    },
    InputInsideCallback,
    ClockOverflow,
    Limit(&'static str),
    Handler(String),
    Failed(String),
    UnfinishedMotion,
}

impl Display for RuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RuntimeError {}
