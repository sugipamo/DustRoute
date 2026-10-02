//! Ordered 1.21.11 receive loop. Local state is committed under one session lock.
mod observations;
/// Explicit Java 1.21.11 operation API and received player state.
pub mod operations;
pub mod players;
pub mod raycast;
pub mod recording;
use super::{
    ids,
    reconstruction::{Action, ClientObservation, Direction, Reconstruction},
    wire::Reader,
    world::{Dimension, World},
};
use crate::protocol::{put_string, put_varint, read_packet, write_packet};
use crate::{
    ConnectionConfig, Error, ErrorKind, MinecraftVersion, Observation, ObservedBlock, Region,
    Result,
};
use anyhow::{Context, bail};
use std::{
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
    time::Instant,
};
use tokio::{
    io::AsyncWriteExt,
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Mutex, Notify},
    time::timeout,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Configuration,
    Play,
}

/// One decompressed server packet in exact receive order.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PacketRecord {
    /// Connection-local receive ordinal.
    pub sequence: u64,
    /// Local frame at application, for replaying moving blocks; not a server tick.
    pub client_tick: u64,
    /// Protocol state in which the packet was received.
    pub phase: &'static str,
    /// Native packet identifier.
    pub packet_id: i32,
    /// Payload before parsing or state application.
    pub payload: Vec<u8>,
}

/// Bounded diagnostic evidence; overflow is explicit.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PacketTrace {
    /// Native version.
    pub minecraft_version: &'static str,
    /// Identity within this client process.
    pub connection_id: u64,
    /// Sequence at the start of capture.
    pub after_sequence: u64,
    /// Sequence at the end of capture.
    pub through_sequence: u64,
    /// False if the configured byte/record bound was reached.
    pub complete: bool,
    /// Exact payloads, including packets unrelated to blocks.
    pub records: Vec<PacketRecord>,
}

struct TraceCapture {
    start: u64,
    bytes: usize,
    limit: usize,
    complete: bool,
    records: Vec<PacketRecord>,
}

impl TraceCapture {
    fn record(&mut self, sequence: u64, client_tick: u64, phase: Phase, id: i32, payload: &[u8]) {
        if !self.complete {
            return;
        }
        if self.bytes.saturating_add(payload.len()) > self.limit || self.records.len() >= 65_536 {
            self.complete = false;
            return;
        }
        self.bytes += payload.len();
        self.records.push(PacketRecord {
            sequence,
            client_tick,
            phase: match phase {
                Phase::Configuration => "configuration",
                Phase::Play => "play",
            },
            packet_id: id,
            payload: payload.to_vec(),
        });
    }
}

struct State {
    recording: Option<recording::Capture>,
    recording_ordinal: u64,
    operations: operations::OperationState,
    players: players::PlayerTracker,
    phase: Phase,
    world: World,
    reconstruction: Reconstruction,
    observations: observations::RegionCache,
    dimensions: Vec<Dimension>,
    position: Option<[f64; 3]>,
    rotation: [f32; 2],
    ready: bool,
    sequence: u64,
    failure: Option<Error>,
    trace: Option<TraceCapture>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            recording: None,
            recording_ordinal: 0,
            operations: operations::OperationState::default(),
            players: players::PlayerTracker::default(),
            phase: Phase::Configuration,
            world: World::default(),
            reconstruction: Reconstruction::default(),
            observations: observations::RegionCache::default(),
            dimensions: Vec::new(),
            position: None,
            rotation: [0.0; 2],
            ready: false,
            sequence: 0,
            failure: None,
            trace: None,
        }
    }
}
impl State {
    // Receive, trace and apply share a single boundary for live operation and replay.
    fn receive(&mut self, id: i32, payload: &[u8], max_chunks: usize) -> anyhow::Result<Responses> {
        if self.failure.is_some() {
            bail!("cannot apply packets after a failed receive");
        }
        self.sequence += 1;
        if let Some(trace) = &mut self.trace {
            trace.record(
                self.sequence,
                self.reconstruction.tick,
                self.phase,
                id,
                payload,
            );
        }
        let result = match self.phase {
            Phase::Configuration => apply_configuration(self, id, payload),
            Phase::Play => apply_play(self, id, payload, max_chunks),
        };
        if let Err(error) = &result {
            self.failure = Some(Error::new(
                ErrorKind::Protocol,
                anyhow::anyhow!("{error:#}"),
            ));
            self.ready = false;
        }
        result
    }
}
struct Writer {
    stream: OwnedWriteHalf,
    compression: Option<i32>,
}
struct Session {
    id: u64,
    started: Instant,
    writer: Mutex<Writer>,
    state: Mutex<State>,
    changed: Notify,
    cancel: Notify,
    stopped: AtomicBool,
    limits: crate::ConnectionOptions,
    interaction_sequence: AtomicI32,
}
struct Lease(Weak<Session>);
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(session) = self.0.upgrade() {
            session.stopped.store(true, Ordering::Release);
            session.cancel.notify_one();
            session.changed.notify_waiters();
        }
    }
}

