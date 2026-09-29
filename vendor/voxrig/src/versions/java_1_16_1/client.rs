//! Connection lifecycle, protocol events, observations, and player operations.

use crate::versions::java_1_16_1::Result;
use crate::versions::java_1_16_1::{
    chat::{ChatMessage, PlayerList, apply_player_info, parse_chat},
    entity::{
        EntityState, EntityTracker, apply_relative, parse_metadata, parse_spawn_living,
        parse_spawn_object, parse_spawn_orb, parse_spawn_painting, parse_spawn_player,
    },
    interaction::{
        BlockBreakProgress, BlockFace, BlockPos, DiggingAcknowledgement, DiggingStatus, Hand,
        parse_break_progress, parse_digging_ack,
    },
    inventory::{
        ClickMode, EquipmentSlot, InventoryState, ItemCollected, ItemStack, OpenWindow,
        PendingClick, SlotUpdate, WindowProperty, WindowTransaction, apply_slot,
        parse_merchant_offers, parse_set_slot, parse_window_items, predict_normal_click, read_slot,
        rollback_click, sync_player_inventory_from_window, write_slot,
    },
    map::{MapData, MapStore, MapUpdate, parse_map_update},
    physics::{
        Aabb, ControlState, MotionState, PhysicsMetrics, PhysicsTracker, PositionCorrection, Vec3,
        VehicleControl, VehiclePose,
    },
    progress::{AdvancementState, RecipeBookState, StatisticsState},
    protocol::*,
    server_registry::{
        CommandTree, ServerRecipes, ServerTags, parse_command_tree, parse_recipes, parse_tags,
    },
    snapshot::{Snapshot, Versioned},
    survival::{
        CombatEvent, Difficulty, Experience, GameStateChange, RespawnState, SurvivalState, Vitals,
        parse_attributes, parse_combat_event, parse_effect, parse_experience, parse_join,
        parse_respawn, parse_vitals, unpack_position,
    },
    ui::{UiState, UiUpdateKind},
    world::{BlockObservation, Fluid, World},
};
use anyhow::Context;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize, Ordering},
    },
};
use tokio::{
    net::{TcpStream, tcp::OwnedWriteHalf},
    sync::{Mutex, Notify, RwLock, broadcast},
    time::{Duration, timeout},
};

macro_rules! bail {
    ($($argument:tt)*) => {
        return Err(crate::versions::java_1_16_1::Error::from(anyhow::anyhow!($($argument)*)).into())
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `Server`.
pub struct Server {
    /// The `host` value.
    pub host: String,
    /// The `port` value.
    pub port: u16,
}
impl Server {
    /// Performs the `new` operation.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }
}
impl Default for Server {
    fn default() -> Self {
        Self::new("127.0.0.1", 25565)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
/// State and protocol data represented by `ConnectionOptions`.
pub struct ConnectionOptions {
    /// Maximum time allowed to establish the TCP connection.
    pub connect_timeout: Duration,
    /// Maximum delay allowed between packets during login.
    pub login_packet_timeout: Duration,
    /// Maximum delay allowed between packets after login.
    pub play_packet_timeout: Duration,
    /// Default deadline used while waiting for the initial playable state.
    pub ready_timeout: Duration,
    /// Maximum chunks retained by one connection.
    pub max_chunks: usize,
    /// Maximum entities retained by one connection.
    pub max_entities: usize,
    /// Maximum map IDs retained by one connection.
    pub max_maps: usize,
    /// Aggregate cardinality limit for smaller state caches.
    pub max_cached_records: usize,
    /// Maximum retained bytes for one custom payload event.
    pub max_custom_payload_bytes: usize,
    /// Number of events retained for each Bot broadcast receiver.
    pub event_channel_capacity: usize,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            login_packet_timeout: Duration::from_secs(15),
            play_packet_timeout: Duration::from_secs(60),
            ready_timeout: Duration::from_secs(15),
            max_chunks: 256,
            max_entities: 4_096,
            max_maps: 128,
            max_cached_records: 4_096,
            max_custom_payload_bytes: 65_536,
            event_channel_capacity: 256,
        }
    }
}

/// Fixed wire-protocol identity of this crate build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolInfo {
    /// The `minecraft_version` value.
    pub minecraft_version: &'static str,
    /// The `protocol_version` value.
    pub protocol_version: i32,
}

/// Feature boundary advertised to embedding controllers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClientCapabilities {
    /// The `offline_mode` value.
    pub offline_mode: bool,
    /// The `online_mode_authentication` value.
    pub online_mode_authentication: bool,
    /// The `multi_bot` value.
    pub multi_bot: bool,
    /// The `movement_physics` value.
    pub movement_physics: bool,
    /// The `world_observation` value.
    pub world_observation: bool,
    /// The `inventory_and_windows` value.
    pub inventory_and_windows: bool,
    /// The `crafting` value.
    pub crafting: bool,
    /// The `entity_interaction` value.
    pub entity_interaction: bool,
    /// The `structured_sound_events` value.
    pub structured_sound_events: bool,
    /// The `audio_playback` value.
    pub audio_playback: bool,
    /// The `raycast_and_reach_queries` value.
    pub raycast_and_reach_queries: bool,
    /// The `low_copy_chunk_snapshots` value.
    pub low_copy_chunk_snapshots: bool,
    /// The `shared_chunk_sections` value.
    pub shared_chunk_sections: bool,
    /// The `structured_map_data` value.
    pub structured_map_data: bool,
    /// The `vehicle_input` value.
    pub vehicle_input: bool,
}

/// Build and protocol information for compatibility checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientInfo {
    /// The `crate_version` value.
    pub crate_version: &'static str,
    /// The `protocol` value.
    pub protocol: ProtocolInfo,
    /// The `capabilities` value.
    pub capabilities: ClientCapabilities,
}

/// Public constant `CLIENT_INFO`.
pub const CLIENT_INFO: ClientInfo = ClientInfo {
    crate_version: env!("CARGO_PKG_VERSION"),
    protocol: ProtocolInfo {
        minecraft_version: "1.16.1",
        protocol_version: PROTOCOL_VERSION,
    },
    capabilities: ClientCapabilities {
        offline_mode: true,
        online_mode_authentication: false,
        multi_bot: true,
        movement_physics: true,
        world_observation: true,
        inventory_and_windows: true,
        crafting: true,
        entity_interaction: true,
        structured_sound_events: true,
        audio_playback: false,
        raycast_and_reach_queries: true,
        low_copy_chunk_snapshots: true,
        shared_chunk_sections: true,
        structured_map_data: true,
        vehicle_input: true,
    },
};

/// Offline identity and live state. `username` is used by the login packet.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    /// The `username` value.
    pub username: String,
    /// The `entity_id` value.
    pub entity_id: Option<i32>,
    /// The `x` value.
    pub x: f64,
    /// The `y` value.
    pub y: f64,
    /// The `z` value.
    pub z: f64,
    /// The `yaw` value.
    pub yaw: f32,
    /// The `pitch` value.
    pub pitch: f32,
    /// The `on_ground` value.
    pub on_ground: bool,
    /// The `spawned` value.
    pub spawned: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
