//! Rust-authored callback rules; const validation runs during compilation.
use crate::law::static_program::{Expression::*, StaticLaw, Step::*};

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
