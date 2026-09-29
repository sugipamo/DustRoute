use super::{
    piston_nbt::{PISTON_TYPE, PistonData},
    reconstruction::*,
    wire::Reader,
    world::{Dimension, World},
};
use crate::{NativeBlockState, protocol::put_varint};

fn state(name: &str, properties: &[(&str, &str)]) -> NativeBlockState {
    NativeBlockState {
        name: format!("minecraft:{name}"),
        properties: properties
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}
fn field(out: &mut Vec<u8>, kind: u8, name: &str) {
    out.push(kind);
    string(out, name);
}
fn string(out: &mut Vec<u8>, text: &str) {
    out.extend((text.len() as u16).to_be_bytes());
    out.extend(text.bytes());
}
fn nbt(carried: &NativeBlockState, progress: f32, extending: bool, source: bool) -> Vec<u8> {
    let mut out = vec![10];
    field(&mut out, 10, "blockState");
    field(&mut out, 8, "Name");
    string(&mut out, &carried.name);
    field(&mut out, 10, "Properties");
    for (k, v) in &carried.properties {
        field(&mut out, 8, k);
        string(&mut out, v);
    }
    out.extend([0, 0]);
    field(&mut out, 1, "facing");
    out.push(5);
    field(&mut out, 5, "progress");
    out.extend(progress.to_be_bytes());
    field(&mut out, 1, "extending");
    out.push(u8::from(extending));
    field(&mut out, 1, "source");
    out.push(u8::from(source));
    out.push(0);
    out
}
fn chunk(data: &[u8], carrier: i32) -> Vec<u8> {
    let mut out = vec![0; 9]; // chunk 0,0 and no heightmaps
    let mut section = vec![0, 1, 16]; // one non-air, direct palette
    for start in (0..4096).step_by(4) {
        let mut word = 0u64;
        for i in 0..4 {
            if start + i == 8 * 256 + 8 * 16 + 8 {
                word |= (carrier as u64) << (16 * i);
            }
        }
        section.extend(word.to_be_bytes());
    }
    section.extend([0, 0]); // biome single palette
    put_varint(&mut out, section.len() as i32);
    out.extend(section);
    out.push(1);
    out.push(0x88);
    out.extend(8i16.to_be_bytes());
    put_varint(&mut out, PISTON_TYPE);
    out.extend(data);
    out.extend([0; 6]);
    out
}
fn world() -> World {
    let mut w = World::default();
    w.select_dimension("minecraft:overworld".into(), Dimension::new(0, 16).unwrap());
    w
}
fn moving_id() -> i32 {
    super::state_id(&state(
        "moving_piston",
        &[("facing", "east"), ("type", "normal")],
    ))
    .unwrap()
}

#[test]
fn chunk_native_carrier_restores_half_progress_and_completes_without_action_history() {
    let mut w = world();
    let bytes = chunk(&nbt(&state("stone", &[]), 0.5, true, false), moving_id());
    let data = w.load(&bytes, 64).unwrap();
    assert_eq!(data.len(), 1);
    let mut r = Reconstruction::default();
    r.chunk_loaded([0, 0], data, 77);
    let m = r.cell(&w, [8, 8, 8]).moving.unwrap();
    assert_eq!(m.progress, MotionProgress::Half);
    assert_eq!(m.last_progress, MotionProgress::Half);
    assert_eq!(m.action_sequence, None);
    assert_eq!(m.chunk_sequence, Some(77));
    r.advance(&w, 7);
    let cell = r.cell(&w, [8, 8, 8]);
    assert_eq!(cell.state.unwrap().name, "minecraft:stone");
    assert!(cell.moving.is_none());
    assert!(matches!(
        cell.origin,
        StateOrigin::ChunkUpdate { chunk_sequence: 77 }
    ));
    assert_eq!(w.block([8, 8, 8]), Some(moving_id()));
}

#[test]
fn native_carrier_source_roles_and_progress_are_validated() {
    for (carried, extending, role) in [
        (
            state(
                "piston_head",
                &[("facing", "east"), ("type", "normal"), ("short", "false")],
            ),
            true,
            CarrierRole::Head,
        ),
        (
            state(
                "sticky_piston",
                &[("facing", "east"), ("extended", "false")],
            ),
            false,
            CarrierRole::Body,
        ),
        (
            state(
                "quartz_stairs",
                &[
                    ("facing", "north"),
                    ("half", "top"),
                    ("shape", "inner_left"),
                    ("waterlogged", "false"),
                ],
            ),
            false,
            CarrierRole::Payload,
        ),
    ] {
        for progress in [0.0, 0.5, 1.0] {
            let bytes = nbt(&carried, progress, extending, role != CarrierRole::Payload);
            let actual = PistonData::read(&mut Reader::new(&bytes)).unwrap().unwrap();
            assert_eq!(actual.carried, carried);
            assert_eq!(actual.role, role);
        }
    }
    assert!(PistonData::read(&mut Reader::new(&[0])).unwrap().is_none());
    for progress in [-1.0, 0.25, f32::NAN, 2.0] {
        assert!(
            PistonData::read(&mut Reader::new(&nbt(
                &state("stone", &[]),
                progress,
                true,
                false
            )))
            .is_err()
        );
    }
    assert!(
        PistonData::read(&mut Reader::new(&nbt(
            &state("stone", &[]),
            0.0,
            true,
            true
        )))
        .is_err()
    );
}

#[test]
fn native_carrier_restore_obeys_the_runtime_resource_limit() {
    let data = PistonData::read(&mut Reader::new(&nbt(
        &state("stone", &[]),
        0.0,
        true,
        false,
    )))
    .unwrap()
    .unwrap();
    let mut r = Reconstruction::default();
    r.chunk_loaded([0, 0], vec![([8, 8, 8], data); 4097], 1);
    assert_eq!(r.issue, Some(ReconstructionIssue::Limit));
    assert!(r.cell(&world(), [8, 8, 8]).state.is_none());
}

#[test]
fn corrupt_chunk_nbt_never_commits_partial_world_data() {
    let full = chunk(&nbt(&state("stone", &[]), 0.0, true, false), moving_id());
    for length in [0, 8, 12, full.len() - 1, full.len() - 8, full.len() - 20] {
        let mut w = world();
        assert!(w.load(&full[..length], 64).is_err());
        assert_eq!(w.block([8, 8, 8]), None);
    }
    assert!(
        world()
            .load(&chunk(&nbt(&state("stone", &[]), 0.0, true, false), 1), 64)
            .is_err()
    );
    let good = nbt(&state("stone", &[]), 0.0, true, false);
    for length in 0..good.len() {
        assert!(PistonData::read(&mut Reader::new(&good[..length])).is_err());
    }
    let mut duplicate = good[..good.len() - 1].to_vec();
    field(&mut duplicate, 3, "facing");
    duplicate.extend(5i32.to_be_bytes());
    duplicate.push(0);
    assert!(PistonData::read(&mut Reader::new(&duplicate)).is_err());
}

#[test]
fn fresh_dependency_chunks_recover_without_reconnect_and_unload_reopens_requirement() {
    let mut w = world();
    w.seed_replay_cell(
        [15, 8, 8],
        super::state_id(&state(
            "piston",
            &[("facing", "east"), ("extended", "false")],
        ))
        .unwrap(),
    );
    w.seed_replay_cell([16, 8, 8], 0);
    let mut r = Reconstruction::default();
    r.action(
        &w,
        [15, 8, 8],
        Action::Extend,
        Direction::East,
        "minecraft:piston",
        1,
    );
    assert!(r.issue.is_none());
    r.chunk_replaced([0, 0]);
    assert_eq!(
        r.recovery_chunks.iter().copied().collect::<Vec<_>>(),
        vec![[0, 0], [1, 0]]
    );
    r.chunk_loaded([0, 0], vec![], 2);
    assert!(r.issue.is_some());
    r.chunk_replaced([0, 0]);
    r.chunk_loaded([1, 0], vec![], 3);
    assert!(r.issue.is_some());
    r.chunk_loaded([0, 0], vec![], 4);
    assert!(r.issue.is_none());
    assert!(r.recovery_chunks.is_empty());
    assert!(r.cell(&w, [15, 8, 8]).state.is_some());
}

#[test]
fn events_during_recovery_require_new_baseline_and_clock_failure_cannot_be_cleared_by_chunk() {
    let mut w = world();
    w.seed_replay_cell(
        [8, 8, 8],
        super::state_id(&state(
            "piston",
            &[("facing", "east"), ("extended", "false")],
        ))
        .unwrap(),
    );
    w.seed_replay_cell([16, 8, 8], 0);
    let mut r = Reconstruction::default();
    r.action(
        &w,
        [8, 8, 8],
        Action::Extend,
        Direction::East,
        "minecraft:piston",
        1,
    );
    r.chunk_replaced([0, 0]);
    r.action(
        &w,
        [8, 8, 8],
        Action::Retract,
        Direction::East,
        "minecraft:piston",
        2,
    );
    assert!(r.recovery_chunks.contains(&[1, 0]));
    r.unsupported_ticking();
    r.chunk_loaded([0, 0], vec![], 3);
    r.chunk_loaded([1, 0], vec![], 4);
    assert_eq!(r.issue, Some(ReconstructionIssue::UnsupportedTickControl));
}
