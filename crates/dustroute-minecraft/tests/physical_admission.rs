use dustroute_minecraft::device_program::BUILTIN_DEVICES;
use dustroute_minecraft::execution_context::{WorldExecutionContext, WorldExecutionProfile};
use dustroute_minecraft::physical::{self, Faces, PhysicalSpec, Shape, Support};
use dustroute_minecraft::piston_electrical::{SIDES, conducts, full_face, validate_evidence};
use dustroute_minecraft::{
    Block, BlockKind, Facing, ObservationClassification, PistonState, Pos, World,
};

#[test]
fn old_kind_discriminants_keep_their_archived_world_id_meaning() {
    // BlockKind's derived Hash contributes these tags to saved ShapeId/StateId.
    // New kinds must not shift any already published tag.
    for (tag, kind) in [
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
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(kind as usize, tag);
    }
}

#[test]
fn registered_identity_is_observable_without_granting_an_execution_profile() {
    for device in BUILTIN_DEVICES {
        let spec = device.spec();
        assert_eq!(spec.physical, physical::of_kind(spec.kind));
        for name in spec.observed_names {
            assert_eq!(
                physical::classify(&format!("minecraft:{name}")),
                (spec.kind, ObservationClassification::Exact)
            );
        }
    }
    for (name, kind) in [
        ("oak_button", BlockKind::Button),
        ("comparator", BlockKind::Comparator),
        ("redstone_wall_torch", BlockKind::RedstoneTorch),
        ("heavy_weighted_pressure_plate", BlockKind::PressurePlate),
    ] {
        assert_eq!(
            physical::classify(name),
            (kind, ObservationClassification::Exact)
        );
        let mut block = Block::new(kind);
        block.observed_name = Some(format!("minecraft:{name}"));
        assert!(validate_evidence(&block).is_err());
    }
    for name in ["another_mod:observer", "copper_bulb", "unknown"] {
        assert_eq!(
            physical::classify(name),
            (BlockKind::Solid, ObservationClassification::Coarse)
        );
    }
}

#[test]
fn physical_support_does_not_imply_conduction_or_device_admission() {
    for (kind, conductor) in [
        (BlockKind::Solid, true),
        (BlockKind::RedstoneLamp, true),
        (BlockKind::Observer, false),
        (BlockKind::RedstoneBlock, false),
        (BlockKind::Transparent, false),
    ] {
        let block = Block::new(kind);
        assert_eq!(conducts(&block), conductor);
        for side in SIDES {
            assert!(full_face(&block, side));
        }
    }
    let mut unknown = Block::new(BlockKind::Solid);
    unknown.observation_classification = ObservationClassification::Coarse;
    unknown.observed_name = Some("minecraft:unmodeled_device".into());
    assert!(!conducts(&unknown));
    assert!(SIDES.into_iter().all(|side| !full_face(&unknown, side)));
    assert!(validate_evidence(&unknown).is_err());
    assert!(
        std::convert::TryInto::<physical::CheckedPhysical>::try_into(PhysicalSpec {
            shape: Shape::Empty,
            conducting: Faces::All,
            ..physical::of_kind(BlockKind::Solid).spec()
        })
        .is_err()
    );
}

#[test]
fn piston_support_faces_follow_body_and_head_state_in_every_direction() {
    for facing in SIDES {
        let mut body = Block::new(BlockKind::Piston);
        body.facing = Some(facing);
        for state in [PistonState::Retracted, PistonState::Extended] {
            body.piston_state = Some(state);
            for side in SIDES {
                assert_eq!(
                    full_face(&body, side),
                    state == PistonState::Retracted || side == facing.opposite()
                );
            }
        }
        let head = Block::piston_head(facing, Default::default(), false);
        for side in SIDES {
            assert_eq!(full_face(&head, side), side == facing);
        }
        assert!(!conducts(&body));
    }
    assert_eq!(
        physical::of_kind(BlockKind::Repeater).support(),
        Support::Below
    );
    assert_eq!(
        physical::of_kind(BlockKind::Button).support(),
        Support::Attached
    );
    assert_eq!(
        physical::of_kind(BlockKind::Observer).support(),
        Support::None
    );
}

#[test]
fn centered_support_and_invalid_attachment_policies_are_distinct_from_full_faces() {
    use physical::{SupportFace, SupportLoss, SupportTrigger};
    for facing in SIDES {
        let mut body = Block::new(BlockKind::Piston);
        body.facing = Some(facing);
        body.piston_state = Some(PistonState::Extended);
        for side in SIDES {
            assert_eq!(
                physical::of_kind(body.kind).center_face(&body, side),
                side != facing
            );
        }
        for short in [false, true] {
            let head = Block::piston_head(facing, Default::default(), short);
            for side in SIDES {
                assert_eq!(
                    physical::of_kind(head.kind).center_face(&head, side),
                    side == facing || side == facing.opposite()
                );
            }
        }
    }
    let wire = physical::of_kind(BlockKind::RedstoneWire).spec();
    for invalid in [
        PhysicalSpec {
            support: Support::None,
            ..wire
        },
        PhysicalSpec {
            support_loss: Some(SupportLoss {
                trigger: SupportTrigger::Shape,
                face: SupportFace::Full,
                notify_around_neighbors: true,
            }),
            ..wire
        },
        PhysicalSpec {
            support_loss: Some(SupportLoss {
                trigger: SupportTrigger::Shape,
                face: SupportFace::StandingCenter,
                notify_around_neighbors: false,
            }),
            ..wire
        },
    ] {
        assert!(physical::CheckedPhysical::try_from(invalid).is_err());
    }
}

#[test]
fn world_contracts_check_admission_independently_from_physical_geometry() {
    let current = WorldExecutionContext::for_profile(
        WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V16,
    );
    let proof =
        WorldExecutionContext::for_profile(WorldExecutionProfile::DustTorchSynchronousGameTickV1);
    assert_eq!(
        current.physical_admission_revision(),
        Some(physical::REVISION)
    );
    assert_eq!(proof.physical_admission_revision(), None);
    let mut world = World::new();
    let observer = world.place(BlockKind::Observer, Pos::new(0, 0, 0));
    observer.facing = Some(Facing::East);
    observer.powered = Some(false);
    current.validate_world_kinds(&world).unwrap();
    assert!(proof.validate_world_kinds(&world).is_err());
    world.place(BlockKind::PressurePlate, Pos::new(1, 0, 0));
    assert!(current.validate_world_kinds(&world).is_err());
}
