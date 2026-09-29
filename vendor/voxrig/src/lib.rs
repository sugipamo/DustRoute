//! A headless Minecraft client with explicitly selected Java version adapters.
//!
//! `Voxrig` implements the protocol-facing body of an agent: connection
//! management, world and entity observation, player state, physics, inventory,
//! crafting, containers, interaction, combat, chat, and structured sound events.
//! It deliberately does not provide pathfinding, semantic perception, planning,
//! memory, or an AI runtime. Those belong in a consumer crate.
//!
//! The established `Bot` API targets Java 1.16.1 (736). [`Client`] also offers a
//! limited Java 1.21.11 (774) adapter for native block observations, packet traces
//! and use-on-block interactions. [`Client::observe_client_region`] additionally
//! reconstructs a bounded set of piston and neighbor effects, including independent
//! moving carriers, while preserving the unchanged received-state view.
//! Neither adapter's local cache is independent confirmation of server state.
//! It does not implement Microsoft authentication or online-mode encryption.
//!
//! # Quick start
//!
//! ```no_run
//! use voxrig::prelude::*;
//!
//! # async fn run() -> anyhow::Result<()> {
//! let manager = BotManager::new(Server::new("127.0.0.1", 25565));
//! let bot = manager.connect(Player::offline("AgentOne")).await?;
//! bot.wait_until_ready().await?;
//!
//! let player = bot.player().await;
//! let nearby_blocks = bot.observe(4).await?;
//! let nearby_entities = bot.observe_entities(16.0).await;
//! println!("{player:?} {} {}", nearby_blocks.len(), nearby_entities.len());
//!
//! bot.set_control(ControlState {
//!     forward: true,
//!     ..ControlState::default()
//! }).await;
//! bot.jump().await?;
//! bot.clear_control().await;
//! bot.disconnect().await?;
//! # Ok(())
//! # }
//! ```
//!
//! See `docs/api.md` in the repository for the complete public API map and
//! `docs/architecture.md` for the ownership boundary with external controllers.
#![warn(missing_docs)]

pub mod block_state;
pub mod connection;
mod error;
mod protocol;
pub mod snapshot;
pub mod versions;

// The established 1.16.1 API remains source-compatible and explicitly pinned.
pub use block_state::NativeBlockState;
pub use connection::{Client, ConnectionConfig, Observation, ObservedBlock, Region};
pub use versions::MinecraftVersion;
pub use versions::java_1_16_1::*;