#[derive(Clone)]
pub(crate) struct Bot {
    session: Arc<Session>,
    _lease: Arc<Lease>,
}

impl Bot {
    pub fn operations(&self) -> operations::Operations {
        operations::Operations { bot: self.clone() }
    }
    pub async fn start_packet_trace(&self, maximum_bytes: usize) -> Result<()> {
        if !(1..=16_777_216).contains(&maximum_bytes) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                anyhow::anyhow!("trace limit must be 1..=16777216"),
            ));
        }
        let mut state = self.session.state.lock().await;
        self.session.check(&state)?;
        if state.trace.is_some() {
            return Err(Error::new(
                ErrorKind::State,
                anyhow::anyhow!("trace already active"),
            ));
        }
        state.trace = Some(TraceCapture {
            start: state.sequence,
            bytes: 0,
            limit: maximum_bytes,
            complete: true,
            records: Vec::new(),
        });
        Ok(())
    }

    pub async fn stop_packet_trace(&self) -> Result<PacketTrace> {
        let mut state = self.session.state.lock().await;
        let trace = state.trace.take().context("no active packet trace")?;
        Ok(PacketTrace {
            minecraft_version: "1.21.11",
            connection_id: self.session.id,
            after_sequence: trace.start,
            through_sequence: state.sequence,
            complete: trace.complete,
            records: trace.records,
        })
    }

    pub async fn interact_block(&self, position: [i32; 3], face: crate::BlockFace) -> Result<()> {
        self.operations()
            .use_on_block(position, face, [0.5; 3])
            .await?;
        Ok(())
    }

    pub async fn connect(config: ConnectionConfig) -> Result<Self> {
        if config.username.is_empty()
            || config.username.len() > 16
            || !config
                .username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                anyhow::anyhow!("invalid offline player name"),
            ));
        }
        let stream = timeout(
            config.limits.connect_timeout,
            TcpStream::connect((config.server.host.as_str(), config.server.port)),
        )
        .await
        .context("connect timed out")??;
        let (mut reader, mut writer) = stream.into_split();
        let mut handshake = Vec::new();
        put_varint(&mut handshake, 774);
        put_string(&mut handshake, &config.server.host);
        handshake.extend(config.server.port.to_be_bytes());
        put_varint(&mut handshake, 2);
        write_packet(&mut writer, None, 0, &handshake).await?;
        let mut login = Vec::new();
        put_string(&mut login, &config.username);
        // Offline servers derive the authenticated profile UUID from the name.
        login.extend([0u8; 16]);
        write_packet(
            &mut writer,
            None,
            ids::login_serverbound::LOGIN_START,
            &login,
        )
        .await?;
        let mut compression = None;
        loop {
            let (id, payload) = timeout(
                config.limits.login_packet_timeout,
                read_packet(&mut reader, compression),
            )
            .await
            .context("login timed out")??;
            let mut r = Reader::new(&payload);
            match id {
                ids::login_clientbound::SUCCESS => {
                    r.take(16)?;
                    let received = r.string()?;
                    if received != config.username {
                        return Err(Error::new(
                            ErrorKind::Protocol,
                            anyhow::anyhow!("login profile name mismatch"),
                        ));
                    }
                    for _ in 0..r.count(1024)? {
                        r.string()?;
                        r.string()?;
                        if r.bool()? {
                            r.string()?;
                        }
                    }
                    r.end()?;
                    write_packet(
                        &mut writer,
                        compression,
                        ids::login_serverbound::LOGIN_ACKNOWLEDGED,
                        &[],
                    )
                    .await?;
                    break;
                }
                ids::login_clientbound::COMPRESS => {
                    let threshold = r.varint()?;
                    r.end()?;
                    if threshold < 0 {
                        return Err(Error::new(
                            ErrorKind::Protocol,
                            anyhow::anyhow!("negative compression threshold"),
                        ));
                    }
                    compression = Some(threshold);
                }
                ids::login_clientbound::ENCRYPTION_BEGIN => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        anyhow::anyhow!("online-mode authentication is unsupported"),
                    ));
                }
                ids::login_clientbound::DISCONNECT => {
                    return Err(Error::new(
                        ErrorKind::Disconnected,
                        anyhow::anyhow!("server rejected login"),
                    ));
                }
                ids::login_clientbound::COOKIE_REQUEST => {
                    let key = r.string()?;
                    r.end()?;
                    let mut response = Vec::new();
                    put_string(&mut response, &key);
                    response.push(0);
                    write_packet(
                        &mut writer,
                        compression,
                        ids::login_serverbound::COOKIE_RESPONSE,
                        &response,
                    )
                    .await?;
                }
                ids::login_clientbound::LOGIN_PLUGIN_REQUEST => {
                    let request = r.varint()?;
                    let mut response = Vec::new();
                    put_varint(&mut response, request);
                    response.push(0);
                    write_packet(
                        &mut writer,
                        compression,
                        ids::login_serverbound::LOGIN_PLUGIN_RESPONSE,
                        &response,
                    )
                    .await?;
                }
                _ => {
                    return Err(Error::new(
                        ErrorKind::Protocol,
                        anyhow::anyhow!("unexpected login packet {id}"),
                    ));
                }
            }
        }
        let session = Arc::new(Session {
            id: crate::connection::next_connection_id(),
            started: Instant::now(),
            writer: Mutex::new(Writer {
                stream: writer,
                compression,
            }),
            state: Mutex::new(State::default()),
            changed: Notify::new(),
            cancel: Notify::new(),
            stopped: AtomicBool::new(false),
            limits: config.limits,
            interaction_sequence: AtomicI32::new(0),
        });
        session
            .send(ids::configuration_serverbound::SETTINGS, &settings())
            .await?;
        let bot = Self {
            _lease: Arc::new(Lease(Arc::downgrade(&session))),
            session: session.clone(),
        };
        tokio::spawn(async move {
            let result = session.receive_loop(reader).await;
            if let Err(error) = result {
                let mut state = session.state.lock().await;
                if state.failure.is_none() {
                    state.failure = Some(Error::from(error));
                }
            }
            session.stopped.store(true, Ordering::Release);
            session.changed.notify_waiters();
            let _ = session.writer.lock().await.stream.shutdown().await;
        });
        Ok(bot)
    }

    pub async fn wait_until_ready(&self) -> Result<()> {
        timeout(self.session.limits.ready_timeout, async {
            loop {
                let notified = self.session.changed.notified();
                {
                    let state = self.session.state.lock().await;
                    self.session.check(&state)?;
                    if state.ready {
                        return Ok(());
                    }
                }
                notified.await;
            }
        })
        .await
        .context("ready timed out")?
    }

    pub async fn observe_region(&self, region: Region) -> Result<Observation> {
        let state = self.session.state.lock().await;
        self.observation(&state, region)
    }

    pub async fn observe_client_region(&self, region: Region) -> Result<ClientObservation> {
        Ok(self
            .observe_shared_client_region(region)
            .await?
            .observation
            .into_owned())
    }

    /// Same-generation acquisitions share cell arrays under the session lock.
    /// The lock also orders packet application and local moving-block advancement.
    pub async fn observe_shared_client_region(
        &self,
        region: Region,
    ) -> Result<super::reconstruction::SharedClientRegion> {
        let mut state = self.session.state.lock().await;
        self.session.check(&state)?;
        state.shared_observation(region, self.session.id, self.session.started.elapsed())
    }

    fn observation(&self, state: &State, region: Region) -> Result<Observation> {
        let volume = region.volume()?;
        self.session.check(state)?;
        let dimension = state
            .world
            .dimension
            .as_ref()
            .context("dimension is not available")?
            .1;
        if region.min[1] < dimension.min_y || region.max[1] >= dimension.min_y + dimension.height {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                anyhow::anyhow!("observation outside dimension"),
            ));
        }
        let mut blocks = Vec::with_capacity(volume);
        for x in region.min[0]..=region.max[0] {
            for y in region.min[1]..=region.max[1] {
                for z in region.min[2]..=region.max[2] {
                    let position = [x, y, z];
                    let native = state
                        .world
                        .block(position)
                        .map(super::native_state)
                        .transpose()?;
                    blocks.push(ObservedBlock {
                        position,
                        state: native,
                    });
                }
            }
        }
        Ok(Observation {
            version: MinecraftVersion::Java1_21_11,
            connection_id: self.session.id,
            revision: state.world.revision,
            receive_sequence: Some(state.sequence),
            captured_at: self.session.started.elapsed(),
            region,
            blocks,
        })
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.session.stopped.store(true, Ordering::Release);
        self.session.cancel.notify_one();
        self.session.changed.notify_waiters();
        self.session.writer.lock().await.stream.shutdown().await?;
        Ok(())
    }
}

