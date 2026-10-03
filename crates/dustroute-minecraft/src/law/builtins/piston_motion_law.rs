//! Immutable Rust definitions; world adapters supply facts and apply effects.
use crate::law::{
    definition::{Definition, Handler},
    static_program::{Expression::*, Step::*},
};

pub const PISTON_CONTROL_JAVA_1_21_11_V1: Definition = Definition::new(
    &[
        ("powered", 1),
        ("extended", 1),
        ("can_push", 1),
        ("matching_carrier", 1),
        ("same_tick", 1),
        ("in_block_tick", 1),
        ("sticky", 1),
        ("pullable", 1),
        ("last_progress", 2),
        ("event", 2),
    ],
    &[
        ("request", 0, 3),
        ("execute", 0, 1),
        ("finish_payload", 0, 1),
        ("pull", 0, 1),
        ("remove_head", 0, 1),
    ],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            If(
                And(
                    &And(&Input("powered"), &Not(&Input("extended"))),
                    &Input("can_push"),
                ),
                &[Set("request", Constant(1))],
                &[If(
                    And(&Not(&Input("powered")), &Input("extended")),
                    &[If(
                        And(
                            &Input("matching_carrier"),
                            &Maximum(
                                &Maximum(
                                    &Not(&AtLeast(&Input("last_progress"), &Constant(1))),
                                    &Input("same_tick"),
                                ),
                                &Input("in_block_tick"),
                            ),
                        ),
                        &[Set("request", Constant(3))],
                        &[Set("request", Constant(2))],
                    )],
                    &[],
                )],
            ),
            If(
                Equal(&Input("event"), &Constant(0)),
                &[Set("execute", Input("powered"))],
                &[Set("execute", Not(&Input("powered")))],
            ),
            Set(
                "finish_payload",
                And(
                    &And(
                        &Not(&Equal(&Input("event"), &Constant(0))),
                        &Input("sticky"),
                    ),
                    &Input("matching_carrier"),
                ),
            ),
            Set(
                "pull",
                And(
                    &And(
                        &And(&Equal(&Input("event"), &Constant(1)), &Input("sticky")),
                        &Not(&Input("matching_carrier")),
                    ),
                    &Input("pullable"),
                ),
            ),
            Set(
                "remove_head",
                And(
                    &And(
                        &Not(&Equal(&Input("event"), &Constant(0))),
                        &Not(&And(
                            &And(
                                &Not(&Equal(&Input("event"), &Constant(0))),
                                &Input("sticky"),
                            ),
                            &Input("matching_carrier"),
                        )),
                    ),
                    &Not(&And(
                        &And(
                            &And(&Equal(&Input("event"), &Constant(1)), &Input("sticky")),
                            &Not(&Input("matching_carrier")),
                        ),
                        &Input("pullable"),
                    )),
                ),
            ),
        ],
    }],
);

pub const PISTON_CARRIER_JAVA_1_21_11_V1: Definition = Definition::new(
    &[
        ("progress", 2),
        ("last_progress", 2),
        ("forced", 1),
        ("source", 1),
    ],
    &[
        ("complete", 0, 1),
        ("discard", 0, 1),
        ("next_progress", 0, 2),
        ("next_last", 0, 2),
        ("save_time", 0, 1),
        ("again", 0, 1),
    ],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            Set("next_progress", Input("progress")),
            Set("next_last", Input("last_progress")),
            If(
                Input("forced"),
                &[If(
                    Not(&Equal(&Input("last_progress"), &Constant(2))),
                    &[
                        Set("complete", Constant(1)),
                        Set("discard", Input("source")),
                        Set("next_progress", Constant(2)),
                        Set("next_last", Constant(2)),
                    ],
                    &[],
                )],
                &[
                    Set("save_time", Constant(1)),
                    Set("next_last", Input("progress")),
                    If(
                        Equal(&Input("progress"), &Constant(2)),
                        &[Set("complete", Constant(1))],
                        &[
                            Set("again", Constant(1)),
                            If(
                                Equal(&Input("progress"), &Constant(0)),
                                &[Set("next_progress", Constant(1))],
                                &[Set("next_progress", Constant(2))],
                            ),
                        ],
                    ),
                ],
            ),
        ],
    }],
);

