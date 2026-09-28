use dustroute_library::flying_machine::*;
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::blueprint_update::BlueprintUpdates;
use dustroute_translate::flying_machine::generate_flying_machine;
use dustroute_translate::promotion::CheckStatus;
use dustroute_translate::{Pos, RotationY};

fn request() -> FlyingMachineRequest {
    FlyingMachineRequest {
        namespace: "generated.test".into(),
        distance: 4,
        body: FlyingMachineBody::Compact,
        rotation: RotationY::R0,
        mirrored: false,
        attachments: vec![],
    }
}

#[test]
fn generated_bodies_rotations_reflections_and_distances_use_common_checks() {
    for body in [
        FlyingMachineBody::Compact,
        FlyingMachineBody::SideBlocks,
        FlyingMachineBody::SlimeWings,
        FlyingMachineBody::HoneyNose,
    ] {
        for (index, rotation) in [
            RotationY::R0,
            RotationY::R90,
            RotationY::R180,
            RotationY::R270,
        ]
        .into_iter()
        .enumerate()
        {
            for mirrored in [false, true] {
                let r = FlyingMachineRequest {
                    body,
                    rotation,
                    mirrored,
                    distance: [1, 4, 8, 16][index],
                    ..request()
                };
                let generated = generate_flying_machine(r, BehaviorBudget::default()).unwrap();
                assert_eq!(
                    generated.verification.status,
                    CheckStatus::Passed,
                    "{body:?}/{rotation:?}/{mirrored}: {:?}",
                    generated.verification
                );
                assert!(!generated.verification.live_world_verified);
                assert_eq!(
                    generated.initial.blocks.len(),
                    generated.moving_positions.len() + 3
                );
                assert_eq!(
                    generated.initial.blocks.len(),
                    generated.expected_arrival.blocks.len()
                );
                assert!(
                    generated.verification.construction_steps > 0
                        && generated.verification.removal_steps > 0
                );
                // Geometry requirements retain each native state after translation.
                for p in &generated.moving_positions {
                    let from = generated
                        .initial
                        .blocks
                        .iter()
                        .find(|b| b.pos == *p)
                        .unwrap();
                    let delta = generated.displacement;
                    let to = generated
                        .expected_arrival
                        .blocks
                        .iter()
                        .find(|b| b.pos == p.offset(delta.x, delta.y, delta.z))
                        .unwrap();
                    assert_eq!((&from.name, &from.properties), (&to.name, &to.properties));
                }
            }
        }
    }
}

#[test]
fn added_blocks_must_arrive_and_drafts_require_fresh_adoption_after_restart() {
    let extra = FlyingMachineAttachment {
        position: Pos::new(0, 0, -1),
        material: FlyingMachineMaterial::Glass,
    };
    let generated = generate_flying_machine(
        FlyingMachineRequest {
            attachments: vec![extra],
            ..request()
        },
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(generated.verification.status, CheckStatus::Passed);
    let mut updates = BlueprintUpdates::new(generated.records.catalog().unwrap());
    updates.create(generated.request.clone()).unwrap();
    let mut restored = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    restored.adopt(&generated.request.id).unwrap();
    let mut detached = request();
    detached.attachments.push(FlyingMachineAttachment {
        position: Pos::new(0, 0, -1),
        material: FlyingMachineMaterial::Honey,
    });
    let rejected = generate_flying_machine(detached, BehaviorBudget::default()).unwrap();
    assert_ne!(
        rejected.verification.status,
        CheckStatus::Passed,
        "detached honey cannot count as successful flight"
    );
    let mut changed = rejected.request;
    changed.id = generated.request.id;
    let mut updates = BlueprintUpdates::new(rejected.records.catalog().unwrap());
    updates.create(changed.clone()).unwrap();
    assert!(updates.adopt(&changed.id).is_err());
}

#[test]
fn generation_rejects_invalid_geometry_limits_and_never_passes_exhausted_search() {
    for distance in [0, 17, u16::MAX] {
        assert!(
            generate_flying_machine(
                FlyingMachineRequest {
                    distance,
                    ..request()
                },
                BehaviorBudget::default()
            )
            .is_err()
        );
    }
    for p in [
        Pos::new(0, 0, 0),
        Pos::new(0, 2, 0),
        Pos::new(i32::MIN, 0, 0),
    ] {
        let r = FlyingMachineRequest {
            attachments: vec![FlyingMachineAttachment {
                position: p,
                material: FlyingMachineMaterial::Stone,
            }],
            ..request()
        };
        assert!(generate_flying_machine(r, BehaviorBudget::default()).is_err());
    }
    let limited = generate_flying_machine(
        request(),
        BehaviorBudget {
            max_states: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(limited.verification.status, CheckStatus::Undetermined);
}

#[test]
fn excessive_connected_payload_is_not_a_generated_success() {
    let mut positions = Vec::new();
    for y in [-1, 0] {
        for z in -5..=-1 {
            positions.push(Pos::new(0, y, z));
        }
    }
    positions.extend([Pos::new(0, -2, -1), Pos::new(0, -2, -2)]);
    let r = FlyingMachineRequest {
        attachments: positions
            .into_iter()
            .map(|position| FlyingMachineAttachment {
                position,
                material: FlyingMachineMaterial::Slime,
            })
            .collect(),
        ..request()
    };
    let generated = generate_flying_machine(r, BehaviorBudget::default()).unwrap();
    assert_ne!(generated.verification.status, CheckStatus::Passed);
}
