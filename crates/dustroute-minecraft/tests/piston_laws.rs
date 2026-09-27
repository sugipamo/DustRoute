use dustroute_minecraft::law::{Expr, Instruction};
use dustroute_minecraft::piston_law::{PistonLaws, builtin_piston_laws, builtin_programs};
use dustroute_minecraft::{
    Block, BlockKind, Facing, ObservationClassification, PistonAction, PistonBlockEntityState,
    PistonState, PistonVariant, WireConnection,
};
use std::collections::BTreeMap;

const KINDS: [BlockKind; 16] = [
    BlockKind::Air,
    BlockKind::Solid,
    BlockKind::Transparent,
    BlockKind::RedstoneWire,
    BlockKind::RedstoneTorch,
    BlockKind::Repeater,
    BlockKind::Comparator,
    BlockKind::Lever,
    BlockKind::Button,
    BlockKind::PressurePlate,
    BlockKind::RedstoneLamp,
    BlockKind::RedstoneBlock,
    BlockKind::Observer,
    BlockKind::Piston,
    BlockKind::PistonHead,
    BlockKind::MovingPiston,
];
const SIDES: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];
const IMMOVABLE: [&str; 15] = [
    "bedrock",
    "obsidian",
    "crying_obsidian",
    "reinforced_deepslate",
    "end_portal_frame",
    "end_portal",
    "nether_portal",
    "moving_piston",
    "piston_head",
    "barrier",
    "structure_block",
    "jigsaw",
    "command_block",
    "chain_command_block",
    "repeating_command_block",
];

#[test]
fn state_and_motion_rules_preserve_all_existing_decisions_and_public_constants() {
    let laws = builtin_piston_laws();
    for state in [
        PistonState::Retracted,
        PistonState::Extending,
        PistonState::Extended,
        PistonState::Retracting,
    ] {
        for powered in [false, true] {
            let expected = match (state, powered) {
                (PistonState::Retracted, true) => Some(PistonAction::Extend),
                (PistonState::Extended, false) => Some(PistonAction::Retract),
                _ => None,
            };
            assert_eq!(laws.requested_action(state, powered), expected);
        }
        for action in [PistonAction::Extend, PistonAction::Retract] {
            assert_eq!(
                laws.permits(state, action),
                matches!(
                    (state, action),
                    (PistonState::Retracted, PistonAction::Extend)
                        | (PistonState::Extended, PistonAction::Retract)
                )
            );
        }
    }
    for action in [PistonAction::Extend, PistonAction::Retract] {
        let extend = action == PistonAction::Extend;
        assert_eq!(
            laws.stable_state(action),
            if extend {
                PistonState::Extended
            } else {
                PistonState::Retracted
            }
        );
        assert_eq!(
            laws.moving_state(action),
            if extend {
                PistonState::Extending
            } else {
                PistonState::Retracting
            }
        );
        for variant in [PistonVariant::Normal, PistonVariant::Sticky] {
            assert_eq!(
                laws.pulls(action, variant),
                !extend && variant == PistonVariant::Sticky
            );
        }
        let effects = laws.motion_effects(action);
        assert_eq!(
            (
                effects.head,
                effects.push_step,
                effects.pull_from,
                effects.pull_to,
                effects.dirty_radius
            ),
            (1, 1, 2, 1, 1)
        );
        assert_eq!(
            (
                effects.source_carrier,
                effects.destination_carrier,
                effects.head_carrier,
                effects.extending,
                effects.progress,
                effects.head_is_payload
            ),
            (!extend, true, true, extend, u8::from(!extend), !extend)
        );
    }
    for count in 0..=12 {
        assert_eq!(laws.can_push_next(count), count < 12);
        for contains_piston in [false, true] {
            assert_eq!(
                laws.chain_supported(count, contains_piston),
                !contains_piston || count == 1
            );
        }
    }
    assert!(!laws.can_push_next(usize::MAX));
    assert_eq!(laws.push_limit(), dustroute_minecraft::PISTON_PUSH_LIMIT);
    assert_eq!(
        laws.default_motion_profile(),
        dustroute_minecraft::DEFAULT_PISTON_MOTION_PROFILE
    );
    assert_eq!(laws.default_motion_profile(), Default::default());
}

