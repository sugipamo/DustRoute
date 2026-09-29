//! Native client observation boundary. Client evidence retains its own source
//! and cannot construct a server-confirmed capability.
use crate::bridge::BotBridgeError;
use dustroute_physical::Pos;
use dustroute_translate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
use serde::Serialize;
use voxrig::versions::java_1_21_11::reconstruction::ClientObservation;
use voxrig::{Client, ConnectionConfig, MinecraftVersion, Region};
mod operations;

pub struct VoxrigBridge {
    client: Client,
    host: String,
    port: u16,
    username: String,
    mutations: tokio::sync::Mutex<()>,
}
impl std::fmt::Debug for VoxrigBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoxrigBridge").finish_non_exhaustive()
    }
}

/// Deliberately not Deserialize: a saved record is not a fresh observation.
#[derive(Debug, Serialize)]
pub struct ClientRegion {
    schema_version: &'static str,
    kind: ClientEvidenceKind,
    snapshot: MinecraftSnapshot,
    /// Includes received cache, connection/sequence, client frame, moving states,
    /// per-cell origin and dimension. No field is presented as server game time.
    observation: ClientObservation,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ClientEvidenceKind {
    ClientReconstructed,
}

impl ClientRegion {
    #[must_use]
    pub fn snapshot(&self) -> &MinecraftSnapshot {
        &self.snapshot
    }
    #[must_use]
    pub fn observation(&self) -> &ClientObservation {
        &self.observation
    }
    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.observation.blocks.iter().any(|b| {
            b.moving.is_some()
                || b.state
                    .as_ref()
                    .is_some_and(|s| s.name == "minecraft:moving_piston")
        })
    }
    fn from_observation(
        observation: ClientObservation,
        region: Region,
        dimension: &str,
    ) -> Result<Self, BotBridgeError> {
        let fail = |s: &str| BotBridgeError::Protocol(s.into());
        let volume = region.volume().map_err(native_error)?;
        if observation.received.version != MinecraftVersion::Java1_21_11
            || observation.received.receive_sequence.is_none()
            || observation.dimension != dimension
            || observation.received.region != region
        {
            return Err(fail(
                "client observation version, dimension or bounds mismatch",
            ));
        }
        if observation.issue.is_some() || !observation.recovery_chunks.is_empty() {
            return Err(fail(
                "client reconstruction incomplete; fresh observation required",
            ));
        }
        if observation.blocks.len() != volume || observation.received.blocks.len() != volume {
            return Err(fail(
                "client observation does not cover the complete region",
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut blocks = Vec::with_capacity(volume);
        for b in &observation.blocks {
            let p = b.position;
            if (0..3).any(|axis| p[axis] < region.min[axis] || p[axis] > region.max[axis])
                || !seen.insert(p)
            {
                return Err(fail(
                    "client observation has duplicate or out-of-bounds cells",
                ));
            }
            let state = b
                .state
                .as_ref()
                .ok_or_else(|| fail("client observation contains unloaded cells"))?;
            blocks.push(MinecraftSnapshotBlock {
                pos: pos(p),
                name: state.name.clone(),
                properties: state.properties.clone(),
            });
        }
        Ok(Self {
            schema_version: "dustroute.client-observation.v1",
            kind: ClientEvidenceKind::ClientReconstructed,
            snapshot: MinecraftSnapshot {
                min: pos(region.min),
                max: pos(region.max),
                blocks,
            },
            observation,
        })
    }
}

impl VoxrigBridge {
    pub async fn connect(config: ConnectionConfig) -> Result<Self, BotBridgeError> {
        if config.version != MinecraftVersion::Java1_21_11 {
            return Err(BotBridgeError::Protocol(
                "DustRoute physical context requires Java 1.21.11".into(),
            ));
        }
        let host = config.server.host.clone();
        let port = config.server.port;
        let username = config.username.clone();
        let client = Client::connect(config).await.map_err(native_error)?;
        client.wait_until_ready().await.map_err(native_error)?;
        Ok(Self {
            client,
            host,
            port,
            username,
            mutations: tokio::sync::Mutex::new(()),
        })
    }
    pub async fn observe_region(
        &self,
        min: Pos,
        max: Pos,
        dimension: &str,
    ) -> Result<ClientRegion, BotBridgeError> {
        let region = Region {
            min: [min.x, min.y, min.z],
            max: [max.x, max.y, max.z],
        };
        let observation = self
            .client
            .observe_client_region(region)
            .await
            .map_err(native_error)?;
        ClientRegion::from_observation(observation, region, dimension)
    }
}
fn pos(p: [i32; 3]) -> Pos {
    Pos::new(p[0], p[1], p[2])
}
fn native_error(error: voxrig::Error) -> BotBridgeError {
    BotBridgeError::Protocol(format!("Voxrig: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxrig::versions::java_1_21_11::reconstruction::{
        ClientBlock, ReconstructionIssue, StateOrigin,
    };
    use voxrig::{NativeBlockState, Observation, ObservedBlock};
    fn sample() -> ClientObservation {
        let state = NativeBlockState {
            name: "minecraft:quartz_stairs".into(),
            properties: [
                ("facing", "north"),
                ("half", "top"),
                ("shape", "inner_left"),
                ("waterlogged", "false"),
            ]
            .map(|(k, v)| (k.into(), v.into()))
            .into(),
        };
        ClientObservation {
            received: Observation {
                version: MinecraftVersion::Java1_21_11,
                connection_id: 7,
                revision: 4,
                receive_sequence: Some(6),
                captured_at: std::time::Duration::from_millis(9),
                region: Region {
                    min: [0, 80, 0],
                    max: [0, 80, 0],
                },
                blocks: vec![ObservedBlock {
                    position: [0, 80, 0],
                    state: Some(state.clone()),
                }],
            },
            dimension: "minecraft:overworld".into(),
            client_tick: 10,
            client_revision: 2,
            blocks: vec![ClientBlock {
                position: [0, 80, 0],
                state: Some(state),
                origin: StateOrigin::ClientUpdate { action_sequence: 5 },
                moving: None,
            }],
            issue: None,
            recovery_chunks: vec![],
        }
    }
    #[test]
    fn preserves_native_properties_and_provenance_without_server_confirmation() {
        let observation = sample();
        let region = observation.received.region;
        let r = ClientRegion::from_observation(observation, region, "minecraft:overworld").unwrap();
        assert_eq!(r.snapshot.blocks[0].properties["shape"], "inner_left");
        let value = serde_json::to_value(r).unwrap();
        assert_eq!(value["kind"], "client_reconstructed");
        assert_eq!(
            value["observation"]["blocks"][0]["origin"]["action_sequence"],
            5
        );
        assert!(serde_json::from_value::<crate::bridge::ConfirmedRegion>(value).is_err());
    }
    #[test]
    fn incomplete_or_mismatched_client_views_never_become_complete_snapshots() {
        for case in 0..7 {
            let mut o = sample();
            let region = o.received.region;
            match case {
                0 => o.blocks[0].state = None,
                1 => o.issue = Some(ReconstructionIssue::Limit),
                2 => o.recovery_chunks.push([0, 0]),
                3 => o.blocks.clear(),
                4 => o.blocks[0].position = [1, 80, 0],
                5 => o.dimension = "minecraft:the_nether".into(),
                _ => o.received.receive_sequence = None,
            }
            assert!(ClientRegion::from_observation(o, region, "minecraft:overworld").is_err());
        }
    }
}
