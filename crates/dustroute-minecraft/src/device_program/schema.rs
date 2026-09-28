//! Rust constants are the source of device definitions. `checked()` in a const
//! initializer rejects invalid bindings and delivery contracts at compile time.
//! It may also be called at runtime, where an invalid specification panics.
//!
//! A property name must be a typed identifier:
//! ```compile_fail,E0308
//! use dustroute_minecraft::device_program::Property;
//! const BAD: Property = Property::Bool("lit");
//! ```
//! A registry cannot contain overlapping definitions:
//! ```compile_fail,E0080
//! use dustroute_minecraft::device_program::{BUILTIN_DEVICES, schema::*};
//! const BAD: [CheckedDevice; 2] = registry([
//!     BUILTIN_DEVICES[0],
//!     DeviceSpec { id: "different.id", ..BUILTIN_DEVICES[0].spec() }.checked(),
//! ]);
//! ```
//! Invalid effect bindings fail at the constant that contains them:
//! ```compile_fail,E0080
//! use dustroute_minecraft::device_program::{*, schema::*};
//! const BASE: DeviceSpec = BUILTIN_DEVICES[2].spec();
//! const BAD: CheckedDevice = DeviceSpec {
//!     handlers: &[HandlerSpec {
//!         callback: Callback::Use, inputs: BASE.handlers[0].inputs,
//!         operations: &[Operation::Notify {
//!             when: Binding::Output("misspelled"), targets: NotifyTargets::SelfAndSupport,
//!         }],
//!     }], ..BASE
//! }.checked();
//! ```
//! Construction settings are readable but cannot be assigned by callbacks:
//! ```compile_fail,E0080
//! use dustroute_minecraft::device_program::{*, schema::*};
//! const BASE: DeviceSpec = BUILTIN_DEVICES[3].spec();
//! const BAD: CheckedDevice = DeviceSpec {
//!     handlers: &[HandlerSpec {
//!         callback: Callback::Tick, inputs: BASE.handlers[1].inputs,
//!         operations: &[Operation::Write {
//!             when: Binding::Constant(1), values: &[(Property::Delay, Binding::Constant(1))],
//!             notifications: WriteNotifications::None,
//!         }],
//!     }], ..BASE
//! }.checked();
//! ```
//! Computed tick priorities must stay in the scheduler's 0..=6 range:
//! ```compile_fail,E0080
//! use dustroute_minecraft::device_program::{*, schema::*};
//! const BASE: DeviceSpec = BUILTIN_DEVICES[3].spec();
//! const BAD: CheckedDevice = DeviceSpec {
//!     handlers: &[HandlerSpec {
//!         callback: Callback::Tick, inputs: BASE.handlers[1].inputs,
//!         operations: &[Operation::Schedule {
//!             delay: Binding::Constant(2), priority: Binding::Output("delay"),
//!         }],
//!     }], ..BASE
//! }.checked();
//! ```
//! An installed shape query cannot be reused for pre-insertion processing:
//! ```compile_fail,E0080
//! use dustroute_minecraft::device_program::{*, schema::*};
//! const BAD: CheckedDevice = DeviceSpec {
//!     preprocess_shapes: true, ..BUILTIN_DEVICES[3].spec()
//! }.checked();
//! ```
//! Boolean properties cannot receive analog values:
//! ```compile_fail,E0080
//! use dustroute_minecraft::device_program::{*, schema::*};
//! const BASE: DeviceSpec = BUILTIN_DEVICES[2].spec();
//! const BAD: CheckedDevice = DeviceSpec {
//!     handlers: &[HandlerSpec {
//!         callback: Callback::Use, inputs: BASE.handlers[0].inputs,
//!         operations: &[Operation::Write {
//!             when: Binding::Constant(1),
//!             values: &[(Property::Bool(BoolProperty::Powered), Binding::Constant(15))],
//!             notifications: WriteNotifications::None,
//!         }],
//!     }], ..BASE
//! }.checked();
//! ```
//! A timer requires a tick handler:
//! ```compile_fail,E0080
//! use dustroute_minecraft::device_program::{*, schema::*};
//! const BASE: DeviceSpec = BUILTIN_DEVICES[2].spec();
//! const BAD: CheckedDevice = DeviceSpec {
//!     handlers: &[BASE.handlers[0]], ..BASE
//! }.checked();
//! ```
//! Pre-write shape callbacks cannot mutate the world:
//! ```compile_fail,E0080
//! use dustroute_minecraft::device_program::{*, schema::*};
//! const BASE: DeviceSpec = BUILTIN_DEVICES[1].spec();
//! const BAD: CheckedDevice = DeviceSpec {
//!     handlers: &[HandlerSpec {
//!         callback: Callback::Shape, inputs: BASE.handlers[0].inputs,
//!         operations: &[Operation::Write {
//!             when: Binding::Constant(1),
//!             values: &[(Property::Bool(BoolProperty::Powered), Binding::Constant(1))],
//!             notifications: WriteNotifications::None,
//!         }],
//!     }], ..BASE
//! }.checked();
//! ```
use super::*;
use crate::{
    BlockKind,
    law::static_program::{StaticLaw, same},
};

