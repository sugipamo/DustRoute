//! Version-selected client API. Local observations never claim server confirmation.

use crate::versions::java_1_16_1 as legacy;
use crate::{Error, ErrorKind, MinecraftVersion, NativeBlockState, Result};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_connection_id() -> u64 {
    NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed)
}

/// Connection settings with an explicit, immutable game version.
#[derive(Clone, Debug)]
pub struct ConnectionConfig {
    /// Server address; no automatic protocol downgrade is performed.
    pub server: legacy::Server,
    /// Offline player name.
    pub username: String,
    /// Version of both wire packets and native registry IDs.
    pub version: MinecraftVersion,
    /// Existing resource and timeout limits.
    pub limits: legacy::ConnectionOptions,
}

impl ConnectionConfig {
    /// Creates an explicit offline-mode connection configuration.
    pub fn offline(
        server: legacy::Server,
        username: impl Into<String>,
        version: MinecraftVersion,
    ) -> Self {
        Self {
            server,
            username: username.into(),
            version,
            limits: legacy::ConnectionOptions::default(),
        }
    }
}

/// Inclusive region, independent of a protocol's chunk representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Region {
    /// Minimum x, y and z coordinates.
    pub min: [i32; 3],
    /// Maximum x, y and z coordinates.
    pub max: [i32; 3],
}

impl Region {
    /// Validates geometry and returns the bounded number of cells.
    pub fn volume(self) -> Result<usize> {
        let mut volume = 1u64;
        for axis in 0..3 {
            if self.min[axis] < -30_000_000
                || self.max[axis] > 30_000_000
                || self.min[axis] > self.max[axis]
            {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    anyhow::anyhow!("invalid region bounds"),
                ));
            }
            let width = (i64::from(self.max[axis]) - i64::from(self.min[axis]) + 1) as u64;
            volume = volume.checked_mul(width).ok_or_else(|| {
                Error::new(
                    ErrorKind::ResourceLimit,
                    anyhow::anyhow!("region volume overflow"),
                )
            })?;
        }
        if volume > 262_144 {
            return Err(Error::new(
                ErrorKind::ResourceLimit,
                anyhow::anyhow!("region exceeds 262144 cells"),
            ));
        }
        Ok(volume as usize)
    }
}

/// One cell in a complete local region observation.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ObservedBlock {
    /// Absolute x, y and z coordinates.
    pub position: [i32; 3],
    /// `None` means unavailable chunk data, never inferred air.
    pub state: Option<NativeBlockState>,
}

/// Local state at one cache revision, bound to its version and connection.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Observation<B = Vec<ObservedBlock>> {
    /// Exact version used to interpret registry IDs.
    pub version: MinecraftVersion,
    /// Process-local connection identity; revisions from different connections
    /// are incomparable. Persisted captures also need their owning process/run
    /// identity: this counter starts again in a new OS process.
    pub connection_id: u64,
    /// World-domain revision, not a server game tick.
    pub revision: u64,
    /// Last applied receive sequence when supported by the adapter; never a server tick.
    pub receive_sequence: Option<u64>,
    /// Local elapsed time of the snapshot.
    pub captured_at: Duration,
    /// Requested inclusive bounds.
    pub region: Region,
    /// Includes air and explicitly unavailable cells.
    pub blocks: B,
}

#[derive(Clone)]
enum Adapter {
    Java1_16_1(Box<legacy::Bot>),
    Java1_21_11(crate::versions::java_1_21_11::Bot),
}

/// A client whose protocol, registry and behavior belong to one version adapter.
#[derive(Clone)]
pub struct Client {
    adapter: Adapter,
}

