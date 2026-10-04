//! Circuit discovery depends only on the bridge and read policy. It does not
//! dispatch MCP tools or own selection, operation, or persistence state.
use crate::failure::{CauseKind, FailureCause};
use crate::performance::{Phase, span};
use crate::{BotBridge, CircuitDiscovery, McpPolicy, discover_connected_region};
use dustroute_physical::Pos;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) struct CircuitCapture<'a> {
    pub bridge: &'a BotBridge,
    pub policy: &'a McpPolicy,
}

/// Acquisition facts stay typed in the circuit cache. This is descriptive
/// evidence, not a fresh observation or permission to modify the region.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub(super) enum ExpansionEvidence {
    AdjacentComponentFloodFill {
        components_loaded: usize,
        component_limit: usize,
        limit_reached: bool,
        scanned_tiles: usize,
        scanned_block_positions: usize,
    },
    ExplicitWorkRegion {
        limit_reached: bool,
        scope: &'static str,
    },
    ExplicitSelectedRegion {
        #[serde(skip_serializing_if = "Option::is_none")]
        components_loaded: Option<usize>,
        component_limit: Option<usize>,
        limit_reached: bool,
    },
    #[cfg(test)]
    #[serde(untagged)]
    Unspecified {},
}
impl ExpansionEvidence {
    pub(super) fn recorded(&self) -> crate::recorded_analysis::RecordedExpansion {
        use crate::recorded_analysis::RecordedExpansion as Record;
        match *self {
            Self::AdjacentComponentFloodFill {
                components_loaded,
                component_limit,
                limit_reached,
                scanned_tiles,
                scanned_block_positions,
            } => Record::AdjacentComponentFloodFill {
                components_loaded,
                component_limit,
                limit_reached,
                scanned_tiles,
                scanned_block_positions,
            },
            Self::ExplicitWorkRegion {
                limit_reached,
                scope,
            } => Record::ExplicitWorkRegion {
                limit_reached,
                scope,
            },
            Self::ExplicitSelectedRegion {
                components_loaded,
                component_limit,
                limit_reached,
            } => Record::ExplicitSelectedRegion {
                components_loaded,
                component_limit,
                limit_reached,
            },
            #[cfg(test)]
            Self::Unspecified {} => Record::Unspecified {},
        }
    }
    pub(super) fn components_loaded(&self) -> Option<usize> {
        match self {
            Self::AdjacentComponentFloodFill {
                components_loaded, ..
            } => Some(*components_loaded),
            Self::ExplicitSelectedRegion {
                components_loaded, ..
            } => *components_loaded,
            Self::ExplicitWorkRegion { .. } => None,
            #[cfg(test)]
            Self::Unspecified {} => None,
        }
    }
    pub(super) fn limit_reached(&self) -> bool {
        match self {
            Self::AdjacentComponentFloodFill { limit_reached, .. }
            | Self::ExplicitWorkRegion { limit_reached, .. }
            | Self::ExplicitSelectedRegion { limit_reached, .. } => *limit_reached,
            #[cfg(test)]
            Self::Unspecified {} => false,
        }
    }
}

pub(super) struct DiscoveryObservation {
    pub candidate: CircuitDiscovery,
    pub dimension: String,
    pub expansion: ExpansionEvidence,
}
impl DiscoveryObservation {
    pub fn response(&self) -> DiscoveryResponse<'_> {
        DiscoveryResponse {
            ok: true,
            candidate: &self.candidate,
            expansion: &self.expansion,
            warning: self
                .expansion
                .limit_reached()
                .then_some("the circuit exceeds the component limit; analysis is incomplete"),
            next_step: "call show_region and ask the player to confirm the highlighted candidate",
        }
    }
}

#[derive(Serialize)]
pub(super) struct DiscoveryResponse<'a> {
    ok: bool,
    candidate: &'a CircuitDiscovery,
    expansion: &'a ExpansionEvidence,
    warning: Option<&'static str>,
    next_step: &'static str,
}