fn prior_connection(block: &Block, direction: Facing) -> Result<bool, u16> {
    match block.kind {
        BlockKind::RedstoneWire => match &block.wire_connections {
            Some(map) => Ok(map
                .get(&direction)
                .is_some_and(|v| *v != WireConnection::None)),
            None if block.observed_name.is_some() => Err(1),
            None => Ok(true),
        },
        BlockKind::Repeater | BlockKind::Comparator | BlockKind::Observer => match block.facing {
            Some(facing) => Ok(facing == direction),
            None if block.observed_name.is_some() => Err(2),
            None => Ok(false),
        },
        BlockKind::Lever
        | BlockKind::Button
        | BlockKind::PressurePlate
        | BlockKind::RedstoneBlock
        | BlockKind::RedstoneTorch => Ok(true),
        _ => Ok(false),
    }
}

#[test]
fn connection_data_matches_the_old_kernel_including_unknown_and_synthetic_inputs() {
    for kind in KINDS {
        for observed in [false, true] {
            for facing in [
                None,
                Some(Facing::North),
                Some(Facing::East),
                Some(Facing::South),
                Some(Facing::West),
                Some(Facing::Up),
                Some(Facing::Down),
            ] {
                for shape in [
                    None,
                    Some(BTreeMap::new()),
                    Some(BTreeMap::from([(Facing::East, WireConnection::None)])),
                    Some(BTreeMap::from([(Facing::East, WireConnection::Side)])),
                    Some(BTreeMap::from([(Facing::East, WireConnection::Up)])),
                ] {
                    let mut block = Block::new(kind);
                    block.observed_name = observed.then(|| "minecraft:fixture".into());
                    block.facing = facing;
                    block.wire_connections = shape;
                    for direction in SIDES {
                        assert_eq!(
                            builtin_piston_laws().input_connected(&block, direction),
                            prior_connection(&block, direction)
                        );
                    }
                }
            }
        }
    }
}

fn prior_rejection(block: &Block) -> u16 {
    let supported_piston = block.kind == BlockKind::Piston
        && block.piston_state == Some(PistonState::Retracted)
        && block.piston_variant == Some(PistonVariant::Normal)
        && block
            .facing
            .is_some_and(|f| f.horizontal_offset().is_some())
        && block.powered != Some(true)
        && block.piston_head.is_none()
        && block.piston_entity.is_none()
        && block
            .observed_name
            .as_deref()
            .is_none_or(|n| matches!(n, "minecraft:piston" | "piston"))
        && block
            .observed_properties
            .get("extended")
            .is_none_or(|v| v == "false");
    if !matches!(block.kind, BlockKind::Solid | BlockKind::Transparent) && !supported_piston {
        1
    } else if block.observation_classification == ObservationClassification::Coarse {
        2
    } else if block.requires_live_observation() {
        3
    } else if block
        .observed_name
        .as_deref()
        .is_some_and(|n| IMMOVABLE.contains(&n.strip_prefix("minecraft:").unwrap_or(n)))
    {
        4
    } else {
        0
    }
}