impl Session {
    fn check(&self, state: &State) -> Result<()> {
        if let Some(error) = &state.failure {
            return Err(Error::new(error.kind(), anyhow::anyhow!("{error}")));
        }
        if self.stopped.load(Ordering::Acquire) {
            return Err(Error::new(
                ErrorKind::Disconnected,
                anyhow::anyhow!("connection closed"),
            ));
        }
        Ok(())
    }
    async fn send(&self, id: i32, payload: &[u8]) -> anyhow::Result<()> {
        let mut writer = self.writer.lock().await;
        let compression = writer.compression;
        write_packet(&mut writer.stream, compression, id, payload).await
    }
    async fn receive_loop(&self, mut reader: OwnedReadHalf) -> anyhow::Result<()> {
        let compression = self.writer.lock().await.compression;
        while !self.stopped.load(Ordering::Acquire) {
            let packet = tokio::select! {
                biased;
                _=self.cancel.notified()=>return Ok(()),
                packet=timeout(self.limits.play_packet_timeout,read_packet(&mut reader,compression))=>packet.context("play packet timed out")??,
            };
            let responses = {
                let mut state = self.state.lock().await;
                let target = self.started.elapsed().as_millis() as u64 / 50;
                let State {
                    world,
                    reconstruction,
                    ..
                } = &mut *state;
                reconstruction.advance(world, target);
                state.receive(packet.0, &packet.1, self.limits.max_chunks)?
            };
            for (id, payload) in responses {
                self.send(id, &payload).await?;
            }
            self.changed.notify_waiters();
        }
        Ok(())
    }
}

