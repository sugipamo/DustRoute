//! Immutable Rust definitions; world adapters supply facts and apply effects.
use crate::law::{
    definition::{Definition, Handler},
    static_program::{Expression::*, Step::*},
};

pub const PISTON_STATE_V1: Definition = Definition::new(
    &[("state", 3), ("action", 1), ("powered", 1), ("sticky", 1)],
    &[
        ("request", 0, 2),
        ("valid", 0, 1),
        ("stable", 0, 3),
        ("moving", 0, 3),
        ("pull", 0, 1),
    ],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            If(
                And(&Equal(&Input("state"), &Constant(0)), &Input("powered")),
                &[Set("request", Constant(1))],
                &[If(
                    And(
                        &Equal(&Input("state"), &Constant(2)),
                        &Not(&Input("powered")),
                    ),
                    &[Set("request", Constant(2))],
                    &[],
                )],
            ),
            Set(
                "valid",
                Maximum(
                    &And(
                        &Equal(&Input("action"), &Constant(0)),
                        &Equal(&Input("state"), &Constant(0)),
                    ),
                    &And(
                        &Equal(&Input("action"), &Constant(1)),
                        &Equal(&Input("state"), &Constant(2)),
                    ),
                ),
            ),
            If(
                Equal(&Input("action"), &Constant(0)),
                &[Set("stable", Constant(2)), Set("moving", Constant(1))],
                &[Set("stable", Constant(0)), Set("moving", Constant(3))],
            ),
            Set(
                "pull",
                And(&Equal(&Input("action"), &Constant(1)), &Input("sticky")),
            ),
        ],
    }],
);

pub const PISTON_MOTION_V1: Definition = Definition::new(
    &[("action", 1), ("count", 12), ("piston_payload", 1)],
    &[
        ("admit_push", 0, 1),
        ("admit_chain", 0, 1),
        ("limit", 0, 12),
        ("head", 0, 4),
        ("push_step", 0, 4),
        ("pull_from", 0, 4),
        ("pull_to", 0, 4),
        ("dirty_radius", 0, 4),
        ("source_carrier", 0, 1),
        ("destination_carrier", 0, 1),
        ("head_carrier", 0, 1),
        ("extending", 0, 1),
        ("progress", 0, 1),
        ("head_is_payload", 0, 1),
        ("activation_min", 0, 65535),
        ("activation_max", 0, 65535),
        ("completion", 0, 65535),
    ],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            Set("admit_push", Not(&AtLeast(&Input("count"), &Constant(12)))),
            Set(
                "admit_chain",
                Maximum(
                    &Not(&Input("piston_payload")),
                    &Equal(&Input("count"), &Constant(1)),
                ),
            ),
            Set("limit", Constant(12)),
            Set("head", Constant(1)),
            Set("push_step", Constant(1)),
            Set("pull_from", Constant(2)),
            Set("pull_to", Constant(1)),
            Set("dirty_radius", Constant(1)),
            Set("destination_carrier", Constant(1)),
            Set("head_carrier", Constant(1)),
            Set("activation_min", Constant(0)),
            Set("activation_max", Constant(1)),
            Set("completion", Constant(2)),
            Set("source_carrier", Equal(&Input("action"), &Constant(1))),
            Set("extending", Equal(&Input("action"), &Constant(0))),
            Set("progress", Equal(&Input("action"), &Constant(1))),
            Set("head_is_payload", Equal(&Input("action"), &Constant(1))),
        ],
    }],
);

