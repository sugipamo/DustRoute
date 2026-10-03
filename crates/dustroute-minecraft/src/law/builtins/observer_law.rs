//! Immutable Rust definitions; world adapters supply facts and apply effects.
use crate::law::{
    definition::{Definition, Handler},
    static_program::{Expression::*, Step::*},
};

pub const OBSERVER_COMPATIBILITY_V1: Definition = Definition::new(
    &[
        ("known", 1),
        ("block_record", 1),
        ("signal", 1),
        ("weak", 1),
        ("strong", 1),
        ("repeater", 1),
        ("torch", 1),
        ("comparator", 1),
        ("observer", 1),
        ("lamp", 1),
        ("present", 1),
        ("due", 1),
    ],
    &[
        ("notify", 0, 1),
        ("apply", 0, 1),
        ("powered", 0, 1),
        ("deadline", 0, 2),
        ("after", 0, 65535),
    ],
    &[],
    &[
        Handler {
            name: "observe",
            body: &[Set(
                "notify",
                And(
                    &Input("known"),
                    &Maximum(
                        &Maximum(
                            &Maximum(
                                &Maximum(
                                    &Maximum(
                                        &Maximum(
                                            &Maximum(
                                                &Maximum(
                                                    &Maximum(&Constant(0), &Input("block_record")),
                                                    &Input("signal"),
                                                ),
                                                &Input("weak"),
                                            ),
                                            &Input("strong"),
                                        ),
                                        &Input("repeater"),
                                    ),
                                    &Input("torch"),
                                ),
                                &Input("comparator"),
                            ),
                            &Input("observer"),
                        ),
                        &Input("lamp"),
                    ),
                ),
            )],
        },
        Handler {
            name: "start",
            body: &[
                Set("apply", Input("present")),
                Set("powered", Constant(1)),
                Set("deadline", Constant(1)),
                Set("after", Constant(1)),
            ],
        },
        Handler {
            name: "end",
            body: &[
                Set("apply", And(&Input("present"), &Input("due"))),
                Set("powered", Constant(0)),
                Set("deadline", Constant(2)),
            ],
        },
    ],
);
