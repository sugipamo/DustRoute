//! Player block targeting with an explicit collision or outline geometry policy.
mod outline;
use super::{State, operations::Operations, players::ObservedPlayer};
use crate::{Error, ErrorKind, NativeBlockState, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::OnceLock};

/// A static block hit. The enclosing observation declares the geometry policy.
#[derive(Clone, Debug, Serialize)]
pub struct BlockHit {
    /// Cell owning the shape, which may protrude outside that cell.
    pub position: [i32; 3],
    /// Reconstructed native state at the observation boundary.
    pub state: NativeBlockState,
    /// Distance from the received player's eye.
    pub distance: f64,
    /// Entry face; None when the ray starts within a collision box.
    pub face: Option<super::Direction>,
}
/// Compatibility name for callers of the collision-only query.
pub type CollisionHit = BlockHit;
/// Player pose and world geometry read under one connection lock.
#[derive(Clone, Debug, Serialize)]
pub struct PlayerTarget {
    /// Connection identity.
    pub connection_id: u64,
    /// Packet receive boundary.
    pub receive_sequence: u64,
    /// Local reconstruction frame; not server time.
    pub client_tick: u64,
    /// Observed dimension.
    pub dimension: String,
    /// Latest received player position/pose.
    pub player: ObservedPlayer,
    /// Selection geometry. Client state is not a server or rendered-frame receipt.
    pub geometry: TargetGeometry,
    /// None only after a complete, available ray found no target.
    pub hit: Option<BlockHit>,
}
/// The geometry used to determine a target.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetGeometry {
    /// Pinned native state collision boxes. Fluids/empty collision shapes are skipped.
    BlockCollision,
    /// Audited native static outlines, including non-collidable circuit parts.
    /// Fluids and entities are not selected. Unsupported shapes reject the query.
    BlockOutline,
}

/// Same pinned native outline traversal, for a prevalidated stationary own player.
pub(super) fn stationary_outline_hit(
    state: &State,
    eye: [f64; 3],
    distance: f64,
) -> Result<Option<BlockHit>> {
    let direction = outline::direction(state.rotation);
    let end = std::array::from_fn(|i| eye[i] + direction[i] * distance);
    outline::cast(eye, end, |p| {
        let cell = state.reconstruction.cell(&state.world, p);
        if cell.moving.is_some() {
            anyhow::bail!("moving own-player target geometry at {p:?}");
        }
        cell.state
            .ok_or_else(|| anyhow::anyhow!("own-player target geometry unavailable at {p:?}"))
    })
    .map_err(|error| Error::new(ErrorKind::State, error))
}

impl Operations {
    /// Query a remote player's static collision target, preserving player/world
    /// provenance. Missing chunks, unsupported pose or a moving carrier reject
    /// the query; they are never treated as empty space.
    pub async fn observe_player_target(
        &self,
        name: &str,
        max_distance: f64,
    ) -> Result<PlayerTarget> {
        self.observe_target(name, max_distance, TargetGeometry::BlockCollision)
            .await
    }
    /// Query native static block outline selection, including dust, switches and
    /// gates. Uses received player pose and reconstructed blocks under one lock.
    /// Unsupported context-dependent shapes, unavailable cells and moving carriers
    /// reject the query. Fluids/entities and graphical interpolation are excluded.
    pub async fn observe_player_outline_target(
        &self,
        name: &str,
        max_distance: f64,
    ) -> Result<PlayerTarget> {
        self.observe_target(name, max_distance, TargetGeometry::BlockOutline)
            .await
    }
    async fn observe_target(
        &self,
        name: &str,
        max_distance: f64,
        geometry: TargetGeometry,
    ) -> Result<PlayerTarget> {
        if !max_distance.is_finite() || !(0.0..=64.0).contains(&max_distance) || max_distance == 0.0
        {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                anyhow::anyhow!("target distance must be greater than zero and at most 64"),
            ));
        }
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        let player = state
            .players
            .observations()
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::State,
                    anyhow::anyhow!("player is not visible to the bot: {name}"),
                )
            })?;
        let origin = player.eye_position.ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                anyhow::anyhow!("player eye position unavailable for the received pose/metadata"),
            )
        })?;
        let target = self.bot.session.started.elapsed().as_millis() as u64 / 50;
        let State {
            world,
            reconstruction,
            ..
        } = &mut *state;
        reconstruction.advance(world, target);
        if reconstruction.issue.is_some() || !reconstruction.recovery_chunks.is_empty() {
            return Err(Error::new(
                ErrorKind::State,
                anyhow::anyhow!("client reconstruction incomplete"),
            ));
        }
        let yaw = f64::from(player.rotation[0]).to_radians();
        let pitch = f64::from(player.rotation[1]).to_radians();
        let direction = [
            -yaw.sin() * pitch.cos(),
            -pitch.sin(),
            yaw.cos() * pitch.cos(),
        ];
        let (dimension, height) = world.dimension.as_ref().expect("ready dimension");
        let read = |p: [i32; 3]| {
            if p[1] < height.min_y || p[1] >= height.min_y + height.height {
                return super::super::native_state(0).map_err(anyhow::Error::from);
            }
            let block = reconstruction.cell(world, p);
            if block.moving.is_some() {
                anyhow::bail!("moving target geometry unavailable at {p:?}");
            }
            block
                .state
                .ok_or_else(|| anyhow::anyhow!("target geometry unavailable at {p:?}"))
        };
        let hit = match geometry {
            TargetGeometry::BlockCollision => cast(origin, direction, max_distance, read),
            TargetGeometry::BlockOutline => {
                let direction = outline::direction(player.rotation);
                let end = std::array::from_fn(|i| origin[i] + direction[i] * max_distance);
                outline::cast(origin, end, read)
            }
        }
        .map_err(|error| Error::new(ErrorKind::State, error))?;
        let dimension = dimension.clone();
        Ok(PlayerTarget {
            connection_id: self.bot.session.id,
            receive_sequence: state.sequence,
            client_tick: state.reconstruction.tick,
            dimension,
            player,
            geometry,
            hit,
        })
    }
}

