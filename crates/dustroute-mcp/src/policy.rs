use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use dustroute_translate::RegionBounds;
use serde::{Deserialize, Serialize};

use crate::discovery::RegionBoundsDto;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpPolicy {
    pub read_only: bool,
    pub preview_required: bool,
    pub max_scan_volume: usize,
    pub max_placement_blocks: usize,
    pub allowed_region: Option<RegionBoundsDto>,
    pub allowed_players: BTreeSet<String>,
    pub allowed_dimensions: BTreeSet<String>,
}

impl Default for McpPolicy {
    fn default() -> Self {
        Self {
            read_only: true,
            preview_required: true,
            max_scan_volume: 262_144,
            max_placement_blocks: 32_768,
            allowed_region: None,
            allowed_players: BTreeSet::new(),
            allowed_dimensions: BTreeSet::from(["minecraft:overworld".to_owned()]),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyError {
    PlayerDenied(String),
    OutsideAllowedRegion,
    ScanVolumeExceeded { actual: usize, limit: usize },
    PlacementLimitExceeded { actual: usize, limit: usize },
    MutationDenied,
    DimensionDenied(String),
}

impl Display for PolicyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerDenied(player) => write!(f, "player {player:?} is not allowed"),
            Self::OutsideAllowedRegion => f.write_str("region is outside the configured bounds"),
            Self::ScanVolumeExceeded { actual, limit } => {
                write!(f, "scan volume {actual} exceeds the {limit} block limit")
            }
            Self::PlacementLimitExceeded { actual, limit } => {
                write!(f, "placement size {actual} exceeds the {limit} block limit")
            }
            Self::MutationDenied => f.write_str("world mutation is disabled by read-only policy"),
            Self::DimensionDenied(dimension) => {
                write!(f, "dimension {dimension:?} is not allowed")
            }
        }
    }
}

impl Error for PolicyError {}

impl McpPolicy {
    pub fn from_environment() -> Result<Self, String> {
        let mut policy = Self::default();
        if let Ok(value) = std::env::var("DUSTROUTE_ALLOWED_PLAYERS") {
            policy.allowed_players = csv(&value);
        }
        if let Ok(value) = std::env::var("DUSTROUTE_ALLOWED_DIMENSIONS") {
            policy.allowed_dimensions = csv(&value);
            if policy.allowed_dimensions.is_empty() {
                return Err("DUSTROUTE_ALLOWED_DIMENSIONS must not be empty".to_owned());
            }
        }
        if let Ok(value) = std::env::var("DUSTROUTE_ALLOWED_REGION") {
            let values = value
                .split(',')
                .map(|part| part.trim().parse::<i32>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("invalid DUSTROUTE_ALLOWED_REGION: {error}"))?;
            let [min_x, min_y, min_z, max_x, max_y, max_z] = values.as_slice() else {
                return Err(
                    "DUSTROUTE_ALLOWED_REGION requires minX,minY,minZ,maxX,maxY,maxZ".to_owned(),
                );
            };
            policy.allowed_region = Some(
                RegionBounds::new(
                    dustroute_physical::Pos::new(*min_x, *min_y, *min_z),
                    dustroute_physical::Pos::new(*max_x, *max_y, *max_z),
                )
                .into(),
            );
        }
        policy.max_scan_volume = env_usize("DUSTROUTE_MAX_SCAN_VOLUME", policy.max_scan_volume)?;
        policy.max_placement_blocks = env_usize(
            "DUSTROUTE_MAX_PLACEMENT_BLOCKS",
            policy.max_placement_blocks,
        )?;
        policy.read_only = env_bool("DUSTROUTE_READ_ONLY", policy.read_only)?;
        policy.preview_required = env_bool("DUSTROUTE_PREVIEW_REQUIRED", policy.preview_required)?;
        Ok(policy)
    }

