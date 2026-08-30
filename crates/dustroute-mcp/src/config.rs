use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpConfig {
    pub server_address: String,
    pub assist_player: String,
    pub bridge_address: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum McpConfigError {
    Missing(&'static str),
    InvalidServerAddress(String),
    InvalidBridgeAddress(String),
    InvalidPlayerName,
}

impl Display for McpConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(name) => write!(f, "required environment variable {name} is not set"),
            Self::InvalidServerAddress(value) => {
                write!(
                    f,
                    "invalid DUSTROUTE_SERVER_ADDRESS {value:?}; expected host:port"
                )
            }
            Self::InvalidBridgeAddress(value) => {
                write!(
                    f,
                    "invalid DUSTROUTE_BOT_BRIDGE {value:?}; expected host:port"
                )
            }
            Self::InvalidPlayerName => f.write_str(
                "invalid DUSTROUTE_ASSIST_PLAYER; expected a non-empty Minecraft player name",
            ),
        }
    }
}

impl Error for McpConfigError {}

impl McpConfig {
    pub fn from_environment() -> Result<Self, McpConfigError> {
        let server_address = std::env::var("DUSTROUTE_SERVER_ADDRESS")
            .map_err(|_| McpConfigError::Missing("DUSTROUTE_SERVER_ADDRESS"))?;
        let assist_player = std::env::var("DUSTROUTE_ASSIST_PLAYER")
            .map_err(|_| McpConfigError::Missing("DUSTROUTE_ASSIST_PLAYER"))?;
        let bridge_address =
            std::env::var("DUSTROUTE_BOT_BRIDGE").unwrap_or_else(|_| "127.0.0.1:25580".to_owned());
        Self::new(server_address, assist_player, bridge_address)
    }

    pub fn new(
        server_address: impl Into<String>,
        assist_player: impl Into<String>,
        bridge_address: impl Into<String>,
    ) -> Result<Self, McpConfigError> {
        let server_address = server_address.into();
        let assist_player = assist_player.into();
        let bridge_address = bridge_address.into();
        if !valid_host_port(&server_address) {
            return Err(McpConfigError::InvalidServerAddress(server_address));
        }
        if !valid_host_port(&bridge_address) {
            return Err(McpConfigError::InvalidBridgeAddress(bridge_address));
        }
        if assist_player.is_empty()
            || assist_player.len() > 16
            || !assist_player
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(McpConfigError::InvalidPlayerName);
        }
        Ok(Self {
            server_address,
            assist_player,
            bridge_address,
        })
    }
}

fn valid_host_port(value: &str) -> bool {
    let Some((host, port)) = value.rsplit_once(':') else {
        return false;
    };
    !host.is_empty()
        && !host.chars().any(char::is_whitespace)
        && port.parse::<u16>().is_ok_and(|port| port != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_required_connection_settings() {
        let config = McpConfig::new("mc.example:25565", "Builder", "127.0.0.1:25580").unwrap();
        assert_eq!(config.server_address, "mc.example:25565");
        assert_eq!(config.assist_player, "Builder");
        assert!(McpConfig::new("mc.example", "Builder", "127.0.0.1:25580").is_err());
        assert!(McpConfig::new("mc.example:25565", "", "127.0.0.1:25580").is_err());
    }
}