pub const PISTON_CONNECTION_V1: Definition = Definition::new(
    &[
        ("kind", 15),
        ("observed", 1),
        ("shape", 1),
        ("arm", 1),
        ("facing", 1),
        ("aligned", 1),
    ],
    &[("connected", 0, 1), ("error", 0, 2)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[If(
            Equal(&Input("kind"), &Constant(3)),
            &[If(
                Input("shape"),
                &[Set("connected", Input("arm"))],
                &[If(
                    Input("observed"),
                    &[Set("error", Constant(1))],
                    &[Set("connected", Constant(1))],
                )],
            )],
            &[If(
                Maximum(
                    &Maximum(
                        &Equal(&Input("kind"), &Constant(5)),
                        &Equal(&Input("kind"), &Constant(6)),
                    ),
                    &Equal(&Input("kind"), &Constant(12)),
                ),
                &[If(
                    Input("facing"),
                    &[Set("connected", Input("aligned"))],
                    &[If(Input("observed"), &[Set("error", Constant(2))], &[])],
                )],
                &[Set(
                    "connected",
                    Maximum(
                        &Maximum(
                            &Maximum(
                                &Maximum(
                                    &Equal(&Input("kind"), &Constant(4)),
                                    &Equal(&Input("kind"), &Constant(7)),
                                ),
                                &Equal(&Input("kind"), &Constant(8)),
                            ),
                            &Equal(&Input("kind"), &Constant(9)),
                        ),
                        &Equal(&Input("kind"), &Constant(11)),
                    ),
                )],
            )],
        )],
    }],
);

pub const PISTON_PAYLOAD_V1: Definition = Definition::new(
    &[
        ("kind", 15),
        ("retracted", 1),
        ("normal", 1),
        ("horizontal", 1),
        ("powered", 1),
        ("head", 1),
        ("entity", 1),
        ("name_missing", 1),
        ("extended_false_or_absent", 1),
        ("coarse", 1),
        ("live", 1),
        ("name:bedrock", 1),
        ("name:obsidian", 1),
        ("name:crying_obsidian", 1),
        ("name:reinforced_deepslate", 1),
        ("name:end_portal_frame", 1),
        ("name:end_portal", 1),
        ("name:nether_portal", 1),
        ("name:moving_piston", 1),
        ("name:piston_head", 1),
        ("name:barrier", 1),
        ("name:structure_block", 1),
        ("name:jigsaw", 1),
        ("name:command_block", 1),
        ("name:chain_command_block", 1),
        ("name:repeating_command_block", 1),
        ("name:piston", 1),
    ],
    &[("immovable", 0, 1), ("rejection", 0, 4)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            Set(
                "immovable",
                Maximum(
                    &Maximum(
                        &Maximum(
                            &Maximum(
                                &Maximum(
                                    &Maximum(
                                        &Maximum(
                                            &Maximum(
                                                &Maximum(
                                                    &Maximum(
                                                        &Maximum(
                                                            &Maximum(
                                                                &Maximum(
                                                                    &Maximum(
                                                                        &Input("name:bedrock"),
                                                                        &Input("name:obsidian"),
                                                                    ),
                                                                    &Input("name:crying_obsidian"),
                                                                ),
                                                                &Input("name:reinforced_deepslate"),
                                                            ),
                                                            &Input("name:end_portal_frame"),
                                                        ),
                                                        &Input("name:end_portal"),
                                                    ),
                                                    &Input("name:nether_portal"),
                                                ),
                                                &Input("name:moving_piston"),
                                            ),
                                            &Input("name:piston_head"),
                                        ),
                                        &Input("name:barrier"),
                                    ),
                                    &Input("name:structure_block"),
                                ),
                                &Input("name:jigsaw"),
                            ),
                            &Input("name:command_block"),
                        ),
                        &Input("name:chain_command_block"),
                    ),
                    &Input("name:repeating_command_block"),
                ),
            ),
            If(
                Not(&Maximum(
                    &Maximum(
                        &Equal(&Input("kind"), &Constant(1)),
                        &Equal(&Input("kind"), &Constant(2)),
                    ),
                    &And(
                        &And(
                            &And(
                                &And(
                                    &And(
                                        &And(
                                            &And(
                                                &And(
                                                    &Equal(&Input("kind"), &Constant(13)),
                                                    &Input("retracted"),
                                                ),
                                                &Input("normal"),
                                            ),
                                            &Input("horizontal"),
                                        ),
                                        &Not(&Input("powered")),
                                    ),
                                    &Not(&Input("head")),
                                ),
                                &Not(&Input("entity")),
                            ),
                            &Maximum(&Input("name_missing"), &Input("name:piston")),
                        ),
                        &Input("extended_false_or_absent"),
                    ),
                )),
                &[Set("rejection", Constant(1))],
                &[If(
                    Input("coarse"),
                    &[Set("rejection", Constant(2))],
                    &[If(
                        Input("live"),
                        &[Set("rejection", Constant(3))],
                        &[If(
                            Maximum(
                                &Maximum(
                                    &Maximum(
                                        &Maximum(
                                            &Maximum(
                                                &Maximum(
                                                    &Maximum(
                                                        &Maximum(
                                                            &Maximum(
                                                                &Maximum(
                                                                    &Maximum(
                                                                        &Maximum(
                                                                            &Maximum(
                                                                                &Maximum(
                                                                                    &Input(
                                                                                        "name:bedrock",
                                                                                    ),
                                                                                    &Input(
                                                                                        "name:obsidian",
                                                                                    ),
                                                                                ),
                                                                                &Input(
                                                                                    "name:crying_obsidian",
                                                                                ),
                                                                            ),
                                                                            &Input(
                                                                                "name:reinforced_deepslate",
                                                                            ),
                                                                        ),
                                                                        &Input(
                                                                            "name:end_portal_frame",
                                                                        ),
                                                                    ),
                                                                    &Input("name:end_portal"),
                                                                ),
                                                                &Input("name:nether_portal"),
                                                            ),
                                                            &Input("name:moving_piston"),
                                                        ),
                                                        &Input("name:piston_head"),
                                                    ),
                                                    &Input("name:barrier"),
                                                ),
                                                &Input("name:structure_block"),
                                            ),
                                            &Input("name:jigsaw"),
                                        ),
                                        &Input("name:command_block"),
                                    ),
                                    &Input("name:chain_command_block"),
                                ),
                                &Input("name:repeating_command_block"),
                            ),
                            &[Set("rejection", Constant(4))],
                            &[],
                        )],
                    )],
                )],
            ),
        ],
    }],
);

