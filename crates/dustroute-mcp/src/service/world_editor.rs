//! Live mutation validation, transport submission and independent readback.
//! No MCP routing, player selection, catalog or operation ownership.
use super::{
    BoundaryBlockRecord, PLACEMENT_VERIFY_ATTEMPTS, PLACEMENT_VERIFY_INTERVAL, block_matches,
    boundary_block_record, bounds_for_changes, placement_baseline_matches,
    world_from_snapshot_for_service,
};
use crate::bridge_protocol::{PhysicalChange, PhysicalSubmission};
use crate::failure::{CauseKind, FailureCause};
use crate::{BlockChange, BotBridge, BotBridgeError, McpPolicy};
use dustroute_physical::{BlockKind, PhysicalBlockChange, Pos};
use dustroute_translate::minecraft_export::JavaExportConfig;
use dustroute_translate::world_reverse::RegionBounds;
use std::collections::BTreeMap;
use tokio::time::sleep;

#[cfg(test)]
#[path = "world_editor_tests.rs"]
mod tests;

pub(super) struct WorldEditor<'a> {
    pub bridge: &'a BotBridge,
    pub policy: &'a McpPolicy,
}
impl WorldEditor<'_> {
    pub(super) async fn verify_placement_changes(
        &self,
        changes: &[BlockChange],
        dimension: &str,
        verify_after: bool,
    ) -> Result<(bool, Vec<Pos>), FailureCause> {
        let Some(first) = changes.first() else {
            return Ok((true, Vec::new()));
        };
        let (mut min, mut max) = (first.pos, first.pos);
        for change in &changes[1..] {
            min = Pos::new(
                min.x.min(change.pos.x),
                min.y.min(change.pos.y),
                min.z.min(change.pos.z),
            );
            max = Pos::new(
                max.x.max(change.pos.x),
                max.y.max(change.pos.y),
                max.z.max(change.pos.z),
            );
        }
        let snapshot = self
            .bridge
            .scan_region(min, max, dimension)
            .await
            .map_err(FailureCause::from)?;
        let world = world_from_snapshot_for_service(&snapshot)?;
        let _verification = crate::performance::span(crate::performance::Phase::Verification);
        let mismatches = changes
            .iter()
            .filter(|change| {
                let expected = if verify_after {
                    &change.after
                } else {
                    &change.before
                };
                let actual = world.get(change.pos);
                let matches = if verify_after {
                    block_matches(actual, expected)
                } else {
                    placement_baseline_matches(actual, expected)
                };
                !matches
            })
            .map(|change| change.pos)
            .collect::<Vec<_>>();
        Ok((mismatches.is_empty(), mismatches))
    }

    /// Minecraft applies neighbor updates asynchronously after a batch write.
    /// A successful bridge response therefore does not guarantee that an
    /// immediately following scan observes every written block. Keep stale
    /// baseline detection strict, but allow post-write verification to settle.
    pub(super) async fn verify_placement_changes_eventually(
        &self,
        changes: &[BlockChange],
        dimension: &str,
    ) -> Result<(bool, Vec<Pos>), FailureCause> {
        let mut last_mismatches = Vec::new();
        for attempt in 0..PLACEMENT_VERIFY_ATTEMPTS {
            let (verified, mismatches) = self
                .verify_placement_changes(changes, dimension, true)
                .await?;
            if verified {
                return Ok((true, Vec::new()));
            }
            last_mismatches = mismatches;
            if attempt + 1 < PLACEMENT_VERIFY_ATTEMPTS {
                sleep(PLACEMENT_VERIFY_INTERVAL).await;
            }
        }
        Ok((false, last_mismatches))
    }

    pub(super) async fn validate_live_changes(
        &self,
        changes: &[BlockChange],
        dimension: &str,
    ) -> Result<dustroute_app::ValidatedBlockChanges, FailureCause> {
        let mut region = dustroute_translate::world::World::new();
        for change in changes {
            region.set(
                change.pos,
                dustroute_translate::world::Block::new(BlockKind::Solid),
            );
        }
        let Some((min, max)) = region.bounds() else {
            return dustroute_app::ValidatedBlockChanges::new(&region, Vec::new())
                .map_err(|e| FailureCause::new(CauseKind::InvalidInput, e.to_string()));
        };
        // Adjacent dependents can themselves need evidence outside this first
        // scan (e.g. the support below a lower wire beside an upward repair).
        // Expand from the shared validator's missing evidence, never by inventing
        // support or dropping boundary blocks. Every rescan remains policy-bound.
        let mut bounds = RegionBounds::new(min.offset(-1, -1, -1), max.offset(1, 1, 1));
        for _ in 0..8 {
            self.policy
                .validate_region(bounds)
                .map_err(FailureCause::from)?;
            let snapshot = self
                .bridge
                .scan_region(bounds.min, bounds.max, dimension)
                .await
                .map_err(FailureCause::from)?;
            let baseline = dustroute_translate::snapshot::literal_world_from_snapshot(&snapshot)
                .map_err(|error| FailureCause::new(CauseKind::InvalidInput, error.to_string()))?;
            let mut candidate = baseline.clone();
            for change in changes {
                candidate.set(change.pos, change.after.clone());
            }
            let mut expanded = bounds;
            for issue in candidate.placement_issues_with_lookup(|pos| {
                bounds.contains(pos).then(|| {
                    candidate
                        .get(pos)
                        .cloned()
                        .unwrap_or_else(|| dustroute_physical::Block::new(BlockKind::Air))
                })
            }) {
                use dustroute_physical::WorldValidationIssue;
                let needed = match issue {
                    WorldValidationIssue::InvalidSupport {
                        support: Some(pos), ..
                    } if !bounds.contains(pos) => vec![pos],
                    WorldValidationIssue::UnknownWireConnection {
                        required_positions, ..
                    } => required_positions,
                    _ => continue,
                };
                for pos in needed {
                    expanded.min = Pos::new(
                        expanded.min.x.min(pos.x),
                        expanded.min.y.min(pos.y),
                        expanded.min.z.min(pos.z),
                    );
                    expanded.max = Pos::new(
                        expanded.max.x.max(pos.x),
                        expanded.max.y.max(pos.y),
                        expanded.max.z.max(pos.z),
                    );
                }
            }
            if expanded == bounds {
                return dustroute_app::ValidatedBlockChanges::new(&baseline, changes.to_vec())
                    .map_err(|error| {
                        FailureCause::new(
                            CauseKind::InvalidInput,
                            format!("{}: {:?}", error, error.issues),
                        )
                    });
            }
            bounds = expanded;
        }
        Err(FailureCause::new(
            CauseKind::ResourceLimit,
            "placement validation needs context beyond the 8-scan limit; no changes submitted",
        ))
    }

    pub(super) async fn write_validated_physical_changes(
        &self,
        changes: &dustroute_app::ValidatedBlockChanges,
        dimension: &str,
    ) -> Result<PhysicalSubmission, BotBridgeError> {
        let changes: Vec<_> = changes
            .changes()
            .iter()
            .map(|c| PhysicalBlockChange {
                pos: c.pos,
                before: c.before.clone(),
                after: c.after.clone(),
            })
            .collect();
        self.write_physical_change_batch(&changes, dimension).await
    }

    // Low-level transport shared with exact undo/rollback. Forward callers must
    // enter through write_validated_physical_changes.
    pub(super) async fn write_physical_change_batch(
        &self,
        changes: &[PhysicalBlockChange],
        dimension: &str,
    ) -> Result<PhysicalSubmission, BotBridgeError> {
        self.policy
            .validate_placement_size(changes.len())
            .map_err(|error| {
                BotBridgeError::Detailed(FailureCause::from(error)).with_submission(
                    Some(0),
                    changes.len(),
                    false,
                )
            })?;
        let mut changes = changes.iter().collect::<Vec<_>>();
        changes.sort_by_key(|change| {
            let priority = match change.after.kind {
                BlockKind::Solid
                | BlockKind::Transparent
                | BlockKind::RedstoneBlock
                | BlockKind::Observer
                | BlockKind::Piston => 0,
                BlockKind::RedstoneTorch | BlockKind::Lever => 2,
                _ => 1,
            };
            (priority, change.pos.y, change.pos.x, change.pos.z)
        });
        let total_changes = changes.len();
        let export = JavaExportConfig {
            relative: false,
            ..JavaExportConfig::default()
        };
        let actions = changes
            .into_iter()
            .map(|change| -> Result<PhysicalChange, BotBridgeError> {
                if change.after.kind == BlockKind::Air {
                    return Ok(PhysicalChange::Dig { pos: change.pos });
                }
                let state = dustroute_translate::minecraft_export::native_block_state(
                    &change.after,
                    &export,
                    dustroute_translate::minecraft_export::ExportPurpose::InitialPlacement,
                )
                .map_err(|error| {
                    BotBridgeError::Detailed(FailureCause::new(
                        CauseKind::InvalidInput,
                        error.to_string(),
                    ))
                    .with_submission(Some(0), total_changes, false)
                })?;
                let item = if change.after.kind == BlockKind::RedstoneWire {
                    "minecraft:redstone".to_owned()
                } else {
                    state.name().to_owned()
                };
                let support_offset = change.after.support_offset.unwrap_or(Pos::new(0, -1, 0));
                let reference =
                    change
                        .pos
                        .offset(support_offset.x, support_offset.y, support_offset.z);
                Ok(PhysicalChange::Place {
                    pos: change.pos,
                    item,
                    state,
                    reference,
                    face: Pos::new(-support_offset.x, -support_offset.y, -support_offset.z),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.bridge.place_physical_blocks(&actions, dimension).await
    }

    pub(super) async fn verify_physical_changes(
        &self,
        changes: &[PhysicalBlockChange],
        dimension: &str,
    ) -> Result<(bool, Vec<Pos>), FailureCause> {
        let Some(bounds) = bounds_for_changes(changes) else {
            return Ok((true, Vec::new()));
        };
        let snapshot = self
            .bridge
            .scan_region(bounds.min, bounds.max, dimension)
            .await
            .map_err(FailureCause::from)?;
        let world = world_from_snapshot_for_service(&snapshot)?;
        let mismatches = changes
            .iter()
            .filter(|change| !block_matches(world.get(change.pos), &change.after))
            .map(|change| change.pos)
            .collect::<Vec<_>>();
        Ok((mismatches.is_empty(), mismatches))
    }

    pub(super) async fn verify_preserved_boundary(
        &self,
        expected: &[BoundaryBlockRecord],
        dimension: &str,
    ) -> Result<(bool, Vec<Pos>), FailureCause> {
        if expected.is_empty() {
            return Ok((true, Vec::new()));
        }
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(
            Pos::new(
                expected
                    .iter()
                    .map(|record| record.pos.x)
                    .min()
                    .unwrap_or(0),
                expected
                    .iter()
                    .map(|record| record.pos.y)
                    .min()
                    .unwrap_or(0),
                expected
                    .iter()
                    .map(|record| record.pos.z)
                    .min()
                    .unwrap_or(0),
            ),
            Pos::new(
                expected
                    .iter()
                    .map(|record| record.pos.x)
                    .max()
                    .unwrap_or(0),
                expected
                    .iter()
                    .map(|record| record.pos.y)
                    .max()
                    .unwrap_or(0),
                expected
                    .iter()
                    .map(|record| record.pos.z)
                    .max()
                    .unwrap_or(0),
            ),
        );
        let snapshot = self
            .bridge
            .scan_region(bounds.min, bounds.max, dimension)
            .await
            .map_err(FailureCause::from)?;
        let actual = snapshot
            .blocks
            .iter()
            .map(boundary_block_record)
            .map(|record| (record.pos, record))
            .collect::<BTreeMap<_, _>>();
        let mismatches = expected
            .iter()
            .filter(|expected| actual.get(&expected.pos) != Some(*expected))
            .map(|record| record.pos)
            .collect::<Vec<_>>();
        Ok((mismatches.is_empty(), mismatches))
    }
}
