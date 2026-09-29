//! Java 1.21.11 block outline selection, with no fluid or entity targets.
//! Data and independent native oracle: scripts/ExportOutlineShapes.java.
use super::{BlockHit, NativeBlockState};
use crate::versions::java_1_21_11::{reconstruction::Direction, state_id};
use serde::Deserialize;
use std::sync::OnceLock;

/// Entity.getRotationVector uses float angles and MathHelper's indexed sine table.
pub(super) fn direction([yaw, pitch]: [f32; 2]) -> [f64; 3] {
    fn trig(angle: f32, cosine: bool) -> f32 {
        let index = ((f64::from(angle) * 10430.378350470453 + if cosine { 16384.0 } else { 0.0 })
            as i64)
            & 65535;
        (index as f64 / 10430.378350470453).sin() as f32
    }
    let pitch = pitch * (std::f32::consts::PI / 180.0);
    let yaw = -yaw * (std::f32::consts::PI / 180.0);
    [
        f64::from(trig(yaw, false) * trig(pitch, true)),
        f64::from(-trig(pitch, false)),
        f64::from(trig(yaw, true) * trig(pitch, true)),
    ]
}

#[derive(Deserialize)]
struct Shapes {
    state_shapes: Vec<Option<[usize; 2]>>,
    shapes: Vec<Vec<[f64; 6]>>,
    #[cfg(test)]
    registry_fnv64: String,
}
fn shapes() -> &'static Shapes {
    static SHAPES: OnceLock<Shapes> = OnceLock::new();
    SHAPES.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../../data/java_1_21_11/outline_shapes.json"
        ))
        .expect("validated native outline data")
    })
}

/// Native DDA visits cells in order and returns the first cell's hit. Neighboring
/// protrusions are not added to this traversal (unlike the collision query).
pub(super) fn cast(
    start: [f64; 3],
    end: [f64; 3],
    mut read: impl FnMut([i32; 3]) -> anyhow::Result<NativeBlockState>,
) -> anyhow::Result<Option<BlockHit>> {
    if start == end {
        return Ok(None);
    }
    let from: [f64; 3] = std::array::from_fn(|i| start[i] - 1e-7 * (end[i] - start[i]));
    let to: [f64; 3] = std::array::from_fn(|i| end[i] - 1e-7 * (start[i] - end[i]));
    let delta: [f64; 3] = std::array::from_fn(|i| to[i] - from[i]);
    let step = delta.map(|d| {
        if d > 0.0 {
            1
        } else if d < 0.0 {
            -1
        } else {
            0
        }
    });
    let mut cell = from.map(|v| v.floor() as i32);
    let interval: [f64; 3] = std::array::from_fn(|i| {
        if step[i] == 0 {
            f64::MAX
        } else {
            f64::from(step[i]) / delta[i]
        }
    });
    let mut boundary: [f64; 3] = std::array::from_fn(|i| {
        let fraction = from[i] - from[i].floor();
        interval[i]
            * if step[i] > 0 {
                1.0 - fraction
            } else {
                fraction
            }
    });
    for _ in 0..512 {
        let state = read(cell)?;
        let data = shapes();
        let pair = data.state_shapes[state_id(&state)? as usize].ok_or_else(|| {
            anyhow::anyhow!("outline geometry unsupported at {cell:?}: {}", state.name)
        })?;
        if let Some((fraction, mut face)) = intersect(start, end, cell, &data.shapes[pair[0]]) {
            // The auxiliary shape may override the face, never the hit position.
            if let Some((nearer, side)) = intersect(start, end, cell, &data.shapes[pair[1]])
                && nearer < fraction
            {
                face = side;
            }
            let distance = start
                .into_iter()
                .zip(end)
                .map(|(a, b)| (b - a).powi(2))
                .sum::<f64>()
                .sqrt()
                * fraction;
            return Ok(Some(BlockHit {
                position: cell,
                state,
                distance,
                face: Some(face),
            }));
        }
        if boundary.iter().all(|&t| t > 1.0) {
            return Ok(None);
        }
        // Equal crossing times choose Z, then Y, then X, as in BlockView.
        let axis = if boundary[0] < boundary[1] {
            if boundary[0] < boundary[2] { 0 } else { 2 }
        } else if boundary[1] < boundary[2] {
            1
        } else {
            2
        };
        cell[axis] += step[axis];
        boundary[axis] += interval[axis];
    }
    anyhow::bail!("outline ray traversal limit exceeded")
}