/// Possible values represented by `ChatMode`.
pub enum ChatMode {
    /// Documentation for this public variant.
    Enabled = 0,
    /// Documentation for this public variant.
    CommandsOnly = 1,
    /// Documentation for this public variant.
    Hidden = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
/// Possible values represented by `MainHand`.
pub enum MainHand {
    /// Documentation for this public variant.
    Left = 0,
    /// Documentation for this public variant.
    Right = 1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `ClientSettings`.
pub struct ClientSettings {
    /// The `locale` value.
    pub locale: String,
    /// The `view_distance` value.
    pub view_distance: i8,
    /// The `chat_mode` value.
    pub chat_mode: ChatMode,
    /// The `chat_colors` value.
    pub chat_colors: bool,
    /// The `skin_parts` value.
    pub skin_parts: u8,
    /// The `main_hand` value.
    pub main_hand: MainHand,
    /// The `brand` value.
    pub brand: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `ResourcePackRequest`.
pub struct ResourcePackRequest {
    /// The `url` value.
    pub url: String,
    /// The `hash` value.
    pub hash: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
/// Possible values represented by `ResourcePackStatus`.
pub enum ResourcePackStatus {
    /// Documentation for this public variant.
    SuccessfullyLoaded = 0,
    /// Documentation for this public variant.
    Declined = 1,
    /// Documentation for this public variant.
    FailedDownload = 2,
    /// Documentation for this public variant.
    Accepted = 3,
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `TabCompletionMatch`.
pub struct TabCompletionMatch {
    /// The `value` value.
    pub value: String,
    /// The `tooltip_json` value.
    pub tooltip_json: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `TabCompletion`.
pub struct TabCompletion {
    /// The `transaction_id` value.
    pub transaction_id: i32,
    /// The `start` value.
    pub start: i32,
    /// The `length` value.
    pub length: i32,
    /// The `matches` value.
    pub matches: Vec<TabCompletionMatch>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// State and protocol data represented by `WorldEvent`.
pub struct WorldEvent {
    /// The `effect_id` value.
    pub effect_id: i32,
    /// The `position` value.
    pub position: BlockPos,
    /// The `data` value.
    pub data: i32,
    /// The `global` value.
    pub global: bool,
}
#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `WorldParticleEvent`.
pub struct WorldParticleEvent {
    /// The `particle_id` value.
    pub particle_id: i32,
    /// The `long_distance` value.
    pub long_distance: bool,
    /// The `position` value.
    pub position: Vec3,
    /// The `offset` value.
    pub offset: [f32; 3],
    /// The `speed` value.
    pub speed: f32,
    /// The `count` value.
    pub count: i32,
    /// The `data` value.
    pub data: crate::versions::java_1_16_1::ParticleData,
    /// The `raw_data` value.
    pub raw_data: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `ExplosionEvent`.
pub struct ExplosionEvent {
    /// The `position` value.
    pub position: Vec3,
    /// The `radius` value.
    pub radius: f32,
    /// The `affected_blocks` value.
    pub affected_blocks: Vec<BlockPos>,
    /// The `player_motion` value.
    pub player_motion: Vec3,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `WorldViewState`.
pub struct WorldViewState {
    /// The `center_x` value.
    pub center_x: i32,
    /// The `center_z` value.
    pub center_z: i32,
    /// The `distance` value.
    pub distance: i32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// State and protocol data represented by `BlockActionEvent`.
pub struct BlockActionEvent {
    /// The `position` value.
    pub position: BlockPos,
    /// The `action` value.
    pub action: u8,
    /// The `parameter` value.
    pub parameter: u8,
    /// The `block_id` value.
    pub block_id: i32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `StopSoundEvent`.
pub struct StopSoundEvent {
    /// The `category` value.
    pub category: Option<i32>,
    /// The `sound_name` value.
    pub sound_name: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// State and protocol data represented by `NbtQueryResponse`.
pub struct NbtQueryResponse {
    /// The `transaction_id` value.
    pub transaction_id: i32,
    /// The `nbt` value.
    pub nbt: Arc<[u8]>,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            locale: "en_us".into(),
            view_distance: 10,
            chat_mode: ChatMode::Enabled,
            chat_colors: true,
            skin_parts: 0x7f,
            main_hand: MainHand::Right,
            brand: "Voxrig".into(),
        }
    }
}
impl Player {
    /// Performs the `offline` operation.
    pub fn offline(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            entity_id: None,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            spawned: false,
        }
    }
    pub(crate) fn validate(&self) -> Result<()> {
        let valid = (3..=16).contains(&self.username.len())
            && self
                .username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if !valid {
            bail!("username must be 3-16 ASCII letters, digits, or underscores");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Possible values represented by `SoundSource`.
pub enum SoundSource {
    /// The `Position` variant.
    Position {
        /// World X coordinate.
        x: f64,
        /// World Y coordinate.
        y: f64,
        /// World Z coordinate.
        z: f64,
    },
    /// The `Entity` variant.
    Entity {
        /// Protocol entity ID producing the sound.
        entity_id: i32,
    },
}
#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `SoundEvent`.
pub struct SoundEvent {
    /// The `sound_id` value.
    pub sound_id: Option<i32>,
    /// The `sound_name` value.
    pub sound_name: Option<String>,
    /// The `category` value.
    pub category: i32,
    /// The `category_name` value.
    pub category_name: Option<&'static str>,
    /// The `source` value.
    pub source: SoundSource,
    /// The `volume` value.
    pub volume: f32,
    /// The `pitch` value.
    pub pitch: f32,
    /// The `sequence` value.
    pub sequence: u64,
    /// The `received_at` value.
    pub received_at: Duration,
}
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
/// Possible values represented by `Event`.
pub enum Event {
    /// The `Login` variant.
    Login,
    /// The `Spawn` variant.
    Spawn,
    /// Documentation for this public variant.
    Position(Player),
    /// The `ChunkLoaded` variant.
    ChunkLoaded {
        /// The `x` value carried by this variant.
        x: i32,
        /// The `z` value carried by this variant.
        z: i32,
    },
    /// The `ChunkUnloaded` variant.
    ChunkUnloaded {
        /// The `x` value carried by this variant.
        x: i32,
        /// The `z` value carried by this variant.
        z: i32,
    },
    /// The `ChunkLightUpdated` variant.
    ChunkLightUpdated {
        /// The `x` value carried by this variant.
        x: i32,
        /// The `z` value carried by this variant.
        z: i32,
    },
    /// The `BlockChanged` variant.
    BlockChanged {
        /// The `x` value carried by this variant.
        x: i32,
        /// The `y` value carried by this variant.
        y: i32,
        /// The `z` value carried by this variant.
        z: i32,
        /// The `state_id` value carried by this variant.
        state_id: i32,
    },
    /// Documentation for this public variant.
    BlockEntityUpdated(crate::versions::java_1_16_1::BlockEntityData),
    /// The `MultiBlockChanged` variant.
    MultiBlockChanged {
        /// The `count` value carried by this variant.
        count: usize,
    },
    /// Documentation for this public variant.
    Sound(SoundEvent),
    /// Documentation for this public variant.
    Vitals(Vitals),
    /// Documentation for this public variant.
    Experience(Experience),
    /// Documentation for this public variant.
    Difficulty(Difficulty),
    /// Documentation for this public variant.
    GameStateChange(GameStateChange),
    /// Documentation for this public variant.
    SpawnPosition(crate::versions::java_1_16_1::SpawnPosition),
    /// The `EntityStatus` variant.
    EntityStatus {
        /// The `entity_id` value carried by this variant.
        entity_id: i32,
        /// The `status` value carried by this variant.
        status: i8,
    },
    /// Documentation for this public variant.
    Combat(CombatEvent),
    /// Documentation for this public variant.
    Respawn(RespawnState),
    /// The `SurvivalStateUpdated` variant.
    SurvivalStateUpdated,
    /// The `InventoryUpdated` variant.
    InventoryUpdated {
        /// The `window_id` value carried by this variant.
        window_id: i8,
    },
    /// Documentation for this public variant.
    SlotUpdated(SlotUpdate),
    /// The `HeldItemChanged` variant.
    HeldItemChanged {
        /// The `slot` value carried by this variant.
        slot: u8,
    },
    /// Documentation for this public variant.
    ItemCollected(ItemCollected),
    /// Documentation for this public variant.
    DiggingAcknowledged(DiggingAcknowledgement),
    /// Documentation for this public variant.
    BlockBreakProgress(BlockBreakProgress),
    /// Documentation for this public variant.
    WindowOpened(OpenWindow),
    /// The `BookOpened` variant.
    BookOpened {
        /// The `hand` value carried by this variant.
        hand: Hand,
    },
    /// The `SignEditorOpened` variant.
    SignEditorOpened {
        /// The `position` value carried by this variant.
        position: BlockPos,
    },
    /// The `WindowClosed` variant.
    WindowClosed {
        /// The `window_id` value carried by this variant.
        window_id: i8,
    },
    /// Documentation for this public variant.
    WindowProperty(WindowProperty),
    /// Documentation for this public variant.
    WindowTransaction(WindowTransaction),
    /// Documentation for this public variant.
    MerchantOffers(crate::versions::java_1_16_1::MerchantOffers),
    /// Documentation for this public variant.
    EntitySpawned(EntityState),
    /// Documentation for this public variant.
    EntityUpdated(EntityState),
    /// The `EntitiesDestroyed` variant.
    EntitiesDestroyed {
        /// The `entity_ids` value carried by this variant.
        entity_ids: Vec<i32>,
    },
    /// The `EntityAnimation` variant.
    EntityAnimation {
        /// The `entity_id` value carried by this variant.
        entity_id: i32,
        /// The `animation` value carried by this variant.
        animation: u8,
    },
    /// The `PassengersUpdated` variant.
    PassengersUpdated {
        /// The `vehicle_id` value carried by this variant.
        vehicle_id: i32,
        /// The `passengers` value carried by this variant.
        passengers: Vec<i32>,
    },
    /// Documentation for this public variant.
    VehiclePosition(VehiclePose),
    /// The `ItemCooldown` variant.
    ItemCooldown {
        /// The `item_id` value carried by this variant.
        item_id: i32,
        /// The `ticks` value carried by this variant.
        ticks: i32,
    },
    /// Documentation for this public variant.
    Chat(ChatMessage),
    /// The `CustomPayload` variant.
    CustomPayload {
        /// The `channel` value carried by this variant.
        channel: String,
        /// The `data` value carried by this variant.
        data: Arc<[u8]>,
    },
    /// Documentation for this public variant.
    ServerBrand(String),
    /// Documentation for this public variant.
    MapUpdated(MapUpdate),
    /// Documentation for this public variant.
    UiStateUpdated(UiUpdateKind),
    /// Documentation for this public variant.
    ResourcePackRequested(ResourcePackRequest),
    /// Documentation for this public variant.
    TabCompletion(TabCompletion),
    /// Documentation for this public variant.
    WorldEvent(WorldEvent),
    /// Documentation for this public variant.
    Particle(WorldParticleEvent),
    /// Documentation for this public variant.
    Explosion(ExplosionEvent),
    /// Documentation for this public variant.
    WorldViewUpdated(WorldViewState),
    /// Documentation for this public variant.
    BlockAction(BlockActionEvent),
    /// The `CraftRecipeResponse` variant.
    CraftRecipeResponse {
        /// The `window_id` value carried by this variant.
        window_id: i8,
        /// The `recipe_id` value carried by this variant.
        recipe_id: String,
    },
    /// The `CameraChanged` variant.
    CameraChanged {
        /// The `entity_id` value carried by this variant.
        entity_id: i32,
    },
    /// The `EntityAttached` variant.
    EntityAttached {
        /// The `entity_id` value carried by this variant.
        entity_id: i32,
        /// The `attached_to` value carried by this variant.
        attached_to: Option<i32>,
    },
    /// Documentation for this public variant.
    StopSound(StopSoundEvent),
    /// Documentation for this public variant.
    NbtQueryResponse(NbtQueryResponse),
    /// The `CommandTreeUpdated` variant.
    CommandTreeUpdated,
    /// The `TagsUpdated` variant.
    TagsUpdated,
    /// The `RecipesDeclared` variant.
    RecipesDeclared,
    /// The `RecipeBookUpdated` variant.
    RecipeBookUpdated,
    /// The `StatisticsUpdated` variant.
    StatisticsUpdated,
    /// The `AdvancementsUpdated` variant.
    AdvancementsUpdated,
    /// The `PlayerListUpdated` variant.
    PlayerListUpdated {
        /// The `action` value carried by this variant.
        action: i32,
        /// The `uuids` value carried by this variant.
        uuids: Vec<[u8; 16]>,
    },
    /// The `Disconnected` variant.
    Disconnected {
        /// The `reason` value carried by this variant.
        reason: String,
    },
    /// The `Error` variant.
    Error {
        /// The `kind` value carried by this variant.
        kind: &'static str,
        /// The `message` value carried by this variant.
        message: String,
    },
    /// Documentation for this public variant.
    PositionCorrection(PositionCorrection),
}

struct Writer {
    inner: OwnedWriteHalf,
    compression: Option<i32>,
}

/// State and protocol data represented by `Bot`.
pub struct Bot {
    connection_id: u64,
    server: Server,
    writer: Arc<Mutex<Writer>>,
    player: Arc<Mutex<Versioned<Player>>>,
    world: Arc<Mutex<Versioned<World>>>,
    positioned: Arc<Mutex<bool>>,
    ready: Arc<Notify>,
    world_updated: Arc<Notify>,
    events: broadcast::Sender<Event>,
    physics: Arc<Mutex<PhysicsTracker>>,
    control: Arc<RwLock<Versioned<ControlState>>>,
    stopped: Arc<AtomicBool>,
    terminal_emitted: Arc<AtomicBool>,
    jump_requested: Arc<AtomicBool>,
    teleport_barrier_ticks: Arc<AtomicU8>,
    motion: Arc<Mutex<Versioned<MotionState>>>,
    survival: Arc<RwLock<Versioned<SurvivalState>>>,
    inventory: Arc<RwLock<Versioned<InventoryState>>>,
    click_lock: Arc<Mutex<()>>,
    entities: Arc<RwLock<Versioned<EntityTracker>>>,
    last_attack: Arc<Mutex<Option<std::time::Instant>>>,
    players: Arc<RwLock<Versioned<PlayerList>>>,
    client_settings: Arc<RwLock<ClientSettings>>,
    server_brand: Arc<RwLock<Versioned<Option<String>>>>,
    maps: Arc<RwLock<Versioned<MapStore>>>,
    ui: Arc<RwLock<Versioned<UiState>>>,
    sound_sequence: Arc<std::sync::atomic::AtomicU64>,
    connected_at: std::time::Instant,
    connection_options: ConnectionOptions,
    next_tab_completion: Arc<AtomicI32>,
    resource_pack: Arc<RwLock<Versioned<Option<ResourcePackRequest>>>>,
    recipe_book: Arc<RwLock<Versioned<RecipeBookState>>>,
    statistics: Arc<RwLock<Versioned<StatisticsState>>>,
    advancements: Arc<RwLock<Versioned<AdvancementState>>>,
    world_view: Arc<RwLock<Versioned<WorldViewState>>>,
    camera_entity_id: Arc<RwLock<Versioned<Option<i32>>>>,
    next_nbt_query: Arc<AtomicI32>,
    command_tree: Arc<RwLock<Versioned<Option<CommandTree>>>>,
    tags: Arc<RwLock<Versioned<ServerTags>>>,
    server_recipes: Arc<RwLock<Versioned<ServerRecipes>>>,
    cancel: Arc<Notify>,
    external_handles: Arc<AtomicUsize>,
    counts_as_external_handle: bool,
}

impl Clone for Bot {
    fn clone(&self) -> Self {
        self.clone_with_handle(true)
    }
}

impl Drop for Bot {
    fn drop(&mut self) {
        if self.counts_as_external_handle
            && self.external_handles.fetch_sub(1, Ordering::AcqRel) == 1
        {
            self.stopped.store(true, Ordering::Release);
            self.cancel.notify_waiters();
            self.ready.notify_waiters();
            self.world_updated.notify_waiters();
        }
    }
}

impl Bot {
    fn clone_with_handle(&self, counts_as_external_handle: bool) -> Self {
        if counts_as_external_handle {
            self.external_handles.fetch_add(1, Ordering::Relaxed);
        }
        Self {
            connection_id: self.connection_id,
            server: self.server.clone(),
            writer: self.writer.clone(),
            player: self.player.clone(),
            world: self.world.clone(),
            positioned: self.positioned.clone(),
            ready: self.ready.clone(),
            world_updated: self.world_updated.clone(),
            events: self.events.clone(),
            physics: self.physics.clone(),
            control: self.control.clone(),
            stopped: self.stopped.clone(),
            terminal_emitted: self.terminal_emitted.clone(),
            jump_requested: self.jump_requested.clone(),
            teleport_barrier_ticks: self.teleport_barrier_ticks.clone(),
            motion: self.motion.clone(),
            survival: self.survival.clone(),
            inventory: self.inventory.clone(),
            click_lock: self.click_lock.clone(),
            entities: self.entities.clone(),
            last_attack: self.last_attack.clone(),
            players: self.players.clone(),
            client_settings: self.client_settings.clone(),
            server_brand: self.server_brand.clone(),
            maps: self.maps.clone(),
            ui: self.ui.clone(),
            sound_sequence: self.sound_sequence.clone(),
            connected_at: self.connected_at,
            connection_options: self.connection_options,
            next_tab_completion: self.next_tab_completion.clone(),
            resource_pack: self.resource_pack.clone(),
            recipe_book: self.recipe_book.clone(),
            statistics: self.statistics.clone(),
            advancements: self.advancements.clone(),
            world_view: self.world_view.clone(),
            camera_entity_id: self.camera_entity_id.clone(),
            next_nbt_query: self.next_nbt_query.clone(),
            command_tree: self.command_tree.clone(),
            tags: self.tags.clone(),
            server_recipes: self.server_recipes.clone(),
            cancel: self.cancel.clone(),
            external_handles: self.external_handles.clone(),
            counts_as_external_handle,
        }
    }

    fn clone_internal(&self) -> Self {
        self.clone_with_handle(false)
    }

    pub(crate) async fn connect(
        server: Server,
        player: Player,
        chunk_storage: Arc<crate::versions::java_1_16_1::SharedChunkStorage>,
        connection_options: ConnectionOptions,
    ) -> Result<Self> {
        player.validate()?;
        let stream = timeout(
            connection_options.connect_timeout,
            TcpStream::connect((&*server.host, server.port)),
        )
        .await
        .context("connect timed out")?
        .context("connect failed")?;
        let (mut reader, writer) = stream.into_split();
        let writer = Arc::new(Mutex::new(Writer {
            inner: writer,
            compression: None,
        }));
        let mut handshake = Vec::new();
        put_varint(&mut handshake, PROTOCOL_VERSION);
        put_string(&mut handshake, &server.host);
        handshake.extend(server.port.to_be_bytes());
        put_varint(&mut handshake, 2);
        write_packet(&mut writer.lock().await.inner, None, 0, &handshake).await?;
        let mut login = Vec::new();
        put_string(&mut login, &player.username);
        write_packet(&mut writer.lock().await.inner, None, 0, &login).await?;
        loop {
            let compression = writer.lock().await.compression;
            let (id, payload) = timeout(
                connection_options.login_packet_timeout,
                read_packet(&mut reader, compression),
            )
            .await
            .context("login packet timed out")??;
            match id {
                0x00 => {
                    let mut p = payload.as_slice();
                    bail!("login rejected: {}", get_string(&mut p).unwrap_or_default());
                }
                0x01 => bail!("server requested encryption; only offline-mode is supported"),
                0x02 => break,
                0x03 => {
                    let mut p = payload.as_slice();
                    let threshold = get_varint(&mut p)?;
                    if threshold < 0 {
                        bail!("server sent negative compression threshold {threshold}");
                    }
                    writer.lock().await.compression = Some(threshold);
                }
                _ => {}
            }
        }
        let (events, _) = broadcast::channel(connection_options.event_channel_capacity.max(1));
        let connected_at = std::time::Instant::now();
        let bot = Self {
            connection_id: crate::connection::next_connection_id(),
            server,
            writer,
            player: Arc::new(Mutex::new(Versioned::new(player, connected_at))),
            world: Arc::new(Mutex::new(Versioned::new(
                World::with_storage(chunk_storage),
                connected_at,
            ))),
            positioned: Arc::new(Mutex::new(false)),
            ready: Arc::new(Notify::new()),
            world_updated: Arc::new(Notify::new()),
            events,
            physics: Arc::new(Mutex::new(PhysicsTracker::new())),
            control: Arc::new(RwLock::new(Versioned::new(
                ControlState::default(),
                connected_at,
            ))),
            stopped: Arc::new(AtomicBool::new(false)),
            terminal_emitted: Arc::new(AtomicBool::new(false)),
            jump_requested: Arc::new(AtomicBool::new(false)),
            teleport_barrier_ticks: Arc::new(AtomicU8::new(0)),
            motion: Arc::new(Mutex::new(Versioned::new(
                MotionState::default(),
                connected_at,
            ))),
            survival: Arc::new(RwLock::new(Versioned::new(
                SurvivalState::default(),
                connected_at,
            ))),
            inventory: Arc::new(RwLock::new(Versioned::new(
                InventoryState::default(),
                connected_at,
            ))),
            click_lock: Arc::new(Mutex::new(())),
            entities: Arc::new(RwLock::new(Versioned::new(
                EntityTracker::default(),
                connected_at,
            ))),
            last_attack: Arc::new(Mutex::new(None)),
            players: Arc::new(RwLock::new(Versioned::new(
                PlayerList::default(),
                connected_at,
            ))),
            client_settings: Arc::new(RwLock::new(ClientSettings::default())),
            server_brand: Arc::new(RwLock::new(Versioned::new(None, connected_at))),
            maps: Arc::new(RwLock::new(Versioned::new(
                MapStore::default(),
                connected_at,
            ))),
            ui: Arc::new(RwLock::new(Versioned::new(
                UiState::default(),
                connected_at,
            ))),
            sound_sequence: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            connected_at,
            connection_options,
            next_tab_completion: Arc::new(AtomicI32::new(1)),
            resource_pack: Arc::new(RwLock::new(Versioned::new(None, connected_at))),
            recipe_book: Arc::new(RwLock::new(Versioned::new(
                RecipeBookState::default(),
                connected_at,
            ))),
            statistics: Arc::new(RwLock::new(Versioned::new(
                StatisticsState::default(),
                connected_at,
            ))),
            advancements: Arc::new(RwLock::new(Versioned::new(
                AdvancementState::default(),
                connected_at,
            ))),
            world_view: Arc::new(RwLock::new(Versioned::new(
                WorldViewState::default(),
                connected_at,
            ))),
            camera_entity_id: Arc::new(RwLock::new(Versioned::new(None, connected_at))),
            next_nbt_query: Arc::new(AtomicI32::new(1)),
            command_tree: Arc::new(RwLock::new(Versioned::new(None, connected_at))),
            tags: Arc::new(RwLock::new(Versioned::new(
                ServerTags::default(),
                connected_at,
            ))),
            server_recipes: Arc::new(RwLock::new(Versioned::new(
                ServerRecipes::default(),
                connected_at,
            ))),
            cancel: Arc::new(Notify::new()),
            external_handles: Arc::new(AtomicUsize::new(1)),
            counts_as_external_handle: true,
        };
        bot.emit(Event::Login);
        let background = bot.clone_internal();
        let reader_task = tokio::spawn(async move { background.read_loop(reader).await });
        let supervisor = bot.clone_internal();
        tokio::spawn(async move {
            match reader_task.await {
                Ok(Ok(())) => supervisor.emit(Event::Disconnected {
                    reason: "connection closed".to_owned(),
                }),
                Ok(Err(error)) => supervisor.emit(Event::Error {
                    kind: "connection",
                    message: error.to_string(),
                }),
                Err(error) => supervisor.emit(Event::Error {
                    kind: "connection",
                    message: format!("reader task terminated unexpectedly: {error}"),
                }),
            }
            supervisor.physics.lock().await.record_disconnect();
            supervisor.stopped.store(true, Ordering::Release);
        });
        let physics = bot.clone_internal();
        tokio::spawn(async move { physics.control_loop().await });
        Ok(bot)
    }
    /// Performs the `subscribe` operation.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
    pub(crate) const fn connection_id(&self) -> u64 {
        self.connection_id
    }
    pub(crate) fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
    /// Public constant `fn`.
    pub const fn client_info() -> ClientInfo {
        CLIENT_INFO
    }
    /// Public constant `fn`.
    pub const fn protocol_info() -> ProtocolInfo {
        CLIENT_INFO.protocol
    }
    /// Public constant `fn`.
    pub const fn capabilities() -> ClientCapabilities {
        CLIENT_INFO.capabilities
    }
    /// Performs the `server_info` operation.
    pub fn server_info(&self) -> &Server {
        &self.server
    }
    /// Performs the `client_settings` operation.
    pub async fn client_settings(&self) -> ClientSettings {
        self.client_settings.read().await.clone()
    }
    /// Performs the `server_brand` operation.
    pub async fn server_brand(&self) -> Option<String> {
        self.server_brand.read().await.clone()
    }
    /// Performs the `map` operation.
    pub async fn map(&self, id: i32) -> Option<MapData> {
        self.maps.read().await.maps.get(&id).cloned()
    }
    /// Performs the `maps_snapshot` operation.
    pub async fn maps_snapshot(&self) -> Snapshot<MapStore> {
        self.maps.read().await.snapshot()
    }
    /// Performs the `ui_state` operation.
    pub async fn ui_state(&self) -> UiState {
        self.ui_snapshot().await.value
    }
    /// Performs the `ui_snapshot` operation.
    pub async fn ui_snapshot(&self) -> Snapshot<UiState> {
        self.ui.read().await.snapshot()
    }
    /// Performs the `resource_pack_request` operation.
    pub async fn resource_pack_request(&self) -> Option<ResourcePackRequest> {
        self.resource_pack.read().await.clone()
    }
    /// Performs the `recipe_book_snapshot` operation.
    pub async fn recipe_book_snapshot(&self) -> Snapshot<RecipeBookState> {
        self.recipe_book.read().await.snapshot()
    }
    /// Performs the `statistics_snapshot` operation.
    pub async fn statistics_snapshot(&self) -> Snapshot<StatisticsState> {
        self.statistics.read().await.snapshot()
    }
    /// Performs the `advancements_snapshot` operation.
    pub async fn advancements_snapshot(&self) -> Snapshot<AdvancementState> {
        self.advancements.read().await.snapshot()
    }
    /// Performs the `world_view_snapshot` operation.
    pub async fn world_view_snapshot(&self) -> Snapshot<WorldViewState> {
        self.world_view.read().await.snapshot()
    }
    /// Performs the `camera_entity_id` operation.
    pub async fn camera_entity_id(&self) -> Option<i32> {
        **self.camera_entity_id.read().await
    }
    /// Performs the `command_tree_snapshot` operation.
    pub async fn command_tree_snapshot(&self) -> Snapshot<Option<CommandTree>> {
        self.command_tree.read().await.snapshot()
    }
    /// Performs the `tags_snapshot` operation.
    pub async fn tags_snapshot(&self) -> Snapshot<ServerTags> {
        self.tags.read().await.snapshot()
    }
    /// Performs the `server_recipes_snapshot` operation.
    pub async fn server_recipes_snapshot(&self) -> Snapshot<ServerRecipes> {
        self.server_recipes.read().await.snapshot()
    }
    /// Performs the `query_block_nbt` operation.
    pub async fn query_block_nbt(
        &self,
        position: BlockPos,
        wait: Duration,
    ) -> Result<NbtQueryResponse> {
        let transaction_id = self.next_nbt_query.fetch_add(1, Ordering::Relaxed);
        let mut events = self.subscribe();
        let mut payload = Vec::new();
        put_varint(&mut payload, transaction_id);
        payload.write_u64::<BigEndian>(position.packed())?;
        self.send(0x01, &payload).await?;
        wait_for_nbt(&mut events, transaction_id, wait).await
    }
    /// Performs the `query_entity_nbt` operation.
    pub async fn query_entity_nbt(
        &self,
        entity_id: i32,
        wait: Duration,
    ) -> Result<NbtQueryResponse> {
        let transaction_id = self.next_nbt_query.fetch_add(1, Ordering::Relaxed);
        let mut events = self.subscribe();
        let mut payload = Vec::new();
        put_varint(&mut payload, transaction_id);
        put_varint(&mut payload, entity_id);
        self.send(0x0d, &payload).await?;
        wait_for_nbt(&mut events, transaction_id, wait).await
    }
    /// Performs the `respond_resource_pack` operation.
    pub async fn respond_resource_pack(&self, status: ResourcePackStatus) -> Result<()> {
        let mut payload = Vec::new();
        put_varint(&mut payload, status as i32);
        self.send(0x20, &payload).await
    }

    /// Replaces and dispatches the standard Client Settings and brand payloads.
    pub async fn set_client_settings(&self, settings: ClientSettings) -> Result<()> {
        if settings.locale.is_empty() || settings.locale.len() > 16 {
            bail!("locale must contain 1..=16 bytes");
        }
        if !(2..=32).contains(&settings.view_distance) {
            bail!("view distance must be 2..=32 chunks");
        }
        let mut payload = Vec::new();
        put_string(&mut payload, &settings.locale);
        payload.write_i8(settings.view_distance)?;
        put_varint(&mut payload, settings.chat_mode as i32);
        payload.push(u8::from(settings.chat_colors));
        payload.push(settings.skin_parts);
        put_varint(&mut payload, settings.main_hand as i32);
        self.send(0x05, &payload).await?;
        let mut brand = Vec::new();
        put_string(&mut brand, "minecraft:brand");
        put_string(&mut brand, &settings.brand);
        self.send(0x0b, &brand).await?;
        *self.client_settings.write().await = settings;
        Ok(())
    }
    /// Performs the `username` operation.
    pub async fn username(&self) -> String {
        self.player.lock().await.username.clone()
    }
    /// Performs the `player` operation.
    pub async fn player(&self) -> Player {
        self.player_snapshot().await.value
    }
    /// Performs the `player_snapshot` operation.
    pub async fn player_snapshot(&self) -> Snapshot<Player> {
        self.player.lock().await.snapshot()
    }
    /// Performs the `physics_metrics` operation.
    pub async fn physics_metrics(&self) -> PhysicsMetrics {
        self.physics.lock().await.snapshot()
    }
    /// Performs the `motion` operation.
    pub async fn motion(&self) -> MotionState {
        self.motion_snapshot().await.value
    }
    /// Performs the `motion_snapshot` operation.
    pub async fn motion_snapshot(&self) -> Snapshot<MotionState> {
        self.motion.lock().await.snapshot()
    }
    /// Returns raw contact/environment facts used by client physics.
    pub async fn environment_state(&self) -> crate::versions::java_1_16_1::EnvironmentState {
        let player = self.player().await;
        self.world
            .lock()
            .await
            .environment_state(Aabb::player(player.x, player.y, player.z))
    }
    /// Performs the `survival_state` operation.
    pub async fn survival_state(&self) -> SurvivalState {
        self.survival_snapshot().await.value
    }
    /// Performs the `survival_snapshot` operation.
    pub async fn survival_snapshot(&self) -> Snapshot<SurvivalState> {
        self.survival.read().await.snapshot()
    }
    /// Performs the `inventory` operation.
    pub async fn inventory(&self) -> InventoryState {
        self.inventory_snapshot().await.value
    }
    /// Performs the `inventory_snapshot` operation.
    pub async fn inventory_snapshot(&self) -> Snapshot<InventoryState> {
        self.inventory.read().await.snapshot()
    }
    /// Performs the `open_window_state` operation.
    pub async fn open_window_state(&self) -> Option<OpenWindow> {
        self.open_window_snapshot().await.value
    }
    /// Performs the `open_window_snapshot` operation.
    pub async fn open_window_snapshot(&self) -> Snapshot<Option<OpenWindow>> {
        self.inventory
            .read()
            .await
            .map_snapshot(|inventory| inventory.open_window.clone())
    }
    /// Performs the `merchant_offers` operation.
    pub async fn merchant_offers(&self) -> Option<crate::versions::java_1_16_1::MerchantOffers> {
        self.inventory.read().await.merchant_offers.clone()
    }
    /// Performs the `entity` operation.
    pub async fn entity(&self, entity_id: i32) -> Option<EntityState> {
        self.entity_snapshot(entity_id).await.value
    }
    /// Performs the `entity_snapshot` operation.
    pub async fn entity_snapshot(&self, entity_id: i32) -> Snapshot<Option<EntityState>> {
        self.entities
            .read()
            .await
            .map_snapshot(|entities| entities.entities.get(&entity_id).cloned())
    }
    /// Returns every currently tracked entity.
    pub async fn entities(&self) -> Vec<EntityState> {
        self.entities_snapshot().await.value
    }
    /// Performs the `entities_snapshot` operation.
    pub async fn entities_snapshot(&self) -> Snapshot<Vec<EntityState>> {
        self.entities
            .read()
            .await
            .map_snapshot(|entities| entities.entities.values().cloned().collect())
    }
    /// Returns the tracked entity carrying this player, when passenger state is known.
    pub async fn vehicle(&self) -> Option<EntityState> {
        let player_id = self.player().await.entity_id?;
        self.entities
            .read()
            .await
            .entities
            .values()
            .find(|entity| entity.passengers.contains(&player_id))
            .cloned()
    }
    /// Performs the `vehicle_pose` operation.
    pub async fn vehicle_pose(&self) -> Option<VehiclePose> {
        self.vehicle().await.map(|vehicle| VehiclePose {
            position: vehicle.position,
            yaw: vehicle.yaw,
            pitch: vehicle.pitch,
        })
    }
    /// Performs the `observe_entities` operation.
    pub async fn observe_entities(&self, radius: f64) -> Vec<EntityState> {
        self.observe_entities_snapshot(radius).await.value
    }
    /// Performs the `observe_entities_snapshot` operation.
    pub async fn observe_entities_snapshot(&self, radius: f64) -> Snapshot<Vec<EntityState>> {
        let player = self.player().await;
        self.entities.read().await.map_snapshot(|entities| {
            entities.observe(
                Vec3 {
                    x: player.x,
                    y: player.y,
                    z: player.z,
                },
                radius,
            )
        })
    }
    /// Performs the `player_list` operation.
    pub async fn player_list(&self) -> PlayerList {
        self.player_list_snapshot().await.value
    }
    /// Performs the `player_list_snapshot` operation.
    pub async fn player_list_snapshot(&self) -> Snapshot<PlayerList> {
        self.players.read().await.snapshot()
    }
    /// Performs the `control` operation.
    pub async fn control(&self) -> ControlState {
        self.control_snapshot().await.value
    }
    /// Performs the `control_snapshot` operation.
    pub async fn control_snapshot(&self) -> Snapshot<ControlState> {
        self.control.read().await.snapshot()
    }
    /// Replaces the persistent input state consumed by the 20 Hz physics loop.
    ///
    /// Returning means the local state was updated; it does not wait for a
    /// movement packet or server acknowledgement. The state remains active
    /// until replaced or cleared.
    pub async fn set_control(&self, control: ControlState) {
        **self.control.write().await = control;
    }
    /// Clears every persistent control input locally.
    pub async fn clear_control(&self) {
        self.set_control(ControlState::default()).await;
    }
    /// Access protocol-adjacent operations with weaker compatibility guarantees.
    pub fn unstable(&self) -> crate::versions::java_1_16_1::UnstableBot<'_> {
        crate::versions::java_1_16_1::UnstableBot { bot: self }
    }
    /// Temporarily stops the 20 Hz movement producer without changing `ControlState`.
    ///
    /// This is useful when an external controller knows that a server teleport is imminent.
    /// The previous control state is applied again automatically after the suspension.
    pub fn suspend_movement_for(&self, duration: Duration) {
        let ticks = duration.as_millis().div_ceil(50).min(u128::from(u8::MAX)) as u8;
        self.teleport_barrier_ticks
            .fetch_max(ticks, Ordering::AcqRel);
    }
    /// Waits until login, spawn, and the initial authoritative position complete.
    pub async fn wait_until_ready(&self) -> Result<()> {
        loop {
            if self.stopped.load(Ordering::Acquire) {
                bail!("connection closed before initial position");
            }
            let notified = self.ready.notified();
            let spawned = self.player.lock().await.spawned;
            let positioned = *self.positioned.lock().await;
            if spawned && positioned {
                return Ok(());
            }
            timeout(self.connection_options.ready_timeout, notified)
                .await
                .context("timed out waiting for initial position")?;
        }
    }
    pub(crate) async fn move_relative_unchecked(&self, forward: f64, strafe: f64) -> Result<()> {
        self.wait_until_ready().await?;
        let mut p = self.player.lock().await;
        let yaw = (p.yaw as f64).to_radians();
        p.x += -yaw.sin() * forward + yaw.cos() * strafe;
        p.z += yaw.cos() * forward + yaw.sin() * strafe;
        drop(p);
        self.send_position().await
    }
    /// Updates yaw/pitch locally and dispatches one movement packet.
    ///
    /// Success guarantees packet dispatch, not server acceptance of the pose.
    pub async fn look(&self, yaw: f32, pitch: f32) -> Result<()> {
        if !yaw.is_finite() || !pitch.is_finite() {
            bail!("look angles must be finite");
        }
        self.wait_until_ready().await?;
        let mut p = self.player.lock().await;
        p.yaw = yaw;
        p.pitch = pitch.clamp(-90.0, 90.0);
        drop(p);
        self.send_position().await
    }
    /// Returns an owned cube of block observations around the current position.
    ///
    /// This waits for the player's current chunk, clamps `radius` to `0..=16`,
    /// and allocates a new `Vec`. Missing entries represent unavailable chunk
    /// data; air is represented by its normal block-state ID.
    pub async fn observe(&self, radius: i32) -> Result<Vec<BlockObservation>> {
        Ok(self.observe_snapshot(radius).await?.value)
    }
    /// Performs the `observe_snapshot` operation.
    pub async fn observe_snapshot(&self, radius: i32) -> Result<Snapshot<Vec<BlockObservation>>> {
        self.wait_until_ready().await?;
        let p = self.player().await;
        self.wait_for_player_chunk(&p).await?;
        Ok(self.world.lock().await.map_snapshot(|world| {
            world.observe_cube(
                p.x.floor() as i32,
                p.y.floor() as i32,
                p.z.floor() as i32,
                radius.clamp(0, 16),
            )
        }))
    }

    pub(crate) async fn observe_region_snapshot(
        &self,
        region: crate::Region,
    ) -> Result<Snapshot<Vec<BlockObservation>>> {
        let volume = region.volume()?;
        if self.is_stopped() {
            return Err(crate::Error::new(
                crate::ErrorKind::Disconnected,
                anyhow::anyhow!("connection closed before observation"),
            ));
        }
        Ok(self.world.lock().await.map_snapshot(|world| {
            let mut blocks = Vec::with_capacity(volume);
            for x in region.min[0]..=region.max[0] {
                for y in region.min[1]..=region.max[1] {
                    for z in region.min[2]..=region.max[2] {
                        blocks.push(BlockObservation {
                            x,
                            y,
                            z,
                            state_id: world.block(x, y, z),
                        });
                    }
                }
            }
            blocks
        }))
    }
    /// Returns a loaded block-state ID, or `None` when the chunk is unavailable.
    ///
    /// `None` never means air.
    pub async fn block(&self, x: i32, y: i32, z: i32) -> Option<i32> {
        self.block_snapshot(x, y, z).await.value
    }
    /// Performs the `block_snapshot` operation.
    pub async fn block_snapshot(&self, x: i32, y: i32, z: i32) -> Snapshot<Option<i32>> {
        self.world
            .lock()
            .await
            .map_snapshot(|world| world.block(x, y, z))
    }
    /// Performs the `loaded_chunks` operation.
    pub async fn loaded_chunks(&self) -> Vec<crate::versions::java_1_16_1::ChunkPos> {
        self.world.lock().await.loaded_chunks()
    }
    /// Performs the `query_blocks` operation.
    pub async fn query_blocks(
        &self,
        region: crate::versions::java_1_16_1::BlockRegion,
        state_ids: &[i32],
        limit: usize,
    ) -> Result<Vec<BlockPos>> {
        Ok(self
            .world
            .lock()
            .await
            .query_blocks(region, state_ids, limit)?)
    }

    /// Performs the `is_chunk_loaded` operation.
    pub async fn is_chunk_loaded(&self, position: crate::versions::java_1_16_1::ChunkPos) -> bool {
        self.world.lock().await.chunk_snapshot(position).is_some()
    }

    /// Returns a low-copy snapshot whose section and NBT buffers are Arc-backed.
    pub async fn chunk_snapshot(
        &self,
        position: crate::versions::java_1_16_1::ChunkPos,
    ) -> Option<crate::versions::java_1_16_1::ChunkSnapshot> {
        self.world.lock().await.chunk_snapshot(position)
    }

    /// Performs the `wait_for_chunk` operation.
    pub async fn wait_for_chunk(
        &self,
        position: crate::versions::java_1_16_1::ChunkPos,
        wait: Duration,
    ) -> Result<crate::versions::java_1_16_1::ChunkSnapshot> {
        Ok(timeout(wait, async {
            loop {
                let notified = self.world_updated.notified();
                if let Some(chunk) = self.chunk_snapshot(position).await {
                    return Ok::<_, anyhow::Error>(chunk);
                }
                if self.stopped.load(Ordering::Acquire) {
                    bail!("connection closed while waiting for chunk");
                }
                notified.await;
            }
        })
        .await
        .context("timed out waiting for chunk")??)
    }
    /// Performs the `wait_for_chunks` operation.
    pub async fn wait_for_chunks(
        &self,
        center: crate::versions::java_1_16_1::ChunkPos,
        radius: i32,
        wait: Duration,
    ) -> Result<Vec<crate::versions::java_1_16_1::ChunkSnapshot>> {
        if !(0..=32).contains(&radius) {
            bail!("chunk wait radius must be 0..=32");
        }
        let min_x = center
            .x
            .checked_sub(radius)
            .context("chunk wait x range underflow")?;
        let max_x = center
            .x
            .checked_add(radius)
            .context("chunk wait x range overflow")?;
        let min_z = center
            .z
            .checked_sub(radius)
            .context("chunk wait z range underflow")?;
        let max_z = center
            .z
            .checked_add(radius)
            .context("chunk wait z range overflow")?;
        Ok(timeout(wait, async {
            loop {
                let notified = self.world_updated.notified();
                let world = self.world.lock().await;
                let mut chunks = Vec::with_capacity(((radius * 2 + 1).pow(2)) as usize);
                let mut complete = true;
                for z in min_z..=max_z {
                    for x in min_x..=max_x {
                        if let Some(chunk) =
                            world.chunk_snapshot(crate::versions::java_1_16_1::ChunkPos { x, z })
                        {
                            chunks.push(chunk)
                        } else {
                            complete = false;
                        }
                    }
                }
                drop(world);
                if complete {
                    return Ok::<_, anyhow::Error>(chunks);
                }
                if self.stopped.load(Ordering::Acquire) {
                    bail!("connection closed while waiting for chunks")
                }
                notified.await
            }
        })
        .await
        .context("timed out waiting for chunks")??)
    }
    /// Waits until the world domain advances beyond `after_revision`, then returns this block.
    pub async fn wait_for_block_revision(
        &self,
        position: BlockPos,
        after_revision: u64,
        wait: Duration,
    ) -> Result<Snapshot<Option<i32>>> {
        Ok(timeout(wait, async {
            loop {
                let notified = self.world_updated.notified();
                let snapshot = self
                    .block_snapshot(position.x, position.y, position.z)
                    .await;
                if snapshot.revision > after_revision {
                    return Ok::<_, anyhow::Error>(snapshot);
                }
                if self.stopped.load(Ordering::Acquire) {
                    bail!("connection closed while waiting for block revision");
                }
                notified.await;
            }
        })
        .await
        .context("timed out waiting for block revision")??)
    }

    /// Waits for the exact block state. `None` means the chunk becoming unavailable.
    pub async fn wait_for_block_state(
        &self,
        position: BlockPos,
        expected: Option<i32>,
        wait: Duration,
    ) -> Result<Snapshot<Option<i32>>> {
        Ok(timeout(wait, async {
            loop {
                let notified = self.world_updated.notified();
                let snapshot = self
                    .block_snapshot(position.x, position.y, position.z)
                    .await;
                if snapshot.value == expected {
                    return Ok::<_, anyhow::Error>(snapshot);
                }
                if self.stopped.load(Ordering::Acquire) {
                    bail!("connection closed while waiting for block state");
                }
                notified.await;
            }
        })
        .await
        .context("timed out waiting for block state")??)
    }
    /// Performs the `wait_for_block_change` operation.
    pub async fn wait_for_block_change(
        &self,
        position: BlockPos,
        previous: Option<i32>,
        wait: Duration,
    ) -> Result<Snapshot<Option<i32>>> {
        Ok(timeout(wait, async {
            loop {
                let notified = self.world_updated.notified();
                let snapshot = self
                    .block_snapshot(position.x, position.y, position.z)
                    .await;
                if snapshot.value != previous {
                    return Ok::<_, anyhow::Error>(snapshot);
                }
                if self.stopped.load(Ordering::Acquire) {
                    bail!("connection closed while waiting for block change")
                }
                notified.await
            }
        })
        .await
        .context("timed out waiting for block change")??)
    }

    /// Waits until any inventory/window state advances beyond `after_revision`.
    pub async fn wait_for_inventory_revision(
        &self,
        after_revision: u64,
        wait: Duration,
    ) -> Result<Snapshot<InventoryState>> {
        let mut events = self.subscribe();
        Ok(timeout(wait, async {
            loop {
                let snapshot = self.inventory_snapshot().await;
                if snapshot.revision > after_revision {
                    return Ok::<_, anyhow::Error>(snapshot);
                }
                match events.recv().await {
                    Ok(Event::Disconnected { reason }) => bail!("disconnected: {reason}"),
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => bail!("event stream closed"),
                }
            }
        })
        .await
        .context("timed out waiting for inventory revision")??)
    }

    /// Waits until tracked entities advance beyond `after_revision`.
    pub async fn wait_for_entities_revision(
        &self,
        after_revision: u64,
        wait: Duration,
    ) -> Result<Snapshot<Vec<EntityState>>> {
        let mut events = self.subscribe();
        Ok(timeout(wait, async {
            loop {
                let snapshot = self.entities_snapshot().await;
                if snapshot.revision > after_revision {
                    return Ok::<_, anyhow::Error>(snapshot);
                }
                match events.recv().await {
                    Ok(Event::Disconnected { reason }) => bail!("disconnected: {reason}"),
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => bail!("event stream closed"),
                }
            }
        })
        .await
        .context("timed out waiting for entity revision")??)
    }
    /// Casts from the player's eyes using the supplied direction and real block shapes.
    pub async fn raycast_blocks(
        &self,
        direction: Vec3,
        max_distance: f64,
    ) -> Option<crate::versions::java_1_16_1::BlockRaycastHit> {
        let player = self.player().await;
        self.world.lock().await.raycast_blocks(
            Vec3 {
                x: player.x,
                y: player.y + 1.62,
                z: player.z,
            },
            direction,
            max_distance,
        )
    }

    /// Returns the block intersected by the current yaw and pitch.
    pub async fn targeted_block(
        &self,
        max_distance: f64,
    ) -> Option<crate::versions::java_1_16_1::BlockRaycastHit> {
        let player = self.player().await;
        let yaw = (player.yaw as f64).to_radians();
        let pitch = (player.pitch as f64).to_radians();
        self.raycast_blocks(
            Vec3 {
                x: -yaw.sin() * pitch.cos(),
                y: -pitch.sin(),
                z: yaw.cos() * pitch.cos(),
            },
            max_distance,
        )
        .await
    }

    /// Casts from the player's eyes and returns the closest entity bounding box hit.
    pub async fn raycast_entities(
        &self,
        direction: Vec3,
        max_distance: f64,
    ) -> Option<crate::versions::java_1_16_1::EntityRaycastHit> {
        let player = self.player().await;
        self.entities.read().await.raycast(
            Vec3 {
                x: player.x,
                y: player.y + 1.62,
                z: player.z,
            },
            direction,
            max_distance,
            player.entity_id,
        )
    }

    /// Returns the visible entity under the current crosshair, accounting for block occlusion.
    pub async fn targeted_entity(
        &self,
        max_distance: f64,
    ) -> Option<crate::versions::java_1_16_1::EntityRaycastHit> {
        let player = self.player().await;
        let yaw = (player.yaw as f64).to_radians();
        let pitch = (player.pitch as f64).to_radians();
        let direction = Vec3 {
            x: -yaw.sin() * pitch.cos(),
            y: -pitch.sin(),
            z: yaw.cos() * pitch.cos(),
        };
        let entity = self.raycast_entities(direction, max_distance).await?;
        let block = self.raycast_blocks(direction, max_distance).await;
        if block.is_some_and(|hit| hit.distance < entity.distance) {
            None
        } else {
            Some(entity)
        }
    }

    /// Performs the `can_reach_block` operation.
    pub async fn can_reach_block(&self, position: BlockPos) -> bool {
        self.digging_info(position).await.reachable
    }

    /// Performs the `can_see_block` operation.
    pub async fn can_see_block(&self, position: BlockPos) -> bool {
        self.digging_info(position).await.visible
    }

    /// Performs the `can_reach_entity` operation.
    pub async fn can_reach_entity(&self, entity_id: i32) -> bool {
        let Some(entity) = self.entity(entity_id).await else {
            return false;
        };
        let player = self.player().await;
        let dx = entity.position.x - player.x;
        let dy = entity.position.y - (player.y + 1.62);
        let dz = entity.position.z - player.z;
        (dx * dx + dy * dy + dz * dz).sqrt() <= 3.0
    }

    /// Performs the `can_see_entity` operation.
    pub async fn can_see_entity(&self, entity_id: i32) -> bool {
        let Some(entity) = self.entity(entity_id).await else {
            return false;
        };
        let player = self.player().await;
        let direction = Vec3 {
            x: entity.position.x - player.x,
            y: entity.position.y + 0.5 - (player.y + 1.62),
            z: entity.position.z - player.z,
        };
        let distance =
            (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z)
                .sqrt();
        let Some(hit) = self.raycast_entities(direction, distance + 1.0).await else {
            return false;
        };
        if hit.entity.entity_id != entity_id {
            return false;
        }
        !self
            .raycast_blocks(direction, hit.distance)
            .await
            .is_some_and(|block| block.distance < hit.distance)
    }

    /// Performs the `digging_info` operation.
    pub async fn digging_info(
        &self,
        position: BlockPos,
    ) -> crate::versions::java_1_16_1::DiggingInfo {
        let player = self.player().await;
        let eye = Vec3 {
            x: player.x,
            y: player.y + 1.62,
            z: player.z,
        };
        let center = Vec3 {
            x: f64::from(position.x) + 0.5,
            y: f64::from(position.y) + 0.5,
            z: f64::from(position.z) + 0.5,
        };
        let direction = Vec3 {
            x: center.x - eye.x,
            y: center.y - eye.y,
            z: center.z - eye.z,
        };
        let distance =
            (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z)
                .sqrt();
        let state_id = self.block(position.x, position.y, position.z).await;
        let tool_id = self
            .inventory
            .read()
            .await
            .selected_item()
            .map(|item| item.item_id);
        let mining = state_id
            .and_then(|id| crate::versions::java_1_16_1::registry::mining_info(id, tool_id));
        let visible = self
            .raycast_blocks(direction, distance + 1.0e-7)
            .await
            .is_some_and(|hit| hit.position == position);
        crate::versions::java_1_16_1::DiggingInfo {
            state_id,
            loaded: state_id.is_some(),
            reachable: state_id.is_some() && distance <= 4.5,
            visible,
            diggable: mining.is_some_and(|info| info.diggable),
            harvestable: mining.is_some_and(|info| info.harvestable),
            effective_tool: mining.is_some_and(|info| info.effective_tool),
            predicted_ticks: mining.and_then(|info| info.predicted_ticks),
        }
    }

    /// Performs the `placement_info` operation.
    pub async fn placement_info(
        &self,
        position: BlockPos,
    ) -> crate::versions::java_1_16_1::PlacementInfo {
        let player = self.player().await;
        let eye = Vec3 {
            x: player.x,
            y: player.y + 1.62,
            z: player.z,
        };
        let center = Vec3 {
            x: f64::from(position.x) + 0.5,
            y: f64::from(position.y) + 0.5,
            z: f64::from(position.z) + 0.5,
        };
        let direction = Vec3 {
            x: center.x - eye.x,
            y: center.y - eye.y,
            z: center.z - eye.z,
        };
        let distance =
            (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z)
                .sqrt();
        let world = self.world.lock().await;
        let target_loaded = world.block(position.x, position.y, position.z).is_some();
        let visible = world
            .raycast_blocks(eye, direction, distance + 1.0e-7)
            .is_some_and(|hit| hit.position == position);
        let player_box = Aabb::player(player.x, player.y, player.z);
        let target = Aabb::block(position.x, position.y, position.z);
        let player_intersects_target_cell = target.max_x > player_box.min_x
            && target.min_x < player_box.max_x
            && target.max_y > player_box.min_y
            && target.min_y < player_box.max_y
            && target.max_z > player_box.min_z
            && target.min_z < player_box.max_z;
        crate::versions::java_1_16_1::PlacementInfo {
            target_loaded,
            reachable: target_loaded && distance <= 4.5,
            visible,
            player_intersects_target_cell,
        }
    }
    /// Queues one edge-triggered ground jump for the next physics tick.
    ///
    /// Success means the request was queued locally. It may have no effect when
    /// the player cannot currently jump. For held swimming/climbing input, use
    /// `ControlState::jump` through [`Bot::set_control`].
    pub async fn jump(&self) -> Result<()> {
        self.wait_until_ready().await?;
        self.jump_requested.store(true, Ordering::Release);
        Ok(())
    }
    /// Sends mounted movement input for steerable entities such as horses and pigs.
    pub async fn set_vehicle_control(&self, control: VehicleControl) -> Result<()> {
        if !control.sideways.is_finite()
            || !control.forward.is_finite()
            || !(-1.0..=1.0).contains(&control.sideways)
            || !(-1.0..=1.0).contains(&control.forward)
        {
            bail!("vehicle input must be finite and within -1.0..=1.0");
        }
        let mut payload = Vec::new();
        payload.write_f32::<BigEndian>(control.sideways)?;
        payload.write_f32::<BigEndian>(control.forward)?;
        payload.push(u8::from(control.jump) | (u8::from(control.dismount) << 1));
        self.send(0x1d, &payload).await
    }

    /// Sends the two paddle inputs used while controlling a boat.
    pub async fn steer_boat(&self, left_paddle: bool, right_paddle: bool) -> Result<()> {
        self.send(0x17, &[u8::from(left_paddle), u8::from(right_paddle)])
            .await
    }

    /// Requests dismount through the mounted-input packet.
    pub async fn dismount(&self) -> Result<()> {
        self.set_vehicle_control(VehicleControl {
            dismount: true,
            ..VehicleControl::default()
        })
        .await
    }
    pub(crate) async fn send_vehicle_pose_unchecked(&self, pose: VehiclePose) -> Result<()> {
        if !pose.position.x.is_finite()
            || !pose.position.y.is_finite()
            || !pose.position.z.is_finite()
            || !pose.yaw.is_finite()
            || !pose.pitch.is_finite()
        {
            bail!("vehicle pose values must be finite");
        }
        let mut payload = Vec::new();
        payload.write_f64::<BigEndian>(pose.position.x)?;
        payload.write_f64::<BigEndian>(pose.position.y)?;
        payload.write_f64::<BigEndian>(pose.position.z)?;
        payload.write_f32::<BigEndian>(pose.yaw)?;
        payload.write_f32::<BigEndian>(pose.pitch)?;
        self.send(0x16, &payload).await
    }
    /// Dispatches a respawn request without waiting for the new spawn state.
    pub async fn respawn(&self) -> Result<()> {
        let mut payload = Vec::new();
        put_varint(&mut payload, 0);
        self.send(0x04, &payload).await
    }
    /// Selects a hotbar slot and updates the predicted local selection.
    ///
    /// Success guarantees packet dispatch; the protocol has no direct
    /// acknowledgement for this action.
    pub async fn select_hotbar(&self, slot: u8) -> Result<()> {
        if slot > 8 {
            bail!("hotbar slot must be 0..=8");
        }
        let mut payload = Vec::new();
        payload.write_i16::<BigEndian>(i16::from(slot))?;
        self.send(0x24, &payload).await?;
        self.inventory.write().await.selected_hotbar = slot;
        Ok(())
    }
    /// Dispatches an item-drop action without waiting for an inventory update.
    pub async fn drop_selected(&self, entire_stack: bool) -> Result<()> {
        let mut payload = Vec::new();
        put_varint(&mut payload, if entire_stack { 3 } else { 4 });
        payload.write_u64::<BigEndian>(0)?;
        payload.write_i8(0)?;
        self.send(0x1b, &payload).await
    }
    /// Drops exactly `count` items from the selected hotbar stack.
    pub async fn drop_selected_count(&self, count: u32) -> Result<()> {
        if count == 0 {
            bail!("drop count must be positive");
        }
        let available = self
            .inventory
            .read()
            .await
            .selected_item()
            .map_or(0, |item| u32::from(item.count.max(0) as u8));
        if count > available {
            bail!("drop count {count} exceeds selected stack size {available}");
        }
        if count == available {
            return self.drop_selected(true).await;
        }
        for _ in 0..count {
            self.drop_selected(false).await?;
        }
        Ok(())
    }

    /// Moves/swaps one explicitly chosen player-inventory slot using acknowledged clicks.
    pub async fn move_player_item(&self, source: i16, destination: i16) -> Result<()> {
        if !(0..=45).contains(&source) || !(0..=45).contains(&destination) || source == destination
        {
            bail!("player slots must be distinct values in 0..=45");
        }
        let inventory = self.inventory.read().await;
        if inventory.cursor.is_some() {
            bail!("inventory cursor must be empty before moving an item");
        }
        if inventory
            .player_slots()
            .get(source as usize)
            .and_then(Option::as_ref)
            .is_none()
        {
            bail!("source player slot is empty");
        }
        drop(inventory);
        self.click_slot_and_wait(0, source, 0, ClickMode::Normal)
            .await?;
        self.click_slot_and_wait(0, destination, 0, ClickMode::Normal)
            .await?;
        if self.inventory.read().await.cursor.is_some() {
            self.click_slot_and_wait(0, source, 0, ClickMode::Normal)
                .await?;
        }
        Ok(())
    }

    /// Performs the `equip_from_player_slot` operation.
    pub async fn equip_from_player_slot(
        &self,
        source: i16,
        destination: EquipmentSlot,
    ) -> Result<()> {
        self.move_player_item(source, destination as i16).await
    }

    /// Sends an edited written/writable book stack. NBT construction remains caller-controlled.
    pub async fn edit_book(&self, book: &ItemStack, signing: bool, hand: Hand) -> Result<()> {
        let mut payload = Vec::new();
        write_slot(&mut payload, Some(book));
        payload.push(u8::from(signing));
        put_varint(&mut payload, hand as i32);
        self.send(0x0c, &payload).await
    }
    pub(crate) async fn send_digging_packet(
        &self,
        status: DiggingStatus,
        position: BlockPos,
        face: BlockFace,
    ) -> Result<()> {
        let mut payload = Vec::new();
        put_varint(&mut payload, status as i32);
        payload.write_u64::<BigEndian>(position.packed())?;
        payload.write_i8(face as i8)?;
        self.send(0x1b, &payload).await
    }
    /// Performs timed digging and waits for the matching server acknowledgement.
    pub async fn dig_block(
        &self,
        position: BlockPos,
        face: BlockFace,
    ) -> Result<DiggingAcknowledgement> {
        let state_id = self
            .block(position.x, position.y, position.z)
            .await
            .context("target block chunk is not loaded")?;
        let tool_id = self
            .inventory
            .read()
            .await
            .selected_item()
            .map(|item| item.item_id);
        let ticks = crate::versions::java_1_16_1::registry::mining_ticks(state_id, tool_id)
            .context("target block is not diggable")?;
        let mut events = self.subscribe();
        self.send_digging_packet(DiggingStatus::Started, position, face)
            .await?;
        tokio::time::sleep(Duration::from_millis(ticks.saturating_mul(50))).await;
        self.send_digging_packet(DiggingStatus::Finished, position, face)
            .await?;
        Ok(timeout(Duration::from_secs(5), async {
            loop {
                if let Event::DiggingAcknowledged(ack) = events.recv().await? {
                    if ack.position == position && ack.status == DiggingStatus::Finished as i32 {
                        return Ok::<_, anyhow::Error>(ack);
                    }
                }
            }
        })
        .await
        .context("timed out waiting for digging acknowledgement")??)
    }
    /// Dispatches a block-placement interaction.
    ///
    /// Success does not guarantee that a block was placed. Observe the target
    /// block or consume `BlockChanged` to confirm the game result.
    pub async fn place_block(
        &self,
        hand: Hand,
        position: BlockPos,
        face: BlockFace,
        cursor: [f32; 3],
        inside_block: bool,
    ) -> Result<()> {
        if cursor.iter().any(|value| !(0.0..=1.0).contains(value)) {
            bail!("block cursor coordinates must be within 0.0..=1.0");
        }
        let mut payload = Vec::new();
        put_varint(&mut payload, hand as i32);
        payload.write_u64::<BigEndian>(position.packed())?;
        put_varint(&mut payload, face as i32);
        for value in cursor {
            payload.write_f32::<BigEndian>(value)?;
        }
        payload.push(u8::from(inside_block));
        self.send(0x2d, &payload).await
    }
    /// Dispatches placement and waits until the caller-selected observed cell changes.
    pub async fn place_block_and_wait_for_change(
        &self,
        request: crate::versions::java_1_16_1::PlacementRequest,
        observed: BlockPos,
        wait: Duration,
    ) -> Result<Snapshot<Option<i32>>> {
        let previous = self.block(observed.x, observed.y, observed.z).await;
        self.place_block(
            request.hand,
            request.clicked,
            request.face,
            request.cursor,
            request.inside_block,
        )
        .await?;
        self.wait_for_block_change(observed, previous, wait).await
    }
    /// Dispatches a use-item action without waiting for its game result.
    pub async fn use_item(&self, hand: Hand) -> Result<()> {
        let mut payload = Vec::new();
        put_varint(&mut payload, hand as i32);
        self.send(0x2e, &payload).await
    }
    /// Dispatches use-item, waits locally for `duration`, then dispatches release.
    ///
    /// Dropping this future after the first dispatch does not automatically send
    /// the release action.
    pub async fn use_item_for(&self, hand: Hand, duration: Duration) -> Result<()> {
        self.use_item(hand).await?;
        tokio::time::sleep(duration).await;
        self.release_item().await
    }
    /// Dispatches release-use-item without waiting for acknowledgement.
    pub async fn release_item(&self) -> Result<()> {
        self.send_digging_packet(
            DiggingStatus::ReleaseUseItem,
            BlockPos { x: 0, y: 0, z: 0 },
            BlockFace::Down,
        )
        .await
    }
    /// Dispatches an arm animation without waiting for acknowledgement.
    pub async fn swing_arm(&self, hand: Hand) -> Result<()> {
        let mut payload = Vec::new();
        put_varint(&mut payload, hand as i32);
        self.send(0x2b, &payload).await
    }
    /// Dispatches an entity interaction without confirming its game result.
    pub async fn interact_entity(&self, entity_id: i32, hand: Hand, sneaking: bool) -> Result<()> {
        let mut payload = Vec::new();
        put_varint(&mut payload, entity_id);
        put_varint(&mut payload, 0);
        put_varint(&mut payload, hand as i32);
        payload.push(u8::from(sneaking));
        self.send(0x0e, &payload).await
    }
    /// Dispatches a position-specific entity interaction without confirmation.
    pub async fn interact_entity_at(
        &self,
        entity_id: i32,
        hit: [f32; 3],
        hand: Hand,
        sneaking: bool,
    ) -> Result<()> {
        if !hit.iter().all(|coordinate| coordinate.is_finite()) {
            bail!("entity interaction coordinates must be finite");
        }
        let mut payload = Vec::new();
        put_varint(&mut payload, entity_id);
        put_varint(&mut payload, 2);
        for coordinate in hit {
            payload.write_f32::<BigEndian>(coordinate)?;
        }
        put_varint(&mut payload, hand as i32);
        payload.push(u8::from(sneaking));
        self.send(0x0e, &payload).await
    }
    /// Waits for the locally calculated attack cooldown, then dispatches attack
    /// and arm-swing packets. Success does not confirm damage.
    pub async fn attack(&self, entity_id: i32) -> Result<()> {
        let survival = self.survival.read().await;
        let attack_speed = survival
            .attributes
            .get("minecraft:generic.attack_speed")
            .or_else(|| survival.attributes.get("generic.attack_speed"))
            .map_or(4.0, |attribute| attribute.value())
            .max(0.1);
        drop(survival);
        let cooldown = Duration::from_secs_f64(1.0 / attack_speed);
        let mut last_attack = self.last_attack.lock().await;
        if let Some(last) = *last_attack {
            tokio::time::sleep(cooldown.saturating_sub(last.elapsed())).await;
        }
        let mut payload = Vec::new();
        put_varint(&mut payload, entity_id);
        put_varint(&mut payload, 1);
        payload.push(u8::from(self.control.read().await.sneak));
        self.send(0x0e, &payload).await?;
        self.swing_arm(Hand::Main).await?;
        *last_attack = Some(std::time::Instant::now());
        Ok(())
    }
    /// Dispatches a chat message after local length validation.
    pub async fn send_chat(&self, message: &str) -> Result<()> {
        if message.is_empty() || message.chars().count() > 256 {
            bail!("chat message must contain 1..=256 characters");
        }
        let mut payload = Vec::new();
        put_string(&mut payload, message);
        self.send(0x03, &payload).await
    }
    /// Requests protocol-level command/chat completion and waits for its matching response.
    pub async fn tab_complete(&self, text: &str, wait: Duration) -> Result<TabCompletion> {
        if text.is_empty() || text.chars().count() > 256 {
            bail!("completion text must contain 1..=256 characters");
        }
        let transaction_id = self.next_tab_completion.fetch_add(1, Ordering::Relaxed);
        let mut events = self.subscribe();
        let mut payload = Vec::new();
        put_varint(&mut payload, transaction_id);
        put_string(&mut payload, text);
        self.send(0x06, &payload).await?;
        Ok(timeout(wait, async {
            loop {
                match events.recv().await {
                    Ok(Event::TabCompletion(result)) if result.transaction_id == transaction_id => {
                        return Ok::<_, anyhow::Error>(result);
                    }
                    Ok(Event::Disconnected { reason }) => bail!("disconnected: {reason}"),
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => bail!("event stream closed"),
                }
            }
        })
        .await
        .context("timed out waiting for tab completion")??)
    }
    /// Dispatches a slash command through the chat protocol.
    pub async fn send_command(&self, command: &str) -> Result<()> {
        let command = command.strip_prefix('/').unwrap_or(command);
        self.send_chat(&format!("/{command}")).await
    }
    /// Applies a predicted click locally, dispatches it, and returns its action ID.
    ///
    /// This does not wait for server acceptance. Use
    /// [`Bot::click_slot_and_wait`] for an acknowledged transaction.
    pub async fn click_slot(
        &self,
        window_id: i8,
        slot: i16,
        button: i8,
        mode: ClickMode,
    ) -> Result<i16> {
        let _click_guard = self.click_lock.lock().await;
        let (action, clicked) = {
            let mut inventory = self.inventory.write().await;
            if window_id != 0
                && inventory.open_window.as_ref().map(|window| window.id) != Some(window_id)
            {
                bail!("window {window_id} is not open");
            }
            if inventory
                .pending_clicks
                .keys()
                .any(|(pending_window, _)| *pending_window == window_id)
            {
                bail!("window {window_id} already has an unacknowledged click");
            }
            let clicked = if slot >= 0 {
                inventory
                    .windows
                    .get(&window_id)
                    .and_then(|slots| slots.get(slot as usize))
                    .cloned()
                    .flatten()
            } else {
                None
            };
            let slot_before = clicked.clone();
            let cursor_before = inventory.cursor.clone();
            if mode == ClickMode::Normal {
                predict_normal_click(&mut inventory, window_id, slot, button)?;
                sync_player_inventory_from_window(&mut inventory, window_id);
            }
            let next = inventory.next_actions.entry(window_id).or_default();
            *next = next.wrapping_add(1);
            let action = *next;
            let pending = PendingClick {
                window_id,
                action,
                slot,
                button,
                mode,
                slot_before,
                cursor_before,
            };
            inventory
                .pending_clicks
                .insert((window_id, action), pending);
            (action, clicked)
        };
        let mut payload = Vec::new();
        payload.push(window_id as u8);
        payload.write_i16::<BigEndian>(slot)?;
        payload.write_i8(button)?;
        payload.write_i16::<BigEndian>(action)?;
        payload.write_i8(mode as i8)?;
        write_slot(&mut payload, clicked.as_ref());
        if let Err(error) = self.send(0x09, &payload).await {
            let mut inventory = self.inventory.write().await;
            if let Some(pending) = inventory.pending_clicks.remove(&(window_id, action)) {
                rollback_click(&mut inventory, &pending);
                sync_player_inventory_from_window(&mut inventory, window_id);
            }
            return Err(error);
        }
        Ok(action)
    }
    /// Dispatches a click and waits for the matching accepted transaction.
    pub async fn click_slot_and_wait(
        &self,
        window_id: i8,
        slot: i16,
        button: i8,
        mode: ClickMode,
    ) -> Result<i16> {
        let mut events = self.subscribe();
        let action = self.click_slot(window_id, slot, button, mode).await?;
        timeout(Duration::from_secs(5), async {
            loop {
                if let Event::WindowTransaction(transaction) = events.recv().await? {
                    if transaction.window_id == window_id && transaction.action == action {
                        if transaction.accepted {
                            return Ok::<(), anyhow::Error>(());
                        }
                        bail!("server rejected window action {action}");
                    }
                }
            }
        })
        .await
        .context("timed out waiting for slot synchronization")??;
        Ok(action)
    }
    /// Performs the `craft_once` operation.
    pub async fn craft_once(
        &self,
        window_id: i8,
        width: usize,
        height: usize,
        ingredients: &[Option<i32>],
    ) -> Result<()> {
        if !matches!((window_id, width, height), (0, 2, 2) | (_, 3, 3)) {
            bail!("player crafting must be 2x2 and an open container must be 3x3");
        }
        if ingredients.len() != width * height {
            bail!("ingredient grid length does not match dimensions");
        }
        let grid_end = 1 + ingredients.len();
        let source_start = if window_id == 0 { 9 } else { grid_end };
        for (grid_index, ingredient) in ingredients.iter().enumerate() {
            let Some(item_id) = ingredient else {
                continue;
            };
            let source = self
                .inventory()
                .await
                .windows
                .get(&window_id)
                .and_then(|slots| {
                    slots
                        .iter()
                        .enumerate()
                        .skip(source_start)
                        .find(|(_, item)| {
                            item.as_ref().is_some_and(|item| item.item_id == *item_id)
                        })
                        .map(|(slot, _)| slot as i16)
                })
                .with_context(|| format!("ingredient item {item_id} is unavailable"))?;
            self.click_slot_and_wait(window_id, source, 0, ClickMode::Normal)
                .await
                .with_context(|| format!("pick ingredient {item_id} from slot {source}"))?;
            self.click_slot_and_wait(window_id, (1 + grid_index) as i16, 1, ClickMode::Normal)
                .await
                .with_context(|| format!("place ingredient {item_id} into crafting grid"))?;
            if self.inventory().await.cursor.is_some() {
                self.click_slot_and_wait(window_id, source, 0, ClickMode::Normal)
                    .await
                    .with_context(|| format!("return remaining ingredient {item_id}"))?;
            }
        }
        timeout(Duration::from_secs(5), async {
            let mut events = self.subscribe();
            loop {
                if self
                    .inventory()
                    .await
                    .windows
                    .get(&window_id)
                    .and_then(|slots| slots.first())
                    .is_some_and(Option::is_some)
                {
                    return Ok::<(), anyhow::Error>(());
                }
                let _ = events.recv().await?;
            }
        })
        .await
        .context("timed out waiting for crafting result")??;
        self.take_crafting_result(window_id, ingredients.len())
            .await
    }
    /// Performs the `take_crafting_result` operation.
    pub async fn take_crafting_result(&self, window_id: i8, grid_slots: usize) -> Result<()> {
        let source_start = if window_id == 0 { 9 } else { 1 + grid_slots };
        self.click_slot_and_wait(window_id, 0, 0, ClickMode::Normal)
            .await
            .context("collect crafting result")?;
        {
            // Accepted crafting-result clicks consume one item from every occupied
            // recipe slot client-side; vanilla does not necessarily echo these slots.
            let mut inventory = self.inventory.write().await;
            if let Some(slots) = inventory.windows.get_mut(&window_id) {
                for slot in slots.iter_mut().take(1 + grid_slots).skip(1).flatten() {
                    slot.count -= 1;
                }
                for item in slots.iter_mut().take(1 + grid_slots).skip(1) {
                    if item.as_ref().is_some_and(|item| item.count <= 0) {
                        *item = None;
                    }
                }
                slots[0] = None;
            }
            sync_player_inventory_from_window(&mut inventory, window_id);
        }
        let destination = self
            .inventory()
            .await
            .windows
            .get(&window_id)
            .and_then(|slots| {
                slots
                    .iter()
                    .enumerate()
                    .skip(source_start)
                    .find(|(_, item)| item.is_none())
                    .map(|(slot, _)| slot as i16)
            })
            .context("no empty inventory slot for crafting result")?;
        self.click_slot_and_wait(window_id, destination, 0, ClickMode::Normal)
            .await
            .context("store crafting result")?;
        Ok(())
    }
    /// Dispatches recipe placement without waiting for inventory synchronization.
    pub async fn place_recipe(&self, window_id: i8, recipe_id: &str, make_all: bool) -> Result<()> {
        let mut payload = vec![window_id as u8];
        put_string(&mut payload, recipe_id);
        payload.push(u8::from(make_all));
        self.send(0x19, &payload).await
    }
    /// Dispatches close-window and immediately clears the predicted local window.
    pub async fn close_window(&self) -> Result<()> {
        let window_id = self
            .inventory
            .read()
            .await
            .open_window
            .as_ref()
            .map(|window| window.id)
            .context("no window is open")?;
        self.send(0x0a, &[window_id as u8]).await?;
        let mut inventory = self.inventory.write().await;
        inventory.open_window = None;
        inventory.merchant_offers = None;
        inventory.windows.remove(&window_id);
        inventory
            .pending_clicks
            .retain(|(id, _), _| *id != window_id);
        drop(inventory);
        self.emit(Event::WindowClosed { window_id });
        Ok(())
    }
    /// Selects one server-advertised merchant offer without choosing it automatically.
    pub async fn select_trade(&self, index: usize) -> Result<()> {
        let offers = self
            .merchant_offers()
            .await
            .context("no merchant offers are open")?;
        if index >= offers.offers.len() {
            bail!("merchant offer index is out of range");
        }
        let mut payload = Vec::new();
        put_varint(&mut payload, i32::try_from(index)?);
        self.send(0x22, &payload).await
    }

    /// Selects one of the three enchantment-table options.
    pub async fn select_enchantment(&self, option: u8) -> Result<()> {
        if option > 2 {
            bail!("enchantment option must be 0..=2");
        }
        let window_id = self
            .open_window_state()
            .await
            .context("no window is open")?
            .id;
        self.send(0x08, &[window_id as u8, option]).await
    }

    /// Sends the raw anvil rename text for the currently open anvil.
    pub async fn rename_item(&self, name: &str) -> Result<()> {
        if name.chars().count() > 50 {
            bail!("item name must not exceed 50 characters");
        }
        let mut payload = Vec::new();
        put_string(&mut payload, name);
        self.send(0x1f, &payload).await
    }

    /// Performs the `set_beacon_effects` operation.
    pub async fn set_beacon_effects(&self, primary: i32, secondary: i32) -> Result<()> {
        let mut payload = Vec::new();
        put_varint(&mut payload, primary);
        put_varint(&mut payload, secondary);
        self.send(0x23, &payload).await
    }

    /// Performs the `update_sign` operation.
    pub async fn update_sign(&self, position: BlockPos, lines: [&str; 4]) -> Result<()> {
        if lines.iter().any(|line| line.chars().count() > 384) {
            bail!("sign lines must not exceed 384 characters");
        }
        let mut payload = Vec::new();
        payload.write_u64::<BigEndian>(position.packed())?;
        for line in lines {
            put_string(&mut payload, line);
        }
        self.send(0x2a, &payload).await
    }

    /// Performs the `swap_hands` operation.
    pub async fn swap_hands(&self) -> Result<()> {
        self.send_digging_packet(
            DiggingStatus::SwapHands,
            BlockPos { x: 0, y: 0, z: 0 },
            BlockFace::Down,
        )
        .await
    }
    /// Stops local movement and closes the TCP writer.
    ///
    /// This does not wait for a server-side disconnect event.
    pub async fn disconnect(&self) -> Result<()> {
        self.emit(Event::Disconnected {
            reason: "client disconnected".to_owned(),
        });
        self.clear_control().await;
        tokio::io::AsyncWriteExt::shutdown(&mut self.writer.lock().await.inner).await?;
        Ok(())
    }

    async fn control_loop(&self) {
        if self.wait_until_ready().await.is_err() {
            return;
        }
        let mut ticker = tokio::time::interval(Duration::from_millis(50));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut jump_held = false;
        let mut sprinting = false;
        let mut sneaking = false;
        while !self.stopped.load(Ordering::Acquire) {
            let scheduled = ticker.tick().await;
            let lag = tokio::time::Instant::now().saturating_duration_since(scheduled);
            if self
                .teleport_barrier_ticks
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |ticks| {
                    ticks.checked_sub(1)
                })
                .is_ok()
            {
                self.physics.lock().await.record_tick(Duration::ZERO, lag);
                continue;
            }
            let control = **self.control.read().await;
            if control.sprint != sprinting {
                if self
                    .send_entity_action(if control.sprint { 3 } else { 4 })
                    .await
                    .is_err()
                {
                    break;
                }
                sprinting = control.sprint;
            }
            if control.sneak != sneaking {
                if self
                    .send_entity_action(if control.sneak { 0 } else { 1 })
                    .await
                    .is_err()
                {
                    break;
                }
                sneaking = control.sneak;
            }
            let jump =
                (control.jump && !jump_held) || self.jump_requested.swap(false, Ordering::AcqRel);
            jump_held = control.jump;
            let tick_started = std::time::Instant::now();
            if self.physics_tick(control, jump).await.is_err() {
                break;
            }
            self.physics
                .lock()
                .await
                .record_tick(tick_started.elapsed(), lag);
        }
    }

    async fn physics_tick(&self, control: ControlState, jump: bool) -> Result<()> {
        let survival = self.survival.read().await;
        let base_movement_speed = survival
            .attributes
            .get("minecraft:generic.movement_speed")
            .or_else(|| survival.attributes.get("generic.movement_speed"))
            .map_or(0.1, |attribute| attribute.value());
        drop(survival);
        let mut player = self.player.lock().await;
        let mut motion = self.motion.lock().await;
        let was_on_ground = player.on_ground;
        let mut forward = f64::from(i8::from(control.forward) - i8::from(control.back));
        let mut strafe = f64::from(i8::from(control.right) - i8::from(control.left));
        if control.sneak {
            forward *= 0.3;
            strafe *= 0.3;
        }
        let input_length = (forward * forward + strafe * strafe).sqrt();
        if input_length > 1.0 {
            forward /= input_length;
            strafe /= input_length;
        }
        forward *= 0.98;
        strafe *= 0.98;

        let aabb = Aabb::player(player.x, player.y, player.z);
        let world = self.world.lock().await;
        let fluid = world.fluid_intersecting(aabb);
        let fluid_flow = fluid.map(|kind| world.fluid_flow(aabb, kind));
        let climbing = world.is_climbable(aabb);
        let below = world.block_below_name(aabb);
        let effects = world.contact_effects(aabb);
        let slipperiness: f64 = match below {
            Some("blue_ice") => 0.989,
            Some("ice" | "packed_ice" | "frosted_ice") => 0.98,
            _ => 0.6,
        };
        let ground_friction: f64 = 0.91 * slipperiness;
        // The server attribute already includes the vanilla sprint modifier
        // (operation 2, +30%) while sprinting. Applying another local factor
        // makes ground acceleration 0.169 instead of the authoritative 0.13.
        let movement_speed = base_movement_speed;
        let acceleration = if fluid.is_some() {
            0.02
        } else if was_on_ground {
            movement_speed * (0.162_771_36 / ground_friction.powi(3))
        } else if control.sprint {
            0.026
        } else {
            0.02
        };
        let yaw = (player.yaw as f64).to_radians();
        motion.velocity.x += (strafe * yaw.cos() - forward * yaw.sin()) * acceleration;
        motion.velocity.z += (forward * yaw.cos() + strafe * yaw.sin()) * acceleration;
        if let Some(flow) = fluid_flow {
            motion.velocity.x += flow.x * 0.014;
            motion.velocity.z += flow.z * 0.014;
        }
        if control.jump && fluid.is_some() {
            motion.velocity.y += 0.04;
        } else if jump && was_on_ground {
            motion.velocity.y = if below == Some("honey_block") {
                0.42 * 0.4
            } else {
                0.42
            };
            if control.sprint {
                motion.velocity.x -= yaw.sin() * 0.2;
                motion.velocity.z += yaw.cos() * 0.2;
            }
        }
        if climbing {
            motion.velocity.x = motion.velocity.x.clamp(-0.15, 0.15);
            motion.velocity.z = motion.velocity.z.clamp(-0.15, 0.15);
            motion.velocity.y = motion
                .velocity
                .y
                .max(if control.sneak { 0.0 } else { -0.15 });
            motion.fall_distance = 0.0;
            if control.jump {
                motion.velocity.y = 0.2;
            }
        }
        if effects.cobweb {
            motion.velocity.x *= 0.25;
            motion.velocity.y *= 0.05;
            motion.velocity.z *= 0.25;
        } else if effects.berry_bush {
            motion.velocity.x *= 0.8;
            motion.velocity.y *= 0.75;
            motion.velocity.z *= 0.8;
        }
        if effects.bubble_push != 0 {
            motion.velocity.y += if effects.bubble_push > 0 { 0.06 } else { -0.03 };
        }

        let mut requested = motion.velocity;
        if control.sneak && was_on_ground {
            let Some((dx, dz)) = world.limit_sneak_movement(aabb, requested.x, requested.z) else {
                motion.velocity = Vec3::default();
                return Ok(());
            };
            requested.x = dx;
            requested.z = dz;
        }
        let Some((moved, actual)) = world.collide_with_step(aabb, requested, was_on_ground, 0.6)
        else {
            motion.velocity = Vec3::default();
            return Ok(());
        };
        let landed_on = world.block_below_name(moved);
        drop(world);
        let collided_x = (requested.x - actual.x).abs() > 1.0e-9;
        let collided_y = (requested.y - actual.y).abs() > 1.0e-9;
        let collided_z = (requested.z - actual.z).abs() > 1.0e-9;
        player.x = (moved.min_x + moved.max_x) * 0.5;
        player.y = moved.min_y;
        player.z = (moved.min_z + moved.max_z) * 0.5;
        player.on_ground = collided_y && requested.y < 0.0;
        motion.collided_horizontal = collided_x || collided_z;
        motion.collided_vertical = collided_y;
        if player.on_ground {
            motion.fall_distance = 0.0;
        } else if actual.y < 0.0 {
            motion.fall_distance += (-actual.y) as f32;
        }
        if collided_x {
            motion.velocity.x = 0.0;
        }
        if collided_y {
            motion.velocity.y =
                if requested.y < 0.0 && landed_on == Some("slime_block") && !control.sneak {
                    -requested.y
                } else {
                    0.0
                };
        }
        if collided_z {
            motion.velocity.z = 0.0;
        }
        if climbing && motion.collided_horizontal {
            motion.velocity.y = 0.2;
        }
        match fluid {
            Some(Fluid::Water) => {
                motion.velocity.x *= 0.8;
                motion.velocity.y = motion.velocity.y * 0.8 - 0.02;
                motion.velocity.z *= 0.8;
            }
            Some(Fluid::Lava) => {
                motion.velocity.x *= 0.5;
                motion.velocity.y = motion.velocity.y * 0.5 - 0.02;
                motion.velocity.z *= 0.5;
            }
            None => {
                motion.velocity.y = (motion.velocity.y - 0.08) * 0.98;
                let horizontal_drag = if was_on_ground { ground_friction } else { 0.91 };
                motion.velocity.x *= horizontal_drag;
                motion.velocity.z *= horizontal_drag;
            }
        }
        if player.on_ground && matches!(landed_on, Some("soul_sand" | "honey_block")) {
            motion.velocity.x *= 0.4;
            motion.velocity.z *= 0.4;
        }
        motion.ticks += 1;
        drop(motion);
        drop(player);
        self.send_position().await
    }

    async fn read_loop(&self, mut reader: tokio::net::tcp::OwnedReadHalf) -> Result<()> {
        while !self.stopped.load(Ordering::Acquire) {
            let compression = self.writer.lock().await.compression;
            let packet = tokio::select! {
                biased;
                _ = self.cancel.notified() => return Ok(()),
                packet = timeout(
                    self.connection_options.play_packet_timeout,
                    read_packet(&mut reader, compression),
                ) => packet.context("play packet timed out")?,
            };
            let (id, p) = packet?;
            match id {
                0x00 => self.insert_entity(parse_spawn_object(&p)?).await?,
                0x01 => self.insert_entity(parse_spawn_orb(&p)?).await?,
                0x02 => self.insert_entity(parse_spawn_living(&p)?).await?,
                0x03 => self.insert_entity(parse_spawn_painting(&p)?).await?,
                0x04 => self.insert_entity(parse_spawn_player(&p)?).await?,
                0x05 => {
                    let mut rest = p.as_slice();
                    self.emit(Event::EntityAnimation {
                        entity_id: get_varint(&mut rest)?,
                        animation: *rest.first().context("missing animation")?,
                    });
                }
                0x06 => {
                    self.statistics.write().await.apply(&p)?;
                    self.emit(Event::StatisticsUpdated);
                }
                0x07 => self.emit(Event::DiggingAcknowledged(parse_digging_ack(&p)?)),
                0x08 => self.emit(Event::BlockBreakProgress(parse_break_progress(&p)?)),
                0x09 => {
                    let update = self.world.lock().await.apply_block_entity(&p)?;
                    self.world_updated.notify_waiters();
                    self.emit(Event::BlockEntityUpdated(update));
                }
                0x0a => {
                    let mut cursor = Cursor::new(&p);
                    let event = BlockActionEvent {
                        position: BlockPos::unpack(cursor.read_u64::<BigEndian>()?),
                        action: cursor.read_u8()?,
                        parameter: cursor.read_u8()?,
                        block_id: {
                            let mut rest = &p[cursor.position() as usize..];
                            get_varint(&mut rest)?
                        },
                    };
                    self.emit(Event::BlockAction(event));
                }
                0x0b => {
                    let (x, y, z, state_id) = self.world.lock().await.apply_block_change(&p)?;
                    self.world_updated.notify_waiters();
                    self.emit(Event::BlockChanged { x, y, z, state_id });
                }
                0x0c => {
                    self.ui.write().await.apply_boss_bar(&p)?;
                    self.emit(Event::UiStateUpdated(UiUpdateKind::BossBar));
                }
                0x0d => {
                    let difficulty = Difficulty {
                        id: *p.first().context("missing difficulty")?,
                        locked: *p.get(1).context("missing difficulty lock")? != 0,
                    };
                    self.survival.write().await.difficulty = Some(difficulty);
                    self.emit(Event::Difficulty(difficulty));
                }
                0x0e => self.emit(Event::Chat(parse_chat(&p)?)),
                0x0f => {
                    let count = self.world.lock().await.apply_multi_block_change(&p)?;
                    self.world_updated.notify_waiters();
                    self.emit(Event::MultiBlockChanged { count });
                }
                0x10 => self.emit(Event::TabCompletion(parse_tab_completion(&p)?)),
                0x11 => {
                    **self.command_tree.write().await = Some(parse_command_tree(&p)?);
                    self.emit(Event::CommandTreeUpdated);
                }
                0x12 => {
                    let mut c = Cursor::new(&p);
                    let transaction = WindowTransaction {
                        window_id: c.read_i8()?,
                        action: c.read_i16::<BigEndian>()?,
                        accepted: c.read_u8()? != 0,
                    };
                    let mut inventory = self.inventory.write().await;
                    let pending = inventory
                        .pending_clicks
                        .remove(&(transaction.window_id, transaction.action));
                    if let Some(pending) = &pending {
                        if !transaction.accepted {
                            rollback_click(&mut inventory, pending);
                        } else if pending.mode == ClickMode::Normal {
                            rollback_click(&mut inventory, pending);
                            predict_normal_click(
                                &mut inventory,
                                pending.window_id,
                                pending.slot,
                                pending.button,
                            )?;
                            sync_player_inventory_from_window(&mut inventory, pending.window_id);
                        }
                    }
                    drop(inventory);
                    if !transaction.accepted {
                        let mut payload = Vec::new();
                        payload.write_i8(transaction.window_id)?;
                        payload.write_i16::<BigEndian>(transaction.action)?;
                        payload.push(1);
                        self.send(0x07, &payload).await?;
                    }
                    self.emit(Event::WindowTransaction(transaction));
                }
                0x13 => {
                    let window_id = *p.first().context("missing closed window ID")? as i8;
                    let mut inventory = self.inventory.write().await;
                    inventory.open_window = None;
                    inventory.merchant_offers = None;
                    inventory.windows.remove(&window_id);
                    inventory
                        .pending_clicks
                        .retain(|(id, _), _| *id != window_id);
                    drop(inventory);
                    self.emit(Event::WindowClosed { window_id });
                }
                0x14 => {
                    let (window_id, slots) = parse_window_items(&p)?;
                    let mut inventory = self.inventory.write().await;
                    inventory.windows.insert(window_id, slots);
                    sync_player_inventory_from_window(&mut inventory, window_id);
                    drop(inventory);
                    self.emit(Event::InventoryUpdated { window_id });
                }
                0x15 => {
                    let mut c = Cursor::new(&p);
                    let property = WindowProperty {
                        window_id: c.read_u8()? as i8,
                        property: c.read_i16::<BigEndian>()?,
                        value: c.read_i16::<BigEndian>()?,
                    };
                    self.inventory
                        .write()
                        .await
                        .properties
                        .insert((property.window_id, property.property), property.value);
                    self.emit(Event::WindowProperty(property));
                }
                0x16 => {
                    let update = parse_set_slot(&p)?;
                    let mut inventory = self.inventory.write().await;
                    apply_slot(&mut inventory, &update)?;
                    drop(inventory);
                    self.emit(Event::SlotUpdated(update));
                }
                0x17 => {
                    let mut rest = p.as_slice();
                    let item_id = get_varint(&mut rest)?;
                    let ticks = get_varint(&mut rest)?;
                    self.survival
                        .write()
                        .await
                        .item_cooldowns
                        .insert(item_id, ticks);
                    self.emit(Event::ItemCooldown { item_id, ticks });
                }
                0x18 => {
                    let mut rest = p.as_slice();
                    let channel = get_string(&mut rest)?;
                    if rest.len() > self.connection_options.max_custom_payload_bytes {
                        bail!(
                            "custom payload has {} bytes, limit is {}",
                            rest.len(),
                            self.connection_options.max_custom_payload_bytes
                        );
                    }
                    let data = Arc::from(rest);
                    if channel == "minecraft:brand" {
                        let mut brand_data = rest;
                        if let Ok(brand) = get_string(&mut brand_data) {
                            **self.server_brand.write().await = Some(brand.clone());
                            self.emit(Event::ServerBrand(brand));
                        }
                    }
                    self.emit(Event::CustomPayload { channel, data });
                }
                0x19 => self.handle_sound(&p, true)?,
                0x1a => {
                    let mut s = p.as_slice();
                    self.physics.lock().await.record_disconnect();
                    self.emit(Event::Disconnected {
                        reason: get_string(&mut s).unwrap_or_default(),
                    });
                    break;
                }
                0x1b => {
                    let mut c = Cursor::new(&p);
                    let entity_id = c.read_i32::<BigEndian>()?;
                    let status = c.read_i8()?;
                    self.emit(Event::EntityStatus { entity_id, status });
                }
                0x1c => {
                    let explosion = parse_explosion(&p)?;
                    self.world
                        .lock()
                        .await
                        .apply_explosion_blocks(&explosion.affected_blocks);
                    self.world_updated.notify_waiters();
                    let mut motion = self.motion.lock().await;
                    motion.velocity.x += explosion.player_motion.x;
                    motion.velocity.y += explosion.player_motion.y;
                    motion.velocity.z += explosion.player_motion.z;
                    drop(motion);
                    self.emit(Event::Explosion(explosion));
                }
                0x1d => {
                    let (x, z) = self.world.lock().await.unload_chunk(&p)?;
                    self.emit(Event::ChunkUnloaded { x, z });
                }
                0x1e => {
                    let mut c = Cursor::new(&p);
                    let change = GameStateChange {
                        reason: c.read_u8()?,
                        value: c.read_f32::<BigEndian>()?,
                    };
                    let mut state = self.survival.write().await;
                    match change.reason {
                        1 => state.raining = Some(false),
                        2 => state.raining = Some(true),
                        3 => state.game_mode = Some(change.value as u8),
                        7 => state.rain_level = Some(change.value),
                        8 => state.thunder_level = Some(change.value),
                        _ => {}
                    }
                    drop(state);
                    self.emit(Event::GameStateChange(change));
                }
                0x1f => {
                    let mut rest = p.as_slice();
                    let window_id = i8::try_from(*rest.first().context("missing horse window ID")?)
                        .context("horse window ID out of range")?;
                    rest = &rest[1..];
                    let declared_slots = get_varint(&mut rest)?;
                    let mut cursor = Cursor::new(rest);
                    let entity_id = cursor.read_i32::<BigEndian>()?;
                    let window = OpenWindow {
                        id: window_id,
                        window_type: -1,
                        title_json: String::new(),
                        entity_id: Some(entity_id),
                        declared_slots: Some(declared_slots),
                    };
                    self.inventory.write().await.open_window = Some(window.clone());
                    self.emit(Event::WindowOpened(window));
                }
                0x20 => self.send(0x10, &p).await?,
                0x21 => match self
                    .world
                    .lock()
                    .await
                    .apply_chunk(&p, self.connection_options.max_chunks)
                {
                    Ok((x, z)) => {
                        self.world_updated.notify_waiters();
                        self.emit(Event::ChunkLoaded { x, z });
                    }
                    Err(e) => self.emit(Event::Error {
                        kind: "chunk_decode",
                        message: e.to_string(),
                    }),
                },
                0x22 => self.emit(Event::WorldEvent(parse_world_event(&p)?)),
                0x23 => self.emit(Event::Particle(parse_particle(&p)?)),
                0x24 => match self
                    .world
                    .lock()
                    .await
                    .apply_light(&p, self.connection_options.max_chunks)
                {
                    Ok((x, z)) => {
                        self.world_updated.notify_waiters();
                        self.emit(Event::ChunkLightUpdated { x, z });
                    }
                    Err(e) => self.emit(Event::Error {
                        kind: "light_decode",
                        message: e.to_string(),
                    }),
                },
                0x25 => {
                    let join = parse_join(&p)?;
                    let mut player = self.player.lock().await;
                    player.entity_id = Some(join.entity_id);
                    player.spawned = true;
                    drop(player);
                    let mut survival = self.survival.write().await;
                    survival.game_mode = Some(join.game_mode);
                    survival.previous_game_mode = Some(join.previous_game_mode);
                    survival.dimension = Some(join.dimension);
                    survival.world_name = Some(join.world_name);
                    drop(survival);
                    self.ready.notify_waiters();
                    self.emit(Event::Spawn);
                    self.set_client_settings(self.client_settings().await)
                        .await?;
                }
                0x26 => {
                    let update = parse_map_update(&p)?;
                    self.maps
                        .write()
                        .await
                        .apply(&update, self.connection_options.max_maps)?;
                    self.emit(Event::MapUpdated(update));
                }
                0x27 => {
                    let offers = parse_merchant_offers(&p)?;
                    self.inventory.write().await.merchant_offers = Some(offers.clone());
                    self.emit(Event::MerchantOffers(offers));
                }
                0x28 | 0x29 => {
                    let mut rest = p.as_slice();
                    let entity_id = get_varint(&mut rest)?;
                    if let Some(entity) = self.entities.write().await.entities.get_mut(&entity_id) {
                        apply_relative(entity, &p, id == 0x29)?;
                        self.emit(Event::EntityUpdated(entity.clone()));
                    }
                }
                0x2a => {
                    let mut rest = p.as_slice();
                    let entity_id = get_varint(&mut rest)?;
                    if let Some(entity) = self.entities.write().await.entities.get_mut(&entity_id) {
                        entity.yaw = f32::from(*rest.first().context("missing entity yaw")? as i8)
                            * 360.0
                            / 256.0;
                        entity.pitch =
                            f32::from(*rest.get(1).context("missing entity pitch")? as i8) * 360.0
                                / 256.0;
                        entity.on_ground = *rest.get(2).context("missing entity ground flag")? != 0;
                        self.emit(Event::EntityUpdated(entity.clone()));
                    }
                }
                0x2b => {
                    let mut rest = p.as_slice();
                    let entity_id = get_varint(&mut rest)?;
                    let on_ground = *rest.first().context("missing entity ground flag")? != 0;
                    if let Some(entity) = self.entities.write().await.entities.get_mut(&entity_id) {
                        entity.on_ground = on_ground;
                        self.emit(Event::EntityUpdated(entity.clone()));
                    }
                }
                0x2c => {
                    let mut cursor = Cursor::new(&p);
                    let pose = VehiclePose {
                        position: Vec3 {
                            x: cursor.read_f64::<BigEndian>()?,
                            y: cursor.read_f64::<BigEndian>()?,
                            z: cursor.read_f64::<BigEndian>()?,
                        },
                        yaw: cursor.read_f32::<BigEndian>()?,
                        pitch: cursor.read_f32::<BigEndian>()?,
                    };
                    validate_position(pose.position.x, pose.position.y, pose.position.z)?;
                    if !pose.yaw.is_finite() || !pose.pitch.is_finite() {
                        bail!("vehicle position contains a non-finite rotation");
                    }
                    if let Some(vehicle) = self.vehicle().await {
                        if let Some(tracked) = self
                            .entities
                            .write()
                            .await
                            .entities
                            .get_mut(&vehicle.entity_id)
                        {
                            tracked.position = pose.position;
                            tracked.yaw = pose.yaw;
                            tracked.pitch = pose.pitch;
                        }
                    }
                    self.emit(Event::VehiclePosition(pose));
                }
                0x2d => {
                    let mut rest = p.as_slice();
                    let hand = match get_varint(&mut rest)? {
                        0 => Hand::Main,
                        1 => Hand::Off,
                        value => bail!("unknown book hand {value}"),
                    };
                    self.emit(Event::BookOpened { hand });
                }
                0x2e => {
                    let mut rest = p.as_slice();
                    let id = get_varint(&mut rest)?;
                    if !(0..=127).contains(&id) {
                        bail!("invalid open window ID {id}");
                    }
                    let window = OpenWindow {
                        id: id as i8,
                        window_type: get_varint(&mut rest)?,
                        title_json: get_string(&mut rest)?,
                        entity_id: None,
                        declared_slots: None,
                    };
                    self.inventory.write().await.open_window = Some(window.clone());
                    self.emit(Event::WindowOpened(window));
                }
                0x2f => {
                    let mut cursor = Cursor::new(&p);
                    self.emit(Event::SignEditorOpened {
                        position: BlockPos::unpack(cursor.read_u64::<BigEndian>()?),
                    });
                }
                0x30 => {
                    let mut rest = p.as_slice();
                    let window = get_varint(&mut rest)?;
                    let window_id =
                        i8::try_from(window).context("craft response window ID out of range")?;
                    let recipe_id = get_string(&mut rest)?;
                    self.emit(Event::CraftRecipeResponse {
                        window_id,
                        recipe_id,
                    });
                }
                0x31 => {
                    let mut c = Cursor::new(&p);
                    let flags = c.read_i8()? as u8;
                    let mut state = self.survival.write().await;
                    state.invulnerable = flags & 0x01 != 0;
                    state.flying = flags & 0x02 != 0;
                    state.flying_allowed = flags & 0x04 != 0;
                    state.creative_mode = flags & 0x08 != 0;
                    state.flying_speed = c.read_f32::<BigEndian>()?;
                    state.walking_speed = c.read_f32::<BigEndian>()?;
                    drop(state);
                    self.emit(Event::SurvivalStateUpdated);
                }
                0x32 => self.emit(Event::Combat(parse_combat_event(&p)?)),
                0x33 => {
                    let mut players = self.players.write().await;
                    let (action, uuids) = apply_player_info(&mut players, &p)?;
                    drop(players);
                    self.emit(Event::PlayerListUpdated { action, uuids });
                }
                0x34 => {
                    let mut rest = p.as_slice();
                    let source_anchor = get_varint(&mut rest)?;
                    let mut cursor = Cursor::new(rest);
                    let target = Vec3 {
                        x: cursor.read_f64::<BigEndian>()?,
                        y: cursor.read_f64::<BigEndian>()?,
                        z: cursor.read_f64::<BigEndian>()?,
                    };
                    let consumed = cursor.position() as usize;
                    rest = &rest[consumed..];
                    let is_entity = *rest.first().context("missing face-player entity flag")? != 0;
                    rest = &rest[1..];
                    if is_entity {
                        let _ = get_varint(&mut rest)?;
                        let _ = get_varint(&mut rest)?;
                    }
                    let mut player = self.player.lock().await;
                    let source_y = player.y + if source_anchor == 1 { 1.62 } else { 0.0 };
                    let dx = target.x - player.x;
                    let dy = target.y - source_y;
                    let dz = target.z - player.z;
                    player.yaw = (-dx).atan2(dz).to_degrees() as f32;
                    player.pitch = (-dy).atan2(dx.hypot(dz)).to_degrees() as f32;
                    self.emit(Event::Position(player.clone()));
                }
                0x35 => self.handle_position(&p).await?,
                0x36 => {
                    self.recipe_book.write().await.apply(&p)?;
                    self.emit(Event::RecipeBookUpdated);
                }
                0x37 => {
                    let mut rest = p.as_slice();
                    let count = get_varint(&mut rest)?;
                    if !(0..=65_536).contains(&count) {
                        bail!("invalid destroyed entity count {count}");
                    }
                    let mut entity_ids = Vec::with_capacity(count as usize);
                    let mut entities = self.entities.write().await;
                    for _ in 0..count {
                        let entity_id = get_varint(&mut rest)?;
                        entities.entities.remove(&entity_id);
                        entity_ids.push(entity_id);
                    }
                    drop(entities);
                    self.emit(Event::EntitiesDestroyed { entity_ids });
                }
                0x38 => {
                    let mut rest = p.as_slice();
                    let entity_id = get_varint(&mut rest)?;
                    let effect_id = *rest.first().context("missing removed effect ID")? as i8;
                    if Some(entity_id) == self.player.lock().await.entity_id {
                        self.survival.write().await.effects.remove(&effect_id);
                        self.emit(Event::SurvivalStateUpdated);
                    }
                }
                0x39 => {
                    let mut rest = p.as_slice();
                    let request = ResourcePackRequest {
                        url: get_string(&mut rest)?,
                        hash: get_string(&mut rest)?,
                    };
                    **self.resource_pack.write().await = Some(request.clone());
                    self.emit(Event::ResourcePackRequested(request));
                }
                0x3a => {
                    let respawn = parse_respawn(&p)?;
                    let mut state = self.survival.write().await;
                    state.dimension = Some(respawn.dimension.clone());
                    state.world_name = Some(respawn.world_name.clone());
                    state.game_mode = Some(respawn.game_mode);
                    state.previous_game_mode = Some(respawn.previous_game_mode);
                    if !respawn.copy_metadata {
                        state.effects.clear();
                        state.attributes.clear();
                    }
                    drop(state);
                    self.world.lock().await.clear();
                    **self.motion.lock().await = MotionState::default();
                    *self.positioned.lock().await = false;
                    **self.entities.write().await = EntityTracker::default();
                    if !respawn.copy_metadata {
                        **self.inventory.write().await = InventoryState::default();
                    }
                    self.emit(Event::Respawn(respawn));
                }
                0x3b => {
                    let mut rest = p.as_slice();
                    let entity_id = get_varint(&mut rest)?;
                    if let Some(entity) = self.entities.write().await.entities.get_mut(&entity_id) {
                        entity.head_yaw =
                            f32::from(*rest.first().context("missing entity head yaw")? as i8)
                                * 360.0
                                / 256.0;
                        self.emit(Event::EntityUpdated(entity.clone()));
                    }
                }
                0x3c => {
                    let mut rest = p.as_slice();
                    let selected = if *rest.first().context("missing advancement tab flag")? != 0 {
                        rest = &rest[1..];
                        Some(get_string(&mut rest)?)
                    } else {
                        None
                    };
                    self.advancements.write().await.selected_tab = selected;
                    self.emit(Event::AdvancementsUpdated);
                }
                0x3d => {
                    self.ui.write().await.apply_border(&p)?;
                    self.emit(Event::UiStateUpdated(UiUpdateKind::WorldBorder));
                }
                0x3e => {
                    let mut rest = p.as_slice();
                    let entity_id = get_varint(&mut rest)?;
                    **self.camera_entity_id.write().await = Some(entity_id);
                    self.emit(Event::CameraChanged { entity_id });
                }
                0x3f => {
                    let raw = *p.first().context("missing held item slot")?;
                    if raw > 8 {
                        bail!("invalid held item slot {raw}");
                    }
                    self.inventory.write().await.selected_hotbar = raw;
                    self.emit(Event::HeldItemChanged { slot: raw });
                }
                0x40 => {
                    let mut rest = p.as_slice();
                    let x = get_varint(&mut rest)?;
                    let z = get_varint(&mut rest)?;
                    let mut view = self.world_view.write().await;
                    view.center_x = x;
                    view.center_z = z;
                    let snapshot = **view;
                    drop(view);
                    self.emit(Event::WorldViewUpdated(snapshot));
                }
                0x41 => {
                    let mut rest = p.as_slice();
                    let distance = get_varint(&mut rest)?;
                    let mut view = self.world_view.write().await;
                    view.distance = distance;
                    let snapshot = **view;
                    drop(view);
                    self.emit(Event::WorldViewUpdated(snapshot));
                }
                0x42 => {
                    let mut c = Cursor::new(&p);
                    let position = unpack_position(c.read_u64::<BigEndian>()?);
                    self.survival.write().await.spawn_position = Some(position);
                    self.emit(Event::SpawnPosition(position));
                }
                0x43 => {
                    self.ui.write().await.apply_display(&p)?;
                    self.emit(Event::UiStateUpdated(UiUpdateKind::DisplayObjective));
                }
                0x44 => {
                    let (entity_id, metadata) = parse_metadata(&p)?;
                    if let Some(entity) = self.entities.write().await.entities.get_mut(&entity_id) {
                        entity.metadata.extend(metadata);
                        self.emit(Event::EntityUpdated(entity.clone()));
                    }
                }
                0x45 => {
                    let mut cursor = Cursor::new(&p);
                    let entity_id = cursor.read_i32::<BigEndian>()?;
                    let raw = cursor.read_i32::<BigEndian>()?;
                    let attached_to = (raw != -1).then_some(raw);
                    if let Some(entity) = self.entities.write().await.entities.get_mut(&entity_id) {
                        entity.attached_to = attached_to;
                    }
                    self.emit(Event::EntityAttached {
                        entity_id,
                        attached_to,
                    });
                }
                0x46 => {
                    let mut rest = p.as_slice();
                    let entity_id = get_varint(&mut rest)?;
                    let mut c = Cursor::new(rest);
                    let velocity = Vec3 {
                        x: f64::from(c.read_i16::<BigEndian>()?) / 8000.0,
                        y: f64::from(c.read_i16::<BigEndian>()?) / 8000.0,
                        z: f64::from(c.read_i16::<BigEndian>()?) / 8000.0,
                    };
                    if Some(entity_id) == self.player.lock().await.entity_id {
                        self.motion.lock().await.velocity = velocity;
                    }
                    if let Some(entity) = self.entities.write().await.entities.get_mut(&entity_id) {
                        entity.velocity = velocity;
                        self.emit(Event::EntityUpdated(entity.clone()));
                    }
                }
                0x47 => {
                    let mut rest = p.as_slice();
                    let entity_id = get_varint(&mut rest)?;
                    let mut equipment = Vec::new();
                    loop {
                        if equipment.len() >= 16 {
                            bail!("entity equipment packet exceeds 16 entries");
                        }
                        let raw_slot = *rest.first().context("missing equipment slot")?;
                        rest = &rest[1..];
                        equipment.push(((raw_slot & 0x7f) as i8, read_slot(&mut rest)?));
                        if raw_slot & 0x80 == 0 {
                            break;
                        }
                    }
                    if let Some(entity) = self.entities.write().await.entities.get_mut(&entity_id) {
                        entity.equipment.extend(equipment);
                        self.emit(Event::EntityUpdated(entity.clone()));
                    }
                }
                0x48 => {
                    let experience = parse_experience(&p)?;
                    self.survival.write().await.experience = experience;
                    self.emit(Event::Experience(experience));
                }
                0x49 => {
                    let vitals = parse_vitals(&p)?;
                    self.survival.write().await.vitals = Some(vitals);
                    self.emit(Event::Vitals(vitals));
                }
                0x4a => {
                    self.ui.write().await.apply_objective(&p)?;
                    self.emit(Event::UiStateUpdated(UiUpdateKind::Objective));
                }
                0x4b => {
                    let mut rest = p.as_slice();
                    let vehicle_id = get_varint(&mut rest)?;
                    let count = get_varint(&mut rest)?;
                    if !(0..=1024).contains(&count) {
                        bail!("invalid passenger count {count}");
                    }
                    let mut passengers = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        passengers.push(get_varint(&mut rest)?);
                    }
                    if let Some(vehicle) = self.entities.write().await.entities.get_mut(&vehicle_id)
                    {
                        vehicle.passengers = passengers.clone();
                    }
                    self.emit(Event::PassengersUpdated {
                        vehicle_id,
                        passengers,
                    });
                }
                0x4c => {
                    self.ui.write().await.apply_team(&p)?;
                    self.emit(Event::UiStateUpdated(UiUpdateKind::Team));
                }
                0x4d => {
                    self.ui.write().await.apply_score(&p)?;
                    self.emit(Event::UiStateUpdated(UiUpdateKind::Score));
                }
                0x4e => {
                    let mut c = Cursor::new(&p);
                    let mut state = self.survival.write().await;
                    state.world_age = c.read_i64::<BigEndian>()?;
                    state.time_of_day = c.read_i64::<BigEndian>()?;
                    drop(state);
                    self.emit(Event::SurvivalStateUpdated);
                }
                0x4f => {
                    self.ui.write().await.apply_title(&p)?;
                    self.emit(Event::UiStateUpdated(UiUpdateKind::Title));
                }
                0x50 => self.handle_entity_sound(&p)?,
                0x51 => self.handle_sound(&p, false)?,
                0x52 => {
                    let mut rest = p.as_slice();
                    let flags = *rest.first().context("missing stop-sound flags")?;
                    rest = &rest[1..];
                    let category = if flags & 1 != 0 {
                        Some(get_varint(&mut rest)?)
                    } else {
                        None
                    };
                    let sound_name = if flags & 2 != 0 {
                        Some(get_string(&mut rest)?)
                    } else {
                        None
                    };
                    self.emit(Event::StopSound(StopSoundEvent {
                        category,
                        sound_name,
                    }));
                }
                0x53 => {
                    self.ui.write().await.apply_tab(&p)?;
                    self.emit(Event::UiStateUpdated(UiUpdateKind::TabList));
                }
                0x54 => {
                    let mut rest = p.as_slice();
                    let transaction_id = get_varint(&mut rest)?;
                    self.emit(Event::NbtQueryResponse(NbtQueryResponse {
                        transaction_id,
                        nbt: Arc::from(rest.to_vec()),
                    }));
                }
                0x55 => {
                    let mut rest = p.as_slice();
                    self.emit(Event::ItemCollected(ItemCollected {
                        collected_entity_id: get_varint(&mut rest)?,
                        collector_entity_id: get_varint(&mut rest)?,
                        count: get_varint(&mut rest)?,
                    }));
                }
                0x56 => {
                    let mut rest = p.as_slice();
                    let entity_id = get_varint(&mut rest)?;
                    let mut c = Cursor::new(rest);
                    if let Some(entity) = self.entities.write().await.entities.get_mut(&entity_id) {
                        let position = Vec3 {
                            x: c.read_f64::<BigEndian>()?,
                            y: c.read_f64::<BigEndian>()?,
                            z: c.read_f64::<BigEndian>()?,
                        };
                        validate_position(position.x, position.y, position.z)?;
                        entity.position = position;
                        entity.yaw = f32::from(c.read_i8()?) * 360.0 / 256.0;
                        entity.pitch = f32::from(c.read_i8()?) * 360.0 / 256.0;
                        entity.on_ground = c.read_u8()? != 0;
                        self.emit(Event::EntityUpdated(entity.clone()));
                    }
                }
                0x57 => {
                    self.advancements.write().await.apply(&p)?;
                    self.emit(Event::AdvancementsUpdated);
                }
                0x58 => {
                    let (entity_id, attributes) = parse_attributes(&p)?;
                    if Some(entity_id) == self.player.lock().await.entity_id {
                        let mut state = self.survival.write().await;
                        for attribute in attributes {
                            state.attributes.insert(attribute.key.clone(), attribute);
                        }
                        drop(state);
                        self.emit(Event::SurvivalStateUpdated);
                    }
                }
                0x59 => {
                    let (entity_id, effect) = parse_effect(&p)?;
                    if Some(entity_id) == self.player.lock().await.entity_id {
                        self.survival
                            .write()
                            .await
                            .effects
                            .insert(effect.id, effect);
                        self.emit(Event::SurvivalStateUpdated);
                    }
                }
                0x5a => {
                    **self.server_recipes.write().await = parse_recipes(&p)?;
                    self.emit(Event::RecipesDeclared);
                }
                0x5b => {
                    **self.tags.write().await = parse_tags(&p)?;
                    self.emit(Event::TagsUpdated);
                }
                _ => {}
            }
            self.enforce_session_limits().await?;
        }
        Ok(())
    }
    async fn insert_entity(&self, entity: EntityState) -> Result<()> {
        validate_position(entity.position.x, entity.position.y, entity.position.z)?;
        if !entity.velocity.x.is_finite()
            || !entity.velocity.y.is_finite()
            || !entity.velocity.z.is_finite()
            || !entity.yaw.is_finite()
            || !entity.pitch.is_finite()
            || !entity.head_yaw.is_finite()
        {
            bail!("entity contains a non-finite motion or rotation");
        }
        let mut entities = self.entities.write().await;
        if !entities.entities.contains_key(&entity.entity_id)
            && entities.entities.len() >= self.connection_options.max_entities
        {
            bail!("entity cache limit exceeded");
        }
        entities.entities.insert(entity.entity_id, entity.clone());
        drop(entities);
        self.emit(Event::EntitySpawned(entity));
        Ok(())
    }

