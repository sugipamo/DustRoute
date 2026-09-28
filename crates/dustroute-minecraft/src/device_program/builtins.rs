//! Built-in devices. Rules and bindings have one Rust source of truth.
use super::schema::*;
use super::*;

const BULB_INPUTS: &[(&str, Query)] = &[
    (
        "powered",
        Query::State {
            property: Property::Bool(BoolProperty::Powered),
        },
    ),
    ("lit", Query::Powered),
    ("input", Query::ReceivingPower),
];
const BULB_WRITE: &[Operation] = &[Operation::Write {
    when: Binding::Output("write"),
    values: &[
        (
            Property::Bool(BoolProperty::Powered),
            Binding::Output("powered"),
        ),
        (Property::Bool(BoolProperty::Lit), Binding::Output("lit")),
    ],
    notifications: WriteNotifications::NeighborsAndShapes,
}];
pub const COPPER_BULB: CheckedDevice = DeviceSpec {
    id: "dustroute.device.waxed-copper-bulb.java-1-21-11.v1",
    kind: crate::BlockKind::CopperBulb,
    physical: crate::physical::of_kind(crate::BlockKind::CopperBulb),
    observed_names: &[
        "waxed_copper_bulb",
        "waxed_exposed_copper_bulb",
        "waxed_weathered_copper_bulb",
        "waxed_oxidized_copper_bulb",
    ],
    synthetic: true,
    predicates: &[],
    primary_power: BoolProperty::Lit,
    properties: &[
        Property::Bool(BoolProperty::Lit),
        Property::Bool(BoolProperty::Powered),
    ],
    law_id: "dustroute.law.waxed-copper-bulb.callback.java-1-21-11.v1",
    law: &builtin_laws::COPPER_BULB,
    signal: Signal::None,
    signal_level: SignalLevel::Binary(BoolProperty::Lit),
    comparator_output: Some(SignalLevel::Binary(BoolProperty::Lit)),
    fresh_powered_requires_history: false,
    initial_neighbor_update: true,
    preprocess_shapes: false,
    command_initialization: None,
    handlers: &[
        HandlerSpec {
            callback: Callback::Neighbor,
            inputs: BULB_INPUTS,
            operations: BULB_WRITE,
        },
        HandlerSpec {
            callback: Callback::Added,
            inputs: BULB_INPUTS,
            operations: BULB_WRITE,
        },
    ],
}
.checked();

