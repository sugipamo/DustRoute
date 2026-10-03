//! Native client observation boundary. Client evidence retains its own source
//! and cannot construct a server-confirmed capability.
use crate::bridge::BotBridgeError;
use dustroute_physical::Pos;
use dustroute_translate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
use serde::Serialize;
use std::sync::{Arc, Mutex, Weak};
use voxrig::versions::java_1_21_11::reconstruction::SharedClientObservation;
use voxrig::{Client, ConnectionConfig, MinecraftVersion, Region};
mod operations;
mod survival;
pub(crate) use survival::SurvivalLease;

pub struct VoxrigBridge {
    client: Mutex<Option<Client>>,
    reconnect: ConnectionConfig,
    host: String,
    port: u16,
    username: String,
    mutations: Arc<tokio::sync::Mutex<()>>,
    snapshots: Mutex<ConvertedSnapshots>,
    pub(crate) contents: Arc<crate::snapshot_content::SnapshotContents>,
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
    snapshot: Arc<MinecraftSnapshot>,
    /// Includes received cache, connection/sequence, client frame, moving states,
    /// per-cell origin and dimension. No field is presented as server game time.
    observation: SharedClientObservation,
    #[serde(skip)]
    moving: bool,
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
    pub fn observation(&self) -> &SharedClientObservation {
        &self.observation
    }
    pub(crate) fn shared_snapshot(&self) -> Arc<MinecraftSnapshot> {
        self.snapshot.clone()
    }
    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.moving
    }
    fn from_observation(
        observation: impl Into<SharedClientObservation>,
        region: Region,
        dimension: &str,
    ) -> Result<Self, BotBridgeError> {
        let observation = observation.into();
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
            let mut cause = crate::failure::FailureCause::new(
                crate::failure::CauseKind::ObservationIncomplete,
                "client reconstruction incomplete; fresh observation required",
            );
            cause.details.reconstruction_issue = observation
                .issue
                .as_ref()
                .map(|issue| crate::failure::ReconstructionDiagnostic::Client(issue.into()));
            cause.details.recovery_chunks = observation
                .recovery_chunks
                .iter()
                .take(64)
                .copied()
                .collect();
            cause.details.recovery_chunks_truncated = observation.recovery_chunks.len() > 64;
            return Err(BotBridgeError::Detailed(cause));
        }
        if observation.blocks.len() != volume || observation.received.blocks.len() != volume {
            return Err(fail(
                "client observation does not cover the complete region",
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut blocks = Vec::with_capacity(volume);
        let mut moving = false;
        for b in observation.blocks.iter() {
            let p = b.position;
            if (0..3).any(|axis| p[axis] < region.min[axis] || p[axis] > region.max[axis])
                || !seen.insert(p)
            {
                return Err(fail(
                    "client observation has duplicate or out-of-bounds cells",
                ));
            }
            let state = b.state.as_ref().ok_or_else(|| {
                let mut cause = crate::failure::FailureCause::new(
                    crate::failure::CauseKind::ObservationIncomplete,
                    "client observation contains unloaded cells",
                );
                cause.details.position = Some(pos(p));
                BotBridgeError::Detailed(cause)
            })?;
            moving |= b.moving.is_some() || state.name == "minecraft:moving_piston";
            blocks.push(MinecraftSnapshotBlock {
                pos: pos(p),
                name: state.name.clone(),
                properties: state.properties.clone(),
            });
        }
        Ok(Self {
            schema_version: "dustroute.client-observation.v1",
            kind: ClientEvidenceKind::ClientReconstructed,
            snapshot: Arc::new(MinecraftSnapshot {
                min: pos(region.min),
                max: pos(region.max),
                blocks,
            }),
            observation,
            moving,
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
        let client = Client::connect(config.clone())
            .await
            .map_err(native_error)?;
        client.wait_until_ready().await.map_err(native_error)?;
        let contents = Arc::default();
        Ok(Self {
            client: Mutex::new(Some(client)),
            reconnect: config,
            host,
            port,
            username,
            mutations: Arc::new(tokio::sync::Mutex::new(())),
            snapshots: Mutex::new(ConvertedSnapshots {
                contents: Arc::clone(&contents),
                ..Default::default()
            }),
            contents,
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
        let acquired = {
            let measurement = crate::performance::span(crate::performance::Phase::NativeObserve);
            let acquired = self
                .client()?
                .observe_shared_client_region(region)
                .await
                .map_err(native_error)?;
            let _measurement = measurement.acquisition(
                acquired.materialized_cells,
                acquired.materialized_cells == 0,
            );
            acquired
        };
        self.snapshots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .convert(acquired.observation, region, dimension)
    }
}

#[derive(Default)]
struct ConvertedSnapshots {
    entries: std::collections::VecDeque<ConvertedSnapshot>,
    cells: usize,
    contents: Arc<crate::snapshot_content::SnapshotContents>,
}
struct ConvertedSnapshot {
    source: Weak<[voxrig::versions::java_1_21_11::reconstruction::ClientBlock]>,
    content: crate::snapshot_content::SharedSnapshot,
    moving: bool,
}
impl ConvertedSnapshots {
    fn convert(
        &mut self,
        observation: impl Into<SharedClientObservation>,
        region: Region,
        dimension: &str,
    ) -> Result<ClientRegion, BotBridgeError> {
        let observation = observation.into();
        let measurement = crate::performance::span(crate::performance::Phase::NativeConvert);
        // The key holds a weak source pointer, so allocator address reuse cannot alias cells.
        // Only fully validated conversions enter the cache; evidence is checked anew.
        if observation.received.version == MinecraftVersion::Java1_21_11
            && observation.received.region == region
            && observation.dimension == dimension
            && observation.received.receive_sequence.is_some()
            && observation.issue.is_none()
            && observation.recovery_chunks.is_empty()
            && let Some(entry) = self.entries.iter().find(|entry| {
                entry
                    .source
                    .upgrade()
                    .is_some_and(|cells| Arc::ptr_eq(&cells, &observation.blocks))
            })
            && entry.content.min == pos(region.min)
            && entry.content.max == pos(region.max)
            && observation.received.blocks.len() == observation.blocks.len()
        {
            let _measurement = measurement.acquisition(0, true);
            return Ok(ClientRegion {
                schema_version: "dustroute.client-observation.v1",
                kind: ClientEvidenceKind::ClientReconstructed,
                snapshot: entry.content.shared_allocation(),
                observation,
                moving: entry.moving,
            });
        }
        let mut result = ClientRegion::from_observation(observation, region, dimension)?;
        let content = self
            .contents
            .intern_arc(result.snapshot.clone())
            .map_err(BotBridgeError::Protocol)?;
        result.snapshot = content.shared_allocation();
        let _measurement = measurement.acquisition(result.snapshot.blocks.len(), false);
        self.entries.retain(|entry| entry.source.strong_count() > 0);
        self.cells = self.entries.iter().map(|e| e.content.blocks.len()).sum();
        let volume = result.snapshot.blocks.len();
        if volume > 65_536 {
            return Ok(result);
        }
        while self.entries.len() >= 16 || self.cells + volume > 65_536 {
            if let Some(old) = self.entries.pop_front() {
                self.cells -= old.content.blocks.len();
            } else {
                break;
            }
        }
        self.entries.push_back(ConvertedSnapshot {
            source: Arc::downgrade(&result.observation.blocks),
            content,
            moving: result.moving,
        });
        self.cells += volume;
        Ok(result)
    }
}
fn pos(p: [i32; 3]) -> Pos {
    Pos::new(p[0], p[1], p[2])
}
fn native_error(error: voxrig::Error) -> BotBridgeError {
    use crate::failure::CauseKind as C;
    use voxrig::ErrorKind as E;
    let kind = match error.kind() {
        E::Unsupported => C::Unsupported,
        E::InvalidInput => C::InvalidInput,
        E::Connection => C::Connection,
        E::Timeout => C::Timeout,
        E::Disconnected => C::Disconnected,
        E::Protocol => C::Protocol,
        E::ResourceLimit => C::ResourceLimit,
        E::Rejected => C::Rejected,
        E::State => C::ObservationUnavailable,
        _ => C::Unknown,
    };
    let mut cause = crate::failure::FailureCause::new(kind, format!("Voxrig: {error}"));
    cause.details.native_error_kind = Some(error.kind().into());
    BotBridgeError::Detailed(cause)
}

impl From<&voxrig::versions::java_1_21_11::reconstruction::ReconstructionIssue>
    for crate::failure::ClientReconstructionDiagnostic
{
    fn from(issue: &voxrig::versions::java_1_21_11::reconstruction::ReconstructionIssue) -> Self {
        use voxrig::versions::java_1_21_11::reconstruction::ReconstructionIssue as Native;
        match issue {
            Native::UnsupportedTickControl => Self::UnsupportedTickControl,
            Native::MissingBlock { position } => Self::MissingBlock {
                position: *position,
            },
            Native::UnsupportedBlock { position, name } => Self::UnsupportedBlock {
                position: *position,
                name: name.clone(),
            },
            Native::MissingCarrier { position } => Self::MissingCarrier {
                position: *position,
            },
            Native::ChunkInvalidated { chunk } => Self::ChunkInvalidated { chunk: *chunk },
            Native::Limit => Self::Limit,
            Native::InvalidAction => Self::InvalidAction,
        }
    }
}

impl From<voxrig::ErrorKind> for crate::failure::NativeFailureKind {
    fn from(kind: voxrig::ErrorKind) -> Self {
        use voxrig::ErrorKind as Native;
        match kind {
            Native::Unsupported => Self::Unsupported,
            Native::InvalidInput => Self::InvalidInput,
            Native::Connection => Self::Connection,
            Native::UncertainDispatch => Self::UncertainDispatch,
            Native::Timeout => Self::Timeout,
            Native::Disconnected => Self::Disconnected,
            Native::Protocol => Self::Protocol,
            Native::ResourceLimit => Self::ResourceLimit,
            Native::Rejected => Self::Rejected,
            Native::State => Self::State,
            Native::Other => Self::Other,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxrig::versions::java_1_21_11::reconstruction::{
        ClientBlock, ClientObservation, ReconstructionIssue, StateOrigin,
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
    fn native_categories_and_incomplete_evidence_reach_public_reports() {
        for (native, expected) in [
            (
                voxrig::ErrorKind::Timeout,
                crate::failure::CauseKind::Timeout,
            ),
            (
                voxrig::ErrorKind::Unsupported,
                crate::failure::CauseKind::Unsupported,
            ),
            (
                voxrig::ErrorKind::Disconnected,
                crate::failure::CauseKind::Disconnected,
            ),
        ] {
            let cause = native_error(voxrig::Error::new(
                native,
                anyhow::anyhow!("opaque diagnostic"),
            ))
            .cause();
            assert_eq!(cause.kind, expected);
            assert_eq!(cause.details.native_error_kind, Some(native.into()));
        }
        let mut observation = sample();
        let region = observation.received.region;
        observation.issue = Some(ReconstructionIssue::Limit);
        observation.recovery_chunks = (0..65).map(|n| [n, 0]).collect();
        let error =
            ClientRegion::from_observation(observation, region, "minecraft:overworld").unwrap_err();
        let cause = error.cause();
        assert_eq!(cause.kind, crate::failure::CauseKind::ObservationIncomplete);
        assert!(cause.details.reconstruction_issue.is_some());
        assert_eq!(cause.details.recovery_chunks.len(), 64);
        assert!(cause.details.recovery_chunks_truncated);
        let response = crate::failure::ExecutionProgress::default()
            .cause(cause)
            .response();
        assert_eq!(
            response["failure"]["primary"]["kind"],
            "observation_incomplete"
        );
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

    #[test]
    fn shared_conversion_keeps_fresh_evidence_and_checks_metadata_on_every_acquisition() {
        let original: SharedClientObservation = sample().into();
        let region = original.received.region;
        let mut cache = ConvertedSnapshots::default();
        let first = cache
            .convert(original.clone(), region, "minecraft:overworld")
            .unwrap();
        let mut later = original.clone();
        later.client_tick += 20;
        later.received.receive_sequence = Some(100);
        later.received.captured_at += std::time::Duration::from_secs(1);
        let second = cache
            .convert(later.clone(), region, "minecraft:overworld")
            .unwrap();
        assert!(Arc::ptr_eq(&first.snapshot, &second.snapshot));
        let contents = crate::snapshot_content::SnapshotContents::default();
        let first = crate::observation_evidence::FreshRegion::client(first, &contents)
            .unwrap()
            .into_shared_record();
        let second = crate::observation_evidence::FreshRegion::client(second, &contents)
            .unwrap()
            .into_shared_record();
        assert!(first.snapshot.shares_storage_with(&second.snapshot));
        assert_ne!(
            serde_json::to_value(first.observation_id).unwrap(),
            serde_json::to_value(second.observation_id).unwrap()
        );
        assert_eq!(
            second
                .readback
                .interval_since(&first.readback)
                .unwrap()
                .client_ticks,
            Some(20)
        );
        for case in 0..6 {
            let mut invalid = later.clone();
            match case {
                0 => invalid.issue = Some(ReconstructionIssue::Limit),
                1 => invalid.recovery_chunks.push([0, 0]),
                2 => invalid.dimension = "minecraft:the_nether".into(),
                3 => invalid.received.version = MinecraftVersion::Java1_16_1,
                4 => invalid.received.receive_sequence = None,
                _ => invalid.received.region.max[0] += 1,
            }
            assert!(
                cache
                    .convert(invalid, region, "minecraft:overworld")
                    .is_err()
            );
        }
        let mut changed = later;
        Arc::make_mut(&mut changed.blocks)[0]
            .state
            .as_mut()
            .unwrap()
            .properties
            .insert("shape".into(), "straight".into());
        let changed = cache
            .convert(changed, region, "minecraft:overworld")
            .unwrap();
        assert_eq!(changed.snapshot.blocks[0].properties["shape"], "straight");
        assert_ne!(
            contents.intern_arc(changed.snapshot).unwrap().id(),
            first.snapshot.id()
        );
    }

    #[test]
    fn conversion_cache_rejects_unavailable_replaced_cells_and_reclaims_dead_sources() {
        let original: SharedClientObservation = sample().into();
        let region = original.received.region;
        let mut cache = ConvertedSnapshots::default();
        cache
            .convert(original.clone(), region, "minecraft:overworld")
            .unwrap();
        let mut invalid = original.clone();
        Arc::make_mut(&mut invalid.blocks)[0].state = None;
        assert!(
            cache
                .convert(invalid, region, "minecraft:overworld")
                .is_err()
        );
        drop(original);
        cache
            .convert(sample(), region, "minecraft:overworld")
            .unwrap();
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.cells, 1);
    }

    #[test]
    fn equal_contents_with_new_origin_or_carrier_keep_the_new_evidence() {
        use voxrig::versions::java_1_21_11::reconstruction::{
            CarrierRole, Direction, MotionProgress, MovingBlock,
        };
        let original: SharedClientObservation = sample().into();
        let region = original.received.region;
        let mut cache = ConvertedSnapshots::default();
        let first = cache
            .convert(original.clone(), region, "minecraft:overworld")
            .unwrap();
        let mut changed_origin = original.clone();
        Arc::make_mut(&mut changed_origin.blocks)[0].origin = StateOrigin::Received;
        let changed_origin = cache
            .convert(changed_origin, region, "minecraft:overworld")
            .unwrap();
        assert!(Arc::ptr_eq(&first.snapshot, &changed_origin.snapshot));
        assert!(matches!(
            changed_origin.observation.blocks[0].origin,
            StateOrigin::Received
        ));
        let mut moving = original;
        let cell = &mut Arc::make_mut(&mut moving.blocks)[0];
        cell.moving = Some(MovingBlock {
            position: cell.position,
            carried: cell.state.clone().unwrap(),
            direction: Direction::East,
            extending: true,
            role: CarrierRole::Payload,
            progress: MotionProgress::Start,
            last_progress: MotionProgress::Start,
            completion_waits: 0,
            action_sequence: Some(5),
            chunk_sequence: None,
        });
        let moving = cache
            .convert(moving, region, "minecraft:overworld")
            .unwrap();
        assert!(Arc::ptr_eq(&first.snapshot, &moving.snapshot));
        assert!(moving.is_moving());
        assert!(
            crate::observation_evidence::FreshRegion::client(moving, &cache.contents)
                .unwrap()
                .into_stationary_record()
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "offline conversion and content-sharing comparison; no network or writes"]
    async fn profile_shared_native_conversion() {
        for volume in [720usize, 4096] {
            for sample_number in 1..=3 {
                let region = if volume == 720 {
                    Region {
                        min: [0, 80, 0],
                        max: [11, 85, 9],
                    }
                } else {
                    Region {
                        min: [0, 80, 0],
                        max: [15, 95, 15],
                    }
                };
                let mut observation = sample();
                observation.received.region = region;
                observation.blocks.clear();
                observation.received.blocks.clear();
                for x in region.min[0]..=region.max[0] {
                    for y in region.min[1]..=region.max[1] {
                        for z in region.min[2]..=region.max[2] {
                            let position = [x, y, z];
                            let state = NativeBlockState {
                                name: "minecraft:air".into(),
                                properties: Default::default(),
                            };
                            observation.blocks.push(ClientBlock {
                                position,
                                state: Some(state.clone()),
                                origin: StateOrigin::Received,
                                moving: None,
                            });
                            observation.received.blocks.push(ObservedBlock {
                                position,
                                state: Some(state),
                            });
                        }
                    }
                }
                let observation: SharedClientObservation = observation.into();
                let started = std::time::Instant::now();
                for _ in 0..100 {
                    std::hint::black_box(
                        ClientRegion::from_observation(
                            observation.clone(),
                            region,
                            "minecraft:overworld",
                        )
                        .unwrap(),
                    );
                }
                let previous_ms = started.elapsed().as_secs_f64() * 1000.0;
                let mut cache = ConvertedSnapshots::default();
                let contents = cache.contents.clone();
                let mut first_ms = 0.0;
                let (records, measurement) =
                    crate::performance::measure("shared_conversion", async {
                        let mut records = Vec::new();
                        for n in 0..100 {
                            let started = std::time::Instant::now();
                            let mut fresh = observation.clone();
                            fresh.client_tick += n;
                            fresh.received.captured_at += std::time::Duration::from_millis(n * 50);
                            let record =
                                cache.convert(fresh, region, "minecraft:overworld").unwrap();
                            records.push(
                                crate::observation_evidence::FreshRegion::client(record, &contents)
                                    .unwrap()
                                    .into_shared_record(),
                            );
                            if n == 0 {
                                first_ms = started.elapsed().as_secs_f64() * 1000.0;
                            }
                        }
                        records
                    })
                    .await;
                assert!(
                    records
                        .iter()
                        .all(|r| r.snapshot.shares_storage_with(&records[0].snapshot))
                );
                let conversion = &measurement.phases["native_convert"];
                assert_eq!(conversion.materialized_cells, volume as u64);
                assert_eq!(conversion.cache_hits, 99);
                println!(
                    "CONVERSION_SHARING {}",
                    serde_json::json!({"sample":sample_number,"cells":volume,"acquisitions":100,"previous_ms":previous_ms,"shared_ms":measurement.elapsed_ms,"first_shared_ms":first_ms,"shared_materialized_cells":conversion.materialized_cells,"cache_hits":conversion.cache_hits})
                );
            }
        }
    }
}
