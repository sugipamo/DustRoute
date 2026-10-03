//! Immutable local law, independent of Blueprint metadata.
use crate::law::{
    definition::{Definition, Handler},
    static_program::{Expression::*, Step::*},
};

pub const DUST: Definition = Definition::new(
    &[("direct", 15), ("neighbor", 15)],
    &[("strength", 0, 15)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[Set(
            "strength",
            Maximum(
                &Input("direct"),
                &SaturatingSubtract(&Input("neighbor"), &Constant(1)),
            ),
        )],
    }],
);
