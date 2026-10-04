//! Minecraft Java 1.16.1 implementation, registry and movement behavior.
//! Existing APIs are re-exported at the crate root for compatibility.

pub mod chat;
pub mod client;
mod collision;
pub mod entity;
pub(crate) use crate::error;
pub mod interaction;
pub mod inventory;
pub mod manager;
pub mod map;
pub mod physics;
pub mod progress;
pub(crate) mod protocol {
    pub(crate) use crate::protocol::*;
    pub const PROTOCOL_VERSION: i32 = 736;
}
pub mod registry;
pub mod server_registry;
pub use crate::snapshot;
pub mod survival;
pub mod ui;
pub mod unstable;
pub mod world;

/// Decodes a state ID exclusively using the Java 1.16.1 registry.
pub fn native_state(id: i32) -> crate::Result<crate::NativeBlockState> {
    static STATES: std::sync::OnceLock<crate::block_state::StateRegistry> =
        std::sync::OnceLock::new();
    STATES
        .get_or_init(|| {
            crate::block_state::StateRegistry::new(crate::tables::java_1_16_1::STATES)
                .expect("bundled Java 1.16.1 state definitions are valid")
        })
        .decode(id)
}

/// Common imports for applications that control one or more clients.
pub mod prelude {
    pub use crate::versions::java_1_16_1::{
        BlockFace, BlockPos, Bot, BotManager, ClickMode, ControlState, Error, ErrorKind, Event,
        Hand, Player, Result, Server,
    };
}

// Root re-exports are retained as the concise and backwards-compatible API.
pub use chat::{ChatMessage, PlayerList, PlayerListEntry, PlayerProperty};
pub use client::{
    BlockActionEvent, Bot, CLIENT_INFO, ChatMode, ClientCapabilities, ClientInfo, ClientSettings,
    ConnectionOptions, Event, ExplosionEvent, MainHand, NbtQueryResponse, Player, ProtocolInfo,
    ResourcePackRequest, ResourcePackStatus, Server, SoundEvent, SoundSource, StopSoundEvent,
    TabCompletion, TabCompletionMatch, WorldEvent, WorldParticleEvent, WorldViewState,
};
pub use entity::{
    EntityKind, EntityRaycastHit, EntityState, EntityTracker, MetadataValue, ParticleData,
};
pub use error::{Error, ErrorKind, Result};
pub use interaction::{
    BlockBreakProgress, BlockFace, BlockPos, BlockRaycastHit, DiggingAcknowledgement, DiggingInfo,
    DiggingStatus, Hand, PlacementInfo, PlacementRequest,
};
pub use inventory::{
    ClickMode, EquipmentSlot, InventoryState, ItemCollected, ItemStack, MerchantOffer,
    MerchantOffers, OpenWindow, PendingClick, SlotUpdate, WindowProperty, WindowTransaction,
};
pub use manager::{BotEvent, BotManager};
pub use map::{MapData, MapIcon, MapRectangle, MapStore, MapUpdate};
pub use physics::{
    Aabb, ControlState, CorrectionReason, MotionState, PhysicsMetrics, PositionCorrection, Vec3,
    VehicleControl, VehiclePose,
};
pub use progress::{
    AdvancementDefinition, AdvancementDisplay, AdvancementProgress, AdvancementState,
    RecipeBookState, StatisticsState,
};
pub use registry::{
    MiningInfo, RawRecipe, RecipeResult, block_name_from_state, entity_dimensions, entity_name,
    item_name, mining_info, recipes_for_output, sound_name,
};
pub use server_registry::{
    CommandNode, CommandNodeKind, CommandTree, Ingredient, ServerRecipe, ServerRecipeData,
    ServerRecipes, ServerTags,
};
pub use snapshot::Snapshot;
pub use survival::{
    Attribute, AttributeModifier, CombatEvent, Difficulty, Experience, GameStateChange,
    RespawnState, SpawnPosition, StatusEffect, SurvivalState, Vitals,
};
pub use ui::{BossBar, ScoreboardObjective, Team, TitleState, UiState, UiUpdateKind, WorldBorder};
pub use unstable::UnstableBot;
pub use world::{
    BlockEntityData, BlockObservation, BlockRegion, ChunkPos, ChunkSnapshot, ChunkStorageStats,
    EnvironmentState, FluidKind, SharedChunkStorage,
};