pub const LAMP: CheckedDevice = DeviceSpec {
    id: "dustroute.device.lamp.java-1-21-11.v1",
    kind: crate::BlockKind::RedstoneLamp,
    physical: crate::physical::of_kind(crate::BlockKind::RedstoneLamp),
    observed_names: &["redstone_lamp"],
    synthetic: true,
    predicates: &[],
    primary_power: BoolProperty::Lit,
    properties: &[Property::Bool(BoolProperty::Lit)],
    law_id: "dustroute.law.lamp.callback.java-1-21-11.v1",
    law: &builtin_laws::LAMP,
    signal: Signal::None,
    signal_level: SignalLevel::Binary(BoolProperty::Lit),
    comparator_output: None,
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
                    priority: Binding::Constant(3),
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
                    priority: Binding::Constant(3),
                },
            ],
        },
    ],
}
.checked();
pub const OBSERVER: CheckedDevice = DeviceSpec {
    id: "dustroute.device.observer.java-1-21-11.v1",
    kind: crate::BlockKind::Observer,
    physical: crate::physical::of_kind(crate::BlockKind::Observer),
    observed_names: &["observer"],
    synthetic: true,
    predicates: &[],
    primary_power: BoolProperty::Powered,
    properties: &[Property::Bool(BoolProperty::Powered)],
    law_id: "dustroute.law.observer.callback.java-1-21-11.v1",
    law: &builtin_laws::OBSERVER,
    signal: Signal::Output,
    signal_level: SignalLevel::Binary(BoolProperty::Powered),
    comparator_output: None,
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
                priority: Binding::Constant(3),
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
                    priority: Binding::Constant(3),
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
    physical: crate::physical::of_kind(crate::BlockKind::Button),
    observed_names: &["stone_button"],
    synthetic: true,
    predicates: &[],
    primary_power: BoolProperty::Powered,
    properties: &[Property::Bool(BoolProperty::Powered)],
    law_id: "dustroute.law.stone-button.callback.java-1-21-11.v1",
    law: &builtin_laws::STONE_BUTTON,
    signal: Signal::Attached,
    signal_level: SignalLevel::Binary(BoolProperty::Powered),
    comparator_output: None,
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
                    priority: Binding::Constant(3),
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
const fn gate_inputs(event: u16) -> [(&'static str, Query); 9] {
    [
        ("event", Query::Constant { value: event }),
        (
            "powered",
            if event <= 1 {
                Query::Powered
            } else {
                Query::Constant { value: 0 }
            },
        ),
        (
            "input",
            if event <= 1 {
                Query::GateInputPowered
            } else {
                Query::Constant { value: 0 }
            },
        ),
        (
            "locked",
            if event <= 2 {
                Query::SideGatePowered
            } else {
                Query::Constant { value: 0 }
            },
        ),
        (
            "misaligned",
            if event == 0 {
                Query::OutputGateMisaligned
            } else {
                Query::Constant { value: 0 }
            },
        ),
        (
            "delay_setting",
            Query::State {
                property: Property::Delay,
            },
        ),
        (
            "ticking",
            if event == 0 {
                Query::TickCollected
            } else {
                Query::Constant { value: 0 }
            },
        ),
        (
            "stored_locked",
            if event == 2 {
                Query::State {
                    property: Property::Bool(BoolProperty::Locked),
                }
            } else {
                Query::Constant { value: 0 }
            },
        ),
        (
            "off_axis",
            if event == 2 {
                Query::SourceOffAxis
            } else {
                Query::Constant { value: 0 }
            },
        ),
    ]
}

pub const REPEATER: CheckedDevice = DeviceSpec {
    id: "dustroute.device.repeater.java-1-21-11.v1",
    kind: BlockKind::Repeater,
    physical: crate::physical::of_kind(BlockKind::Repeater),
    observed_names: &["repeater"],
    synthetic: true,
    predicates: &[],
    primary_power: BoolProperty::Powered,
    properties: &[
        Property::Bool(BoolProperty::Powered),
        Property::Bool(BoolProperty::Locked),
        Property::Delay,
    ],
    law_id: "dustroute.law.repeater.callback.java-1-21-11.v2",
    law: &builtin_laws::REPEATER,
    signal: Signal::Output,
    signal_level: SignalLevel::Binary(BoolProperty::Powered),
    comparator_output: None,
    fresh_powered_requires_history: false,
    initial_neighbor_update: true,
    preprocess_shapes: false,
    command_initialization: None,
    handlers: &[
        HandlerSpec {
            callback: Callback::Neighbor,
            inputs: &gate_inputs(0),
            operations: &[Operation::Schedule {
                delay: Binding::Output("delay"),
                priority: Binding::Output("priority"),
            }],
        },
        HandlerSpec {
            callback: Callback::Tick,
            inputs: &gate_inputs(1),
            operations: &[
                Operation::Write {
                    when: Binding::Output("write"),
                    values: &[(
                        Property::Bool(BoolProperty::Powered),
                        Binding::Output("powered"),
                    )],
                    notifications: WriteNotifications::OutputAndShapes,
                },
                Operation::Schedule {
                    delay: Binding::Output("delay"),
                    priority: Binding::Output("priority"),
                },
            ],
        },
        HandlerSpec {
            callback: Callback::Shape,
            inputs: &gate_inputs(2),
            operations: &[Operation::Write {
                when: Binding::Output("write"),
                values: &[(
                    Property::Bool(BoolProperty::Locked),
                    Binding::Output("locked"),
                )],
                notifications: WriteNotifications::OutputAndShapes,
            }],
        },
        HandlerSpec {
            callback: Callback::Added,
            inputs: &gate_inputs(3),
            operations: &[Operation::Notify {
                when: Binding::Output("notify"),
                targets: NotifyTargets::Output,
            }],
        },
        HandlerSpec {
            callback: Callback::Removed,
            inputs: &gate_inputs(4),
            operations: &[Operation::Notify {
                when: Binding::Output("notify"),
                targets: NotifyTargets::Output,
            }],
        },
    ],
}
.checked();

pub const DEVICES: [CheckedDevice; DEVICE_COUNT] = {
    let devices = registry([LAMP, OBSERVER, STONE_BUTTON, REPEATER, COPPER_BULB]);
    let mut i = 0;
    while i < devices.len() {
        let device = devices[i].spec();
        assert!(
            device.physical.same(crate::physical::of_kind(device.kind)),
            "built-in device must share its declared physical facts"
        );
        i += 1;
    }
    devices
};
