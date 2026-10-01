//! Creative player placement. Each mutation is observed before the next one;
//! a failed operation may already have changed the world and must be inspected.
use super::*;
use crate::bridge_protocol::{
    PHYSICAL_LIMIT, PhysicalChange, PhysicalPlacementMode, PhysicalSubmission,
};
use voxrig::versions::java_1_21_11::operations::GameMode;

fn air(state: &NativeBlockState) -> bool {
    matches!(
        state.name.as_str(),
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
    )
}
fn face_vector(face: Pos) -> Result<(BlockFace, [f32; 3]), BotBridgeError> {
    Ok(match array(face) {
        [0, -1, 0] => (BlockFace::Down, [0.5, 0.0, 0.5]),
        [0, 1, 0] => (BlockFace::Up, [0.5, 1.0, 0.5]),
        [0, 0, -1] => (BlockFace::North, [0.5, 0.5, 0.0]),
        [0, 0, 1] => (BlockFace::South, [0.5, 0.5, 1.0]),
        [-1, 0, 0] => (BlockFace::West, [0.0, 0.5, 0.5]),
        [1, 0, 0] => (BlockFace::East, [1.0, 0.5, 0.5]),
        _ => return Err(fail("physical reference face must be one unit axis")),
    })
}
fn validate(changes: &[PhysicalChange]) -> Result<(), BotBridgeError> {
    if changes.is_empty() || changes.len() > PHYSICAL_LIMIT {
        return Err(fail("physical write limit exceeded"));
    }
    for change in changes {
        match change {
            PhysicalChange::Dig { pos } => valid_position(*pos)?,
            PhysicalChange::Place {
                pos,
                item,
                reference,
                face,
                ..
            } => {
                valid_position(*pos)?;
                valid_position(*reference)?;
                face_vector(*face)?;
                if array(*reference)
                    .into_iter()
                    .zip(array(*face))
                    .zip(array(*pos))
                    .any(|((r, f), p)| r + f != p)
                {
                    return Err(fail(
                        "placement target must adjoin the given reference face",
                    ));
                }
                let name = item.strip_prefix("minecraft:").unwrap_or(item);
                voxrig::versions::java_1_21_11::operations::default_item(item, 1)
                    .map_err(native_error)?;
                if name.is_empty()
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    return Err(fail("invalid placement item"));
                }
            }
        }
    }
    Ok(())
}

