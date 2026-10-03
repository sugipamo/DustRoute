//! Immutable Rust definitions; world adapters supply facts and apply effects.
use crate::law::{
    definition::{Definition, Handler},
    static_program::{Expression::*, Step::*},
};

pub const REPEATER_COMPATIBILITY_V1: Definition = Definition::new(
    &[("delay", 4), ("rear_powered", 1), ("locked", 1)],
    &[
        ("powered", 0, 1),
        ("pending", 0, 1),
        ("q0", 0, 1),
        ("q1", 0, 1),
        ("q2", 0, 1),
        ("q3", 0, 1),
    ],
    &[],
    &[
        Handler {
            name: "initialize",
            body: &[
                Set("powered", Input("rear_powered")),
                Set("pending", Constant(0)),
                Set("q0", Constant(0)),
                If(
                    AtLeast(&Input("delay"), &Constant(1)),
                    &[Set("q0", Input("rear_powered"))],
                    &[],
                ),
                Set("q1", Constant(0)),
                If(
                    AtLeast(&Input("delay"), &Constant(2)),
                    &[Set("q1", Input("rear_powered"))],
                    &[],
                ),
                Set("q2", Constant(0)),
                If(
                    AtLeast(&Input("delay"), &Constant(3)),
                    &[Set("q2", Input("rear_powered"))],
                    &[],
                ),
                Set("q3", Constant(0)),
                If(
                    AtLeast(&Input("delay"), &Constant(4)),
                    &[Set("q3", Input("rear_powered"))],
                    &[],
                ),
            ],
        },
        Handler {
            name: "boundary",
            body: &[If(
                Not(&Input("locked")),
                &[
                    If(
                        AtLeast(&Input("delay"), &Constant(2)),
                        &[Set("q0", Register("q1"))],
                        &[],
                    ),
                    If(
                        Equal(&Input("delay"), &Constant(1)),
                        &[Set("q0", Input("rear_powered"))],
                        &[],
                    ),
                    If(
                        AtLeast(&Input("delay"), &Constant(3)),
                        &[Set("q1", Register("q2"))],
                        &[],
                    ),
                    If(
                        Equal(&Input("delay"), &Constant(2)),
                        &[Set("q1", Input("rear_powered"))],
                        &[],
                    ),
                    If(
                        AtLeast(&Input("delay"), &Constant(4)),
                        &[Set("q2", Register("q3"))],
                        &[],
                    ),
                    If(
                        Equal(&Input("delay"), &Constant(3)),
                        &[Set("q2", Input("rear_powered"))],
                        &[],
                    ),
                    If(
                        Equal(&Input("delay"), &Constant(4)),
                        &[Set("q3", Input("rear_powered"))],
                        &[],
                    ),
                    Set("powered", Register("q0")),
                    Set(
                        "pending",
                        Maximum(
                            &Maximum(
                                &Maximum(
                                    &Maximum(
                                        &Constant(0),
                                        &And(
                                            &AtLeast(&Input("delay"), &Constant(1)),
                                            &Not(&Equal(&Register("q0"), &Register("powered"))),
                                        ),
                                    ),
                                    &And(
                                        &AtLeast(&Input("delay"), &Constant(2)),
                                        &Not(&Equal(&Register("q1"), &Register("powered"))),
                                    ),
                                ),
                                &And(
                                    &AtLeast(&Input("delay"), &Constant(3)),
                                    &Not(&Equal(&Register("q2"), &Register("powered"))),
                                ),
                            ),
                            &And(
                                &AtLeast(&Input("delay"), &Constant(4)),
                                &Not(&Equal(&Register("q3"), &Register("powered"))),
                            ),
                        ),
                    ),
                ],
                &[],
            )],
        },
    ],
);

pub const REPEATER_BOUNDED_V1: Definition = Definition::new(
    &[
        ("powered", 1),
        ("rear_powered", 1),
        ("expected_powered", 1),
        ("delay", 4),
    ],
    &[
        ("needs_update", 0, 1),
        ("apply_tick", 0, 1),
        ("next_powered", 0, 1),
        ("delay_game_ticks", 0, 8),
    ],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            Set(
                "needs_update",
                Not(&Equal(&Input("powered"), &Input("rear_powered"))),
            ),
            Set(
                "apply_tick",
                Equal(&Input("rear_powered"), &Input("expected_powered")),
            ),
            Set("next_powered", Input("expected_powered")),
            If(
                Equal(&Input("delay"), &Constant(1)),
                &[Set("delay_game_ticks", Constant(2))],
                &[],
            ),
            If(
                Equal(&Input("delay"), &Constant(2)),
                &[Set("delay_game_ticks", Constant(4))],
                &[],
            ),
            If(
                Equal(&Input("delay"), &Constant(3)),
                &[Set("delay_game_ticks", Constant(6))],
                &[],
            ),
            If(
                Equal(&Input("delay"), &Constant(4)),
                &[Set("delay_game_ticks", Constant(8))],
                &[],
            ),
        ],
    }],
);