#[test]
fn payload_rules_retain_rejection_precedence_and_exact_name_matching() {
    for kind in KINDS {
        for name in [
            None,
            Some("minecraft:stone"),
            Some("minecraft:glass"),
            Some("minecraft:obsidian"),
            Some("minecraft:piston"),
            Some("minecraft:chest"),
            Some("mod:obsidian"),
        ] {
            for coarse in [false, true] {
                let mut block = Block::new(kind);
                block.observed_name = name.map(str::to_owned);
                if coarse {
                    block.observation_classification = ObservationClassification::Coarse;
                }
                assert_eq!(
                    builtin_piston_laws().payload_rejection(&block).unwrap(),
                    prior_rejection(&block)
                );
            }
        }
    }
    for mask in 0..512 {
        let flag = |n| mask & (1 << n) != 0;
        let mut block = Block::new(BlockKind::Piston);
        block.piston_state = Some(if flag(0) {
            PistonState::Extended
        } else {
            PistonState::Retracted
        });
        block.piston_variant = Some(if flag(1) {
            PistonVariant::Sticky
        } else {
            PistonVariant::Normal
        });
        block.facing = Some(if flag(2) { Facing::Up } else { Facing::East });
        block.powered = Some(flag(3));
        block.piston_head = flag(4).then(|| {
            Block::piston_head(Facing::East, PistonVariant::Normal, false)
                .piston_head
                .unwrap()
        });
        block.piston_entity = flag(5).then(|| {
            Box::new(PistonBlockEntityState {
                pushed_block: Box::new(Block::new(BlockKind::Solid)),
                facing: Facing::East,
                extending: true,
                source: false,
                progress: 0,
            })
        });
        if flag(6) {
            block.observed_name = Some("minecraft:sticky_piston".into());
        }
        if flag(7) {
            block
                .observed_properties
                .insert("extended".into(), "true".into());
        }
        if flag(8) {
            block.observation_classification = ObservationClassification::Coarse;
        }
        assert_eq!(
            builtin_piston_laws().payload_rejection(&block).unwrap(),
            prior_rejection(&block),
            "mask={mask}"
        );
    }
    for name in IMMOVABLE {
        for value in [
            name.to_owned(),
            format!("minecraft:{name}"),
            format!("mod:{name}"),
            format!("minecraft:minecraft:{name}"),
        ] {
            assert_eq!(
                builtin_piston_laws().immovable_name(&value).unwrap(),
                IMMOVABLE.contains(&value.strip_prefix("minecraft:").unwrap_or(&value))
            );
        }
    }
}

fn set(register: &str, value: u16) -> Instruction {
    Instruction::Set {
        register: register.into(),
        value: Expr::Constant { value },
    }
}

#[test]
fn altered_programs_control_decisions_geometry_timing_and_literal_predicates() {
    let original = builtin_programs().clone();
    let mut changed = original.clone();
    changed[0].handlers.get_mut("evaluate").unwrap().extend([
        set("request", 0),
        set("valid", 0),
        set("pull", 0),
    ]);
    changed[1].handlers.get_mut("evaluate").unwrap().extend([
        set("admit_push", 0),
        set("head", 2),
        set("push_step", 2),
        set("pull_from", 3),
        set("progress", 0),
        set("completion", 7),
    ]);
    changed[2]
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(set("connected", 0));
    changed[3].inputs.insert("name:stone".into(), 1);
    changed[3]
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "immovable".into(),
            value: Expr::Input {
                name: "name:stone".into(),
            },
        });
    let laws = PistonLaws::compile(&changed).unwrap();
    assert_eq!(laws.requested_action(PistonState::Retracted, true), None);
    assert!(!laws.permits(PistonState::Retracted, PistonAction::Extend));
    assert!(!laws.pulls(PistonAction::Retract, PistonVariant::Sticky));
    assert!(!laws.can_push_next(0));
    let motion = laws.motion_effects(PistonAction::Retract);
    assert_eq!(
        (
            motion.head,
            motion.push_step,
            motion.pull_from,
            motion.progress
        ),
        (2, 2, 3, 0)
    );
    assert_eq!(laws.default_motion_profile().movement_game_ticks, 7);
    assert_eq!(
        laws.input_connected(&Block::new(BlockKind::Lever), Facing::East),
        Ok(false)
    );
    assert!(laws.immovable_name("minecraft:stone").unwrap());
    assert!(!laws.immovable_name("minecraft:obsidian").unwrap());
    assert_eq!(builtin_programs(), &original);
}

#[test]
fn incompatible_programs_and_failed_evaluation_cannot_supply_piston_effects() {
    let mut programs = builtin_programs().clone();
    programs[1]
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(set("progress", 2));
    assert!(PistonLaws::compile(&programs).is_err());
    let mut programs = builtin_programs().clone();
    programs[3].inputs.insert("unrecognized".into(), 1);
    assert!(PistonLaws::compile(&programs).is_err());
    let mut programs = builtin_programs().clone();
    programs[3]
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::ScheduleIfAbsent {
            event: "evaluate".into(),
            after: 1,
        });
    assert!(PistonLaws::compile(&programs).is_err());
    let mut programs = builtin_programs().clone();
    programs[3]
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(set("rejection", 5));
    let laws = PistonLaws::compile(&programs).unwrap();
    assert!(
        laws.payload_rejection(&Block::new(BlockKind::Solid))
            .is_err()
    );
}
