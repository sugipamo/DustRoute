use std::collections::BTreeMap;

use super::*;
use crate::piston_motion_law::{ControlFacts, builtin_laws};
use crate::{
    Block, BlockChange, BlockKind, BlockMove, ChangeReason, DeltaCause, Facing,
    ObservationClassification, PistonBlockEntityState, PistonState, PistonVariant, Pos, RegionSet,
    WorldDelta, piston_state, piston_variant,
};

pub(super) fn facing(body: &Block) -> Result<Facing, RuntimeError> {
    body.facing
        .ok_or_else(|| unsupported("piston needs explicit facing"))
}

pub(super) fn facing_name(dir: Facing) -> &'static str {
    match dir {
        Facing::North => "north",
        Facing::East => "east",
        Facing::South => "south",
        Facing::West => "west",
        Facing::Up => "up",
        Facing::Down => "down",
    }
}

pub(super) fn along(pos: Pos, dir: Facing, distance: i32) -> Result<Pos, RuntimeError> {
    let v = dir.offset();
    Ok(Pos::new(
        pos.x
            .checked_add(
                v.x.checked_mul(distance)
                    .ok_or(RuntimeError::ClockOverflow)?,
            )
            .ok_or(RuntimeError::ClockOverflow)?,
        pos.y
            .checked_add(
                v.y.checked_mul(distance)
                    .ok_or(RuntimeError::ClockOverflow)?,
            )
            .ok_or(RuntimeError::ClockOverflow)?,
        pos.z
            .checked_add(
                v.z.checked_mul(distance)
                    .ok_or(RuntimeError::ClockOverflow)?,
            )
            .ok_or(RuntimeError::ClockOverflow)?,
    ))
}

pub(super) fn offset(pos: Pos, v: Pos) -> Result<Pos, RuntimeError> {
    Ok(Pos::new(
        pos.x.checked_add(v.x).ok_or(RuntimeError::ClockOverflow)?,
        pos.y.checked_add(v.y).ok_or(RuntimeError::ClockOverflow)?,
        pos.z.checked_add(v.z).ok_or(RuntimeError::ClockOverflow)?,
    ))
}

pub(super) fn state(mut body: Block, extended: bool) -> Block {
    body.piston_state = Some(if extended {
        PistonState::Extended
    } else {
        PistonState::Retracted
    });
    if body.observed_name.is_some() {
        body.observed_properties
            .insert("extended".into(), extended.to_string());
    }
    body
}

pub(super) fn head(body: &Block) -> Result<Block, RuntimeError> {
    let dir = facing(body)?;
    let variant = piston_variant(body);
    let mut block = Block::piston_head(dir, variant, false);
    if body.observed_name.is_some() {
        block.observed_name = Some("minecraft:piston_head".into());
        block.observation_classification = body.observation_classification;
        block.observed_properties.insert(
            "facing".into(),
            match dir {
                Facing::North => "north",
                Facing::East => "east",
                Facing::South => "south",
                Facing::West => "west",
                Facing::Up => "up",
                Facing::Down => "down",
            }
            .into(),
        );
        block.observed_properties.insert(
            "type".into(),
            if variant == PistonVariant::Sticky {
                "sticky"
            } else {
                "normal"
            }
            .into(),
        );
        block
            .observed_properties
            .insert("short".into(), "false".into());
    }
    Ok(block)
}

pub(super) fn moving(block: Block, dir: Facing, extending: bool, source: bool) -> Block {
    let variant = if source {
        piston_variant(&block)
    } else {
        PistonVariant::Normal
    };
    let mut carrier = Block::moving_piston(PistonBlockEntityState {
        pushed_block: Box::new(block),
        facing: dir,
        extending,
        source,
        progress: 0,
    });
    carrier.piston_variant = Some(variant);
    carrier
}

pub(super) fn delta(
    view: RuntimeView<'_>,
    writes: impl IntoIterator<Item = (Pos, Block)>,
    moves: Vec<BlockMove>,
    cause: DeltaCause,
) -> Result<WorldDelta, RuntimeError> {
    let writes: BTreeMap<_, _> = writes.into_iter().collect();
    let mut changes = Vec::new();
    for (position, after) in writes {
        let before = view.block(position)?;
        if before != after || moves.iter().any(|m| m.from == position || m.to == position) {
            changes.push(BlockChange {
                position,
                before,
                after,
                reason: ChangeReason::NeighborUpdate,
            });
        }
    }
    Ok(WorldDelta {
        parent_shape: view.world().shape_id(),
        dirty_region: RegionSet::around_positions(changes.iter().map(|c| c.position), 1),
        changes,
        moves,
        cause,
    })
}

