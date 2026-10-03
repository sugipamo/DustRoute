//! Immutable Rust definitions; world adapters supply facts and apply effects.
use crate::law::{
    definition::{Definition, Handler},
    static_program::{Expression::*, Step::*},
};

pub const COMPARATOR_COMPATIBILITY_V1: Definition = Definition::new(
    &[
        ("rear", 255),
        ("side_a", 255),
        ("side_b", 255),
        ("subtract", 1),
    ],
    &[("strength", 0, 255)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[If(
            Input("subtract"),
            &[Set(
                "strength",
                SaturatingSubtract(&Input("rear"), &Maximum(&Input("side_a"), &Input("side_b"))),
            )],
            &[If(
                AtLeast(&Input("rear"), &Maximum(&Input("side_a"), &Input("side_b"))),
                &[Set("strength", Input("rear"))],
                &[Set("strength", Constant(0))],
            )],
        )],
    }],
);
