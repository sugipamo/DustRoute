//! Explicit version implementations. Wire IDs and registries never select a version implicitly.

pub mod java_1_16_1;
pub mod java_1_21_11;

/// Explicit Minecraft wire and registry version for one connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize)]
pub enum MinecraftVersion {
    /// Existing Java 1.16.1 implementation (protocol 736).
    Java1_16_1,
    /// Java 1.21.11 (protocol 774); availability is checked before connecting.
    Java1_21_11,
}

impl MinecraftVersion {
    /// The game's exact release name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Java1_16_1 => "1.16.1",
            Self::Java1_21_11 => "1.21.11",
        }
    }

    /// The exact wire protocol number, not a runtime capability declaration.
    pub const fn protocol(self) -> i32 {
        match self {
            Self::Java1_16_1 => 736,
            Self::Java1_21_11 => 774,
        }
    }
}