pub(super) fn powered(
    view: RuntimeView<'_>,
    pos: Pos,
    _body: &Block,
) -> Result<bool, RuntimeError> {
    super::electrical::world(view)?.piston_powered(pos)
}

pub(super) fn control_facts(
    view: RuntimeView<'_>,
    pos: Pos,
    body: &Block,
) -> Result<ControlFacts, RuntimeError> {
    let mut facts = ControlFacts {
        powered: powered(view, pos, body)?,
        extended: piston_state(body).is_extended(),
        in_block_tick: view.time().in_block_tick(),
        sticky: piston_variant(body) == PistonVariant::Sticky,
        ..ControlFacts::default()
    };
    // Only a retract request inspects the extending carrier two cells ahead.
    // In particular, do not demand unknown geometry beyond an idle retracted
    // payload piston merely because it received an onBlockAdded notification.
    if !matches!(
        builtin_laws().control(facts).request,
        Some(
            crate::piston_motion_law::PistonBlockEvent::Retract
                | crate::piston_motion_law::PistonBlockEvent::RetractDrop
        )
    ) {
        return Ok(facts);
    }
    let geometry = builtin_laws()
        .geometry(false, false, view.time().section, 0)
        .expect("fixed facts");
    let payload = along(pos, facing(body)?, geometry.payload)?;
    let block = view.block(payload)?;
    let matching = block
        .piston_entity
        .as_deref()
        .is_some_and(|e| e.extending && Some(e.facing) == body.facing);
    let history = if matching {
        Some(
            view.carrier(payload)
                .ok_or(RuntimeError::CarrierConflict(payload))?
                .history,
        )
    } else {
        None
    };
    facts.matching_carrier = matching;
    facts.last_progress = history.map_or(HalfProgress::Zero, |h| h.last_progress);
    facts.same_tick = history.is_some_and(|h| h.saved_world_time == view.time().game_tick);
    Ok(facts)
}

/// Known immobility is a failed push, while unsupported movement is an
/// execution error. It must not turn into a fabricated successful no-op.
pub(super) fn movable(pos: Pos, block: &Block) -> Result<bool, RuntimeError> {
    if builtin_laws().immovable(
        block.kind == BlockKind::MovingPiston || block.piston_entity.is_some(),
        block.kind == BlockKind::Piston && piston_state(block).is_extended(),
    ) {
        return Ok(false);
    }
    if block.observation_classification == ObservationClassification::Coarse
        || block.requires_live_observation()
    {
        return Err(unsupported(format!(
            "unsupported payload evidence at {pos:?}"
        )));
    }
    let laws = crate::piston_law::electrical_payload_laws();
    let rejection = laws.payload_rejection(block).map_err(unsupported)?;
    match rejection {
        // Java tests a piston body's EXTENDED state, not its facing, variant
        // or current power. A powered retracted body can still be moved before
        // its queued block event runs; delivery guards handle the old position.
        0 => Ok(true),
        4 => Ok(false),
        _ => Err(unsupported(format!(
            "unsupported payload {:?} at {pos:?} (law rejection {rejection})",
            block.kind
        ))),
    }
}

pub(super) fn push_moves(
    view: RuntimeView<'_>,
    pos: Pos,
    body: &Block,
) -> Result<Option<Vec<BlockMove>>, RuntimeError> {
    super::adhesion::collect(view, pos, body, true)
}

pub(super) fn head_supported(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
) -> Result<bool, RuntimeError> {
    let Some(head) = block.piston_head.as_ref() else {
        return Err(unsupported("head metadata is missing"));
    };
    if block.facing != Some(head.facing) || block.piston_variant != Some(head.variant) {
        return Err(unsupported("contradictory head metadata"));
    }
    let behind = along(pos, head.facing, -1)?;
    let body = view.block(behind)?;
    Ok(builtin_laws().head_supported(
        body.kind == BlockKind::Piston,
        body.kind == BlockKind::MovingPiston,
        body.facing == Some(head.facing),
        piston_variant(&body) == head.variant,
        piston_state(&body).is_extended(),
    ))
}
