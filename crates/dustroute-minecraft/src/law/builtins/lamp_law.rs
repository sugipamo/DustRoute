//! Immutable Rust definitions; world adapters supply facts and apply effects.
use crate::law::{
    definition::{Definition, Handler},
    static_program::{Expression::*, Step::*},
};

pub const LAMP_COMPATIBILITY_V1: Definition = Definition::new(
    &[
        ("initial", 1),
        ("powered", 1),
        ("lit", 1),
        ("has_deadline", 1),
        ("due", 1),
        ("last_tick", 1),
    ],
    &[
        ("write", 0, 1),
        ("lit", 0, 1),
        ("deadline", 0, 2),
        ("after", 0, 65535),
    ],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[If(
            Input("initial"),
            &[
                Set("write", Constant(1)),
                Set("lit", Input("powered")),
                Set("deadline", Constant(2)),
            ],
            &[If(
                Input("powered"),
                &[
                    Set("write", Constant(1)),
                    Set("lit", Constant(1)),
                    Set("deadline", Constant(2)),
                ],
                &[If(
                    Input("lit"),
                    &[If(
                        Input("has_deadline"),
                        &[If(
                            Input("due"),
                            &[
                                Set("write", Constant(1)),
                                Set("lit", Constant(0)),
                                Set("deadline", Constant(2)),
                            ],
                            &[],
                        )],
                        &[If(
                            Input("last_tick"),
                            &[
                                Set("write", Constant(1)),
                                Set("lit", Constant(0)),
                                Set("deadline", Constant(2)),
                            ],
                            &[Set("deadline", Constant(1)), Set("after", Constant(2))],
                        )],
                    )],
                    &[],
                )],
            )],
        )],
    }],
);

pub const LAMP_BOUNDED_V1: Definition = Definition::new(
    &[("powered", 1)],
    &[("lit", 0, 1)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[Set("lit", Input("powered"))],
    }],
);