pub const PISTON_ELECTRICAL_PAYLOAD_JAVA_1_21_11_V3: Definition = Definition::new(
    &[
        ("kind", 15),
        ("retracted", 1),
        ("normal", 1),
        ("horizontal", 1),
        ("powered", 1),
        ("head", 1),
        ("entity", 1),
        ("name_missing", 1),
        ("extended_false_or_absent", 1),
        ("coarse", 1),
        ("live", 1),
        ("name:bedrock", 1),
        ("name:obsidian", 1),
        ("name:crying_obsidian", 1),
        ("name:reinforced_deepslate", 1),
        ("name:end_portal_frame", 1),
        ("name:end_portal", 1),
        ("name:nether_portal", 1),
        ("name:moving_piston", 1),
        ("name:piston_head", 1),
        ("name:barrier", 1),
        ("name:structure_block", 1),
        ("name:jigsaw", 1),
        ("name:command_block", 1),
        ("name:chain_command_block", 1),
        ("name:repeating_command_block", 1),
        ("name:piston", 1),
        ("name:sticky_piston", 1),
    ],
    &[("immovable", 0, 1), ("rejection", 0, 4)],
    &[],
    &[Handler {
        name: "evaluate",
        body: &[
            Set(
                "immovable",
                Maximum(
                    &Maximum(
                        &Maximum(
                            &Maximum(
                                &Maximum(
                                    &Maximum(
                                        &Maximum(
                                            &Maximum(
                                                &Maximum(
                                                    &Maximum(
                                                        &Maximum(
                                                            &Maximum(
                                                                &Maximum(
                                                                    &Maximum(
                                                                        &Maximum(
                                                                            &Input("name:bedrock"),
                                                                            &Input("name:obsidian"),
                                                                        ),
                                                                        &Input(
                                                                            "name:crying_obsidian",
                                                                        ),
                                                                    ),
                                                                    &Input(
                                                                        "name:reinforced_deepslate",
                                                                    ),
                                                                ),
                                                                &Input("name:end_portal_frame"),
                                                            ),
                                                            &Input("name:end_portal"),
                                                        ),
                                                        &Input("name:nether_portal"),
                                                    ),
                                                    &Input("name:moving_piston"),
                                                ),
                                                &Input("name:piston_head"),
                                            ),
                                            &Input("name:barrier"),
                                        ),
                                        &Input("name:structure_block"),
                                    ),
                                    &Input("name:jigsaw"),
                                ),
                                &Input("name:command_block"),
                            ),
                            &Input("name:chain_command_block"),
                        ),
                        &Input("name:repeating_command_block"),
                    ),
                    &Equal(&Input("kind"), &Constant(14)),
                ),
            ),
            If(
                Not(&Maximum(
                    &Maximum(
                        &Maximum(
                            &Maximum(
                                &Maximum(
                                    &Equal(&Input("kind"), &Constant(1)),
                                    &Equal(&Input("kind"), &Constant(2)),
                                ),
                                &And(
                                    &And(
                                        &And(
                                            &And(
                                                &And(
                                                    &Equal(&Input("kind"), &Constant(13)),
                                                    &Input("retracted"),
                                                ),
                                                &Not(&Input("head")),
                                            ),
                                            &Not(&Input("entity")),
                                        ),
                                        &Maximum(
                                            &Maximum(&Input("name_missing"), &Input("name:piston")),
                                            &Input("name:sticky_piston"),
                                        ),
                                    ),
                                    &Input("extended_false_or_absent"),
                                ),
                            ),
                            &Equal(&Input("kind"), &Constant(11)),
                        ),
                        &Equal(&Input("kind"), &Constant(14)),
                    ),
                    &Equal(&Input("kind"), &Constant(12)),
                )),
                &[Set("rejection", Constant(1))],
                &[If(
                    Input("coarse"),
                    &[Set("rejection", Constant(2))],
                    &[If(
                        Input("live"),
                        &[Set("rejection", Constant(3))],
                        &[If(
                            Maximum(
                                &Maximum(
                                    &Maximum(
                                        &Maximum(
                                            &Maximum(
                                                &Maximum(
                                                    &Maximum(
                                                        &Maximum(
                                                            &Maximum(
                                                                &Maximum(
                                                                    &Maximum(
                                                                        &Maximum(
                                                                            &Maximum(
                                                                                &Maximum(
                                                                                    &Maximum(
                                                                                        &Input(
                                                                                            "name:bedrock",
                                                                                        ),
                                                                                        &Input(
                                                                                            "name:obsidian",
                                                                                        ),
                                                                                    ),
                                                                                    &Input(
                                                                                        "name:crying_obsidian",
                                                                                    ),
                                                                                ),
                                                                                &Input(
                                                                                    "name:reinforced_deepslate",
                                                                                ),
                                                                            ),
                                                                            &Input(
                                                                                "name:end_portal_frame",
                                                                            ),
                                                                        ),
                                                                        &Input("name:end_portal"),
                                                                    ),
                                                                    &Input("name:nether_portal"),
                                                                ),
                                                                &Input("name:moving_piston"),
                                                            ),
                                                            &Input("name:piston_head"),
                                                        ),
                                                        &Input("name:barrier"),
                                                    ),
                                                    &Input("name:structure_block"),
                                                ),
                                                &Input("name:jigsaw"),
                                            ),
                                            &Input("name:command_block"),
                                        ),
                                        &Input("name:chain_command_block"),
                                    ),
                                    &Input("name:repeating_command_block"),
                                ),
                                &Equal(&Input("kind"), &Constant(14)),
                            ),
                            &[Set("rejection", Constant(4))],
                            &[],
                        )],
                    )],
                )],
            ),
        ],
    }],
);