    async fn enforce_session_limits(&self) -> Result<()> {
        let limit = self.connection_options.max_cached_records;
        let players = self.players.read().await.entries.len();
        let maps = self.maps.read().await.maps.len();
        let entities = self.entities.read().await.entities.len();
        let inventory = self.inventory.read().await;
        let inventory_records = inventory.windows.values().map(Vec::len).sum::<usize>()
            + inventory.properties.len()
            + inventory.pending_clicks.len();
        drop(inventory);
        let ui = self.ui.read().await;
        let ui_records = ui.boss_bars.len()
            + ui.objectives.len()
            + ui.display_objectives.len()
            + ui.scores.len()
            + ui.teams.len();
        drop(ui);
        let progress = self.advancements.read().await;
        let progress_records = progress.definitions.len() + progress.progress.len();
        drop(progress);
        let statistics = self.statistics.read().await.values.len();
        let recipe_book = self.recipe_book.read().await;
        let recipe_records = recipe_book.unlocked.len() + recipe_book.displayed.len();
        let records = players
            .saturating_add(maps)
            .saturating_add(entities)
            .saturating_add(inventory_records)
            .saturating_add(ui_records)
            .saturating_add(progress_records)
            .saturating_add(statistics)
            .saturating_add(recipe_records);
        if records > limit {
            bail!("session cache contains {records} records, limit is {limit}");
        }
        Ok(())
    }
    async fn handle_position(&self, p: &[u8]) -> Result<()> {
        let mut c = Cursor::new(p);
        let x = c.read_f64::<BigEndian>()?;
        let y = c.read_f64::<BigEndian>()?;
        let z = c.read_f64::<BigEndian>()?;
        let yaw = c.read_f32::<BigEndian>()?;
        let pitch = c.read_f32::<BigEndian>()?;
        let flags = c.read_i8()? as u8;
        let mut rest = &p[c.position() as usize..];
        let teleport = get_varint(&mut rest)?;
        let mut s = self.player.lock().await;
        let next_x = if flags & 1 != 0 { s.x + x } else { x };
        let next_y = if flags & 2 != 0 { s.y + y } else { y };
        let next_z = if flags & 4 != 0 { s.z + z } else { z };
        validate_position(next_x, next_y, next_z)?;
        let next_yaw = if flags & 8 != 0 { s.yaw + yaw } else { yaw };
        let next_pitch = if flags & 16 != 0 {
            s.pitch + pitch
        } else {
            pitch
        };
        if !next_yaw.is_finite() || !next_pitch.is_finite() {
            bail!("server position contains a non-finite rotation");
        }
        s.x = next_x;
        s.y = next_y;
        s.z = next_z;
        s.yaw = next_yaw;
        s.pitch = next_pitch;
        let snapshot = s.clone();
        drop(s);
        let mut positioned = self.positioned.lock().await;
        let was_positioned = *positioned;
        *positioned = true;
        drop(positioned);
        if was_positioned {
            // A server teleport can move the player into chunks whose block data is still in
            // flight. Keep confirming server corrections, but briefly stop the 20 Hz movement
            // producer so stale/local-physics positions cannot race the new authoritative one.
            self.teleport_barrier_ticks.fetch_max(20, Ordering::AcqRel);
        }
        if was_positioned {
            if let Some(correction) = self.physics.lock().await.record_server_position(&snapshot) {
                **self.motion.lock().await = MotionState::default();
                self.emit(Event::PositionCorrection(correction));
            }
        } else if !was_positioned {
            **self.motion.lock().await = MotionState::default();
        }
        self.ready.notify_waiters();
        let mut payload = Vec::new();
        put_varint(&mut payload, teleport);
        self.send(0x00, &payload).await?;
        self.send_position().await?;
        self.emit(Event::Position(snapshot));
        Ok(())
    }
    fn handle_sound(&self, p: &[u8], named: bool) -> Result<()> {
        let mut rest = p;
        let (sound_id, sound_name) = if named {
            (None, Some(get_string(&mut rest)?))
        } else {
            let id = get_varint(&mut rest)?;
            (
                Some(id),
                crate::versions::java_1_16_1::registry::sound_name(id).map(str::to_owned),
            )
        };
        let category = get_varint(&mut rest)?;
        let mut c = Cursor::new(rest);
        let x = c.read_i32::<BigEndian>()? as f64 / 8.0;
        let y = c.read_i32::<BigEndian>()? as f64 / 8.0;
        let z = c.read_i32::<BigEndian>()? as f64 / 8.0;
        let volume = c.read_f32::<BigEndian>()?;
        let pitch = c.read_f32::<BigEndian>()?;
        self.emit(Event::Sound(SoundEvent {
            sound_id,
            sound_name,
            category,
            category_name: sound_category_name(category),
            source: SoundSource::Position { x, y, z },
            volume,
            pitch,
            sequence: self.sound_sequence.fetch_add(1, Ordering::Relaxed),
            received_at: self.connected_at.elapsed(),
        }));
        Ok(())
    }
    fn handle_entity_sound(&self, p: &[u8]) -> Result<()> {
        let mut rest = p;
        let sound_id = get_varint(&mut rest)?;
        let category = get_varint(&mut rest)?;
        let entity_id = get_varint(&mut rest)?;
        let mut c = Cursor::new(rest);
        let volume = c.read_f32::<BigEndian>()?;
        let pitch = c.read_f32::<BigEndian>()?;
        self.emit(Event::Sound(SoundEvent {
            sound_id: Some(sound_id),
            sound_name: crate::versions::java_1_16_1::registry::sound_name(sound_id)
                .map(str::to_owned),
            category,
            category_name: sound_category_name(category),
            source: SoundSource::Entity { entity_id },
            volume,
            pitch,
            sequence: self.sound_sequence.fetch_add(1, Ordering::Relaxed),
            received_at: self.connected_at.elapsed(),
        }));
        Ok(())
    }
    async fn wait_for_player_chunk(&self, p: &Player) -> Result<()> {
        loop {
            let notified = self.world_updated.notified();
            if self
                .world
                .lock()
                .await
                .has_chunk_at(p.x.floor() as i32, p.z.floor() as i32)
            {
                return Ok(());
            }
            timeout(Duration::from_secs(15), notified)
                .await
                .context("timed out waiting for player chunk")?;
        }
    }
    async fn send_position(&self) -> Result<()> {
        let p = self.player().await;
        let mut b = Vec::new();
        b.write_f64::<BigEndian>(p.x)?;
        b.write_f64::<BigEndian>(p.y)?;
        b.write_f64::<BigEndian>(p.z)?;
        b.write_f32::<BigEndian>(p.yaw)?;
        b.write_f32::<BigEndian>(p.pitch)?;
        b.push(p.on_ground as u8);
        self.send(0x13, &b).await?;
        self.physics.lock().await.record_movement(p);
        Ok(())
    }
    async fn send(&self, id: i32, p: &[u8]) -> Result<()> {
        let mut w = self.writer.lock().await;
        let compression = w.compression;
        Ok(write_packet(&mut w.inner, compression, id, p).await?)
    }
    async fn send_entity_action(&self, action: i32) -> Result<()> {
        let entity_id = self
            .player
            .lock()
            .await
            .entity_id
            .context("Join Game entity ID is unavailable")?;
        let mut payload = Vec::new();
        put_varint(&mut payload, entity_id);
        put_varint(&mut payload, action);
        put_varint(&mut payload, 0);
        self.send(0x1c, &payload).await
    }
    fn emit(&self, event: Event) {
        let terminal = matches!(
            event,
            Event::Disconnected { .. }
                | Event::Error {
                    kind: "connection",
                    ..
                }
        );
        if terminal {
            if self.terminal_emitted.swap(true, Ordering::AcqRel) {
                return;
            }
            self.stopped.store(true, Ordering::Release);
            self.cancel.notify_waiters();
            self.ready.notify_waiters();
            self.world_updated.notify_waiters();
        }
        let _ = self.events.send(event);
    }
}
fn parse_tab_completion(payload: &[u8]) -> Result<TabCompletion> {
    let mut rest = payload;
    let transaction_id = get_varint(&mut rest)?;
    let start = get_varint(&mut rest)?;
    let length = get_varint(&mut rest)?;
    let count = get_varint(&mut rest)?;
    if !(0..=4096).contains(&count) {
        bail!("invalid completion match count {count}");
    }
    let mut matches = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let value = get_string(&mut rest)?;
        let has_tooltip = *rest.first().context("missing completion tooltip flag")? != 0;
        rest = &rest[1..];
        let tooltip_json = if has_tooltip {
            Some(get_string(&mut rest)?)
        } else {
            None
        };
        matches.push(TabCompletionMatch {
            value,
            tooltip_json,
        });
    }
    Ok(TabCompletion {
        transaction_id,
        start,
        length,
        matches,
    })
}
async fn wait_for_nbt(
    events: &mut broadcast::Receiver<Event>,
    transaction_id: i32,
    wait: Duration,
) -> Result<NbtQueryResponse> {
    Ok(timeout(wait, async {
        loop {
            match events.recv().await {
                Ok(Event::NbtQueryResponse(response))
                    if response.transaction_id == transaction_id =>
                {
                    return Ok::<_, anyhow::Error>(response);
                }
                Ok(Event::Disconnected { reason }) => bail!("disconnected: {reason}"),
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => bail!("event stream closed"),
            }
        }
    })
    .await
    .context("timed out waiting for NBT query response")??)
}
fn parse_world_event(payload: &[u8]) -> Result<WorldEvent> {
    let mut cursor = Cursor::new(payload);
    Ok(WorldEvent {
        effect_id: cursor.read_i32::<BigEndian>()?,
        position: BlockPos::unpack(cursor.read_u64::<BigEndian>()?),
        data: cursor.read_i32::<BigEndian>()?,
        global: cursor.read_u8()? != 0,
    })
}
fn parse_particle(payload: &[u8]) -> Result<WorldParticleEvent> {
    let mut cursor = Cursor::new(payload);
    let particle_id = cursor.read_i32::<BigEndian>()?;
    let long_distance = cursor.read_u8()? != 0;
    let position = Vec3 {
        x: cursor.read_f64::<BigEndian>()?,
        y: cursor.read_f64::<BigEndian>()?,
        z: cursor.read_f64::<BigEndian>()?,
    };
    let offset = [
        cursor.read_f32::<BigEndian>()?,
        cursor.read_f32::<BigEndian>()?,
        cursor.read_f32::<BigEndian>()?,
    ];
    let speed = cursor.read_f32::<BigEndian>()?;
    let count = cursor.read_i32::<BigEndian>()?;
    validate_position(position.x, position.y, position.z)?;
    if offset.iter().any(|value| !value.is_finite()) || !speed.is_finite() {
        bail!("particle contains a non-finite offset or speed");
    }
    let raw_data = payload[cursor.position() as usize..].to_vec();
    let mut rest = raw_data.as_slice();
    let data = match particle_id {
        3 | 23 => crate::versions::java_1_16_1::ParticleData::BlockState(get_varint(&mut rest)?),
        14 => crate::versions::java_1_16_1::ParticleData::Dust {
            red: read_f32_slice(&mut rest)?,
            green: read_f32_slice(&mut rest)?,
            blue: read_f32_slice(&mut rest)?,
            scale: read_f32_slice(&mut rest)?,
        },
        34 => crate::versions::java_1_16_1::ParticleData::Item(read_slot(&mut rest)?),
        _ => crate::versions::java_1_16_1::ParticleData::None,
    };
    Ok(WorldParticleEvent {
        particle_id,
        long_distance,
        position,
        offset,
        speed,
        count,
        data,
        raw_data,
    })
}
fn parse_explosion(payload: &[u8]) -> Result<ExplosionEvent> {
    let mut cursor = Cursor::new(payload);
    let position = Vec3 {
        x: f64::from(cursor.read_f32::<BigEndian>()?),
        y: f64::from(cursor.read_f32::<BigEndian>()?),
        z: f64::from(cursor.read_f32::<BigEndian>()?),
    };
    let radius = cursor.read_f32::<BigEndian>()?;
    validate_position(position.x, position.y, position.z)?;
    if !radius.is_finite() || !(0.0..=1024.0).contains(&radius) {
        bail!("invalid explosion radius {radius}");
    }
    let count = cursor.read_i32::<BigEndian>()?;
    if !(0..=1_000_000).contains(&count) {
        bail!("invalid explosion affected-block count {count}");
    }
    let remaining = payload.len().saturating_sub(cursor.position() as usize);
    let required = (count as usize)
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add(12))
        .context("explosion affected-block length overflow")?;
    if required > remaining {
        bail!("explosion affected-block count exceeds packet length");
    }
    let origin = BlockPos {
        x: position.x.floor() as i32,
        y: position.y.floor() as i32,
        z: position.z.floor() as i32,
    };
    let mut affected_blocks = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let offset_x = i32::from(cursor.read_i8()?);
        let offset_y = i32::from(cursor.read_i8()?);
        let offset_z = i32::from(cursor.read_i8()?);
        affected_blocks.push(BlockPos {
            x: origin
                .x
                .checked_add(offset_x)
                .context("explosion x overflow")?,
            y: origin
                .y
                .checked_add(offset_y)
                .context("explosion y overflow")?,
            z: origin
                .z
                .checked_add(offset_z)
                .context("explosion z overflow")?,
        });
    }
    let player_motion = Vec3 {
        x: f64::from(cursor.read_f32::<BigEndian>()?),
        y: f64::from(cursor.read_f32::<BigEndian>()?),
        z: f64::from(cursor.read_f32::<BigEndian>()?),
    };
    if !player_motion.x.is_finite() || !player_motion.y.is_finite() || !player_motion.z.is_finite()
    {
        bail!("explosion contains non-finite player motion");
    }
    Ok(ExplosionEvent {
        position,
        radius,
        affected_blocks,
        player_motion,
    })
}

fn validate_position(x: f64, y: f64, z: f64) -> Result<()> {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        bail!("position contains a non-finite coordinate");
    }
    if x.abs() > 30_000_000.0 || z.abs() > 30_000_000.0 || y.abs() > 2048.0 {
        bail!("position is outside the supported Minecraft coordinate range");
    }
    Ok(())
}
fn read_f32_slice(rest: &mut &[u8]) -> Result<f32> {
    let mut cursor = Cursor::new(*rest);
    let value = cursor.read_f32::<BigEndian>()?;
    *rest = &rest[cursor.position() as usize..];
    Ok(value)
}
fn sound_category_name(id: i32) -> Option<&'static str> {
    [
        "master", "music", "record", "weather", "block", "hostile", "neutral", "player", "ambient",
        "voice",
    ]
    .get(id as usize)
    .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::AsyncReadExt, net::TcpListener};
    #[test]
    fn explosion_offsets_are_relative_to_floored_center() {
        let mut packet = Vec::new();
        for value in [10.75f32, 64.5, -2.25, 4.0] {
            packet.extend(value.to_be_bytes());
        }
        packet.extend(2i32.to_be_bytes());
        packet.extend([1u8, 0, 255, 255, 2, 0]);
        for value in [0.1f32, 0.2, 0.3] {
            packet.extend(value.to_be_bytes());
        }
        let event = parse_explosion(&packet).unwrap();
        assert_eq!(
            event.affected_blocks,
            vec![
                BlockPos {
                    x: 11,
                    y: 64,
                    z: -4
                },
                BlockPos { x: 9, y: 66, z: -3 }
            ]
        );
        assert!((event.player_motion.y - 0.2).abs() < 1.0e-6);
    }

