//! Checked native calls; policy/sequence state stays in the parent executor.
use super::*;
use crate::survival_cleanup::choose_empty_hand;
use dustroute_translate::{
    snapshot::{LiteralSnapshotIndex, MinecraftSnapshot},
    world::Pos,
};
use std::time::Duration;
use voxrig::checked_survival::{
    HypotheticalMovementPreview, HypotheticalPlacement, InventorySlot, PlacementStatus,
    PlayerState, SurvivalMotionContract, SurvivalMotionStatus,
};
use voxrig::{BlockFace, Region};

mod mining;

const WAIT: Duration = Duration::from_secs(5);
pub(super) fn region(r: dustroute_translate::world::Region) -> Region {
    Region {
        min: [r.min.x, r.min.y, r.min.z],
        max: [r.max.x, r.max.y, r.max.z],
    }
}
fn face(id: u8) -> Result<BlockFace> {
    Ok(match id {
        0 => BlockFace::Down,
        1 => BlockFace::Up,
        2 => BlockFace::North,
        3 => BlockFace::South,
        4 => BlockFace::West,
        5 => BlockFace::East,
        _ => return Err(ExecutionError::new("invalid_face", "unknown native face")),
    })
}
fn positions(r: Region) -> impl Iterator<Item = [i32; 3]> {
    (r.min[0]..=r.max[0]).flat_map(move |x| {
        (r.min[1]..=r.max[1]).flat_map(move |y| (r.min[2]..=r.max[2]).map(move |z| [x, y, z]))
    })
}
pub(super) fn scene_blocks(
    scene: &CapturedSurvivalScene,
) -> Result<BTreeMap<[i32; 3], NativeBlockState>> {
    let s = scene.scenario();
    positions(scene.region())
        .map(|p| Ok((p, s.block(p)?)))
        .collect()
}
pub(super) fn check_snapshot(
    scene: &CapturedSurvivalScene,
    snapshot: &MinecraftSnapshot,
) -> Result<()> {
    let index = LiteralSnapshotIndex::new(snapshot)
        .map_err(|e| ExecutionError::new("snapshot_invalid", e))?;
    let scenario = scene.scenario();
    for p in positions(Region {
        min: [snapshot.min.x, snapshot.min.y, snapshot.min.z],
        max: [snapshot.max.x, snapshot.max.y, snapshot.max.z],
    }) {
        let expected = index.get(Pos::new(p[0], p[1], p[2])).map_or_else(
            || NativeBlockState {
                name: "minecraft:air".into(),
                properties: BTreeMap::new(),
            },
            |b| NativeBlockState {
                name: b.name.clone(),
                properties: b.properties.clone(),
            },
        );
        if scenario.block(p)? != expected {
            return Err(ExecutionError::new(
                "snapshot_mismatch",
                format!("declared site differs at {p:?}"),
            ));
        }
    }
    Ok(())
}
fn inventory_ready(p: &PlayerState) -> Result<()> {
    let i = &p.inventory;
    if i.window_id != Some(0)
        || i.cursor != InventorySlot::Empty
        || i.unsupported_components
        || i.pending_swap.is_some()
        || !i.pending_creative.is_empty()
        || i.slots.len() != 46
        || i.slots[9..45].contains(&InventorySlot::Unavailable)
    {
        return Err(ExecutionError::new(
            "inventory_unavailable",
            "complete plain player inventory required",
        ));
    }
    Ok(())
}
pub(crate) fn received_materials(p: &PlayerState) -> Result<BTreeMap<String, usize>> {
    inventory_ready(p)?;
    let mut counts = BTreeMap::new();
    for slot in &p.inventory.slots[9..45] {
        if let InventorySlot::Item { item } = slot {
            if let Ok(count) = usize::try_from(item.count) {
                *counts.entry(item.name.clone()).or_default() += count;
            }
        }
    }
    Ok(counts)
}
pub(super) fn require_supplied(p: &PlayerState, budget: &BTreeMap<String, usize>) -> Result<()> {
    inventory_ready(p)?;
    for (name, needed) in budget {
        let received: usize = p.inventory.slots[9..45]
            .iter()
            .filter_map(|s| match s {
                InventorySlot::Item { item } if item.name == *name => {
                    usize::try_from(item.count).ok()
                }
                _ => None,
            })
            .sum();
        if received < *needed {
            return Err(ExecutionError::new(
                "supplied_materials_missing",
                format!("{name}: need {needed}, received {received}"),
            ));
        }
    }
    Ok(())
}

