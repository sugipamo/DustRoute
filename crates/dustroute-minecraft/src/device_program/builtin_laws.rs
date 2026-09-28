//! Rust-authored callback rules; const validation runs during compilation.
use crate::law::static_program::{Expression::*, StaticLaw, Step::*};

pub const COPPER_BULB: StaticLaw = StaticLaw::new(
    &[("powered", 1), ("lit", 1), ("input", 1)],
    &[("write", 1), ("powered", 1), ("lit", 1)],
    &[
        Set("powered", Input("input")),
        Set("lit", Input("lit")),
        If(
            Not(&Equal(&Input("powered"), &Input("input"))),
            &[
                Set("write", Constant(1)),
                If(
                    Not(&Input("powered")),
                    &[Set("lit", Not(&Input("lit")))],
                    &[],
                ),
            ],
            &[],
        ),
    ],
);

// Discrete target delay, in game ticks. No timer is emitted when the callback
// leaves `delay` at zero. Keep arithmetic in the checked local rule.
const GATE_DELAY: &[crate::law::static_program::Step] = &[
    If(
        Equal(&Input("delay_setting"), &Constant(1)),
        &[Set("delay", Constant(2))],
        &[],
    ),
    If(
        Equal(&Input("delay_setting"), &Constant(2)),
        &[Set("delay", Constant(4))],
        &[],
    ),
    If(
        Equal(&Input("delay_setting"), &Constant(3)),
        &[Set("delay", Constant(6))],
        &[],
    ),
    If(
        Equal(&Input("delay_setting"), &Constant(4)),
        &[Set("delay", Constant(8))],
        &[],
    ),
];

pub const REPEATER: StaticLaw = StaticLaw::new(
    &[
        ("event", 4),
        ("powered", 1),
        ("input", 1),
        ("locked", 1),
        ("misaligned", 1),
        ("delay_setting", 4),
        ("ticking", 1),
        ("stored_locked", 1),
        ("off_axis", 1),
    ],
    &[
        ("write", 1),
        ("powered", 1),
        ("locked", 1),
        ("delay", 8),
        ("priority", 2),
        ("notify", 1),
    ],
    &[
        Set("powered", Input("powered")),
        Set("locked", Input("locked")),
        If(
            Equal(&Input("event"), &Constant(0)),
            &[If(
                And(
                    &Not(&Input("locked")),
                    &And(
                        &Not(&Input("ticking")),
                        &Not(&Equal(&Input("powered"), &Input("input"))),
                    ),
                ),
                &[
                    If(Constant(1), GATE_DELAY, &[]),
                    If(
                        Input("misaligned"),
                        &[],
                        &[If(
                            Input("powered"),
                            &[Set("priority", Constant(1))],
                            &[Set("priority", Constant(2))],
                        )],
                    ),
                ],
                &[],
            )],
            &[],
        ),
        If(
            Equal(&Input("event"), &Constant(1)),
            &[If(
                Not(&Input("locked")),
                &[If(
                    Input("powered"),
                    &[If(
                        Not(&Input("input")),
                        &[Set("write", Constant(1)), Set("powered", Constant(0))],
                        &[],
                    )],
                    &[
                        Set("write", Constant(1)),
                        Set("powered", Constant(1)),
                        If(
                            Not(&Input("input")),
                            &[
                                If(Constant(1), GATE_DELAY, &[]),
                                Set("priority", Constant(1)),
                            ],
                            &[],
                        ),
                    ],
                )],
                &[],
            )],
            &[],
        ),
        If(
            Equal(&Input("event"), &Constant(2)),
            &[If(
                And(
                    &Input("off_axis"),
                    &Not(&Equal(&Input("locked"), &Input("stored_locked"))),
                ),
                &[Set("write", Constant(1))],
                &[],
            )],
            &[],
        ),
        If(
            AtLeast(&Input("event"), &Constant(3)),
            &[Set("notify", Constant(1))],
            &[],
        ),
    ],
);

pub const LAMP: StaticLaw = StaticLaw::new(
    &[("lit", 1), ("input", 1), ("scheduled", 1)],
    &[("write", 1), ("lit", 1), ("delay", 4)],
    &[
        Set("lit", Input("lit")),
        If(
            Input("scheduled"),
            &[If(
                And(&Input("lit"), &Not(&Input("input"))),
                &[Set("write", Constant(1)), Set("lit", Constant(0))],
                &[],
            )],
            &[If(
                Not(&Equal(&Input("lit"), &Input("input"))),
                &[If(
                    Input("lit"),
                    &[Set("delay", Constant(4))],
                    &[Set("write", Constant(1)), Set("lit", Constant(1))],
                )],
                &[],
            )],
        ),
    ],
);

pub const OBSERVER: StaticLaw = StaticLaw::new(
    &[("event", 3), ("powered", 1), ("queued", 1), ("front", 1)],
    &[
        ("write", 1),
        ("powered", 1),
        ("delay", 2),
        ("notify", 1),
        ("signal", 15),
    ],
    &[
        Set("powered", Input("powered")),
        If(Input("powered"), &[Set("signal", Constant(15))], &[]),
        If(
            Equal(&Input("event"), &Constant(0)),
            &[If(
                And(
                    &Input("front"),
                    &And(&Not(&Input("powered")), &Not(&Input("queued"))),
                ),
                &[Set("delay", Constant(2))],
                &[],
            )],
            &[],
        ),
        If(
            Equal(&Input("event"), &Constant(1)),
            &[
                Set("write", Constant(1)),
                Set("notify", Constant(1)),
                If(
                    Input("powered"),
                    &[Set("powered", Constant(0))],
                    &[Set("powered", Constant(1)), Set("delay", Constant(2))],
                ),
            ],
            &[],
        ),
        If(
            Equal(&Input("event"), &Constant(2)),
            &[If(
                And(&Input("powered"), &Not(&Input("queued"))),
                &[
                    Set("write", Constant(1)),
                    Set("powered", Constant(0)),
                    Set("notify", Constant(1)),
                ],
                &[],
            )],
            &[],
        ),
        If(
            Equal(&Input("event"), &Constant(3)),
            &[If(
                And(&Input("powered"), &Input("queued")),
                &[Set("notify", Constant(1))],
                &[],
            )],
            &[],
        ),
    ],
);

