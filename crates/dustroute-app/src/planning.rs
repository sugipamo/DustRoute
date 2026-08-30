use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use dustroute_physical::{Block, BlockKind, Pos, World};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockChange {
    pub pos: Pos,
    pub before: Block,
    pub after: Block,
    pub collision: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UndoPlan {
    pub operation_id: Uuid,
    pub changes: Vec<BlockChange>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlacementPlan {
    pub operation_id: Uuid,
    pub origin: Pos,
    pub changes: Vec<BlockChange>,
    pub collision_count: usize,
    pub materials: BTreeMap<String, usize>,
    pub undo: UndoPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanningError {
    BlockLimitExceeded { limit: usize, actual: usize },
}

impl Display for PlanningError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockLimitExceeded { limit, actual } => {
                write!(
                    f,
                    "placement has {actual} blocks and exceeds the {limit} block limit"
                )
            }
        }
    }
}

impl Error for PlanningError {}

#[must_use]
pub fn relocate_world(source: &World, origin: Pos) -> World {
    let mut result = World::new();
    for (pos, block) in source.iter() {
        result.set(
            Pos::new(pos.x + origin.x, pos.y + origin.y, pos.z + origin.z),
            block.clone(),
        );
    }
    result
}

pub fn plan_world_overlay(
    existing: &World,
    proposed_local: &World,
    origin: Pos,
    max_blocks: usize,
) -> Result<PlacementPlan, PlanningError> {
    if proposed_local.iter().count() > max_blocks {
        return Err(PlanningError::BlockLimitExceeded {
            limit: max_blocks,
            actual: proposed_local.iter().count(),
        });
    }
    let proposed = relocate_world(proposed_local, origin);
    let mut changes = Vec::new();
    let mut materials = BTreeMap::new();
    for (pos, after) in proposed.iter() {
        let before = existing
            .get(*pos)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air));
        if before == *after {
            continue;
        }
        let collision = before.kind != BlockKind::Air;
        *materials.entry(format!("{:?}", after.kind)).or_insert(0) += 1;
        changes.push(BlockChange {
            pos: *pos,
            before,
            after: after.clone(),
            collision,
        });
    }
    let operation_id = Uuid::new_v4();
    let collision_count = changes.iter().filter(|change| change.collision).count();
    let undo = UndoPlan {
        operation_id,
        changes: changes
            .iter()
            .map(|change| BlockChange {
                pos: change.pos,
                before: change.after.clone(),
                after: change.before.clone(),
                collision: false,
            })
            .collect(),
    };
    Ok(PlacementPlan {
        operation_id,
        origin,
        changes,
        collision_count,
        materials,
        undo,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_overlay_collisions_materials_and_exact_undo() {
        let mut existing = World::new();
        existing.set(Pos::new(11, 64, 10), Block::new(BlockKind::Transparent));
        let mut proposed = World::new();
        proposed.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        proposed.set(Pos::new(1, 0, 0), Block::new(BlockKind::RedstoneBlock));
        let plan = plan_world_overlay(&existing, &proposed, Pos::new(10, 64, 10), 10).unwrap();
        assert_eq!(plan.changes.len(), 2);
        assert_eq!(plan.collision_count, 1);
        assert_eq!(plan.materials["Solid"], 1);
        assert_eq!(plan.materials["RedstoneBlock"], 1);
        assert_eq!(plan.undo.operation_id, plan.operation_id);
        let restored = plan
            .undo
            .changes
            .iter()
            .find(|change| change.pos == Pos::new(11, 64, 10))
            .unwrap();
        assert_eq!(restored.after.kind, BlockKind::Transparent);
    }

    #[test]
    fn enforces_block_limit_before_creating_plan() {
        let mut proposed = World::new();
        proposed.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        proposed.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
        assert_eq!(
            plan_world_overlay(&World::new(), &proposed, Pos::new(0, 0, 0), 1),
            Err(PlanningError::BlockLimitExceeded {
                limit: 1,
                actual: 2
            })
        );
    }
}