impl Client {
    /// Version-specific modern controls. The established 1.16.1 Bot API coexists.
    pub fn java_1_21_11_operations(
        &self,
    ) -> Result<crate::versions::java_1_21_11::operations::Operations> {
        match &self.adapter {
            Adapter::Java1_21_11(bot) => Ok(bot.operations()),
            Adapter::Java1_16_1(_) => Err(Error::new(
                ErrorKind::Unsupported,
                anyhow::anyhow!("Java 1.21.11 operations require that version adapter"),
            )),
        }
    }
    /// Returns the Java 1.21.11 client reconstruction alongside the unchanged received cache.
    /// Inspect both the typed issue and cell availability; this is not server confirmation.
    pub async fn observe_client_region(
        &self,
        region: Region,
    ) -> Result<crate::versions::java_1_21_11::reconstruction::ClientObservation> {
        match &self.adapter {
            Adapter::Java1_21_11(bot) => bot.observe_client_region(region).await,
            Adapter::Java1_16_1(_) => Err(Error::new(
                ErrorKind::Unsupported,
                anyhow::anyhow!("client piston reconstruction is not implemented for Java 1.16.1"),
            )),
        }
    }
    /// Shares immutable region cells within one receive/reconstruction generation.
    /// Every call still captures a new receive boundary and local frame.
    pub async fn observe_shared_client_region(
        &self,
        region: Region,
    ) -> Result<crate::versions::java_1_21_11::reconstruction::SharedClientRegion> {
        match &self.adapter {
            Adapter::Java1_21_11(bot) => bot.observe_shared_client_region(region).await,
            Adapter::Java1_16_1(_) => Err(Error::new(
                ErrorKind::Unsupported,
                anyhow::anyhow!("client piston reconstruction is not implemented for Java 1.16.1"),
            )),
        }
    }
    /// Captures exact incoming packets for a bounded diagnostic interval.
    pub async fn start_packet_trace(&self, maximum_bytes: usize) -> Result<()> {
        match &self.adapter {
            Adapter::Java1_21_11(bot) => bot.start_packet_trace(maximum_bytes).await,
            Adapter::Java1_16_1(_) => Err(Error::new(
                ErrorKind::Unsupported,
                anyhow::anyhow!(
                    "packet capture is not implemented for the 1.16.1 compatibility adapter"
                ),
            )),
        }
    }

    /// Finishes a diagnostic capture; incomplete captures are explicitly marked.
    pub async fn stop_packet_trace(&self) -> Result<crate::versions::java_1_21_11::PacketTrace> {
        match &self.adapter {
            Adapter::Java1_21_11(bot) => bot.stop_packet_trace().await,
            Adapter::Java1_16_1(_) => Err(Error::new(
                ErrorKind::Unsupported,
                anyhow::anyhow!(
                    "packet capture is not implemented for the 1.16.1 compatibility adapter"
                ),
            )),
        }
    }

    /// Sends an ordinary use-on-block interaction. Dispatch is not acceptance.
    pub async fn interact_block(&self, position: [i32; 3], face: crate::BlockFace) -> Result<()> {
        match &self.adapter {
            Adapter::Java1_21_11(bot) => bot.interact_block(position, face).await,
            Adapter::Java1_16_1(bot) => {
                bot.place_block(
                    legacy::Hand::Main,
                    legacy::BlockPos {
                        x: position[0],
                        y: position[1],
                        z: position[2],
                    },
                    face,
                    [0.5; 3],
                    false,
                )
                .await
            }
        }
    }

    /// Connects using only the selected version. Unsupported adapters fail before I/O.
    pub async fn connect(config: ConnectionConfig) -> Result<Self> {
        match config.version {
            MinecraftVersion::Java1_16_1 => {
                let bot = legacy::Bot::connect(
                    config.server,
                    legacy::Player::offline(config.username),
                    Arc::new(legacy::SharedChunkStorage::default()),
                    config.limits,
                )
                .await?;
                Ok(Self {
                    adapter: Adapter::Java1_16_1(Box::new(bot)),
                })
            }
            MinecraftVersion::Java1_21_11 => Ok(Self {
                adapter: Adapter::Java1_21_11(
                    crate::versions::java_1_21_11::Bot::connect(config).await?,
                ),
            }),
        }
    }

    /// Exact version of this connection.
    pub fn version(&self) -> MinecraftVersion {
        match self.adapter {
            Adapter::Java1_16_1(_) => MinecraftVersion::Java1_16_1,
            Adapter::Java1_21_11(_) => MinecraftVersion::Java1_21_11,
        }
    }

    /// Waits for the selected adapter's initial playable state.
    pub async fn wait_until_ready(&self) -> Result<()> {
        match &self.adapter {
            Adapter::Java1_16_1(bot) => bot.wait_until_ready().await,
            Adapter::Java1_21_11(bot) => bot.wait_until_ready().await,
        }
    }