fn settings() -> Vec<u8> {
    let mut payload = Vec::new();
    put_string(&mut payload, "en_us");
    payload.push(4);
    put_varint(&mut payload, 0);
    payload.extend([1, 127]);
    put_varint(&mut payload, 1);
    payload.extend([0, 0]);
    put_varint(&mut payload, 2);
    payload
}

type Responses = Vec<(i32, Vec<u8>)>;

fn apply_configuration(state: &mut State, id: i32, payload: &[u8]) -> anyhow::Result<Responses> {
    use ids::{configuration_clientbound as input, configuration_serverbound as output};
    let mut r = Reader::new(payload);
    let mut responses = Vec::new();
    match id {
        input::FINISH_CONFIGURATION => {
            r.end()?;
            if state.dimensions.is_empty() {
                bail!("configuration supplied no dimension registry");
            }
            state.phase = Phase::Play;
            responses.push((output::FINISH_CONFIGURATION, Vec::new()));
        }
        input::KEEP_ALIVE => {
            r.take(8)?;
            r.end()?;
            responses.push((output::KEEP_ALIVE, payload.to_vec()));
        }
        input::PING => {
            r.take(4)?;
            r.end()?;
            responses.push((output::PONG, payload.to_vec()));
        }
        input::SELECT_KNOWN_PACKS => {
            for _ in 0..r.count(1024)? {
                r.string()?;
                r.string()?;
                r.string()?;
            }
            r.end()?;
            // Ask for complete registry entries rather than assuming a local known pack.
            responses.push((output::SELECT_KNOWN_PACKS, vec![0]));
        }
        input::REGISTRY_DATA => {
            let name = r.string()?;
            let count = r.count(65_536)?;
            let mut dimensions = Vec::new();
            for _ in 0..count {
                r.string()?;
                let present = r.bool()?;
                if name == "minecraft:dimension_type" {
                    if !present {
                        bail!("dimension data omitted despite empty known-packs response");
                    }
                    let (min_y, height) = r.dimension_nbt()?;
                    dimensions.push(Dimension::new(
                        min_y.context("missing min_y")?,
                        height.context("missing height")?,
                    )?);
                } else if present {
                    r.skip_nbt()?;
                }
            }
            r.end()?;
            if name == "minecraft:dimension_type" {
                state.dimensions = dimensions;
            }
        }
        input::COOKIE_REQUEST => {
            let name = r.string()?;
            r.end()?;
            let mut response = Vec::new();
            put_string(&mut response, &name);
            response.push(0);
            responses.push((output::COOKIE_RESPONSE, response));
        }
        input::DISCONNECT => bail!("server disconnected during configuration"),
        input::TRANSFER => bail!("server transfer requires a new explicitly configured connection"),
        input::CODE_OF_CONDUCT => bail!("server requires code-of-conduct acceptance"),
        input::ADD_RESOURCE_PACK => bail!("resource-pack negotiation is unsupported"),
        input::FEATURE_FLAGS => {
            let mut features = Vec::new();
            for _ in 0..r.count(1024)? {
                let name = r.string()?;
                if name != "minecraft:vanilla" {
                    bail!("unsupported experimental feature set");
                }
                features.push(name);
            }
            r.end()?;
            state.operations.features = Some(features);
        }
        // Presentation, tags and cookie storage do not change block-state IDs.
        input::CUSTOM_PAYLOAD
        | input::RESET_CHAT
        | input::REMOVE_RESOURCE_PACK
        | input::STORE_COOKIE
        | input::TAGS
        | input::CUSTOM_REPORT_DETAILS
        | input::SERVER_LINKS
        | input::CLEAR_DIALOG
        | input::SHOW_DIALOG => {}
        _ => bail!("unknown configuration packet {id}"),
    }
    Ok(responses)
}

