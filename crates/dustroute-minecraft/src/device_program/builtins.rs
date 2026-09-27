//! Built-in devices. Rules and bindings have one Rust source of truth.
use super::schema::*;
use super::*;

pub const LAMP: CheckedDevice = DeviceSpec {
    id: "dustroute.device.lamp.java-1-21-11.v1",
    kind: crate::BlockKind::RedstoneLamp,
    observed_names: &["redstone_lamp"],
    synthetic: true,
    predicates: &[],
    primary_power: BoolProperty::Lit,
    properties: &[Property::Bool(BoolProperty::Lit)],
    law_id: "dustroute.law.lamp.callback.java-1-21-11.v1",
    law: &builtin_laws::LAMP,
    orientation: Orientation::None,
    signal: Signal::None,
    signal_level: SignalLevel::Binary(BoolProperty::Lit),
    conducts: true,
    full_support: true,
    fresh_powered_requires_history: false,
    initial_neighbor_update: true,
    preprocess_shapes: false,
    command_initialization: None,
    handlers: &[
        HandlerSpec {
            callback: Callback::Neighbor,
            inputs: &[
                ("lit", Query::Powered),
                ("input", Query::ReceivingPower),
                ("scheduled", Query::Constant { value: 0 }),
            ],
            operations: &[
                Operation::Write {
                    when: Binding::Output("write"),
                    values: &[(Property::Bool(BoolProperty::Lit), Binding::Output("lit"))],
                    notifications: WriteNotifications::Shapes,
                },
                Operation::Schedule {
                    delay: Binding::Output("delay"),
                    priority: 3,
                },
            ],
        },
        HandlerSpec {
            callback: Callback::Tick,
            inputs: &[
                ("lit", Query::Powered),
                ("input", Query::ReceivingPower),
                ("scheduled", Query::Constant { value: 1 }),
            ],
            operations: &[
                Operation::Write {
                    when: Binding::Output("write"),
                    values: &[(Property::Bool(BoolProperty::Lit), Binding::Output("lit"))],
                    notifications: WriteNotifications::Shapes,
                },
                Operation::Schedule {
                    delay: Binding::Output("delay"),
                    priority: 3,
                },
            ],
        },
    ],
}
.checked();
pub const OBSERVER: CheckedDevice = DeviceSpec {
    id: "dustroute.device.observer.java-1-21-11.v1",
    kind: crate::BlockKind::Observer,
    observed_names: &["observer"],
    synthetic: true,
    predicates: &[],
    primary_power: BoolProperty::Powered,
    properties: &[Property::Bool(BoolProperty::Powered)],
    law_id: "dustroute.law.observer.callback.java-1-21-11.v1",
    law: &builtin_laws::OBSERVER,
    orientation: Orientation::Output,
    signal: Signal::Output,
    signal_level: SignalLevel::Binary(BoolProperty::Powered),
    conducts: false,
    full_support: true,
    fresh_powered_requires_history: true,
    initial_neighbor_update: false,
    preprocess_shapes: true,
    command_initialization: Some((false, true)),
    handlers: &[
        HandlerSpec {
            callback: Callback::Shape,
            inputs: &[
                ("event", Query::Constant { value: 0 }),
                ("powered", Query::Powered),
                ("queued", Query::TickQueued),
                ("front", Query::SourceAtFront),
            ],
            operations: &[Operation::Schedule {
                delay: Binding::Output("delay"),
                priority: 3,
            }],
        },
        HandlerSpec {
            callback: Callback::Tick,
            inputs: &[
                ("event", Query::Constant { value: 1 }),
                ("powered", Query::Powered),
                ("queued", Query::TickQueued),
                ("front", Query::SourceAtFront),
            ],
            operations: &[
                Operation::Write {
                    when: Binding::Output("write"),
                    values: &[(
                        Property::Bool(BoolProperty::Powered),
                        Binding::Output("powered"),
                    )],
                    notifications: WriteNotifications::Shapes,
                },
                Operation::Schedule {
                    delay: Binding::Output("delay"),
                    priority: 3,
                },
                Operation::Notify {
                    when: Binding::Output("notify"),
                    targets: NotifyTargets::Output,
                },
            ],
        },
        HandlerSpec {
            callback: Callback::Added,
            inputs: &[
                ("event", Query::Constant { value: 2 }),
                ("powered", Query::Powered),
                ("queued", Query::TickQueued),
                ("front", Query::SourceAtFront),
            ],
            operations: &[Operation::Write {
                when: Binding::Output("write"),
                values: &[(
                    Property::Bool(BoolProperty::Powered),
                    Binding::Output("powered"),
                )],
                notifications: WriteNotifications::None,
            }],
        },
        HandlerSpec {
            callback: Callback::Removed,
            inputs: &[
                ("event", Query::Constant { value: 3 }),
                ("powered", Query::Powered),
                ("queued", Query::TickQueued),
                ("front", Query::SourceAtFront),
            ],
            operations: &[Operation::Notify {
                when: Binding::Output("notify"),
                targets: NotifyTargets::Output,
            }],
        },
    ],
}
.checked();
pub const STONE_BUTTON: CheckedDevice = DeviceSpec {
    id: "dustroute.device.stone-button.java-1-21-11.v1",
    kind: crate::BlockKind::Button,
    observed_names: &["stone_button"],
    synthetic: true,
    predicates: &[],
    primary_power: BoolProperty::Powered,
    properties: &[Property::Bool(BoolProperty::Powered)],
    law_id: "dustroute.law.stone-button.callback.java-1-21-11.v1",
    law: &builtin_laws::STONE_BUTTON,
    orientation: Orientation::Attached,
    signal: Signal::Attached,
    signal_level: SignalLevel::Binary(BoolProperty::Powered),
    conducts: false,
    full_support: false,
    fresh_powered_requires_history: true,
    initial_neighbor_update: false,
    preprocess_shapes: false,
    command_initialization: None,
    handlers: &[
        HandlerSpec {
            callback: Callback::Use,
            inputs: &[
                ("event", Query::Constant { value: 0 }),
                ("powered", Query::Powered),
            ],
            operations: &[
                Operation::Write {
                    when: Binding::Output("write"),
                    values: &[(
                        Property::Bool(BoolProperty::Powered),
                        Binding::Output("powered"),
                    )],
                    notifications: WriteNotifications::NeighborsAndShapes,
                },
                Operation::Notify {
                    when: Binding::Output("notify"),
                    targets: NotifyTargets::SelfAndSupport,
                },
                Operation::Schedule {
                    delay: Binding::Output("delay"),
                    priority: 3,
                },
            ],
        },
        HandlerSpec {
            callback: Callback::Tick,
            inputs: &[
                ("event", Query::Constant { value: 1 }),
                ("powered", Query::Powered),
            ],
            operations: &[
                Operation::Write {
                    when: Binding::Output("write"),
                    values: &[(
                        Property::Bool(BoolProperty::Powered),
                        Binding::Output("powered"),
                    )],
                    notifications: WriteNotifications::NeighborsAndShapes,
                },
                Operation::Notify {
                    when: Binding::Output("notify"),
                    targets: NotifyTargets::SelfAndSupport,
                },
            ],
        },
        HandlerSpec {
            callback: Callback::Removed,
            inputs: &[
                ("event", Query::Constant { value: 2 }),
                ("powered", Query::Powered),
            ],
            operations: &[Operation::Notify {
                when: Binding::Output("notify"),
                targets: NotifyTargets::SelfAndSupport,
            }],
        },
    ],
}
.checked();
pub const DEVICES: [CheckedDevice; 3] = registry([LAMP, OBSERVER, STONE_BUTTON]);