#[derive(Clone, Copy, Debug)]
pub enum Binding {
    Constant(u16),
    Output(&'static str),
}

impl Binding {
    const fn bound(self, law: &StaticLaw) -> u16 {
        match self {
            Self::Constant(n) => n,
            Self::Output(name) => law.output_bound(name),
        }
    }
    fn owned(self) -> Value {
        match self {
            Self::Constant(number) => Value::Constant { number },
            Self::Output(name) => Value::Output { name: name.into() },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Operation {
    Write {
        when: Binding,
        values: &'static [(Property, Binding)],
        notifications: WriteNotifications,
    },
    Schedule {
        delay: Binding,
        priority: Binding,
    },
    Notify {
        when: Binding,
        targets: NotifyTargets,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct HandlerSpec {
    pub callback: Callback,
    pub inputs: &'static [(&'static str, Query)],
    pub operations: &'static [Operation],
}

#[derive(Clone, Copy, Debug)]
pub struct DeviceSpec {
    pub id: &'static str,
    pub kind: BlockKind,
    pub observed_names: &'static [&'static str],
    pub synthetic: bool,
    pub predicates: &'static [(Property, u16)],
    pub primary_power: BoolProperty,
    pub properties: &'static [Property],
    pub law_id: &'static str,
    pub law: &'static StaticLaw,
    pub orientation: Orientation,
    pub signal: Signal,
    pub wire_connection: WireConnectionRule,
    pub signal_level: SignalLevel,
    pub conducts: bool,
    pub full_support: bool,
    pub fresh_powered_requires_history: bool,
    pub initial_neighbor_update: bool,
    pub preprocess_shapes: bool,
    pub command_initialization: Option<(bool, bool)>,
    pub handlers: &'static [HandlerSpec],
}

#[derive(Clone, Copy, Debug)]
pub struct CheckedDevice(DeviceSpec);

impl DeviceSpec {
    const fn declares(&self, property: Property) -> bool {
        let mut i = 0;
        while i < self.properties.len() {
            if self.properties[i].same(property) {
                return true;
            }
            i += 1;
        }
        false
    }

    const fn handles(&self, callback: Callback) -> bool {
        let mut i = 0;
        while i < self.handlers.len() {
            if self.handlers[i].callback as u8 == callback as u8 {
                return true;
            }
            i += 1;
        }
        false
    }

    pub const fn checked(self) -> CheckedDevice {
        assert!(
            !self.id.is_empty() && !self.law_id.is_empty(),
            "empty device identity"
        );
        assert!(
            !self.observed_names.is_empty(),
            "device needs concrete identities"
        );
        let mut i = 0;
        while i < self.observed_names.len() {
            let name = self.observed_names[i].as_bytes();
            assert!(!name.is_empty(), "empty concrete identity");
            let mut j = 0;
            while j < name.len() {
                assert!(name[j] != b':', "device identity must omit namespace");
                j += 1;
            }
            j = 0;
            while j < i {
                assert!(
                    !same(self.observed_names[i], self.observed_names[j]),
                    "duplicate concrete identity"
                );
                j += 1;
            }
            i += 1;
        }
        assert!(
            self.declares(Property::Bool(self.primary_power)),
            "missing primary state"
        );
        assert!(
            self.declares(self.signal_level.property()),
            "undeclared signal state"
        );
        i = 0;
        while i < self.properties.len() {
            let mut j = 0;
            while j < i {
                assert!(
                    !self.properties[i].same(self.properties[j]),
                    "duplicate device property"
                );
                j += 1;
            }
            i += 1;
        }
        i = 0;
        while i < self.predicates.len() {
            let (p, v) = self.predicates[i];
            assert!(
                self.declares(p) && v <= p.maximum(),
                "invalid selector property"
            );
            let mut j = 0;
            while j < i {
                assert!(!p.same(self.predicates[j].0), "duplicate selector property");
                j += 1;
            }
            i += 1;
        }
        assert!(
            !matches!(self.signal, Signal::Output) || self.orientation.has_output(),
            "output signal needs orientation"
        );
        assert!(
            !matches!(self.signal, Signal::Attached)
                || matches!(self.orientation, Orientation::Attached),
            "attached signal needs orientation"
        );
        assert!(
            !matches!(
                self.wire_connection,
                WireConnectionRule::Output | WireConnectionRule::Axis
            ) || self.orientation.has_output(),
            "wire connection needs orientation"
        );
        assert!(
            !self.preprocess_shapes || self.handles(Callback::Shape),
            "missing prewrite shape handler"
        );
        assert!(!self.handlers.is_empty(), "device needs handlers");
        i = 0;
        while i < self.handlers.len() {
            let h = &self.handlers[i];
            let mut j = 0;
            while j < i {
                assert!(
                    h.callback as u8 != self.handlers[j].callback as u8,
                    "duplicate device callback"
                );
                j += 1;
            }
            assert!(
                h.inputs.len() == self.law.inputs().len(),
                "law query ABI mismatch"
            );
            j = 0;
            while j < h.inputs.len() {
                let (name, q) = h.inputs[j];
                let mut k = 0;
                while k < j {
                    assert!(!same(name, h.inputs[k].0), "duplicate query binding");
                    k += 1;
                }
                let maximum = match q {
                    Query::Constant { value } => value,
                    Query::State { property } => {
                        assert!(self.declares(property), "undeclared query property");
                        property.maximum()
                    }
                    Query::ReceivingLevel | Query::SideLevel { .. } => 15,
                    _ => 1,
                };
                assert!(
                    maximum <= self.law.input_bound(name),
                    "query exceeds law input range"
                );
                assert!(
                    !matches!(q, Query::SourceAtFront | Query::SourceOffAxis)
                        || self.orientation.has_output(),
                    "front query needs orientation"
                );
                assert!(
                    !matches!(
                        q,
                        Query::GateInputPowered
                            | Query::SideGatePowered
                            | Query::OutputGateMisaligned
                    ) || matches!(self.orientation, Orientation::FloorOutput),
                    "gate query needs horizontal floor output"
                );
                assert!(
                    !(matches!(h.callback, Callback::Removed | Callback::Added)
                        || matches!(h.callback, Callback::Shape) && self.preprocess_shapes)
                        || !matches!(
                            q,
                            Query::ReceivingPower
                                | Query::ReceivingLevel
                                | Query::SideLevel { .. }
                                | Query::GateInputPowered
                                | Query::SideGatePowered
                                | Query::OutputGateMisaligned
                        ),
                    "detached query needs installed receiver"
                );
                j += 1;
            }
            assert!(h.operations.len() <= 32, "device operation budget exceeded");
            let mut writes = 0;
            j = 0;
            while j < h.operations.len() {
                let op = &h.operations[j];
                assert!(
                    !matches!(h.callback, Callback::Shape)
                        || !self.preprocess_shapes
                        || matches!(op, Operation::Schedule { .. }),
                    "prewrite shape may only schedule"
                );
                match op {
                    Operation::Write {
                        when,
                        values,
                        notifications,
                    } => {
                        writes += 1;
                        assert!(
                            writes <= 1 && !matches!(h.callback, Callback::Removed),
                            "only one atomic write; none on removal"
                        );
                        assert!(
                            when.bound(self.law) <= 1 && !values.is_empty(),
                            "invalid write guard or empty write"
                        );
                        assert!(
                            !matches!(notifications, WriteNotifications::OutputAndShapes)
                                || self.orientation.has_output(),
                            "output notification needs orientation"
                        );
                        let mut k = 0;
                        while k < values.len() {
                            let (p, v) = values[k];
                            assert!(
                                self.declares(p)
                                    && p.writable()
                                    && v.bound(self.law) <= p.maximum(),
                                "property assignment outside declared range"
                            );
                            let mut m = 0;
                            while m < k {
                                assert!(!p.same(values[m].0), "duplicate property assignment");
                                m += 1;
                            }
                            k += 1;
                        }
                    }
                    Operation::Schedule { delay, priority } => {
                        delay.bound(self.law);
                        assert!(
                            priority.bound(self.law) <= 6
                                && self.handles(Callback::Tick)
                                && !matches!(h.callback, Callback::Removed),
                            "schedule needs tick handler and valid priority"
                        );
                    }
                    Operation::Notify { when, targets } => {
                        assert!(
                            when.bound(self.law) <= 1,
                            "notification guard needs Boolean"
                        );
                        assert!(
                            match targets {
                                NotifyTargets::Output => self.orientation.has_output(),
                                NotifyTargets::SelfAndSupport =>
                                    matches!(self.orientation, Orientation::Attached),
                            },
                            "notification needs matching orientation"
                        );
                    }
                }
                j += 1;
            }
            i += 1;
        }
        CheckedDevice(self)
    }
}

/// Validate the whole registry in a const initializer, including synthetic
/// defaults and intersecting state predicates. There is no first-match priority.
pub const fn registry<const N: usize>(devices: [CheckedDevice; N]) -> [CheckedDevice; N] {
    let mut i = 0;
    while i < N {
        let a = &devices[i].0;
        let mut j = 0;
        while j < i {
            let b = &devices[j].0;
            assert!(!same(a.id, b.id), "duplicate device id");
            if a.kind as u8 == b.kind as u8 {
                let mut overlaps = a.synthetic && b.synthetic;
                let mut n = 0;
                while n < a.observed_names.len() {
                    let mut m = 0;
                    while m < b.observed_names.len() {
                        if same(a.observed_names[n], b.observed_names[m]) {
                            overlaps = true;
                        }
                        m += 1;
                    }
                    n += 1;
                }
                let mut disjoint = false;
                n = 0;
                while n < a.predicates.len() {
                    let mut m = 0;
                    while m < b.predicates.len() {
                        if a.predicates[n].0.same(b.predicates[m].0)
                            && a.predicates[n].1 != b.predicates[m].1
                        {
                            disjoint = true;
                        }
                        m += 1;
                    }
                    n += 1;
                }
                // The canonical powered field must mean the same thing for all
                // variants of a kind, including across state-dependent selection.
                assert!(
                    a.primary_power as u8 == b.primary_power as u8,
                    "variant primary state mismatch"
                );
                assert!(!overlaps || disjoint, "ambiguous device variants");
            }
            j += 1;
        }
        i += 1;
    }
    devices
}

impl CheckedDevice {
    /// Copy a specification for a new checked variant. The compiled program
    /// remains immutable; edited specifications must pass `checked()` again.
    pub const fn spec(self) -> DeviceSpec {
        self.0
    }
    pub fn definition(self) -> DeviceDefinition {
        let s = self.0;
        DeviceDefinition {
            id: s.id.into(),
            kind: s.kind,
            observed_names: s.observed_names.iter().map(|n| (*n).into()).collect(),
            synthetic: s.synthetic,
            predicates: s.predicates.into(),
            primary_power: s.primary_power,
            properties: s.properties.into(),
            law: s.law_id.into(),
            orientation: s.orientation,
            signal: s.signal,
            wire_connection: s.wire_connection,
            signal_level: s.signal_level,
            conducts: s.conducts,
            full_support: s.full_support,
            fresh_powered_requires_history: s.fresh_powered_requires_history,
            initial_neighbor_update: s.initial_neighbor_update,
            preprocess_shapes: s.preprocess_shapes,
            command_initialization: s.command_initialization.map(
                |(declared_powered, requested_powered)| CommandInitialization {
                    declared_powered,
                    requested_powered,
                },
            ),
            handlers: s
                .handlers
                .iter()
                .map(|h| {
                    (
                        h.callback,
                        Handler {
                            inputs: h
                                .inputs
                                .iter()
                                .map(|(name, sample)| Input {
                                    name: (*name).into(),
                                    maximum: s.law.input_bound(name),
                                    sample: *sample,
                                })
                                .collect(),
                            outputs: s
                                .law
                                .outputs()
                                .iter()
                                .map(|(name, maximum)| Output {
                                    name: (*name).into(),
                                    maximum: *maximum,
                                })
                                .collect(),
                            effects: h
                                .operations
                                .iter()
                                .map(|op| match op {
                                    Operation::Write {
                                        when,
                                        values,
                                        notifications,
                                    } => Effect::WriteState {
                                        when: when.owned(),
                                        values: values
                                            .iter()
                                            .map(|(p, v)| (*p, v.owned()))
                                            .collect(),
                                        notifications: *notifications,
                                    },
                                    Operation::Schedule { delay, priority } => Effect::Schedule {
                                        delay: delay.owned(),
                                        priority: priority.owned(),
                                    },
                                    Operation::Notify { when, targets } => Effect::Notify {
                                        when: when.owned(),
                                        targets: *targets,
                                    },
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
        }
    }
    pub fn compile(self) -> DeviceProgram {
        DeviceProgram::compile(self.definition(), &self.0.law.program())
            .expect("const-checked device")
    }
}