#[derive(Debug)]
pub(super) struct AdaptiveComponentScan {
    pub(super) snapshot: dustroute_translate::snapshot::MinecraftSnapshot,
    pub(super) component_count: usize,
    pub(super) component_limit: usize,
    pub(super) limit_reached: bool,
    pub(super) scanned_tiles: usize,
    pub(super) scanned_block_positions: usize,
}

impl CircuitCapture<'_> {
    pub(super) async fn discover(
        &self,
        player: &str,
        max_components: usize,
        padding: i32,
        fragment_gap: u32,
    ) -> Result<DiscoveryObservation, FailureCause> {
        for (name, actual, min, max) in [
            ("max_components", max_components as f64, 1.0, 32768.0),
            ("padding", f64::from(padding), 0.0, 8.0),
            ("fragment_gap", f64::from(fragment_gap), 1.0, 16.0),
        ] {
            if !(min..=max).contains(&actual) {
                return Err(FailureCause::input_range(name, actual, min, max));
            }
        }
        let observation = self
            .bridge
            .observe_player(player, 64.0)
            .await
            .map_err(FailureCause::from)?;
        let target = observation.targeted_block.ok_or_else(|| {
            FailureCause::new(CauseKind::NotFound, "the player is not looking at a block")
        })?;
        self.policy
            .authorize_dimension(&observation.dimension)
            .map_err(FailureCause::from)?;
        let scan = self
            .scan_connected_components(
                target,
                &observation.dimension,
                max_components,
                fragment_gap as i32,
            )
            .await?;
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(
            scan.snapshot.min,
            scan.snapshot.max,
        );
        let world = dustroute_translate::snapshot::world_from_snapshot(&scan.snapshot)
            .map_err(FailureCause::from)?;
        let analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
        let candidate =
            discover_connected_region(&analysis, target, 2, fragment_gap, padding, usize::MAX)
                .map_err(FailureCause::from)?;
        Ok(DiscoveryObservation {
            candidate,
            dimension: observation.dimension,
            expansion: ExpansionEvidence::AdjacentComponentFloodFill {
                components_loaded: scan.component_count,
                component_limit: scan.component_limit,
                limit_reached: scan.limit_reached,
                scanned_tiles: scan.scanned_tiles,
                scanned_block_positions: scan.scanned_block_positions,
            },
        })
    }

    pub(super) async fn scan_connected_components(
        &self,
        target: Pos,
        dimension: &str,
        max_components: usize,
        component_gap: i32,
    ) -> Result<AdaptiveComponentScan, FailureCause> {
        const TILE_SIZE: i32 = 16;
        const SEED_DISTANCE: i32 = 2;

        let seed_bounds = dustroute_translate::world_reverse::RegionBounds::new(
            Pos::new(
                target.x - SEED_DISTANCE,
                target.y - SEED_DISTANCE,
                target.z - SEED_DISTANCE,
            ),
            Pos::new(
                target.x + SEED_DISTANCE,
                target.y + SEED_DISTANCE,
                target.z + SEED_DISTANCE,
            ),
        );
        self.policy
            .validate_region(seed_bounds)
            .map_err(FailureCause::from)?;
        let seed_snapshot = self
            .bridge
            .scan_region(seed_bounds.min, seed_bounds.max, dimension)
            .await
            .map_err(FailureCause::from)?;
        let seed = seed_snapshot
            .blocks
            .iter()
            .filter(|block| is_redstone_candidate_name(&block.name))
            .min_by_key(|block| (manhattan_pos(block.pos, target), block.pos))
            .filter(|block| manhattan_pos(block.pos, target) <= SEED_DISTANCE)
            .map(|block| block.pos)
            .ok_or_else(|| {
                FailureCause::new(
                    CauseKind::NotFound,
                    "no redstone component was found within 2 blocks of the gaze target",
                )
            })?;

        let merge_measurement = span(Phase::DiscoveryMerge);
        let mut blocks = seed_snapshot
            .blocks
            .into_iter()
            .map(|block| (block.pos, block))
            .collect::<BTreeMap<_, _>>();
        let mut candidates = blocks
            .values()
            .filter(|block| is_redstone_candidate_name(&block.name))
            .map(|block| block.pos)
            .collect::<BTreeSet<_>>();
        drop(merge_measurement);
        let mut loaded_tiles = BTreeSet::<(i32, i32, i32)>::new();
        let mut queued = BTreeSet::from([seed]);
        let mut queue = VecDeque::from([seed]);
        let mut connected = BTreeSet::new();
        let mut limit_reached = false;

        while let Some(current) = queue.pop_front() {
            if connected.len() == max_components {
                limit_reached = true;
                break;
            }
            queued.remove(&current);
            if !connected.insert(current) {
                continue;
            }

            let min_tile = Pos::new(
                (current.x - component_gap).div_euclid(TILE_SIZE),
                (current.y - component_gap).div_euclid(TILE_SIZE),
                (current.z - component_gap).div_euclid(TILE_SIZE),
            );
            let max_tile = Pos::new(
                (current.x + component_gap).div_euclid(TILE_SIZE),
                (current.y + component_gap).div_euclid(TILE_SIZE),
                (current.z + component_gap).div_euclid(TILE_SIZE),
            );
            for tile_x in min_tile.x..=max_tile.x {
                for tile_y in min_tile.y..=max_tile.y {
                    for tile_z in min_tile.z..=max_tile.z {
                        let tile = (tile_x, tile_y, tile_z);
                        if !loaded_tiles.insert(tile) {
                            continue;
                        }
                        let min =
                            Pos::new(tile_x * TILE_SIZE, tile_y * TILE_SIZE, tile_z * TILE_SIZE);
                        let max = Pos::new(
                            min.x + TILE_SIZE - 1,
                            min.y + TILE_SIZE - 1,
                            min.z + TILE_SIZE - 1,
                        );
                        let bounds =
                            dustroute_translate::world_reverse::RegionBounds::new(min, max);
                        self.policy
                            .validate_region(bounds)
                            .map_err(FailureCause::from)?;
                        let snapshot = self
                            .bridge
                            .scan_region(min, max, dimension)
                            .await
                            .map_err(FailureCause::from)?;
                        let _measurement = span(Phase::DiscoveryMerge);
                        for block in snapshot.blocks {
                            if is_redstone_candidate_name(&block.name) {
                                candidates.insert(block.pos);
                            }
                            blocks.insert(block.pos, block);
                        }
                    }
                }
            }

            let _measurement = span(Phase::DiscoveryNeighbors);
            for dx in -component_gap..=component_gap {
                for dy in -component_gap..=component_gap {
                    for dz in -component_gap..=component_gap {
                        if dx.abs() + dy.abs() + dz.abs() > component_gap {
                            continue;
                        }
                        let neighbor = current.offset(dx, dy, dz);
                        if neighbor != current
                            && candidates.contains(&neighbor)
                            && !connected.contains(&neighbor)
                            && queued.insert(neighbor)
                        {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }
        if !queue.is_empty() {
            limit_reached = true;
        }

        let _measurement = span(Phase::DiscoveryMerge);
        let mut min = target;
        let mut max = target;
        for pos in &connected {
            min = Pos::new(min.x.min(pos.x), min.y.min(pos.y), min.z.min(pos.z));
            max = Pos::new(max.x.max(pos.x), max.y.max(pos.y), max.z.max(pos.z));
        }
        min = Pos::new(min.x - 1, min.y - 1, min.z - 1);
        max = Pos::new(max.x + 1, max.y + 1, max.z + 1);
        let snapshot_blocks = blocks
            .into_values()
            .filter(|block| {
                block.pos.x >= min.x
                    && block.pos.x <= max.x
                    && block.pos.y >= min.y
                    && block.pos.y <= max.y
                    && block.pos.z >= min.z
                    && block.pos.z <= max.z
            })
            .collect();
        let scanned_tiles = loaded_tiles.len();
        Ok(AdaptiveComponentScan {
            snapshot: dustroute_translate::snapshot::MinecraftSnapshot {
                min,
                max,
                blocks: snapshot_blocks,
            },
            component_count: connected.len(),
            component_limit: max_components,
            limit_reached,
            scanned_tiles,
            scanned_block_positions: 125 + scanned_tiles * 4096,
        })
    }
}

fn manhattan_pos(a: Pos, b: Pos) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs()
}

pub(super) fn is_redstone_candidate_name(name: &str) -> bool {
    is_supported_redstone_name(name)
        || matches!(
            name,
            "minecraft:observer"
                | "minecraft:redstone_lamp"
                | "minecraft:target"
                | "minecraft:dispenser"
                | "minecraft:dropper"
                | "minecraft:hopper"
                | "minecraft:daylight_detector"
                | "minecraft:tripwire_hook"
                | "minecraft:sculk_sensor"
                | "minecraft:calibrated_sculk_sensor"
        )
        || name.ends_with("_copper_bulb")
        || name == "minecraft:copper_bulb"
        || name.ends_with("_button")
        || name.ends_with("_pressure_plate")
}

pub(super) fn is_supported_redstone_name(name: &str) -> bool {
    dustroute_translate::world::device_program::BUILTIN_DEVICES
        .iter()
        .any(|d| {
            let short = name.strip_prefix("minecraft:").unwrap_or(name);
            d.spec().observed_names.contains(&short)
        })
        || matches!(
            name,
            "minecraft:redstone_wire"
                | "minecraft:redstone_torch"
                | "minecraft:redstone_wall_torch"
                | "minecraft:repeater"
                | "minecraft:comparator"
                | "minecraft:lever"
                | "minecraft:redstone_block"
                | "minecraft:observer"
                | "minecraft:piston"
                | "minecraft:sticky_piston"
        )
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unavailable_count_stays_distinct_from_an_observed_zero() {
        let unknown = ExpansionEvidence::ExplicitSelectedRegion {
            components_loaded: None,
            component_limit: None,
            limit_reached: false,
        };
        let empty = ExpansionEvidence::ExplicitSelectedRegion {
            components_loaded: Some(0),
            component_limit: None,
            limit_reached: false,
        };
        assert_eq!(unknown.components_loaded(), None);
        assert_eq!(empty.components_loaded(), Some(0));
        let wire = serde_json::to_value(&unknown).unwrap();
        assert_eq!(
            wire,
            json!({"strategy":"explicit_selected_region","component_limit":null,"limit_reached":false})
        );
        assert_eq!(
            serde_json::to_value(&empty).unwrap()["components_loaded"],
            0
        );
        assert!(!unknown.limit_reached());
    }

    #[test]
    fn incomplete_discovery_preserves_its_counts_and_public_field_names() {
        let evidence = ExpansionEvidence::AdjacentComponentFloodFill {
            components_loaded: 128,
            component_limit: 128,
            limit_reached: true,
            scanned_tiles: 7,
            scanned_block_positions: 400,
        };
        assert_eq!(evidence.components_loaded(), Some(128));
        assert!(evidence.limit_reached());
        assert_eq!(
            serde_json::to_value(evidence).unwrap(),
            json!({
                "strategy":"adjacent_component_flood_fill","components_loaded":128,
                "component_limit":128,"limit_reached":true,"scanned_tiles":7,
                "scanned_block_positions":400
            })
        );
    }

    #[test]
    fn work_region_does_not_claim_circuit_discovery_counts() {
        let evidence = ExpansionEvidence::ExplicitWorkRegion {
            limit_reached: false,
            scope: "complete requested cuboid; circuit and movement may extend beyond it",
        };
        assert_eq!(evidence.components_loaded(), None);
        assert!(!evidence.limit_reached());
        assert_eq!(
            serde_json::to_value(evidence).unwrap(),
            json!({
                "strategy":"explicit_work_region","limit_reached":false,
                "scope":"complete requested cuboid; circuit and movement may extend beyond it"
            })
        );
    }
}
