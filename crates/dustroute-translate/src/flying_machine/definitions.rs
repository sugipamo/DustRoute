//! Engine authoring is data. These definitions grant no physical behavior or
//! verification evidence. Coordinates describe an eastbound, resting machine.
use dustroute_library::flying_machine::{
    FlyingMachineBody, FlyingMachineEngine, FlyingMachineMaterial,
};
use dustroute_minecraft::{Facing, Pos};

#[derive(Clone, Copy)]
pub(super) enum State {
    Piston {
        facing: Facing,
        sticky: bool,
    },
    /// Native Minecraft facing: the watched side, not the output side.
    Observer {
        watching: Facing,
    },
    Material(FlyingMachineMaterial),
    WallLever {
        facing: Facing,
        powered: bool,
    },
    Obsidian,
}

#[derive(Clone, Copy)]
pub(super) struct Part {
    pub position: Pos,
    pub state: State,
}
const fn part(x: i32, y: i32, z: i32, state: State) -> Part {
    Part {
        position: Pos::new(x, y, z),
        state,
    }
}

#[derive(Clone, Copy)]
pub(super) struct EngineDefinition {
    pub moving: &'static [Part],
    pub fixed: &'static [Part],
    /// One initially OFF wall lever, held ON after launch.
    pub launch: Pos,
    /// y/z of the moving row whose leading block meets the generated stopper.
    pub stopper_lane: (i32, i32),
    /// Native endpoint states, at their source positions, before translation.
    /// Unlisted parts retain their initial state. Includes the launch lever.
    pub arrival_overrides: &'static [Part],
    pub bodies: &'static [(FlyingMachineBody, &'static [Part])],
}

use Facing::{East, Up, West};
use FlyingMachineBody::{Compact, HoneyNose, SideBlocks, SlimeWings};
use FlyingMachineMaterial::{Glass, Honey, Slime, Stone};
use State::{Material, Observer, Piston, WallLever};

const SIDES: &[Part] = &[
    part(0, 0, -1, Material(Stone)),
    part(1, 0, 2, Material(Stone)),
];
const WINGS: &[Part] = &[
    part(0, 0, -1, Material(Slime)),
    part(1, 0, 2, Material(Slime)),
    part(0, 0, -2, Material(Stone)),
    part(1, 0, 3, Material(Stone)),
];
const NOSE: &[Part] = &[
    part(2, 0, 1, Material(Honey)),
    part(2, 0, 2, Material(Glass)),
];

const SLIME_RELAY: EngineDefinition = EngineDefinition {
    moving: &[
        part(0, 0, 0, Material(Slime)),
        part(
            0,
            0,
            1,
            Piston {
                facing: East,
                sticky: false,
            },
        ),
        part(0, 1, 0, Observer { watching: Up }),
        part(
            1,
            0,
            0,
            Piston {
                facing: West,
                sticky: true,
            },
        ),
        part(1, 0, 1, Material(Slime)),
        part(1, 1, 1, Observer { watching: Up }),
    ],
    fixed: &[
        part(-1, 2, 0, Material(Stone)),
        part(
            0,
            2,
            0,
            WallLever {
                facing: East,
                powered: false,
            },
        ),
    ],
    launch: Pos::new(0, 2, 0),
    stopper_lane: (0, 1),
    arrival_overrides: &[part(
        0,
        2,
        0,
        WallLever {
            facing: East,
            powered: true,
        },
    )],
    bodies: &[
        (Compact, &[]),
        (SideBlocks, SIDES),
        (SlimeWings, WINGS),
        (HoneyNose, NOSE),
    ],
};

// Horizontal observers directly drive the pistons. Four honey blocks retain
// both observers without requiring conductive adhesion blocks.
const HONEY_DIRECT: EngineDefinition = EngineDefinition {
    moving: &[
        part(-1, 0, 0, Material(Honey)),
        part(0, 0, 0, Material(Honey)),
        part(
            0,
            0,
            1,
            Piston {
                facing: East,
                sticky: false,
            },
        ),
        part(-1, 0, 1, Observer { watching: West }),
        part(
            1,
            0,
            0,
            Piston {
                facing: West,
                sticky: true,
            },
        ),
        part(1, 0, 1, Material(Honey)),
        part(2, 0, 1, Material(Honey)),
        part(2, 0, 0, Observer { watching: East }),
    ],
    fixed: &[
        part(-3, 0, 1, Material(Stone)),
        part(
            -2,
            0,
            1,
            WallLever {
                facing: East,
                powered: false,
            },
        ),
    ],
    launch: Pos::new(-2, 0, 1),
    stopper_lane: (0, 1),
    arrival_overrides: &[part(
        -2,
        0,
        1,
        WallLever {
            facing: East,
            powered: true,
        },
    )],
    bodies: &[(Compact, &[]), (SideBlocks, SIDES)],
};

pub(super) fn definition(engine: FlyingMachineEngine) -> &'static EngineDefinition {
    match engine {
        FlyingMachineEngine::SlimeRelay => &SLIME_RELAY,
        FlyingMachineEngine::HoneyDirect => &HONEY_DIRECT,
    }
}
