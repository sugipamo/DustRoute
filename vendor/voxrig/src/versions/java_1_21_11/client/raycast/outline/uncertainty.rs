//! Conservative continuous origin uncertainty for admitted full-cube targets.
//! This covers visited cells, not arbitrary outline AABBs: native outline DDA
//! only queries shapes owned by visited cells, including protruding shapes.
use super::{BlockHit, Direction, NativeBlockState, cast, direction};

const REACH: f64 = 4.5;
const FACE_ERROR: &str = "target face/reach differs across observed position uncertainty";

pub(in super::super) fn check(
    eye: [f64; 3],
    error: [f64; 3],
    rotation: [f32; 2],
    hit: &BlockHit,
    mut read: impl FnMut([i32; 3]) -> anyhow::Result<NativeBlockState>,
) -> anyhow::Result<()> {
    // Bound both arithmetic and cell enumeration. Real admitted aim errors are
    // below 1/16 block; this private helper does not admit new pose evidence.
    anyhow::ensure!(
        eye.iter().all(|v| v.is_finite() && v.abs() < 30_000_001.0)
            && error
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && rotation.iter().all(|v| v.is_finite()),
        "invalid uncertain eye geometry"
    );
    let d = direction(rotation); // Native float sine table, not normalized.
    let (axis, upper) = match hit.face {
        Some(Direction::Down) => (1, false),
        Some(Direction::Up) => (1, true),
        Some(Direction::North) => (2, false),
        Some(Direction::South) => (2, true),
        Some(Direction::West) => (0, false),
        Some(Direction::East) => (0, true),
        None => anyhow::bail!(FACE_ERROR),
    };
    // This must be an entry plane, not the exit of a ray starting inside a cube.
    anyhow::ensure!(
        if upper {
            d[axis] < -1e-7
        } else {
            d[axis] > 1e-7
        },
        FACE_ERROR
    );
    let plane = f64::from(hit.position[axis]) + f64::from(upper);
    // Cover native DDA's 1e-7 extension, outline clipping tolerance and rounding
    // at world coordinates. Closed/tangent contact is deliberately refused.
    let pad = eye.map(|v| 1e-6 + v.abs() * f64::EPSILON * 16.0);
    let mut latest: f64 = 0.0;
    let mut origins = Vec::with_capacity(8);
    for mask in 0..8 {
        let origin: [f64; 3] = std::array::from_fn(|i| {
            eye[i]
                + if mask & (1 << i) == 0 {
                    -error[i]
                } else {
                    error[i]
                }
        });
        let t = (plane - origin[axis]) / d[axis];
        // Plane time and both tangential coordinates are affine in origin.
        // Their extrema over the WHOLE error box are attained at these vertices.
        // Leave the native initial-inside probe (0.001 of ray length) behind.
        anyhow::ensure!(t > REACH * 0.001 + 1e-6 && t < REACH - 1e-6, FACE_ERROR);
        for i in (0..3).filter(|&i| i != axis) {
            let v = origin[i] + d[i] * t;
            let low = f64::from(hit.position[i]);
            anyhow::ensure!(v > low + pad[i] && v < low + 1.0 - pad[i], FACE_ERROR);
        }
        latest = latest.max(t);
        origins.push(origin);
    }
    let end = std::array::from_fn(|i| eye[i] + d[i] * latest);
    let expanded = std::array::from_fn(|i| error[i] + pad[i]);
    let min: [i32; 3] = std::array::from_fn(|i| (eye[i].min(end[i]) - expanded[i]).floor() as i32);
    let max: [i32; 3] = std::array::from_fn(|i| (eye[i].max(end[i]) + expanded[i]).floor() as i32);
    for x in min[0]..=max[0] {
        for y in min[1]..=max[1] {
            for z in min[2]..=max[2] {
                let cell = [x, y, z];
                if cell == hit.position || !intersects(eye, end, expanded, cell) {
                    continue;
                }
                // Every possible DDA cell before the target is covered. Refuse
                // ALL non-air, even empty/partial/unsupported outline shapes.
                // Unknown cells propagate failure; off-beam cells need no read.
                let state = read(cell)?;
                anyhow::ensure!(
                    matches!(
                        state.name.as_str(),
                        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
                    ),
                    "uncertain eye corridor is not clear at {cell:?}"
                );
            }
        }
    }
    // Independent consistency check with the actual native outline traversal.
    // This is NOT the occlusion proof: the swept cell volume above is continuous.
    for origin in origins {
        let end = std::array::from_fn(|i| origin[i] + d[i] * REACH);
        let observed = cast(origin, end, &mut read)?;
        anyhow::ensure!(
            observed.is_some_and(|h| h.position == hit.position
                && h.face == hit.face
                && h.state == hit.state),
            FACE_ERROR
        );
    }
    Ok(())
}