fn intersect(
    start: [f64; 3],
    end: [f64; 3],
    cell: [i32; 3],
    boxes: &[[f64; 6]],
) -> Option<(f64, Direction)> {
    use Direction::*;
    let delta: [f64; 3] = std::array::from_fn(|i| end[i] - start[i]);
    if delta.iter().map(|d| d * d).sum::<f64>() < 1e-7 || boxes.is_empty() {
        return None;
    }
    let local: [f64; 3] = std::array::from_fn(|i| start[i] - f64::from(cell[i]));
    let inside: [f64; 3] =
        std::array::from_fn(|i| start[i] + delta[i] * 0.001 - f64::from(cell[i]));
    if boxes
        .iter()
        .any(|b| (0..3).all(|i| inside[i] >= b[i] && inside[i] < b[i + 3]))
    {
        // getFacing converts to float and uses enum order for equal dot products.
        let d = delta.map(|v| v as f32);
        let mut best = f32::from_bits(1);
        let mut face = South;
        for (dot, opposite) in [
            (-d[1], Up),
            (d[1], Down),
            (-d[2], South),
            (d[2], North),
            (-d[0], East),
            (d[0], West),
        ] {
            if dot > best {
                best = dot;
                face = opposite;
            }
        }
        return Some((0.001, face));
    }
    let mut best = 1.0;
    let mut face = None;
    for bounds in boxes {
        for axis in 0..3 {
            if delta[axis].abs() <= 1e-7 {
                continue;
            }
            let near = if delta[axis] > 0.0 {
                bounds[axis]
            } else {
                bounds[axis + 3]
            };
            let t = (near - local[axis]) / delta[axis];
            if t > 0.0
                && t < best
                && (0..3).filter(|&i| i != axis).all(|i| {
                    let v = local[i] + t * delta[i];
                    bounds[i] - 1e-7 < v && v < bounds[i + 3] + 1e-7
                })
            {
                best = t;
                face = Some(if delta[axis] > 0.0 {
                    [West, Down, North][axis]
                } else {
                    [East, Up, South][axis]
                });
            }
        }
    }
    face.map(|f| (best, f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_21_11::native_state;
    use std::io::Read;
    #[test]
    fn matches_native_entity_rotation_math() {
        #[derive(Deserialize)]
        struct Rotation {
            rotation: [f32; 2],
            direction: [f64; 3],
        }
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../../../../data/java_1_21_11/outline_rotation_cases.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: Vec<Rotation> = serde_json::from_str(&text).unwrap();
        assert_eq!(cases.len(), 2304);
        for case in cases {
            for (got, want) in direction(case.rotation).into_iter().zip(case.direction) {
                assert!(
                    (got - want).abs() < 1e-15,
                    "{:?}: {got} vs {want}",
                    case.rotation
                );
            }
        }
    }
    #[derive(Deserialize, Debug)]
    struct Cell {
        position: [i32; 3],
        state: i32,
    }
    #[derive(Deserialize, Debug)]
    struct Hit {
        position: [i32; 3],
        point: [f64; 3],
        face: String,
    }
    #[derive(Deserialize, Debug)]
    struct Case {
        cells: Vec<Cell>,
        start: [f64; 3],
        end: [f64; 3],
        hit: Option<Hit>,
    }
    #[test]
    fn matches_independent_native_outline_raycast_cases() {
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../../../../data/java_1_21_11/outline_raycast_cases.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: Vec<Case> = serde_json::from_str(&text).unwrap();
        assert_eq!(cases.len(), 25370);
        for (index, case) in cases.iter().enumerate() {
            let got = cast(case.start, case.end, |p| {
                native_state(
                    case.cells
                        .iter()
                        .find(|c| c.position == p)
                        .map_or(0, |c| c.state),
                )
                .map_err(Into::into)
            })
            .unwrap();
            assert_eq!(got.is_some(), case.hit.is_some(), "case {index}: {case:?}");
            if let (Some(got), Some(want)) = (got, &case.hit) {
                assert_eq!(got.position, want.position, "case {index}: {case:?}");
                assert_eq!(
                    format!("{:?}", got.face.unwrap()).to_lowercase(),
                    want.face,
                    "case {index}: {case:?}"
                );
                let distance = case
                    .start
                    .into_iter()
                    .zip(want.point)
                    .map(|(a, b)| (b - a).powi(2))
                    .sum::<f64>()
                    .sqrt();
                assert!(
                    (got.distance - distance).abs() < 1e-10,
                    "case {index}: {got:?} vs {case:?}"
                );
            }
        }
    }
    #[test]
    fn native_shape_registry_matches_every_state_and_property() {
        let mut hash = 0xcbf29ce484222325u64;
        for id in 0..shapes().state_shapes.len() {
            let state = native_state(id as i32).unwrap();
            let properties = state
                .properties
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",");
            for b in format!("{id}:{}:{properties}\n", state.name).bytes() {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
            }
        }
        assert_eq!(hash.to_string(), shapes().registry_fnv64);
        assert!(native_state(shapes().state_shapes.len() as i32).is_err());
    }
    #[test]
    fn unavailable_geometry_before_a_hit_fails_without_collision_fallback() {
        let unsupported = NativeBlockState {
            name: "minecraft:light".into(),
            properties: [
                ("level".into(), "15".into()),
                ("waterlogged".into(), "false".into()),
            ]
            .into(),
        };
        assert!(
            cast([0.5, 0.5, -1.0], [0.5, 0.5, 3.0], |p| {
                if p == [0, 0, 0] {
                    Ok(unsupported.clone())
                } else {
                    Ok(native_state(1)?)
                }
            })
            .is_ok()
        ); // A known nearer stone occludes the unsupported cell.
        let error = cast([0.5, 0.5, -1.0], [0.5, 0.5, 3.0], |p| {
            if p == [0, 0, 0] {
                Ok(unsupported.clone())
            } else {
                Ok(native_state(0)?)
            }
        })
        .unwrap_err();
        assert!(error.to_string().contains("outline geometry unsupported"));
        assert!(
            cast([0.5, 0.5, 0.5], [0.5, 0.5, 3.0], |_| anyhow::bail!(
                "unloaded"
            ))
            .is_err()
        );
    }
}
