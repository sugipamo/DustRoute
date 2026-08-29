use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dustroute_model::Pos;
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BotStatus {
    pub connected: bool,
    pub username: String,
    pub host: String,
    pub port: u16,
    pub version: String,
    pub dimension: Option<String>,
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VisiblePlayer {
    pub player: String,
    pub position: Vec3,
    pub distance_from_bot: f64,
    pub dimension: String,
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
        self.request(
            "observe_player",
            json!({ "player": player, "max_distance": max_distance }),
        )
        .await
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
        self.request(
            "scan_region",
            json!({ "min": min, "max": max, "dimension": dimension }),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

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
