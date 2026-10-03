//! Immutable Rust definitions; world adapters supply facts and apply effects.
use crate::law::{
    definition::{Definition, Handler},
    static_program::{Expression::*, Step::*},
};

pub const PISTON_SIGNAL_EMISSION_JAVA_1_21_11_V1: Definition = Definition::new(
    &[
        ("source", 4),
        ("level", 15),
        ("query", 2),
        ("direction_match", 1),
        ("wire_connected", 1),
        ("wires_enabled", 1),
    ],
    &[("weak", 0, 15), ("strong", 0, 15)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            If(
                Equal(&Input("source"), &Constant(1)),
                &[
                    Set("weak", Input("level")),
                    If(
                        Input("direction_match"),
                        &[Set("strong", Input("level"))],
                        &[],
                    ),
                ],
                &[],
            ),
            If(
                Equal(&Input("source"), &Constant(2)),
                &[Set("weak", Constant(15))],
                &[],
            ),
            If(
                And(
                    &Equal(&Input("source"), &Constant(3)),
                    &Input("direction_match"),
                ),
                &[Set("weak", Input("level")), Set("strong", Input("level"))],
                &[],
            ),
            If(
                And(
                    &Equal(&Input("source"), &Constant(4)),
                    &And(
                        &Input("wires_enabled"),
                        &And(
                            &Not(&Equal(&Input("query"), &Constant(0))),
                            &Not(&And(
                                &Not(&Equal(&Input("query"), &Constant(1))),
                                &Not(&Input("wire_connected")),
                            )),
                        ),
                    ),
                ),
                &[Set("weak", Input("level")), Set("strong", Input("level"))],
                &[],
            ),
        ],
    }],
);

pub const PISTON_CONDUCTOR_JAVA_1_21_11_V1: Definition = Definition::new(
    &[("weak", 15), ("received_strong", 15), ("conducts", 1)],
    &[("emitted", 0, 15)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            Set("emitted", Input("weak")),
            If(
                Input("conducts"),
                &[Set(
                    "emitted",
                    Maximum(&Input("weak"), &Input("received_strong")),
                )],
                &[],
            ),
        ],
    }],
);

pub const PISTON_POWER_QUERY_JAVA_1_21_11_V1: Definition = Definition::new(
    &[("query", 2), ("front", 1), ("powered", 1)],
    &[("accept", 0, 1)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[If(
            And(&Equal(&Input("query"), &Constant(0)), &Input("front")),
            &[],
            &[Set("accept", Input("powered"))],
        )],
    }],
);

pub const REPEATER_CALLBACK_JAVA_1_21_11_V1: Definition = Definition::new(
    &[
        ("powered", 1),
        ("input", 1),
        ("locked", 1),
        ("target_misaligned", 1),
        ("delay", 4),
    ],
    &[
        ("request", 0, 1),
        ("priority", 0, 2),
        ("apply", 0, 1),
        ("next_powered", 0, 1),
        ("schedule_off", 0, 1),
        ("delay_game_ticks", 0, 8),
    ],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            Set("priority", Constant(2)),
            If(Input("powered"), &[Set("priority", Constant(1))], &[]),
            If(
                Input("target_misaligned"),
                &[Set("priority", Constant(0))],
                &[],
            ),
            Set("next_powered", Input("powered")),
            If(
                Not(&Input("locked")),
                &[
                    Set("request", Not(&Equal(&Input("powered"), &Input("input")))),
                    If(
                        Input("powered"),
                        &[If(
                            Not(&Input("input")),
                            &[Set("apply", Constant(1)), Set("next_powered", Constant(0))],
                            &[],
                        )],
                        &[
                            Set("apply", Constant(1)),
                            Set("next_powered", Constant(1)),
                            Set("schedule_off", Not(&Input("input"))),
                        ],
                    ),
                ],
                &[],
            ),
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