    pub fn authorize_player(&self, player: &str) -> Result<(), PolicyError> {
        if self.allowed_players.is_empty() || self.allowed_players.contains(player) {
            Ok(())
        } else {
            Err(PolicyError::PlayerDenied(player.to_owned()))
        }
    }

    pub fn validate_region(&self, bounds: RegionBounds) -> Result<(), PolicyError> {
        let volume = axis_len(bounds.min.x, bounds.max.x)
            .saturating_mul(axis_len(bounds.min.y, bounds.max.y))
            .saturating_mul(axis_len(bounds.min.z, bounds.max.z));
        if volume > self.max_scan_volume {
            return Err(PolicyError::ScanVolumeExceeded {
                actual: volume,
                limit: self.max_scan_volume,
            });
        }
        if let Some(allowed) = self.allowed_region {
            let allowed: RegionBounds = allowed.into();
            if !allowed.contains(bounds.min) || !allowed.contains(bounds.max) {
                return Err(PolicyError::OutsideAllowedRegion);
            }
        }
        Ok(())
    }

    pub fn authorize_dimension(&self, dimension: &str) -> Result<(), PolicyError> {
        if self.allowed_dimensions.contains(dimension) {
            Ok(())
        } else {
            Err(PolicyError::DimensionDenied(dimension.to_owned()))
        }
    }

    pub fn validate_placement_size(&self, blocks: usize) -> Result<(), PolicyError> {
        if blocks > self.max_placement_blocks {
            Err(PolicyError::PlacementLimitExceeded {
                actual: blocks,
                limit: self.max_placement_blocks,
            })
        } else {
            Ok(())
        }
    }

    pub fn authorize_mutation(&self) -> Result<(), PolicyError> {
        if self.read_only {
            Err(PolicyError::MutationDenied)
        } else {
            Ok(())
        }
    }
}

fn csv(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn env_usize(name: &str, default: usize) -> Result<usize, String> {
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| format!("invalid {name}: {error}")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(format!("invalid {name}: {error}")),
    }
}

fn env_bool(name: &str, default: bool) -> Result<bool, String> {
    match std::env::var(name) {
        Ok(value) => match value.as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => Err(format!("invalid {name}: expected true or false")),
        },
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(format!("invalid {name}: {error}")),
    }
}

fn axis_len(a: i32, b: i32) -> usize {
    i64::from(a.max(b))
        .saturating_sub(i64::from(a.min(b)))
        .saturating_add(1)
        .try_into()
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use dustroute_physical::Pos;

    use super::*;

    #[test]
    fn enforces_player_region_and_volume_limits() {
        let policy = McpPolicy {
            allowed_players: BTreeSet::from(["builder".to_owned()]),
            allowed_region: Some(RegionBoundsDto {
                min: Pos::new(0, 0, 0),
                max: Pos::new(20, 20, 20),
            }),
            max_scan_volume: 1000,
            allowed_dimensions: BTreeSet::from(["minecraft:overworld".to_owned()]),
            ..McpPolicy::default()
        };
        assert!(policy.authorize_player("builder").is_ok());
        assert!(matches!(
            policy.authorize_player("visitor"),
            Err(PolicyError::PlayerDenied(_))
        ));
        assert!(
            policy
                .validate_region(RegionBounds::new(Pos::new(1, 1, 1), Pos::new(5, 5, 5)))
                .is_ok()
        );
        assert_eq!(
            policy.validate_region(RegionBounds::new(Pos::new(-1, 1, 1), Pos::new(5, 5, 5))),
            Err(PolicyError::OutsideAllowedRegion)
        );
        assert!(policy.authorize_dimension("minecraft:overworld").is_ok());
        assert!(matches!(
            policy.authorize_dimension("minecraft:the_nether"),
            Err(PolicyError::DimensionDenied(_))
        ));
        assert_eq!(
            policy.authorize_mutation(),
            Err(PolicyError::MutationDenied)
        );
    }
}