    /// Gets all cells under one world lock; this does not send confirmation commands.
    pub async fn observe_region(&self, region: Region) -> Result<Observation> {
        region.volume()?;
        match &self.adapter {
            Adapter::Java1_21_11(bot) => bot.observe_region(region).await,
            Adapter::Java1_16_1(bot) => {
                if region.min[1] < 0 || region.max[1] > 255 {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        anyhow::anyhow!("region is outside Java 1.16.1 dimension height"),
                    ));
                }
                let snapshot = bot.observe_region_snapshot(region).await?;
                let mut blocks = Vec::with_capacity(snapshot.value.len());
                for block in snapshot.value {
                    blocks.push(ObservedBlock {
                        position: [block.x, block.y, block.z],
                        state: block.state_id.map(legacy::native_state).transpose()?,
                    });
                }
                Ok(Observation {
                    version: self.version(),
                    connection_id: bot.connection_id(),
                    revision: snapshot.revision,
                    receive_sequence: None,
                    captured_at: snapshot.captured_at,
                    region,
                    blocks,
                })
            }
        }
    }

    /// Ends the connection. A clone refers to the same session.
    pub async fn disconnect(&self) -> Result<()> {
        match &self.adapter {
            Adapter::Java1_16_1(bot) => bot.disconnect().await,
            Adapter::Java1_21_11(bot) => bot.disconnect().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_volume_is_checked_before_allocation() {
        assert_eq!(
            Region {
                min: [-16, 0, 0],
                max: [15, 1, 0]
            }
            .volume()
            .unwrap(),
            64
        );
        assert!(
            Region {
                min: [1, 0, 0],
                max: [0, 0, 0]
            }
            .volume()
            .is_err()
        );
        assert!(
            Region {
                min: [0, 0, 0],
                max: [1000, 1000, 1000]
            }
            .volume()
            .is_err()
        );
        assert!(
            Region {
                min: [i32::MIN, 0, 0],
                max: [i32::MAX, 0, 0]
            }
            .volume()
            .is_err()
        );
    }

    #[tokio::test]
    async fn invalid_modern_identity_is_rejected_before_network_io() {
        let config = ConnectionConfig::offline(
            legacy::Server::new("invalid.invalid", 1),
            "",
            MinecraftVersion::Java1_21_11,
        );
        let error = match Client::connect(config).await {
            Ok(_) => panic!("invalid identity connected"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn legacy_adapter_binds_snapshot_to_session_and_marks_unloaded_cells() {
        use crate::protocol::{get_varint, read_packet, write_packet};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (_, payload) = read_packet(&mut stream, None).await.unwrap();
                assert_eq!(get_varint(&mut payload.as_slice()).unwrap(), 736);
                read_packet(&mut stream, None).await.unwrap();
                write_packet(&mut stream, None, 2, &[]).await.unwrap();
                // No chunk arrives; the client must not label missing data as air.
                while read_packet(&mut stream, None).await.is_ok() {}
            }
        });
        let config = ConnectionConfig::offline(
            legacy::Server::new("127.0.0.1", port),
            "Observe",
            MinecraftVersion::Java1_16_1,
        );
        let region = Region {
            min: [-1, 0, -1],
            max: [0, 0, 0],
        };
        let one = Client::connect(config.clone()).await.unwrap();
        let first = one.observe_region(region).await.unwrap();
        assert_eq!(first.blocks.len(), 4);
        assert!(first.blocks.iter().all(|b| b.state.is_none()));
        assert_eq!(first.version, MinecraftVersion::Java1_16_1);
        assert!(
            one.observe_region(Region {
                min: [0, -1, 0],
                max: [0, 0, 0]
            })
            .await
            .is_err()
        );
        one.disconnect().await.unwrap();
        assert!(one.observe_region(region).await.is_err());
        let two = Client::connect(config).await.unwrap();
        let second = two.observe_region(region).await.unwrap();
        assert_ne!(first.connection_id, second.connection_id);
        two.disconnect().await.unwrap();
        server.await.unwrap();
    }
}