pub const PISTON_GEOMETRY_JAVA_1_21_11_V2: Definition = Definition::new(
    &[
        ("retract", 1),
        ("front", 1),
        ("block_entity_section", 1),
        ("neighbor_index", 5),
        ("count", 12),
        ("piston_payload", 1),
        ("has_block_entity", 1),
        ("extended_piston", 1),
    ],
    &[
        ("head", 0, 2),
        ("payload", 0, 2),
        ("limit", 0, 12),
        ("body_carrier", 0, 1),
        ("head_carrier", 0, 1),
        ("accept_input", 0, 1),
        ("first_tick_delay", 0, 1),
        ("next_tick_delay", 0, 1),
        ("dx", 0, 2),
        ("dy", 0, 2),
        ("dz", 0, 2),
        ("admit_chain", 0, 1),
        ("immovable", 0, 1),
        ("shape_dx", 1, 2),
        ("shape_dy", 1, 2),
        ("shape_dz", 1, 2),
    ],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            Set("head", Constant(1)),
            Set("payload", Constant(2)),
            Set("limit", Constant(12)),
            Set("body_carrier", Input("retract")),
            Set("head_carrier", Not(&Input("retract"))),
            Set("accept_input", Not(&Input("front"))),
            Set("first_tick_delay", Input("block_entity_section")),
            Set("next_tick_delay", Constant(1)),
            If(
                Equal(&Input("neighbor_index"), &Constant(0)),
                &[
                    Set("dx", Constant(0)),
                    Set("dy", Constant(1)),
                    Set("dz", Constant(1)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(1)),
                &[
                    Set("dx", Constant(2)),
                    Set("dy", Constant(1)),
                    Set("dz", Constant(1)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(2)),
                &[
                    Set("dx", Constant(1)),
                    Set("dy", Constant(0)),
                    Set("dz", Constant(1)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(3)),
                &[
                    Set("dx", Constant(1)),
                    Set("dy", Constant(2)),
                    Set("dz", Constant(1)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(4)),
                &[
                    Set("dx", Constant(1)),
                    Set("dy", Constant(1)),
                    Set("dz", Constant(0)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(5)),
                &[
                    Set("dx", Constant(1)),
                    Set("dy", Constant(1)),
                    Set("dz", Constant(2)),
                ],
                &[],
            ),
            Set("admit_chain", Not(&AtLeast(&Input("count"), &Constant(13)))),
            Set(
                "immovable",
                Maximum(&Input("has_block_entity"), &Input("extended_piston")),
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(0)),
                &[
                    Set("shape_dx", Constant(0)),
                    Set("shape_dy", Constant(1)),
                    Set("shape_dz", Constant(1)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(1)),
                &[
                    Set("shape_dx", Constant(2)),
                    Set("shape_dy", Constant(1)),
                    Set("shape_dz", Constant(1)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(2)),
                &[
                    Set("shape_dx", Constant(1)),
                    Set("shape_dy", Constant(1)),
                    Set("shape_dz", Constant(0)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(3)),
                &[
                    Set("shape_dx", Constant(1)),
                    Set("shape_dy", Constant(1)),
                    Set("shape_dz", Constant(2)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(4)),
                &[
                    Set("shape_dx", Constant(1)),
                    Set("shape_dy", Constant(0)),
                    Set("shape_dz", Constant(1)),
                ],
                &[],
            ),
            If(
                Equal(&Input("neighbor_index"), &Constant(5)),
                &[
                    Set("shape_dx", Constant(1)),
                    Set("shape_dy", Constant(2)),
                    Set("shape_dz", Constant(1)),
                ],
                &[],
            ),
        ],
    }],
);

pub const PISTON_HEAD_JAVA_1_21_11_V1: Definition = Definition::new(
    &[
        ("body_piston", 1),
        ("body_moving", 1),
        ("aligned", 1),
        ("same_variant", 1),
        ("extended", 1),
    ],
    &[("attached", 0, 1), ("supported", 0, 1)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            Set(
                "attached",
                And(
                    &And(&Input("body_piston"), &Input("aligned")),
                    &And(&Input("same_variant"), &Input("extended")),
                ),
            ),
            Set(
                "supported",
                Maximum(
                    &And(
                        &And(&Input("body_piston"), &Input("aligned")),
                        &And(&Input("same_variant"), &Input("extended")),
                    ),
                    &And(&Input("body_moving"), &Input("aligned")),
                ),
            ),
        ],
    }],
);