#[derive(Deserialize)]
struct Shapes {
    state_shapes: Vec<usize>,
    shapes: Vec<Vec<[f64; 6]>>,
}
fn shapes() -> &'static Shapes {
    static SHAPES: OnceLock<Shapes> = OnceLock::new();
    SHAPES.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../data/java_1_21_11/collision_shapes.json"
        ))
        .expect("validated generated collision data")
    })
}
fn boxes(state: &NativeBlockState) -> anyhow::Result<&'static [[f64; 6]]> {
    let id = super::super::state_id(state)? as usize;
    let shapes = shapes();
    Ok(&shapes.shapes[shapes.state_shapes[id]])
}
fn intersection(
    origin: [f64; 3],
    direction: [f64; 3],
    bounds: [f64; 6],
    limit: f64,
) -> Option<(f64, Option<super::Direction>)> {
    let mut enter = 0.0f64;
    let mut exit = limit;
    let mut face = None;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-15 {
            if origin[axis] < bounds[axis] || origin[axis] > bounds[axis + 3] {
                return None;
            }
            continue;
        }
        let a = (bounds[axis] - origin[axis]) / direction[axis];
        let b = (bounds[axis + 3] - origin[axis]) / direction[axis];
        let near = a.min(b);
        if near > enter {
            enter = near;
            use super::Direction::*;
            face = Some(if a < b {
                [West, Down, North][axis]
            } else {
                [East, Up, South][axis]
            });
        }
        exit = exit.min(a.max(b));
        if enter > exit {
            return None;
        }
    }
    Some((enter, face))
}
fn cast(
    mut origin: [f64; 3],
    direction: [f64; 3],
    limit: f64,
    mut read: impl FnMut([i32; 3]) -> anyhow::Result<NativeBlockState>,
) -> anyhow::Result<Option<CollisionHit>> {
    // Normalize signed zero for deterministic boundary handling.
    for v in &mut origin {
        if *v == 0.0 {
            *v = 0.0;
        }
    }
    let mut cell = origin.map(|v| v.floor() as i32);
    let step = direction.map(|v| {
        if v > 0.0 {
            1
        } else if v < 0.0 {
            -1
        } else {
            0
        }
    });
    let delta = direction.map(|v| {
        if v.abs() < 1e-15 {
            f64::INFINITY
        } else {
            1.0 / v.abs()
        }
    });
    let mut boundary = std::array::from_fn::<_, 3, _>(|axis| {
        if delta[axis].is_infinite() {
            f64::INFINITY
        } else {
            ((f64::from(cell[axis]) + if step[axis] > 0 { 1.0 } else { 0.0 }) - origin[axis])
                / direction[axis]
        }
    });
    let mut candidates = BTreeSet::new();
    loop {
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    candidates.insert([cell[0] + dx, cell[1] + dy, cell[2] + dz]);
                }
            }
        }
        let axis = (0..3)
            .min_by(|&a, &b| boundary[a].total_cmp(&boundary[b]))
            .unwrap();
        if boundary[axis] > limit {
            break;
        }
        cell[axis] += step[axis];
        boundary[axis] += delta[axis];
        if candidates.len() > 8192 {
            anyhow::bail!("ray candidate limit exceeded");
        }
    }
    // Generated shape bounds lie in [-0.25,1.5]. Order candidates by their
    // earliest possible hit, so unloaded space beyond a nearer hit is irrelevant.
    let mut candidates: Vec<_> = candidates
        .into_iter()
        .filter_map(|p| {
            let bounds = std::array::from_fn(|axis| {
                f64::from(p[axis % 3]) + if axis < 3 { -0.25 } else { 1.5 }
            });
            intersection(origin, direction, bounds, limit).map(|(distance, _)| (distance, p))
        })
        .collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut hit: Option<CollisionHit> = None;
    for (earliest, p) in candidates {
        let best = hit.as_ref().map_or(limit, |h| h.distance);
        if earliest > best {
            break;
        }
        let state = read(p)?;
        if state.name == "minecraft:moving_piston" {
            anyhow::bail!("moving geometry is not static");
        }
        for shape in boxes(&state)? {
            let bounds = std::array::from_fn(|axis| f64::from(p[axis % 3]) + shape[axis]);
            if let Some((distance, face)) = intersection(origin, direction, bounds, best)
                .filter(|(distance, _)| hit.as_ref().is_none_or(|h| *distance < h.distance))
            {
                hit = Some(CollisionHit {
                    position: p,
                    state: state.clone(),
                    distance,
                    face,
                });
            }
        }
    }
    Ok(hit)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn native(name: &str, properties: &[(&str, &str)]) -> NativeBlockState {
        NativeBlockState {
            name: format!("minecraft:{name}"),
            properties: properties
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
    #[test]
    fn ray_uses_partial_shapes_and_protruding_collision_boxes() {
        let slab = native(
            "smooth_stone_slab",
            &[("type", "bottom"), ("waterlogged", "false")],
        );
        let stone = native("stone", &[]);
        let read = |p| {
            Ok(if p == [0, 0, 1] {
                slab.clone()
            } else if p == [0, 0, 3] {
                stone.clone()
            } else {
                native("air", &[])
            })
        };
        let hit = cast([0.5, 0.75, 0.5], [0.0, 0.0, 1.0], 8.0, read)
            .unwrap()
            .unwrap();
        assert_eq!(hit.position, [0, 0, 3]);
        assert_eq!(hit.distance, 2.5);
        assert!(matches!(hit.face, Some(super::super::Direction::North)));
        let fence = native(
            "oak_fence",
            &[
                ("east", "false"),
                ("north", "false"),
                ("south", "false"),
                ("west", "false"),
                ("waterlogged", "false"),
            ],
        );
        let hit = cast([0.5, 1.25, 0.5], [0.0, 0.0, 1.0], 8.0, |p| {
            Ok(if p == [0, 0, 2] {
                fence.clone()
            } else {
                native("air", &[])
            })
        })
        .unwrap()
        .unwrap();
        assert_eq!(hit.position, [0, 0, 2]);
    }
    #[test]
    fn unknown_cells_before_a_hit_fail_but_distant_unknown_space_does_not() {
        let read = |p: [i32; 3]| {
            if p[2] >= 7 {
                anyhow::bail!("unloaded");
            }
            Ok(if p == [0, 0, 2] {
                native("stone", &[])
            } else {
                native("air", &[])
            })
        };
        assert_eq!(
            cast([0.5, 0.5, 0.5], [0.0, 0.0, 1.0], 8.0, read)
                .unwrap()
                .unwrap()
                .position,
            [0, 0, 2]
        );
        assert!(
            cast([0.5, 0.5, 0.5], [0.0, 0.0, 1.0], 8.0, |_| anyhow::bail!(
                "unloaded"
            ))
            .is_err()
        );
        assert!(
            cast([0.5, 0.5, 0.5], [0.0, 0.0, 1.0], 8.0, |_| Ok(native(
                "air",
                &[]
            )))
            .unwrap()
            .is_none()
        );
    }
}