impl SurvivalExecutor {
    async fn select_material(&mut self, name: &str) -> Result<()> {
        let ops = self.bot.survival()?;
        let p = ops.player_state().await?;
        inventory_ready(&p)?;
        let slot = (9..45).rev().find(|&s| matches!(&p.inventory.slots[s], InventorySlot::Item { item } if item.name == name && item.count > 0))
            .ok_or_else(|| ExecutionError::new("material_unavailable", name))?;
        let hotbar = if slot >= 36 {
            (slot - 36) as u8
        } else {
            // Only a received empty slot can be used; do not discard another item.
            let empty = choose_empty_hand(&p)?;
            self.journal.intend("inventory_swap", json!({"main_slot":slot,"hotbar":empty,"inventory_sequence":p.inventory.receive_sequence}))?;
            let swap = ops.swap_player_hotbar(slot as u8, empty).await?;
            let received = ops.wait_inventory_swap(&swap, WAIT).await?;
            self.journal.event(
                "inventory_received",
                OperationOutcome::Observed,
                Continuation::NeedsInspection,
                json!(received),
            )?;
            empty
        };
        self.journal
            .intend("select_material", json!({"hotbar":hotbar,"material":name}))?;
        ops.select_hotbar(hotbar).await?;
        Ok(())
    }
    pub(super) async fn place(
        &mut self,
        scene: &CapturedSurvivalScene,
        p: &HypotheticalPlacement,
    ) -> Result<()> {
        p.aim_requirement.validate_standing(scene.source())?;
        let live = scene.scenario().preview_cube_placement(
            p.support,
            face(p.face_id)?,
            p.rotation,
            &p.edit.after.name,
        )?;
        if !same_edit(&live.edit, &p.edit) || live.cursor != p.cursor {
            return Err(ExecutionError::new(
                "placement_plan_changed",
                "fresh geometry no longer matches checked placement",
            ));
        }
        self.select_material(&p.edit.after.name).await?;
        let ops = self.bot.survival()?;
        self.journal.intend(
            "placement_aim",
            json!({"rotation":p.rotation,"edit":p.edit}),
        )?;
        ops.look(p.rotation).await?;
        self.journal.intend("placement_send", json!(p))?;
        let intent = ops.place_survival_cube(p.support, face(p.face_id)?).await?;
        if intent.target != p.edit.position
            || intent.before != p.edit.before
            || intent.expected != p.edit.after
            || intent.cursor != p.cursor
        {
            return Err(ExecutionError::new(
                "placement_intent_changed",
                "sent intent differs; outcome requires inspection",
            ));
        }
        let status = ops.wait_survival_placement(&intent, WAIT).await?;
        if !matches!(status, PlacementStatus::ObservedPlaced { .. }) {
            return Err(ExecutionError::new(
                "placement_unconfirmed",
                format!("{status:?}"),
            ));
        }
        self.journal.event(
            "placement_observed",
            OperationOutcome::Observed,
            Continuation::NeedsInspection,
            json!({"intent":intent,"status":status,"world_evidence":"builder_received"}),
        )?;
        Ok(())
    }
    pub(super) async fn move_along(&mut self, p: &HypotheticalMovementPreview) -> Result<()> {
        let ops = self.bot.survival()?;
        let live = ops.preview_survival_path(&p.controls).await?;
        p.initial_aim_requirement.validate_standing(&live.initial)?;
        if p.endpoint_contract != SurvivalMotionContract::Predicted {
            return Err(ExecutionError::new(
                "movement_contract_changed",
                "checked plan requires a different endpoint contract",
            ));
        }
        if live.initial_frame != p.initial_frame || live.frames != p.frames {
            let first = p
                .frames
                .iter()
                .zip(&live.frames)
                .position(|(a, b)| a != b)
                .or_else(|| {
                    (p.frames.len() != live.frames.len())
                        .then_some(p.frames.len().min(live.frames.len()))
                });
            self.journal.event(
                "movement_prediction_mismatch",
                OperationOutcome::NotStarted,
                Continuation::NeedsInspection,
                json!({"first_differing_frame_index":first,"checked":p,"fresh":live}),
            )?;
            return Err(ExecutionError::new(
                "movement_plan_changed",
                "fresh trajectory differs from checked plan",
            ));
        }
        self.journal.intend(
            "movement_send",
            json!({"controls":p.controls,"planned_initial_position":p.initial_position}),
        )?;
        ops.start_previewed_predicted_survival_motion(&live).await?;
        let motion = tokio::time::timeout(Duration::from_secs(40), async {
            loop {
                let record = ops.survival_motion().await.ok_or_else(|| {
                    ExecutionError::new("movement_missing", "native record unavailable")
                })?;
                if record.status == SurvivalMotionStatus::Predicted {
                    return Ok(record);
                }
                if record.status == SurvivalMotionStatus::RequiresInspection {
                    return Err(ExecutionError::new(
                        "movement_needs_inspection",
                        format!("{:?}", record.problem),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| ExecutionError::new("movement_timeout", "native movement did not finish"))??;
        self.journal.event(
            "movement_predicted",
            OperationOutcome::Predicted,
            Continuation::NeedsInspection,
            json!(motion),
        )?;
        Ok(())
    }
}

fn same_edit(a: &HypotheticalBlockEdit, b: &HypotheticalBlockEdit) -> bool {
    a.position == b.position && a.before == b.before && a.after == b.after
}
