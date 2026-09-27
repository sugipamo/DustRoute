//! Declarative local devices. Definitions bind finite laws to shared queries and
//! ordered effects; world access and delivery are supplied by the runtime.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::law::{LawProgram, finite::FiniteLaw};
use crate::{Block, BlockKind};

pub const REVISION: &str = "dustroute.device-programs.java-1-21-11.v1";

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

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    Constant { value: u16 },
    Powered,
    ReceivingPower,
    TickQueued,
    SourceAtFront,
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
    WritePower {
        when: Value,
        powered: Value,
        notifications: WriteNotifications,
    },
    Schedule {
        delay: Value,
        priority: u8,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    None,
    Output,
    Attached,
}

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
    pub observed_names: Vec<String>,
    pub law: String,
    pub power_property: String,
    pub orientation: Orientation,
    pub signal: Signal,
    pub conducts: bool,
    pub full_support: bool,
    pub fresh_powered_requires_history: bool,
    pub initial_neighbor_update: bool,
    pub preprocess_shapes: bool,
    pub command_initialization: Option<CommandInitialization>,
    pub handlers: BTreeMap<Callback, Handler>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum ResolvedEffect {
    WritePower {
        powered: bool,
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
            || !matches!(definition.power_property.as_str(), "lit" | "powered")
            || definition.handlers.is_empty()
        {
            return Err("invalid device definition identity or state property".into());
        }
        if definition.signal == Signal::Output && definition.orientation != Orientation::Output
            || definition.signal == Signal::Attached
                && definition.orientation != Orientation::Attached
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
                    _ => 1,
                };
                if maximum > input.maximum {
                    return Err("device query exceeds law input domain".into());
                }
                if matches!(input.sample, Query::SourceAtFront)
                    && definition.orientation != Orientation::Output
                {
                    return Err("front query requires output orientation".into());
                }
                if matches!(
                    callback,
                    Callback::Removed | Callback::Shape | Callback::Added
                ) && matches!(input.sample, Query::ReceivingPower)
                {
                    return Err(
                        "detached/lifecycle query cannot assume an installed receiver".into(),
                    );
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
                    Effect::WritePower { when, powered, .. } => {
                        writes += 1;
                        if bound(when)? > 1
                            || bound(powered)? > 1
                            || writes > 1
                            || *callback == Callback::Removed
                        {
                            return Err("invalid device power write".into());
                        }
                    }
                    Effect::Schedule { delay, priority } => {
                        bound(delay)?;
                        if *priority > 6
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
                                && definition.orientation != Orientation::Output
                            || *targets == NotifyTargets::SelfAndSupport
                                && definition.orientation != Orientation::Attached
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
        if before.kind != self.definition.kind {
            return Err("device definition does not match block kind".into());
        }
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
                Effect::WritePower {
                    when,
                    powered,
                    notifications,
                } => (value(when) != 0).then_some(ResolvedEffect::WritePower {
                    powered: value(powered) != 0,
                    notifications: *notifications,
                }),
                Effect::Schedule { delay, priority } => {
                    (value(delay) != 0).then_some(ResolvedEffect::Schedule {
                        delay: value(delay).into(),
                        priority: *priority,
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

pub fn definitions() -> &'static [DeviceDefinition; 3] {
    static DEFINITIONS: OnceLock<[DeviceDefinition; 3]> = OnceLock::new();
    DEFINITIONS.get_or_init(|| {
        [
            include_str!("../devices/lamp-java-1-21-11-v1.json"),
            include_str!("../devices/observer-java-1-21-11-v1.json"),
            include_str!("../devices/stone-button-java-1-21-11-v1.json"),
        ]
        .map(|json| serde_json::from_str(json).expect("embedded device definition"))
    })
}

pub fn programs() -> &'static [DeviceProgram; 3] {
    static PROGRAMS: OnceLock<[DeviceProgram; 3]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        definitions().clone().map(|definition| {
            let index = crate::device_callback_law::LAW_IDS
                .iter()
                .position(|id| *id == definition.law)
                .expect("pinned device law identity");
            DeviceProgram::compile(
                definition,
                &crate::device_callback_law::builtin_programs()[index],
            )
            .expect("compiled device definition")
        })
    })
}

pub fn program(kind: BlockKind) -> Option<&'static DeviceProgram> {
    programs().iter().find(|p| p.definition.kind == kind)
}
