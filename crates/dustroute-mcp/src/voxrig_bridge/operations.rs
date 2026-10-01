//! Native transport operations. Submission and client observation retain their
//! own meanings; this module never constructs server-readback evidence.
mod physical;
mod recording;
use super::{VoxrigBridge, native_error};
use crate::bridge::{
    BotBridgeError, BotStatus, LeverActivation, LeverApproach, PlayerObservation,
    TargetingGeometry, Vec3, VisiblePlayer, is_valid_minecraft_username,
};
use crate::bridge_protocol::{COMMAND_LIMIT, CommandSubmission, CommandWrite, MutationProtocol};
use dustroute_physical::Pos;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use voxrig::versions::java_1_21_11::operations::{Operations, PlayerState};
use voxrig::{BlockFace, NativeBlockState, Region};

fn fail(message: impl Into<String>) -> BotBridgeError {
    BotBridgeError::Protocol(message.into())
}
fn array(p: Pos) -> [i32; 3] {
    [p.x, p.y, p.z]
}
fn vector(p: [f64; 3]) -> Vec3 {
    Vec3 {
        x: p[0],
        y: p[1],
        z: p[2],
    }
}
fn squared(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum()
}
fn needs_player_space(bot: [f64; 3], player: [f64; 3]) -> bool {
    (bot[0] - player[0]).powi(2) + (bot[2] - player[2]).powi(2) < 9.0
        && (bot[1] - player[1]).abs() < 3.0
}
fn player_approach_commands(player: &str) -> [String; 3] {
    // Ignore pitch: looking up/down must not turn the horizontal offset vertical.
    // Center the bot in an empty column so its body fits within the checked cells.
    [[0, 2, -4], [4, 2, 0], [-4, 2, 0]].map(|[left, up, forward]| {
        format!(
            "execute at {player} rotated as {player} rotated ~ 0 positioned ^{left} ^{up} ^{forward} align xyz positioned ~0.5 ~ ~0.5 if block ~ ~ ~ minecraft:air if block ~ ~1 ~ minecraft:air run tp @s ~ ~ ~"
        )
    })
}
fn valid_position(pos: Pos) -> Result<(), BotBridgeError> {
    Region {
        min: array(pos),
        max: array(pos),
    }
    .volume()
    .map_err(native_error)?;
    Ok(())
}
impl VoxrigBridge {
    async fn acquire_player(
        &self,
        player: &str,
        operations: &Operations,
    ) -> Result<bool, BotBridgeError> {
        if !is_valid_minecraft_username(player) || player == self.username {
            return Err(fail("invalid player"));
        }
        let _guard = self.mutations.lock().await;
        let before = operations.player_state().await.map_err(native_error)?;
        let observed = operations.visible_players().await.map_err(native_error)?;
        let existing = observed.players.iter().find(|p| p.name == player);
        let origin = before
            .position
            .ok_or_else(|| fail("bot position unavailable"))?;
        if existing.is_some_and(|p| !needs_player_space(origin, p.position)) {
            return Ok(false);
        }
        let reacquired = existing.is_none();
        let dimension = before
            .dimension
            .ok_or_else(|| fail("bot dimension unavailable"))?;
        operations.set_flying(true).await.map_err(native_error)?;
        let offsets = player_approach_commands(player).map(|command| (command, false));
        // Prefer space, but allow overlap as the last resort so cramped builds
        // do not prevent work. Keep all four attempts inside the bridge timeout.
        for (command, allow_overlap) in offsets
            .into_iter()
            .chain(std::iter::once((format!("tp @s {player}"), true)))
        {
            operations
                .send_command(&command)
                .await
                .map_err(native_error)?;
            let deadline = Instant::now() + Duration::from_secs(if allow_overlap { 3 } else { 2 });
            loop {
                let after = self.require_dimension(&dimension).await?;
                if after.receive_sequence > before.receive_sequence
                    && after.position_from_server
                    && operations
                        .visible_players()
                        .await
                        .map_err(native_error)?
                        .players
                        .iter()
                        .any(|p| {
                            p.name == player
                                && after.position.is_some_and(|position| {
                                    if allow_overlap {
                                        squared(position, p.position) <= 1.0
                                    } else {
                                        squared(position, origin) > 0.0001
                                            && !needs_player_space(position, p.position)
                                            && squared(position, p.position) <= 64.0
                                    }
                                })
                        })
                {
                    return Ok(reacquired);
                }
                if Instant::now() >= deadline {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        Err(fail(format!(
            "player position could not be acquired even with overlap allowed: {player}; player may be offline or teleport permission unavailable"
        )))
    }
    pub async fn observe_player_context(
        &self,
        player: &str,
    ) -> Result<crate::bridge::PlayerContext, BotBridgeError> {
        let operations = self.operations()?;
        let reacquired = self.acquire_player(player, &operations).await?;
        let state = operations.player_state().await.map_err(native_error)?;
        let observed = operations.visible_players().await.map_err(native_error)?;
        if state.connection_id != observed.connection_id
            || state.dimension.as_deref() != Some(observed.dimension.as_str())
            || !observed.players.iter().any(|p| p.name == player)
        {
            return Err(fail("player context changed or unavailable"));
        }
        Ok(crate::bridge::PlayerContext {
            player: player.into(),
            dimension: observed.dimension,
            reacquired,
        })
    }

    pub async fn observe_player(
        &self,
        player: &str,
        max_distance: f64,
    ) -> Result<PlayerObservation, BotBridgeError> {
        if !is_valid_minecraft_username(player)
            || !max_distance.is_finite()
            || !(0.0..=64.0).contains(&max_distance)
            || max_distance == 0.0
        {
            return Err(fail("invalid player or target distance"));
        }
        let operations = self.operations()?;
        let reacquired = self.acquire_player(player, &operations).await?;
        let target = operations
            .observe_player_outline_target(player, max_distance)
            .await
            .map_err(native_error)?;
        use voxrig::versions::java_1_21_11::reconstruction::Direction;
        let face = target.hit.as_ref().and_then(|h| h.face).map(|d| {
            match d {
                Direction::Down => "down",
                Direction::Up => "up",
                Direction::North => "north",
                Direction::South => "south",
                Direction::West => "west",
                Direction::East => "east",
            }
            .to_owned()
        });
        Ok(PlayerObservation {
            player: target.player.name,
            eye_position: vector(
                target
                    .player
                    .eye_position
                    .ok_or_else(|| fail("player eye unavailable"))?,
            ),
            yaw: std::f64::consts::PI - f64::from(target.player.rotation[0]).to_radians(),
            pitch: -f64::from(target.player.rotation[1]).to_radians(),
            targeted_block: target.hit.as_ref().map(|h| super::pos(h.position)),
            targeted_face: face,
            distance: target.hit.as_ref().map(|h| h.distance),
            dimension: target.dimension,
            reacquired,
            targeting_geometry: Some(TargetingGeometry::BlockOutline),
            connection_id: Some(target.connection_id),
            receive_sequence: Some(target.receive_sequence),
        })
    }
    pub async fn preview_region(
        &self,
        player: &str,
        min: Pos,
        max: Pos,
        dimension: &str,
    ) -> Result<Value, BotBridgeError> {
        if !is_valid_minecraft_username(player) {
            return Err(fail("invalid preview player"));
        }
        Region {
            min: array(min),
            max: array(max),
        }
        .volume()
        .map_err(native_error)?;
        let _guard = self.mutations.lock().await;
        self.require_dimension(dimension).await?;
        let operations = self.operations()?;
        for x in [min.x, max.x + 1] {
            for y in [min.y, max.y + 1] {
                for z in [min.z, max.z + 1] {
                    operations.send_command(&format!("particle minecraft:end_rod {x} {y} {z} 0.15 0.15 0.15 0.01 12 force {player}")).await.map_err(native_error)?;
                }
            }
        }
        Ok(json!({"min":min,"max":max,"particle_corners":8,"submission_only":true}))
    }
    fn operations(&self) -> Result<Operations, BotBridgeError> {
        self.client.java_1_21_11_operations().map_err(native_error)
    }
    async fn require_dimension(&self, dimension: &str) -> Result<PlayerState, BotBridgeError> {
        let state = self
            .operations()?
            .player_state()
            .await
            .map_err(native_error)?;
        if state.dimension.as_deref() != Some(dimension) {
            return Err(fail("Voxrig dimension changed or unavailable"));
        }
        Ok(state)
    }
    pub async fn status(&self) -> Result<BotStatus, BotBridgeError> {
        let state = self
            .operations()?
            .player_state()
            .await
            .map_err(native_error)?;
        Ok(BotStatus {
            connected: state.position.is_some() && state.dimension.is_some(),
            username: self.username.clone(),
            host: self.host.clone(),
            port: self.port,
            version: "1.21.11".into(),
            dimension: state.dimension,
            enabled_features: state.enabled_features,
            metrics: Default::default(),
        })
    }
    pub async fn visible_players(&self) -> Result<Vec<VisiblePlayer>, BotBridgeError> {
        let operations = self.operations()?;
        let state = operations.player_state().await.map_err(native_error)?;
        let origin = state
            .position
            .ok_or_else(|| fail("bot position unavailable"))?;
        let players = operations.visible_players().await.map_err(native_error)?;
        if state.connection_id != players.connection_id
            || state.dimension.as_deref() != Some(&players.dimension)
        {
            return Err(fail(
                "connection or dimension changed during player observation",
            ));
        }
        Ok(players
            .players
            .into_iter()
            .map(|player| VisiblePlayer {
                player: player.name,
                position: vector(player.position),
                distance_from_bot: squared(origin, player.position).sqrt(),
                dimension: players.dimension.clone(),
            })
            .collect())
    }
    pub async fn wait_ticks(&self, ticks: u16, dimension: &str) -> Result<Value, BotBridgeError> {
        if !(1..=200).contains(&ticks) {
            return Err(fail("ticks must be 1..200"));
        }
        let before = self.require_dimension(dimension).await?;
        tokio::time::sleep(Duration::from_millis(u64::from(ticks) * 50)).await;
        let after = self.require_dimension(dimension).await?;
        if before.connection_id != after.connection_id {
            return Err(fail("connection changed during wait"));
        }
        Ok(
            json!({"waited_ticks":ticks,"clock":"client_wall_time_20hz","receive_sequence":after.receive_sequence,"server_time_sample":after.server_time}),
        )
    }
    pub async fn write_blocks(
        &self,
        changes: &[CommandWrite],
        dimension: &str,
    ) -> Result<CommandSubmission, BotBridgeError> {
        self.write_blocks_tracked(changes, dimension, &mut Default::default())
            .await
    }
    pub(crate) async fn write_blocks_tracked(
        &self,
        changes: &[CommandWrite],
        dimension: &str,
        progress: &mut crate::failure::SubmissionProgress,
    ) -> Result<CommandSubmission, BotBridgeError> {
        if changes.len() > COMMAND_LIMIT {
            return Err(fail("command write limit exceeded"));
        }
        for change in changes {
            valid_position(change.pos)?;
        }
        let _guard = self.mutations.lock().await;
        self.require_dimension(dimension).await?;
        let operations = self.operations()?;
        for (index, change) in changes.iter().enumerate() {
            self.require_dimension(dimension).await?;
            progress.may_have_changed_world = true;
            operations
                .send_command(&format!(
                    "setblock {} {} {} {} replace",
                    change.pos.x, change.pos.y, change.pos.z, change.state
                ))
                .await
                .map_err(native_error)?;
            progress.submitted_changes = index + 1;
            if (index + 1) % 64 == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        self.wait_ticks(2, dimension).await?;
        Ok(CommandSubmission {
            protocol: MutationProtocol::V1,
            submitted_changes: changes.len(),
        })
    }
    async fn block(&self, pos: Pos, dimension: &str) -> Result<NativeBlockState, BotBridgeError> {
        let region = self.observe_region(pos, pos, dimension).await?;
        if region.is_moving() {
            return Err(fail("interaction target is moving"));
        }
        region.observation().blocks[0]
            .state
            .clone()
            .ok_or_else(|| fail("interaction target unavailable"))
    }
    async fn teleport(&self, destination: [f64; 3], dimension: &str) -> Result<(), BotBridgeError> {
        if destination
            .iter()
            .any(|n| !n.is_finite() || n.abs() > 30_000_000.0)
        {
            return Err(fail("invalid teleport destination"));
        }
        let before = self.require_dimension(dimension).await?;
        let operations = self.operations()?;
        operations.set_flying(true).await.map_err(native_error)?;
        operations
            .send_command(&format!(
                "tp @s {} {} {}",
                destination[0], destination[1], destination[2]
            ))
            .await
            .map_err(native_error)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let after = self.require_dimension(dimension).await?;
            if after.receive_sequence > before.receive_sequence
                && after.position_from_server
                && after
                    .position
                    .is_some_and(|p| squared(p, destination) < 0.0001)
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(fail(
                    "teleport position was not received; permission or server acceptance unavailable",
                ));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    async fn ensure_lever(
        &self,
        pos: Pos,
        dimension: &str,
    ) -> Result<LeverApproach, BotBridgeError> {
        valid_position(pos)?;
        if self.block(pos, dimension).await?.name != "minecraft:lever" {
            return Err(fail("target is not a lever"));
        }
        let state = self.require_dimension(dimension).await?;
        let mut feet = state
            .position
            .ok_or_else(|| fail("bot position unavailable"))?;
        let center = [
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        ];
        let mut moved = false;
        if squared([feet[0], feet[1] + 1.62, feet[2]], center) > 16.0 {
            let mut destination = None;
            for [dx, dz] in [[0, -2], [2, 0], [0, 2], [-2, 0]] {
                let p = pos.offset(dx, 0, dz);
                let region = self.observe_region(p, p.offset(0, 1, 0), dimension).await?;
                if region.snapshot().blocks.iter().all(|b| {
                    matches!(
                        b.name.as_str(),
                        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
                    )
                }) {
                    destination =
                        Some([f64::from(p.x) + 0.5, f64::from(p.y), f64::from(p.z) + 0.5]);
                    break;
                }
            }
            feet =
                destination.ok_or_else(|| fail("no observed air position within lever reach"))?;
            self.teleport(feet, dimension).await?;
            if self.block(pos, dimension).await?.name != "minecraft:lever" {
                return Err(fail("lever changed during approach"));
            }
            moved = true;
        }
        Ok(LeverApproach {
            pos,
            moved,
            distance: squared(feet, center).sqrt(),
        })
    }
    pub async fn approach_lever(
        &self,
        pos: Pos,
        dimension: &str,
    ) -> Result<LeverApproach, BotBridgeError> {
        let _guard = self.mutations.lock().await;
        self.ensure_lever(pos, dimension).await
    }
    pub async fn activate_lever(
        &self,
        pos: Pos,
        dimension: &str,
    ) -> Result<LeverActivation, BotBridgeError> {
        let _guard = self.mutations.lock().await;
        let approach = self.ensure_lever(pos, dimension).await?;
        let before = self.block(pos, dimension).await?;
        let powered = match before.properties.get("powered").map(String::as_str) {
            Some("true") => true,
            Some("false") => false,
            _ => return Err(fail("lever powered state unavailable")),
        };
        self.operations()?
            .use_on_block(array(pos), BlockFace::Up, [0.5, 1.0, 0.5])
            .await
            .map_err(native_error)?;
        for _ in 0..20 {
            self.wait_ticks(1, dimension).await?;
            let after = self.block(pos, dimension).await?;
            if after.name != "minecraft:lever" {
                return Err(fail("lever changed during activation"));
            }
            if after
                .properties
                .get("powered")
                .is_some_and(|p| p == if powered { "false" } else { "true" })
            {
                return Ok(LeverActivation {
                    pos,
                    before_powered: powered,
                    after_powered: !powered,
                    bot_approached: approach.moved,
                });
            }
        }
        Err(fail(
            "lever toggle not observed; inspect state before retry",
        ))
    }
}