    #[test]
    fn explosion_rejects_non_finite_coordinates() {
        let mut packet = Vec::new();
        for value in [f32::NAN, 64.0, 0.0, 4.0] {
            packet.extend(value.to_be_bytes());
        }
        packet.extend(0_i32.to_be_bytes());
        for value in [0.0_f32, 0.0, 0.0] {
            packet.extend(value.to_be_bytes());
        }
        assert!(parse_explosion(&packet).is_err());
    }

    #[tokio::test]
    async fn dropping_last_bot_handle_closes_connection_tasks() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            read_packet(&mut reader, None).await.unwrap();
            read_packet(&mut reader, None).await.unwrap();
            write_packet(&mut writer, None, 0x02, &[]).await.unwrap();
            let mut byte = [0_u8; 1];
            tokio::time::timeout(Duration::from_secs(2), reader.read_exact(&mut byte))
                .await
                .expect("client tasks retained the connection")
                .expect_err("client unexpectedly wrote play data");
        });
        let bot = Bot::connect(
            Server::new("127.0.0.1", port),
            Player::offline("DropProbe"),
            Arc::new(crate::versions::java_1_16_1::SharedChunkStorage::default()),
            ConnectionOptions::default(),
        )
        .await
        .unwrap();
        drop(bot);
        server.await.unwrap();
    }
}