/// Segment versus closed cell expanded by all origin errors (Minkowski sum).
/// The rectangular swept superset can reject some valid rays; it cannot omit
/// an intermediate origin which is absent from a finite set of sampled rays.
fn intersects(start: [f64; 3], end: [f64; 3], error: [f64; 3], cell: [i32; 3]) -> bool {
    let mut enter: f64 = 0.0;
    let mut exit: f64 = 1.0;
    for i in 0..3 {
        let lo = f64::from(cell[i]) - error[i];
        let hi = f64::from(cell[i]) + 1.0 + error[i];
        let delta = end[i] - start[i];
        if delta == 0.0 {
            if start[i] < lo || start[i] > hi {
                return false;
            }
        } else {
            let a = (lo - start[i]) / delta;
            let b = (hi - start[i]) / delta;
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn block(name: &str) -> NativeBlockState {
        NativeBlockState {
            name: format!("minecraft:{name}"),
            properties: Default::default(),
        }
    }
    fn scene(p: [i32; 3]) -> anyhow::Result<NativeBlockState> {
        Ok(block(if p == [0, 0, 0] { "stone" } else { "air" }))
    }
    fn target(eye: [f64; 3], rotation: [f32; 2]) -> BlockHit {
        let d = direction(rotation);
        cast(eye, std::array::from_fn(|i| eye[i] + d[i] * REACH), scene)
            .unwrap()
            .unwrap()
    }
    #[test]
    fn admits_all_six_cube_faces_with_three_axis_uncertainty() {
        for (eye, rotation, face) in [
            ([0.5, 0.5, -2.0], [0., 0.], Direction::North),
            ([0.5, 0.5, 3.0], [180., 0.], Direction::South),
            ([-2.0, 0.5, 0.5], [-90., 0.], Direction::West),
            ([3.0, 0.5, 0.5], [90., 0.], Direction::East),
            ([0.5, 3.0, 0.5], [0., 90.], Direction::Up),
            ([0.5, -2.0, 0.5], [0., -90.], Direction::Down),
        ] {
            let hit = target(eye, rotation);
            assert_eq!(hit.face, Some(face));
            check(eye, [0.01; 3], rotation, &hit, scene).unwrap();
        }
    }
    #[test]
    fn refuses_edges_corners_and_reach_endpoints() {
        let rot = [0., 0.];
        for (eye, error) in [
            ([0.01, 0.5, -2.0], [0.02, 0., 0.]),
            ([0.01, 0.01, -2.0], [0.02, 0.02, 0.]),
            ([0.5, 0.5, -4.49], [0., 0., 0.02]),
            ([0.5, 0.5, -4.4999999], [0., 0., 1e-8]),
            ([0.5, 0.5, -0.001], [0., 0., 1e-5]),
        ] {
            let hit = target(eye, rot);
            assert!(
                check(eye, error, rot, &hit, scene)
                    .unwrap_err()
                    .to_string()
                    .contains(FACE_ERROR)
            );
        }
        let eye = [0.5, 0.5, -4.48];
        check(eye, [0.001; 3], rot, &target(eye, rot), scene).unwrap();
    }
    #[test]
    fn rejects_obstructions_and_unknown_cells_only_when_beam_can_visit_them() {
        let eye = [0.5, 0.5, -2.0];
        let rot = [0., 0.];
        let hit = target(eye, rot);
        for name in ["stone", "oak_slab", "tall_grass"] {
            let result = check(eye, [0.001; 3], rot, &hit, |p| {
                if p == [0, 0, -1] {
                    Ok(block(name))
                } else {
                    scene(p)
                }
            });
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("corridor is not clear")
            );
        }
        assert!(
            check(eye, [0.001; 3], rot, &hit, |p| {
                if p == [0, 0, -1] {
                    anyhow::bail!("unknown cell")
                } else {
                    scene(p)
                }
            })
            .unwrap_err()
            .to_string()
            .contains("unknown cell")
        );
        check(eye, [0.001; 3], rot, &hit, |p| {
            if p == [1, 0, -1] {
                anyhow::bail!("off-beam unknown")
            } else {
                scene(p)
            }
        })
        .unwrap();
        for error in [[f64::NAN, 0., 0.], [-0.01, 0., 0.], [2., 0., 0.]] {
            assert!(check(eye, error, rot, &hit, scene).is_err());
        }
    }
    #[test]
    fn swept_volume_covers_intermediate_origins_and_closed_endpoint_contacts() {
        // The continuous volume must cover intermediate origins as well as vertices.
        let start = [-1., 0.5, -1.];
        let end = [2., 0.5, 2.];
        let cell = [0, 0, 0];
        let error = [1.1, 0., 1.1];
        assert!(intersects(start, end, error, cell));
        // Exhaustive finite samples are a regression check of the superset,
        // while the slab/Minkowski construction supplies the continuous bound.
        for x in -11..=11 {
            for z in -11..=11 {
                let offset = [f64::from(x) / 10., 0., f64::from(z) / 10.];
                let a = std::array::from_fn(|i| start[i] + offset[i]);
                let b = std::array::from_fn(|i| end[i] + offset[i]);
                for cx in -3..=3 {
                    for cz in -3..=3 {
                        let c = [cx, 0, cz];
                        if intersects(a, b, [0.; 3], c) {
                            assert!(intersects(start, end, error, c));
                        }
                    }
                }
            }
        }
        assert!(intersects([-1., 0.5, 0.5], [0., 0.5, 0.5], [0.; 3], cell));
        assert!(intersects([0., 1., 0.5], [1., 1., 0.5], [0.; 3], cell));
        assert!(!intersects(
            [0., 1.001, 0.5],
            [1., 1.001, 0.5],
            [0.; 3],
            cell
        ));
    }
}