fn spawn_info(state: &mut State, r: &mut Reader<'_>) -> anyhow::Result<()> {
    let id = r.count(1024)?;
    let name = r.string()?;
    r.take(8)?;
    let game_mode = r.u8()?;
    r.u8()?;
    r.bool()?;
    r.bool()?;
    if r.bool()? {
        r.string()?;
        r.take(8)?;
    }
    r.varint()?;
    r.varint()?;
    let dimension = *state
        .dimensions
        .get(id)
        .context("unknown dimension registry ID")?;
    state.world.select_dimension(name, dimension);
    if let Some(capture) = &mut state.recording {
        capture.invalidate(recording::RecordingIssue::WorldChanged);
    }
    state.reconstruction = Reconstruction::default();
    state.operations.reset_world(game_mode)?;
    state.players.reset_world();
    state.ready = false;
    state.position = None;
    Ok(())
}

fn apply_play(
    state: &mut State,
    id: i32,
    payload: &[u8],
    max_chunks: usize,
) -> anyhow::Result<Responses> {
    use ids::{play_clientbound as input, play_serverbound as output};
    let mut r = Reader::new(payload);
    let mut responses = Vec::new();
    if operations::receive(state, id, payload)? {
        return Ok(responses);
    }
    if state.players.receive(id, payload, state.sequence)? {
        return Ok(responses);
    }
    match id {
        input::LOGIN => {
            let entity_id = r.i32()?;
            if entity_id < 0 {
                bail!("invalid own entity identifier");
            }
            r.bool()?;
            for _ in 0..r.count(1024)? {
                r.string()?;
            }
            r.varint()?;
            r.varint()?;
            r.varint()?;
            r.bool()?;
            r.bool()?;
            r.bool()?;
            spawn_info(state, &mut r)?;
            r.bool()?;
            r.end()?;
            state.operations.local_player = operations::LocalPlayerState::spawned(entity_id);
        }
        input::RESPAWN => {
            spawn_info(state, &mut r)?;
            r.u8()?;
            r.end()?;
        }
        input::KEEP_ALIVE => {
            r.take(8)?;
            r.end()?;
            responses.push((output::KEEP_ALIVE, payload.to_vec()));
        }
        input::PING => {
            r.take(4)?;
            r.end()?;
            responses.push((output::PONG, payload.to_vec()));
        }
        input::POSITION => {
            let teleport = r.varint()?;
            let mut position = [r.f64()?, r.f64()?, r.f64()?];
            let delta = [r.f64()?, r.f64()?, r.f64()?];
            let mut rotation = [r.f32()?, r.f32()?];
            let flags = r.u32()?;
            r.end()?;
            if flags & !511 != 0 {
                bail!("unknown position flags");
            }
            for (axis, value) in position.iter_mut().enumerate() {
                if flags & (1 << axis) != 0 {
                    *value += state
                        .position
                        .context("relative position without baseline")?[axis];
                }
                if !value.is_finite() || value.abs() > 30_000_000.0 {
                    bail!("position outside world bounds");
                }
            }
            if flags & 8 != 0 {
                rotation[0] += state.rotation[0];
            }
            if flags & 16 != 0 {
                rotation[1] += state.rotation[1];
            }
            if rotation.iter().any(|v| !v.is_finite()) {
                bail!("non-finite relative rotation");
            }
            rotation[1] = rotation[1].clamp(-90.0, 90.0);
            state
                .operations
                .local_player
                .correct_velocity(delta, flags, state.sequence)?;
            state.position = Some(position);
            state.operations.position_from_server = true;
            state.rotation = rotation;
            state.ready = true;
            let mut confirm = Vec::new();
            put_varint(&mut confirm, teleport);
            responses.push((output::TELEPORT_CONFIRM, confirm));
            let mut moved = Vec::new();
            for value in position {
                moved.extend(value.to_be_bytes());
            }
            for value in rotation {
                moved.extend(value.to_be_bytes());
            }
            moved.push(0);
            responses.push((output::POSITION_LOOK, moved));
        }
        input::MAP_CHUNK => {
            let chunk = [r.i32()?, r.i32()?];
            let pistons = state.world.load(payload, max_chunks)?;
            if let Some(capture) = &mut state.recording {
                capture.chunk_changed(chunk);
            }
            state
                .reconstruction
                .chunk_loaded(chunk, pistons, state.sequence);
        }
        input::BLOCK_CHANGE => {
            let changes = state.world.block_change(payload)?;
            if let Some(capture) = &mut state.recording {
                capture.received(&changes, state.sequence, state.reconstruction.tick)?;
            }
            state.reconstruction.received(&changes);
        }
        input::MULTI_BLOCK_CHANGE => {
            let changes = state.world.section_changes(payload)?;
            if let Some(capture) = &mut state.recording {
                capture.received(&changes, state.sequence, state.reconstruction.tick)?;
            }
            state.reconstruction.received(&changes);
        }
        input::UNLOAD_CHUNK => {
            let z = r.i32()?;
            let x = r.i32()?;
            state.world.unload(payload)?;
            if let Some(capture) = &mut state.recording {
                capture.chunk_changed([x, z]);
            }
            state.reconstruction.chunk_replaced([x, z]);
        }
        input::BLOCK_ACTION => {
            let p = super::wire::unpack_position(r.u64()?);
            let action = r.u8()?;
            let parameter = r.u8()?;
            let block_id = r.varint()?;
            r.end()?;
            let name = super::registry()
                .block_name(block_id)
                .context("unknown block action ID")?;
            if matches!(name, "piston" | "sticky_piston") {
                let action = Action::from_id(action).context("unknown piston action")?;
                let facing =
                    Direction::from_id(parameter & 7).context("invalid piston direction")?;
                state.reconstruction.action(
                    &state.world,
                    p,
                    action,
                    facing,
                    &format!("minecraft:{name}"),
                    state.sequence,
                );
            }
        }
        input::SET_TICKING_STATE => {
            let rate = r.f32()?;
            let frozen = r.bool()?;
            r.end()?;
            if rate != 20.0 || frozen {
                state.reconstruction.unsupported_ticking();
            }
        }
        input::STEP_TICK => {
            let steps = r.count(1_000_000)?;
            r.end()?;
            if steps != 0 {
                state.reconstruction.unsupported_ticking();
            }
        }
        input::CHUNK_BATCH_FINISHED => {
            r.count(65_536)?;
            r.end()?;
            responses.push((output::CHUNK_BATCH_RECEIVED, 8f32.to_be_bytes().to_vec()));
        }
        input::START_CONFIGURATION => {
            r.end()?;
            state.phase = Phase::Configuration;
            state.ready = false;
            state.position = None;
            state.dimensions.clear();
            state.world.reset();
            state.reconstruction = Reconstruction::default();
            state.operations.reset_configuration(state.sequence);
            state.players = players::PlayerTracker::default();
            if let Some(capture) = &mut state.recording {
                capture.invalidate(recording::RecordingIssue::WorldChanged);
            }
            responses.push((output::CONFIGURATION_ACKNOWLEDGED, vec![]));
            responses.push((ids::configuration_serverbound::SETTINGS, settings()));
        }
        input::KICK_DISCONNECT => bail!("server disconnected"),
        input::TRANSFER => bail!("server transfer requires a new explicitly configured connection"),
        input::ADD_RESOURCE_PACK => bail!("resource-pack negotiation is unsupported"),
        input::COOKIE_REQUEST => {
            let key = r.string()?;
            r.end()?;
            let mut response = Vec::new();
            put_string(&mut response, &key);
            response.push(0);
            responses.push((output::COOKIE_RESPONSE, response));
        }
        // Other known packets currently expose no advertised observation/action capability.
        id if ids::KNOWN_PLAY_CLIENTBOUND.contains(&id) => {}
        _ => bail!("unknown Java 1.21.11 play packet {id}"),
    }
    Ok(responses)
}

#[cfg(test)]
mod mining_native_trials;
#[cfg(test)]
mod tests;