impl VoxrigBridge {
    async fn wait_block(
        &self,
        pos: Pos,
        dimension: &str,
        predicate: impl Fn(&NativeBlockState) -> bool,
    ) -> Result<NativeBlockState, BotBridgeError> {
        for _ in 0..30 {
            self.wait_ticks(1, dimension).await?;
            let state = self.block(pos, dimension).await?;
            if predicate(&state) {
                return Ok(state);
            }
        }
        Err(fail(
            "physical operation result not observed; inspect live state before retry",
        ))
    }
    pub async fn place_physical_blocks(
        &self,
        changes: &[PhysicalChange],
        dimension: &str,
    ) -> Result<PhysicalSubmission, BotBridgeError> {
        self.place_physical_blocks_tracked(changes, dimension, &mut Default::default())
            .await
    }
    pub(crate) async fn place_physical_blocks_tracked(
        &self,
        changes: &[PhysicalChange],
        dimension: &str,
        progress: &mut crate::failure::SubmissionProgress,
    ) -> Result<PhysicalSubmission, BotBridgeError> {
        validate(changes)?;
        let _guard = self.mutations.lock().await;
        let operations = self.operations()?;
        if self.require_dimension(dimension).await?.game_mode != Some(GameMode::Creative) {
            operations
                .send_command("gamemode creative @s")
                .await
                .map_err(native_error)?;
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if self.require_dimension(dimension).await?.game_mode == Some(GameMode::Creative) {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(fail("creative mode permission was not received"));
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        let mut highest = i32::MIN;
        let mut total = [0.0; 2];
        for (index, change) in changes.iter().enumerate() {
            let pos = match change {
                PhysicalChange::Dig { pos } | PhysicalChange::Place { pos, .. } => *pos,
            };
            highest = highest.max(pos.y);
            total[0] += f64::from(pos.x);
            total[1] += f64::from(pos.z);
            // Creative construction only; the received teleport is required before
            // interaction. This does not promise a survival path to this position.
            self.teleport(
                [
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 2.0,
                    f64::from(pos.z) + 0.5,
                ],
                dimension,
            )
            .await?;
            if !air(&self.block(pos, dimension).await?) {
                progress.may_have_changed_world = true;
                operations
                    .dig_creative(array(pos), BlockFace::Up)
                    .await
                    .map_err(native_error)?;
                self.wait_block(pos, dimension, air).await?;
            }
            if let PhysicalChange::Place {
                item,
                state,
                reference,
                face,
                ..
            } = change
            {
                if air(&self.block(*reference, dimension).await?) {
                    return Err(fail("placement reference is air"));
                }
                let item = if item.starts_with("minecraft:") {
                    item.clone()
                } else {
                    format!("minecraft:{item}")
                };
                operations
                    .set_creative_hotbar(0, Some((&item, 1)))
                    .await
                    .map_err(native_error)?;
                operations.select_hotbar(0).await.map_err(native_error)?;
                let facing = state.properties().get("facing").map(String::as_str);
                // Native piston/repeater placement faces opposite the view. Other
                // placement rules are verified after the one submitted interaction.
                let rotation = match facing {
                    Some("north") => [0.0, 0.0],
                    Some("south") => [180.0, 0.0],
                    Some("east") => [90.0, 0.0],
                    Some("west") => [-90.0, 0.0],
                    Some("up") => [0.0, 90.0],
                    Some("down") => [0.0, -90.0],
                    _ => [0.0, 90.0],
                };
                operations.look(rotation).await.map_err(native_error)?;
                let (side, cursor) = face_vector(*face)?;
                progress.may_have_changed_world = true;
                operations
                    .use_on_block(array(*reference), side, cursor)
                    .await
                    .map_err(native_error)?;
                let placed = self.wait_block(pos, dimension, |s| !air(s)).await?;
                if placed.name != state.name()
                    || facing.is_some_and(|f| {
                        placed.properties.get("facing").map(String::as_str) != Some(f)
                    })
                {
                    return Err(fail(
                        "physical placement created a different block/orientation; inspect live state before retry",
                    ));
                }
            }
            progress.submitted_changes = index + 1;
        }
        let n = changes.len() as f64;
        let retreat = [
            (total[0] / n).floor() + 0.5,
            f64::from(highest) + 16.0,
            (total[1] / n).floor() + 0.5,
        ];
        self.teleport(retreat, dimension).await?;
        Ok(PhysicalSubmission {
            protocol: MutationProtocol::V1,
            placed_changes: changes.len(),
            placement_mode: PhysicalPlacementMode::VoxrigCreativePlayer,
            retreat: vector(retreat),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_physical_batches_reject_before_world_mutations() {
        let valid = PhysicalChange::Place {
            pos: Pos::new(0, 81, 0),
            reference: Pos::new(0, 80, 0),
            face: Pos::new(0, 1, 0),
            item: "minecraft:stone".into(),
            state: "minecraft:stone".parse().unwrap(),
        };
        assert!(validate(std::slice::from_ref(&valid)).is_ok());
        let mut bad = valid.clone();
        if let PhysicalChange::Place { face, .. } = &mut bad {
            *face = Pos::new(0, 2, 0);
        }
        assert!(validate(&[valid.clone(), bad]).is_err());
        let mut bad = valid.clone();
        if let PhysicalChange::Place { pos, .. } = &mut bad {
            *pos = Pos::new(0, 80, 0);
        }
        assert!(validate(&[valid, bad]).is_err());
    }
}
