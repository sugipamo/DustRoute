use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dustroute_ir::{EventCause, EventKind, EventSource, TransitionPhase};
use dustroute_physical::Pos;
use dustroute_translate::MinecraftSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[derive(Clone, Debug)]
pub struct BotBridge {
    address: String,
    timeout: Duration,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BotBridgeMetrics {
    #[serde(default)]
    pub requests_total: u64,
    #[serde(default)]
    pub errors_total: u64,
    #[serde(default)]
    pub request_bytes: u64,
    #[serde(default)]
    pub response_bytes: u64,
    #[serde(default)]
    pub total_duration_micros: u64,
    #[serde(default)]
    pub max_duration_micros: u64,
    #[serde(default)]
    pub scan_requests: u64,
    #[serde(default)]
    pub scan_volume_blocks: u64,
    #[serde(default)]
    pub scan_non_air_blocks: u64,
    #[serde(default)]
    pub requests_by_method: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BotStatus {
    pub connected: bool,
    pub username: String,
    pub host: String,
    pub port: u16,
    pub version: String,
    pub dimension: Option<String>,
    /// Server configuration packet, absent until actually observed.
    #[serde(default)]
    pub enabled_features: Option<Vec<String>>,
    #[serde(default)]
    pub metrics: BotBridgeMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerObservation {
    pub player: String,
    pub eye_position: Vec3,
    pub yaw: f64,
    pub pitch: f64,
    pub targeted_block: Option<Pos>,
    pub targeted_face: Option<String>,
    pub distance: Option<f64>,
    pub dimension: String,
    /// True when the bot had to move to the configured player before observing.
    #[serde(default)]
    pub reacquired: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VisiblePlayer {
    pub player: String,
    pub position: Vec3,
    pub distance_from_bot: f64,
    pub dimension: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservedBlockState {
    pub name: String,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservedBlock {
    pub pos: Pos,
    #[serde(flatten)]
    pub state: ObservedBlockState,
}

/// Evidence of command checks in one server game tick, not a world lock or a
/// claim that scheduled work is exhausted. Saved receipts never authorize reuse.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServerReadback {
    pub schema_version: String,
    pub kind: String,
    pub request_id: String,
    pub dimension: String,
    pub min: Pos,
    pub max: Pos,
    pub checked_cells: u64,
    pub start_game_tick: u64,
    pub end_game_tick: u64,
    pub nonce: String,
    pub snapshot_sha256: String,
    pub corrections: Vec<Value>,
    #[serde(default)]
    pub predicate_ticks: Vec<u64>,
    #[serde(default)]
    pub predicate_batch_cells: Option<u64>,
    #[serde(default)]
    pub attempts: Vec<Value>,
    pub hidden_runtime_observed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConfirmedRegion {
    #[serde(flatten)]
    pub snapshot: MinecraftSnapshot,
    pub readback: ServerReadback,
}

impl ConfirmedRegion {
    fn validate(
        &self,
        min: Pos,
        max: Pos,
        dimension: &str,
        request: &str,
    ) -> Result<(), BotBridgeError> {
        let e = &self.readback;
        let volume = [(min.x, max.x), (min.y, max.y), (min.z, max.z)]
            .into_iter()
            .try_fold(1u64, |n, (a, b)| {
                let width = i64::from(b) - i64::from(a) + 1;
                u64::try_from(width)
                    .ok()
                    .filter(|w| *w > 0)
                    .and_then(|w| n.checked_mul(w))
            });
        if self.snapshot.min != min
            || self.snapshot.max != max
            || e.schema_version != "dustroute.server-readback.v1"
            || e.kind != "server_confirmed"
            || e.request_id != request
            || e.dimension != dimension
            || e.min != min
            || e.max != max
            || volume != Some(e.checked_cells)
            || e.checked_cells > 262_144
            || e.start_game_tick != e.end_game_tick
            || (!e.predicate_ticks.is_empty()
                && (!(1..=8880).contains(&e.predicate_batch_cells.unwrap_or(48))
                    || e.predicate_ticks.len() as u64
                        != e.checked_cells
                            .div_ceil(e.predicate_batch_cells.unwrap_or(48).max(1))
                    || e.predicate_ticks
                        .iter()
                        .any(|tick| *tick != e.start_game_tick)))
            || e.hidden_runtime_observed
            || e.nonce.len() != 32
            || !e.nonce.bytes().all(|b| b.is_ascii_hexdigit())
            || e.snapshot_sha256.len() != 64
            || !e.snapshot_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(BotBridgeError::Protocol(
                "incomplete, stale or mismatched server readback evidence".into(),
            ));
        }
        dustroute_translate::snapshot::index_literal_snapshot(&self.snapshot)
            .map_err(BotBridgeError::Protocol)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LeverActivation {
    pub pos: Pos,
    pub before_powered: bool,
    pub after_powered: bool,
    #[serde(default)]
    pub bot_approached: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeverApproach {
    pub pos: Pos,
    pub moved: bool,
    pub distance: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateRecordingStarted {
    pub recording_id: String,
    pub started_game_tick: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockUpdateEvent {
    pub sequence: u64,
    pub game_tick: u64,
    /// Packet order within the observed physics/game tick. Mineflayer cannot
    /// identify the vanilla scheduler cause, but preserving this order keeps
    /// same-tick transitions distinguishable for later temporal analysis.
    #[serde(default)]
    pub sub_tick_order: u64,
    /// Scheduler phase is normally unavailable from Mineflayer packet
    /// updates. A future instrumented bridge may provide it without changing
    /// the recording shape.
    #[serde(default, skip_serializing_if = "TransitionPhase::is_unknown")]
    pub phase: TransitionPhase,
    /// Coarse classification of the packet-visible transition.
    #[serde(default)]
    pub event_kind: EventKind,
    /// The strongest cause known at the bridge boundary. Mineflayer only
    /// exposes the packet observation itself, not the vanilla scheduler.
    #[serde(default)]
    pub cause: EventCause,
    #[serde(default)]
    pub source: EventSource,
    /// Reserved for a scheduler-aware bridge; packet observations have no
    /// trustworthy parent event yet.
    #[serde(default)]
    pub cause_sequence: Option<u64>,
    pub pos: Pos,
    pub before: Option<ObservedBlockState>,
    pub after: Option<ObservedBlockState>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateRecording {
    pub recording_id: String,
    pub started_game_tick: u64,
    pub stopped_game_tick: u64,
    pub seen_events: usize,
    pub truncated: bool,
    pub events: Vec<BlockUpdateEvent>,
}

#[derive(Debug)]
pub enum BotBridgeError {
    Io(std::io::Error),
    Protocol(String),
    Json(serde_json::Error),
    Timeout(Duration),
}

impl Display for BotBridgeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, f),
            Self::Protocol(message) => write!(f, "bot bridge protocol error: {message}"),
            Self::Json(error) => Display::fmt(error, f),
            Self::Timeout(duration) => {
                write!(
                    f,
                    "bot bridge request timed out after {}ms",
                    duration.as_millis()
                )
            }
        }
    }
}

impl Error for BotBridgeError {}

impl From<std::io::Error> for BotBridgeError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for BotBridgeError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl BotBridge {
    #[must_use]
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            timeout: Duration::from_secs(10),
        }
    }

    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, BotBridgeError> {
        let timeout = self.timeout;
        let operation = async {
            let mut stream = TcpStream::connect(&self.address).await?;
            let request = json!({
                "id": NEXT_ID.fetch_add(1, Ordering::Relaxed),
                "method": method,
                "params": params,
            });
            stream
                .write_all(serde_json::to_string(&request)?.as_bytes())
                .await?;
            stream.write_all(b"\n").await?;
            let mut response = String::new();
            BufReader::new(stream).read_line(&mut response).await?;
            let response: Value = serde_json::from_str(&response)?;
            if let Some(error) = response.get("error") {
                return Err(BotBridgeError::Protocol(
                    error.as_str().unwrap_or("unknown bridge error").to_owned(),
                ));
            }
            serde_json::from_value(
                response
                    .get("result")
                    .cloned()
                    .ok_or_else(|| BotBridgeError::Protocol("response has no result".to_owned()))?,
            )
            .map_err(Into::into)
        };
        tokio::time::timeout(timeout, operation)
            .await
            .map_err(|_| BotBridgeError::Timeout(timeout))?
    }

    pub async fn status(&self) -> Result<BotStatus, BotBridgeError> {
        self.request("status", json!({})).await
    }

    pub async fn observe_player(
        &self,
        player: &str,
        max_distance: f64,
    ) -> Result<PlayerObservation, BotBridgeError> {
        if !is_valid_minecraft_username(player) {
            return Err(BotBridgeError::Protocol(format!(
                "invalid Minecraft player name: {player}"
            )));
        }
        let params = json!({ "player": player, "max_distance": max_distance });
        match self.request("observe_player", params.clone()).await {
            Ok(observation) => Ok(observation),
            Err(BotBridgeError::Protocol(message))
                if message.starts_with("player is not visible to the bot:") =>
            {
                let _: Value = self
                    .request("approach_player", json!({ "player": player }))
                    .await?;
                let mut observation: PlayerObservation =
                    self.request("observe_player", params).await?;
                observation.reacquired = true;
                Ok(observation)
            }
            Err(error) => Err(error),
        }
    }

    pub async fn visible_players(&self) -> Result<Vec<VisiblePlayer>, BotBridgeError> {
        self.request("visible_players", json!({})).await
    }

    pub async fn scan_region(
        &self,
        min: Pos,
        max: Pos,
        dimension: &str,
    ) -> Result<MinecraftSnapshot, BotBridgeError> {
        Ok(self
            .scan_region_confirmed(min, max, dimension)
            .await?
            .snapshot)
    }

    pub async fn scan_region_confirmed(
        &self,
        min: Pos,
        max: Pos,
        dimension: &str,
    ) -> Result<ConfirmedRegion, BotBridgeError> {
        let request = uuid::Uuid::new_v4().to_string();
        let region: ConfirmedRegion = self.request(
            "scan_region",
            json!({ "min": min, "max": max, "dimension": dimension, "readback_request_id": request }),
        )
        .await?;
        region.validate(min, max, dimension, &request)?;
        Ok(region)
    }

    pub async fn get_block(
        &self,
        pos: Pos,
        dimension: &str,
    ) -> Result<ObservedBlock, BotBridgeError> {
        #[derive(Deserialize)]
        struct ConfirmedBlock {
            #[serde(flatten)]
            block: ObservedBlock,
            readback: ServerReadback,
        }
        let request = uuid::Uuid::new_v4().to_string();
        let result: ConfirmedBlock = self
            .request(
                "get_block",
                json!({ "pos": pos, "dimension": dimension, "readback_request_id": request }),
            )
            .await?;
        let region = ConfirmedRegion {
            snapshot: MinecraftSnapshot {
                min: pos,
                max: pos,
                blocks: vec![dustroute_translate::MinecraftSnapshotBlock {
                    pos: result.block.pos,
                    name: result.block.state.name.clone(),
                    properties: result.block.state.properties.clone(),
                }],
            },
            readback: result.readback,
        };
        region.validate(pos, pos, dimension, &request)?;
        Ok(result.block)
    }

    pub async fn activate_lever(
        &self,
        pos: Pos,
        dimension: &str,
    ) -> Result<LeverActivation, BotBridgeError> {
        self.request(
            "activate_lever",
            json!({ "pos": pos, "dimension": dimension }),
        )
        .await
    }

    pub async fn approach_lever(
        &self,
        pos: Pos,
        dimension: &str,
    ) -> Result<LeverApproach, BotBridgeError> {
        self.request(
            "approach_lever",
            json!({ "pos": pos, "dimension": dimension }),
        )
        .await
    }

    pub async fn wait_ticks(&self, ticks: u16, dimension: &str) -> Result<Value, BotBridgeError> {
        self.request(
            "wait_ticks",
            json!({ "ticks": ticks, "dimension": dimension }),
        )
        .await
    }

    pub async fn start_update_recording(
        &self,
        min: Pos,
        max: Pos,
        dimension: &str,
        max_events: usize,
    ) -> Result<UpdateRecordingStarted, BotBridgeError> {
        self.request(
            "start_update_recording",
            json!({
                "min": min,
                "max": max,
                "dimension": dimension,
                "max_events": max_events
            }),
        )
        .await
    }

    pub async fn stop_update_recording(
        &self,
        recording_id: &str,
        dimension: &str,
    ) -> Result<UpdateRecording, BotBridgeError> {
        self.request(
            "stop_update_recording",
            json!({ "recording_id": recording_id, "dimension": dimension }),
        )
        .await
    }

    pub async fn preview_region(
        &self,
        player: &str,
        min: Pos,
        max: Pos,
        dimension: &str,
    ) -> Result<Value, BotBridgeError> {
        self.request(
            "preview_region",
            json!({ "player": player, "min": min, "max": max, "dimension": dimension }),
        )
        .await
    }

    pub async fn write_blocks(
        &self,
        changes: Value,
        dimension: &str,
    ) -> Result<Value, BotBridgeError> {
        self.request(
            "write_blocks",
            json!({ "changes": changes, "dimension": dimension }),
        )
        .await
    }

    pub async fn place_physical_blocks(
        &self,
        changes: Value,
        dimension: &str,
    ) -> Result<Value, BotBridgeError> {
        self.request(
            "place_physical_blocks",
            json!({ "changes": changes, "dimension": dimension }),
        )
        .await
    }
}

fn is_valid_minecraft_username(player: &str) -> bool {
    !player.is_empty()
        && player.len() <= 16
        && player
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || character == b'_')
}

/// Transport stubs explicitly provide a receipt. This does not simulate native
/// confirmation; that behavior is covered by the JS and isolated live trials.
#[cfg(test)]
pub(crate) fn test_scan_world(request: &Value, mut world: Value) -> Value {
    if request["method"] == "scan_region" {
        let min = &request["params"]["min"];
        let max = &request["params"]["max"];
        world["blocks"].as_array_mut().unwrap().retain(|b| {
            ["x", "y", "z"].into_iter().all(|a| {
                let v = b["pos"][a].as_i64().unwrap();
                min[a].as_i64().unwrap() <= v && v <= max[a].as_i64().unwrap()
            })
        });
        world["min"] = min.clone();
        world["max"] = max.clone();
    }
    world
}

#[cfg(test)]
pub(crate) fn test_readback_response(request: &Value, mut result: Value) -> Value {
    let method = request["method"].as_str().unwrap_or("");
    if !matches!(method, "scan_region" | "get_block") {
        return result;
    }
    let min = if method == "get_block" {
        &request["params"]["pos"]
    } else {
        &request["params"]["min"]
    };
    let max = if method == "get_block" {
        min
    } else {
        &request["params"]["max"]
    };
    let cells: i64 = ["x", "y", "z"]
        .into_iter()
        .map(|a| max[a].as_i64().unwrap() - min[a].as_i64().unwrap() + 1)
        .product();
    result["readback"] = json!({
        "schema_version":"dustroute.server-readback.v1", "kind":"server_confirmed",
        "request_id":request["params"]["readback_request_id"], "dimension":request["params"]["dimension"],
        "min":min, "max":max, "checked_cells":cells, "start_game_tick":42, "end_game_tick":42,
        "nonce":"a".repeat(32), "snapshot_sha256":"b".repeat(64), "corrections":[], "hidden_runtime_observed":false,
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    #[test]
    fn region_receipt_requires_fresh_request_exact_coverage_and_server_tick() {
        let pos = Pos::new(0, 180, 0);
        let request = json!({"method":"scan_region","params":{"min":pos,"max":pos,"dimension":"minecraft:overworld","readback_request_id":"fresh"}});
        let snapshot = json!({"min":pos,"max":pos,"blocks":[]});
        assert!(serde_json::from_value::<ConfirmedRegion>(snapshot.clone()).is_err());
        let mut valid = test_readback_response(&request, snapshot);
        valid["readback"]["predicate_ticks"] = json!([42]);
        valid["readback"]["predicate_batch_cells"] = json!(8880);
        let region: ConfirmedRegion = serde_json::from_value(valid.clone()).unwrap();
        region
            .validate(pos, pos, "minecraft:overworld", "fresh")
            .unwrap();
        for (field, value) in [
            ("request_id", json!("old")),
            ("dimension", json!("minecraft:the_nether")),
            ("kind", json!("client_snapshot")),
            ("checked_cells", json!(0)),
            ("end_game_tick", json!(43)),
            ("predicate_ticks", json!([43])),
            ("predicate_ticks", json!([42, 42])),
            ("predicate_batch_cells", json!(0)),
            ("hidden_runtime_observed", json!(true)),
        ] {
            let mut broken = valid.clone();
            broken["readback"][field] = value;
            let region: ConfirmedRegion = serde_json::from_value(broken).unwrap();
            assert!(
                region
                    .validate(pos, pos, "minecraft:overworld", "fresh")
                    .is_err(),
                "{field}"
            );
        }
        let mut wrong_bounds = region.clone();
        wrong_bounds.snapshot.max.x += 1;
        assert!(
            wrong_bounds
                .validate(pos, pos, "minecraft:overworld", "fresh")
                .is_err()
        );
    }

    async fn fake_bridge(result: Value) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut request)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            let response = json!({ "id": request["id"], "result": result });
            stream
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        });
        address
    }

    #[tokio::test]
    async fn reads_status_from_fake_visible_bot() {
        let address = fake_bridge(json!({
            "connected": true,
            "username": "DustRouteBot",
            "host": "minecraft.test",
            "port": 25565,
            "version": "1.21.11",
            "dimension": "minecraft:overworld"
        }))
        .await;
        let status = BotBridge::new(address).status().await.unwrap();
        assert!(status.connected);
        assert_eq!(status.username, "DustRouteBot");
        assert_eq!(status.version, "1.21.11");
        assert_eq!(status.metrics.requests_total, 0);
    }

    #[tokio::test]
    async fn reads_bridge_metrics_from_status() {
        let address = fake_bridge(json!({
            "connected": true,
            "username": "DustRouteBot",
            "host": "minecraft.test",
            "port": 25565,
            "version": "1.21.11",
            "dimension": "minecraft:overworld",
            "metrics": {
                "requests_total": 12,
                "errors_total": 1,
                "request_bytes": 1024,
                "response_bytes": 2048,
                "total_duration_micros": 3300,
                "max_duration_micros": 900,
                "scan_requests": 3,
                "scan_volume_blocks": 4096,
                "scan_non_air_blocks": 180,
                "requests_by_method": { "scan_region": 3 }
            }
        }))
        .await;
        let status = BotBridge::new(address).status().await.unwrap();
        assert_eq!(status.metrics.requests_total, 12);
        assert_eq!(status.metrics.scan_volume_blocks, 4096);
        assert_eq!(status.metrics.requests_by_method["scan_region"], 3);
    }

    #[tokio::test]
    async fn reads_player_gaze_from_fake_visible_bot() {
        let address = fake_bridge(json!({
            "player": "builder",
            "eye_position": { "x": 1.5, "y": 65.62, "z": 2.5 },
            "yaw": 0.0,
            "pitch": 0.25,
            "targeted_block": { "x": 1, "y": 64, "z": -4 },
            "targeted_face": "up",
            "distance": 6.0,
            "dimension": "minecraft:overworld"
        }))
        .await;
        let observation = BotBridge::new(address)
            .observe_player("builder", 64.0)
            .await
            .unwrap();
        assert_eq!(observation.player, "builder");
        assert_eq!(observation.targeted_block, Some(Pos::new(1, 64, -4)));
        assert_eq!(observation.targeted_face.as_deref(), Some("up"));
        assert!(!observation.reacquired);
    }

    #[tokio::test]
    async fn moves_to_an_out_of_range_player_and_retries_observation() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            for attempt in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut request)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&request).unwrap();
                let response = match attempt {
                    0 => {
                        assert_eq!(request["method"], "observe_player");
                        json!({ "id": request["id"], "error": "player is not visible to the bot: builder" })
                    }
                    1 => {
                        assert_eq!(request["method"], "approach_player");
                        json!({ "id": request["id"], "result": {
                            "player": "builder", "moved": true,
                            "position": { "x": 1.0, "y": 64.0, "z": 1.0 },
                            "distance": 2.0, "dimension": "minecraft:overworld"
                        }})
                    }
                    _ => {
                        assert_eq!(request["method"], "observe_player");
                        json!({ "id": request["id"], "result": {
                            "player": "builder",
                            "eye_position": { "x": 1.5, "y": 65.62, "z": 2.5 },
                            "yaw": 0.0, "pitch": 0.25,
                            "targeted_block": { "x": 1, "y": 64, "z": -4 },
                            "targeted_face": null, "distance": 6.0,
                            "dimension": "minecraft:overworld"
                        }})
                    }
                };
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let observation = BotBridge::new(address)
            .observe_player("builder", 64.0)
            .await
            .unwrap();
        assert!(observation.reacquired);
        assert_eq!(observation.player, "builder");
    }

    #[tokio::test]
    async fn rejects_a_player_name_before_sending_a_command() {
        let error = BotBridge::new("127.0.0.1:1")
            .observe_player("builder /kill", 64.0)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("invalid Minecraft player name"));
    }

    #[tokio::test]
    async fn reports_when_an_offline_player_cannot_be_reacquired() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            for message in [
                "player is not visible to the bot: builder",
                "player could not be reacquired after moving the bot: builder",
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut request)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&request).unwrap();
                let response = json!({ "id": request["id"], "error": message });
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let error = BotBridge::new(address)
            .observe_player("builder", 64.0)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("could not be reacquired"));
    }

    #[tokio::test]
    async fn surfaces_bridge_protocol_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut request)
                .await
                .unwrap();
            stream
                .write_all(b"{\"id\":1,\"error\":\"chunk unavailable\"}\n")
                .await
                .unwrap();
        });
        let error = BotBridge::new(address).status().await.unwrap_err();
        assert!(error.to_string().contains("chunk unavailable"));
    }

    #[tokio::test]
    async fn times_out_when_bridge_stops_responding() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let error = BotBridge::new(address)
            .with_timeout(Duration::from_millis(10))
            .status()
            .await
            .unwrap_err();
        assert!(matches!(error, BotBridgeError::Timeout(_)));
    }
}
