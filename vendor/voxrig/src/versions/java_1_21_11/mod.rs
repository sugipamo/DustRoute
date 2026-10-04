//! Java 1.21.11 wire protocol (774), registries and client behavior.
mod client;
mod component_nbt;
#[allow(dead_code)]
mod ids;
mod math;
mod piston_nbt;
pub mod reconstruction;
#[cfg(test)]
mod recovery_tests;
mod wire;
mod world;
pub(crate) use client::Bot;
pub use client::operations;
pub use client::players;
pub use client::raycast;
pub use client::recording;
pub use client::{PacketRecord, PacketTrace};

fn registry() -> &'static crate::block_state::StateRegistry {
    static REGISTRY: std::sync::OnceLock<crate::block_state::StateRegistry> =
        std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        crate::tables::validate_shapes(
            crate::tables::java_1_21_11::STATES,
            &crate::tables::java_1_21_11::COLLISION,
        );
        crate::block_state::StateRegistry::new(crate::tables::java_1_21_11::STATES)
            .expect("valid bundled 1.21.11 registry")
    })
}

/// Interprets an ID only in the Java 1.21.11 registry.
pub fn native_state(id: i32) -> crate::Result<crate::NativeBlockState> {
    registry().decode(id)
}

/// Resolves a complete native state using only the Java 1.21.11 registry.
pub fn state_id(state: &crate::NativeBlockState) -> crate::Result<i32> {
    registry().encode(state)
}

fn validate_state_id(id: i32) -> anyhow::Result<()> {
    registry().validate_id(id)?;
    Ok(())
}
