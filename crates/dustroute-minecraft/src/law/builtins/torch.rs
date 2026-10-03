//! Immutable local law, independent of Blueprint metadata.
use crate::law::{
    definition::{Definition, Handler},
    static_program::{Expression::*, Step::*},
};

pub const TORCH: Definition = Definition::new(
    &[("powered", 1)],
    &[("lit", 1, 1), ("burnout_seen", 0, 1)],
    &[("off_edges", 60, 8)],
    &[
        Handler {
            name: "neighbor_update",
            body: &[If(
                Equal(&Register("lit"), &Input("powered")),
                &[ScheduleIfAbsent("scheduled_tick", 2)],
                &[],
            )],
        },
        Handler {
            name: "scheduled_tick",
            body: &[If(
                Register("lit"),
                &[If(
                    Input("powered"),
                    &[
                        Set("lit", Constant(0)),
                        Remember("off_edges"),
                        If(
                            AtLeast(&Count("off_edges"), &Constant(8)),
                            &[
                                Set("burnout_seen", Constant(1)),
                                ScheduleIfAbsent("scheduled_tick", 160),
                            ],
                            &[],
                        ),
                    ],
                    &[],
                )],
                &[If(
                    And(
                        &Not(&Input("powered")),
                        &Not(&AtLeast(&Count("off_edges"), &Constant(8))),
                    ),
                    &[Set("lit", Constant(1))],
                    &[],
                )],
            )],
        },
    ],
);