pub const STONE_BUTTON: StaticLaw = StaticLaw::new(
    &[("event", 2), ("powered", 1)],
    &[("write", 1), ("powered", 1), ("delay", 20), ("notify", 1)],
    &[If(
        Equal(&Input("event"), &Constant(0)),
        &[If(
            Not(&Input("powered")),
            &[
                Set("write", Constant(1)),
                Set("powered", Constant(1)),
                Set("delay", Constant(20)),
                Set("notify", Constant(1)),
            ],
            &[],
        )],
        &[If(
            Input("powered"),
            &[
                Set("notify", Constant(1)),
                If(
                    Equal(&Input("event"), &Constant(1)),
                    &[Set("write", Constant(1))],
                    &[],
                ),
            ],
            &[],
        )],
    )],
);

// Java circuit-only comparator arithmetic. Separate finite input aggregation
// keeps the callback decision bounded without folding hidden state into blocks.
pub const COMPARATOR_SIGNAL: StaticLaw = StaticLaw::new(
    &[("rear", 15), ("side", 15), ("subtract", 1)],
    &[("level", 15)],
    &[If(
        Input("subtract"),
        &[Set(
            "level",
            SaturatingSubtract(&Input("rear"), &Input("side")),
        )],
        &[If(
            AtLeast(&Input("rear"), &Input("side")),
            &[Set("level", Input("rear"))],
            &[],
        )],
    )],
);

pub const COMPARATOR: StaticLaw = StaticLaw::new(
    &[
        ("event", 2),
        ("level", 15),
        ("changed", 1),
        ("powered", 1),
        ("subtract", 1),
        ("ticking", 1),
        ("misaligned", 1),
    ],
    &[
        ("level", 15),
        ("powered", 1),
        ("write", 1),
        ("notify", 1),
        ("delay", 2),
        ("priority", 3),
    ],
    &[
        Set("level", Input("level")),
        Set("powered", Not(&Equal(&Input("level"), &Constant(0)))),
        Set("priority", Constant(3)),
        If(Input("misaligned"), &[Set("priority", Constant(2))], &[]),
        If(
            Equal(&Input("event"), &Constant(0)),
            &[If(
                Not(&Input("ticking")),
                &[
                    If(Input("changed"), &[Set("delay", Constant(2))], &[]),
                    If(
                        Not(&Equal(&Input("powered"), &Register("powered"))),
                        &[Set("delay", Constant(2))],
                        &[],
                    ),
                ],
                &[],
            )],
            &[],
        ),
        If(
            Equal(&Input("event"), &Constant(1)),
            &[
                If(Input("changed"), &[Set("notify", Constant(1))], &[]),
                If(Not(&Input("subtract")), &[Set("notify", Constant(1))], &[]),
                If(
                    And(
                        &Register("notify"),
                        &Not(&Equal(&Input("powered"), &Register("powered"))),
                    ),
                    &[Set("write", Constant(1))],
                    &[],
                ),
            ],
            &[],
        ),
        If(
            Equal(&Input("event"), &Constant(2)),
            &[Set("notify", Constant(1))],
            &[],
        ),
    ],
);

// History is a bounded world query. Recording occurs after the lit write and
// its nested callbacks, so feedback can reserve a short tick before recovery.
pub const TORCH: StaticLaw = StaticLaw::new(
    &[
        ("event", 3),
        ("lit", 1),
        ("support_powered", 1),
        ("ticking", 1),
        ("off_count", 8),
    ],
    &[
        ("write", 1),
        ("lit", 1),
        ("record", 1),
        ("delay", 160),
        ("notify", 1),
    ],
    &[
        Set("lit", Input("lit")),
        If(
            Equal(&Input("event"), &Constant(0)),
            &[If(
                And(
                    &Equal(&Input("lit"), &Input("support_powered")),
                    &Not(&Input("ticking")),
                ),
                &[Set("delay", Constant(2))],
                &[],
            )],
            &[],
        ),
        If(
            Equal(&Input("event"), &Constant(1)),
            &[
                If(
                    And(&Input("lit"), &Input("support_powered")),
                    &[
                        Set("write", Constant(1)),
                        Set("lit", Constant(0)),
                        Set("record", Constant(1)),
                        If(
                            AtLeast(&Input("off_count"), &Constant(7)),
                            &[Set("delay", Constant(160))],
                            &[],
                        ),
                    ],
                    &[],
                ),
                If(
                    And(
                        &Not(&Input("lit")),
                        &And(
                            &Not(&Input("support_powered")),
                            &Not(&AtLeast(&Input("off_count"), &Constant(8))),
                        ),
                    ),
                    &[Set("write", Constant(1)), Set("lit", Constant(1))],
                    &[],
                ),
            ],
            &[],
        ),
        If(
            AtLeast(&Input("event"), &Constant(2)),
            &[Set("notify", Constant(1))],
            &[],
        ),
    ],
);
