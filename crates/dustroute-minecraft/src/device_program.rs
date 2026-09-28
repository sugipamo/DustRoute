//! Declarative local devices. Definitions bind finite laws to shared queries and
//! ordered effects; world access and delivery are supplied by the runtime.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::law::{LawProgram, finite::FiniteLaw};
use crate::{Block, BlockKind};

pub(crate) mod builtin_laws;
mod builtins;
pub mod schema;
mod state;
pub use state::{BoolProperty, Property, SignalLevel};

pub const REVISION: &str = "dustroute.device-programs.java-1-21-11.v4";
pub const BUILTIN_DEVICES: [schema::CheckedDevice; 4] = builtins::DEVICES;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Callback {
    Neighbor,
    Shape,
    Tick,
    Added,
    Removed,
    Use,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    Constant {
        value: u16,
    },
    Powered,
    ReceivingPower,
    TickQueued,
    /// Tick collected for the current batch, distinct from a future reservation.
    TickCollected,
    SourceAtFront,
    SourceOffAxis,
    GateInputPowered,
    SideGatePowered,
    OutputGateMisaligned,
    State {
        property: Property,
    },
    ReceivingLevel,
    SideLevel {
        side: crate::Facing,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub name: String,
    pub maximum: u16,
    pub sample: Query,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub name: String,
    pub maximum: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum Value {
    Constant { number: u16 },
    Output { name: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteNotifications {
    None,
    Shapes,
    NeighborsAndShapes,
    /// Installed gate output callbacks run before the write's shape callbacks.
    OutputAndShapes,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyTargets {
    Output,
    SelfAndSupport,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    WriteState {
        when: Value,
        values: Vec<(Property, Value)>,
        notifications: WriteNotifications,
    },
    Schedule {
        delay: Value,
        priority: Value,
    },
    Notify {
        when: Value,
        targets: NotifyTargets,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handler {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub effects: Vec<Effect>,
}

pub use crate::physical::{Orientation, WireConnectionRule};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    None,
    Output,
    Attached,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandInitialization {
    pub declared_powered: bool,
    pub requested_powered: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceDefinition {
    pub id: String,
    pub kind: BlockKind,
    pub physical: crate::physical::CheckedPhysical,
    pub observed_names: Vec<String>,
    pub law: String,
    pub primary_power: BoolProperty,
    pub properties: Vec<Property>,
    pub synthetic: bool,
    pub predicates: Vec<(Property, u16)>,
    pub signal_level: SignalLevel,
    pub signal: Signal,
    pub fresh_powered_requires_history: bool,
    pub initial_neighbor_update: bool,
    pub preprocess_shapes: bool,
    pub command_initialization: Option<CommandInitialization>,
    pub handlers: BTreeMap<Callback, Handler>,
}

impl DeviceDefinition {
    pub const fn physical(&self) -> crate::physical::CheckedPhysical {
        self.physical
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum ResolvedEffect {
    WriteState {
        values: Vec<(Property, u16)>,
        notifications: WriteNotifications,
    },
    Schedule {
        delay: u64,
        priority: u8,
    },
    Notify {
        targets: NotifyTargets,
    },
}

/// A program's captured decision and program counter are real runtime state.
/// Only compilation/preparation can create one; serialized diagnostics cannot
/// be imported as trusted continuations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeviceRun {
    pub(crate) revision: String,
    pub(crate) definition: String,
    pub(crate) before: Box<Block>,
    pub(crate) effects: Vec<ResolvedEffect>,
    pub(crate) next: usize,
}

impl DeviceRun {
    pub fn effects(&self) -> &[ResolvedEffect] {
        &self.effects
    }
}

#[derive(Debug)]
pub struct DeviceProgram {
    pub(crate) definition: DeviceDefinition,
    tables: BTreeMap<Callback, FiniteLaw>,
}

impl DeviceProgram {
    pub fn definition(&self) -> &DeviceDefinition {
        &self.definition
    }

    pub fn compile(definition: DeviceDefinition, law: &LawProgram) -> Result<Self, String> {
        if definition.id.trim().is_empty()
            || definition.law.trim().is_empty()
            || definition.observed_names.is_empty()
            || definition
                .observed_names
                .iter()
                .any(|n| n.is_empty() || n.contains(':'))
            || definition
                .observed_names
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != definition.observed_names.len()
            || !definition
                .properties
                .contains(&Property::Bool(definition.primary_power))
            || !definition
                .properties
                .contains(&definition.signal_level.property())
            || definition
                .properties
                .iter()
                .enumerate()
                .any(|(i, p)| definition.properties[..i].contains(p))
            || definition.predicates.iter().enumerate().any(|(i, (p, v))| {
                !definition.properties.contains(p)
                    || *v > p.maximum()
                    || definition.predicates[..i].iter().any(|(q, _)| p == q)
            })
            || definition.handlers.is_empty()
        {
            return Err("invalid device definition identity or state property".into());
        }
        if definition.signal == Signal::Output && !definition.physical().orientation().has_output()
            || definition.signal == Signal::Attached
                && definition.physical().orientation() != Orientation::Attached
        {
            return Err("device signal requires matching orientation".into());
        }

        let mut tables = BTreeMap::new();
        for (callback, handler) in &definition.handlers {
            if handler.effects.len() > 32 {
                return Err("device effect budget exceeded".into());
            }
            for input in &handler.inputs {
                let maximum = match input.sample {
                    Query::Constant { value } => value,
                    Query::State { property } => {
                        if !definition.properties.contains(&property) {
                            return Err("undeclared device state query".into());
                        }
                        property.maximum()
                    }
                    Query::ReceivingLevel | Query::SideLevel { .. } => 15,
                    _ => 1,
                };
                if maximum > input.maximum {
                    return Err("device query exceeds law input domain".into());
                }
                if matches!(input.sample, Query::SourceAtFront | Query::SourceOffAxis)
                    && !definition.physical().orientation().has_output()
                {
                    return Err("front query requires output orientation".into());
                }
                if matches!(
                    input.sample,
                    Query::GateInputPowered | Query::SideGatePowered | Query::OutputGateMisaligned
                ) && definition.physical().orientation() != Orientation::FloorOutput
                {
                    return Err("gate query requires horizontal floor output".into());
                }
                if matches!(callback, Callback::Removed | Callback::Added)
                    && matches!(
                        input.sample,
                        Query::ReceivingPower
                            | Query::ReceivingLevel
                            | Query::SideLevel { .. }
                            | Query::GateInputPowered
                            | Query::SideGatePowered
                            | Query::OutputGateMisaligned
                    )
                {
                    return Err(
                        "detached/lifecycle query cannot assume an installed receiver".into(),
                    );
                }
                if *callback == Callback::Shape
                    && definition.preprocess_shapes
                    && matches!(
                        input.sample,
                        Query::ReceivingPower
                            | Query::ReceivingLevel
                            | Query::SideLevel { .. }
                            | Query::GateInputPowered
                            | Query::SideGatePowered
                            | Query::OutputGateMisaligned
                    )
                {
                    return Err("pre-write query cannot assume an installed receiver".into());
                }
            }
            let bound = |value: &Value| -> Result<u16, String> {
                match value {
                    Value::Constant { number } => Ok(*number),
                    Value::Output { name } => handler
                        .outputs
                        .iter()
                        .find(|o| o.name == *name)
                        .map(|o| o.maximum)
                        .ok_or_else(|| "unknown device output".into()),
                }
            };
            let mut writes = 0;
            for effect in &handler.effects {
                match effect {
                    Effect::WriteState {
                        when,
                        values,
                        notifications,
                    } => {
                        writes += 1;
                        if bound(when)? > 1
                            || values.is_empty()
                            || writes > 1
                            || *callback == Callback::Removed
                        {
                            return Err("invalid device state write".into());
                        }
                        if *notifications == WriteNotifications::OutputAndShapes
                            && !definition.physical().orientation().has_output()
                        {
                            return Err("output notifications require output orientation".into());
                        }
                        for (index, (property, value)) in values.iter().enumerate() {
                            if !definition.properties.contains(property)
                                || !property.writable()
                                || bound(value)? > property.maximum()
                                || values[..index].iter().any(|(p, _)| p == property)
                            {
                                return Err("invalid device property assignment".into());
                            }
                        }
                    }
                    Effect::Schedule { delay, priority } => {
                        bound(delay)?;
                        if bound(priority)? > 6
                            || !definition.handlers.contains_key(&Callback::Tick)
                            || *callback == Callback::Removed
                        {
                            return Err(
                                "device schedule requires a tick handler and valid priority".into(),
                            );
                        }
                    }
                    Effect::Notify { when, targets } => {
                        if bound(when)? > 1
                            || *targets == NotifyTargets::Output
                                && !definition.physical().orientation().has_output()
                            || *targets == NotifyTargets::SelfAndSupport
                                && definition.physical().orientation() != Orientation::Attached
                        {
                            return Err("invalid notification binding".into());
                        }
                    }
                }
                if *callback == Callback::Shape
                    && definition.preprocess_shapes
                    && !matches!(effect, Effect::Schedule { .. })
                {
                    return Err("pre-write shapes may only enqueue ticks".into());
                }
            }
            let inputs: Vec<_> = handler
                .inputs
                .iter()
                .map(|i| (i.name.as_str(), i.maximum))
                .collect();
            let outputs: Vec<_> = handler
                .outputs
                .iter()
                .map(|o| (o.name.as_str(), o.maximum))
                .collect();
            tables.insert(*callback, FiniteLaw::compile(law, &inputs, &outputs)?);
        }
        if definition.preprocess_shapes && !definition.handlers.contains_key(&Callback::Shape) {
            return Err("preprocessing requires a shape handler".into());
        }
        Ok(Self { definition, tables })
    }

    pub fn prepare(
        &self,
        callback: Callback,
        before: &Block,
        mut sample: impl FnMut(&Query) -> Result<u16, String>,
    ) -> Result<Option<DeviceRun>, String> {
        if !self.definition.matches(before) {
            return Err("device definition does not match block kind".into());
        }
        self.definition.validate_state(before)?;
        let Some(handler) = self.definition.handlers.get(&callback) else {
            return Ok(None);
        };
        let values = handler
            .inputs
            .iter()
            .map(|i| sample(&i.sample))
            .collect::<Result<Vec<_>, _>>()?;
        let row = self.tables[&callback]
            .evaluate(&values)
            .ok_or("device facts outside law domain")?;
        let value = |v: &Value| match v {
            Value::Constant { number } => *number,
            Value::Output { name } => {
                row[handler
                    .outputs
                    .iter()
                    .position(|o| o.name == *name)
                    .expect("compiled output")]
            }
        };
        let effects = handler
            .effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::WriteState {
                    when,
                    values,
                    notifications,
                } => (value(when) != 0).then_some(ResolvedEffect::WriteState {
                    values: values.iter().map(|(p, v)| (*p, value(v))).collect(),
                    notifications: *notifications,
                }),
                Effect::Schedule { delay, priority } => {
                    (value(delay) != 0).then_some(ResolvedEffect::Schedule {
                        delay: value(delay).into(),
                        priority: value(priority) as u8,
                    })
                }
                Effect::Notify { when, targets } => {
                    (value(when) != 0).then_some(ResolvedEffect::Notify { targets: *targets })
                }
            })
            .collect();
        Ok(Some(DeviceRun {
            revision: REVISION.into(),
            definition: self.definition.id.clone(),
            before: Box::new(before.clone()),
            effects,
            next: 0,
        }))
    }
}

impl DeviceDefinition {
    pub fn matches(&self, block: &Block) -> bool {
        block.kind == self.kind
            && match block.observed_name.as_deref() {
                Some(name) => self
                    .observed_names
                    .iter()
                    .any(|n| n == name.strip_prefix("minecraft:").unwrap_or(name)),
                None => self.synthetic,
            }
            && self
                .predicates
                .iter()
                .all(|(p, v)| self.state(block, *p) == Ok(*v))
    }
}

pub fn definitions() -> &'static [DeviceDefinition; 4] {
    static DEFINITIONS: OnceLock<[DeviceDefinition; 4]> = OnceLock::new();
    DEFINITIONS.get_or_init(|| builtins::DEVICES.map(|d| d.definition()))
}

pub fn programs() -> &'static [DeviceProgram; 4] {
    static PROGRAMS: OnceLock<[DeviceProgram; 4]> = OnceLock::new();
    PROGRAMS.get_or_init(|| builtins::DEVICES.map(|d| d.compile()))
}

/// Concrete identity and state select the program; unknown variants never fall
/// back to a different material. Ambiguity fails closed, even for custom slices.
pub fn select<'a>(programs: &'a [DeviceProgram], block: &Block) -> Option<&'a DeviceProgram> {
    let mut matches = programs.iter().filter(|p| p.definition.matches(block));
    let selected = matches.next()?;
    matches.next().is_none().then_some(selected)
}

pub fn program(block: &Block) -> Option<&'static DeviceProgram> {
    select(programs(), block)
}
