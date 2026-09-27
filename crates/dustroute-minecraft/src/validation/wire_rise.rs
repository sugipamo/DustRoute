//! Initial-state consistency of explicit vertical dust connections. This does
//! not infer or rewrite a stored arm, normalize observations, or require every
//! decorative horizontal arm to terminate at a component.
use crate::{Block, BlockKind, Pos, WireConnection, World, WorldValidationIssue};

/// A complete World supplies known air for absent cells. Partial observations
/// supply None. Known contradictions take precedence over missing evidence.
pub fn wire_rise_issues(
    world: &World,
    block_at: impl Fn(Pos) -> Option<Block>,
) -> Vec<WorldValidationIssue> {
    let mut issues = Vec::new();
    for (position, block) in world
        .iter()
        .filter(|(_, b)| b.kind == BlockKind::RedstoneWire)
    {
        for (facing, arm) in block.wire_connections.iter().flatten() {
            if *arm == WireConnection::None {
                continue;
            }
            let invalid = |reason: &str| WorldValidationIssue::InvalidWireConnection {
                position: *position,
                facing: *facing,
                reason: reason.into(),
            };
            let Some(offset) = facing.horizontal_offset() else {
                issues.push(invalid("dust arms must use horizontal directions"));
                continue;
            };
            let offset_checked = |dx, dy, dz| {
                Some(Pos::new(
                    position.x.checked_add(dx)?,
                    position.y.checked_add(dy)?,
                    position.z.checked_add(dz)?,
                ))
            };
            let (Some(side), Some(upper), Some(above)) = (
                offset_checked(offset.x, 0, offset.z),
                offset_checked(offset.x, 1, offset.z),
                offset_checked(0, 1, 0),
            ) else {
                issues.push(invalid(
                    "wire connection neighborhood exceeds coordinate bounds",
                ));
                continue;
            };
            let support = block_at(side);
            let target = block_at(upper);
            // A Side arm need not be a rise (straight endpoints are allowed).
            // When known geometry makes it a top-half rise, both kernels also
            // require clearance above the lower wire.
            let side_rise = *arm == WireConnection::Side
                && support
                    .as_ref()
                    .is_some_and(|b| b.redstone_traits().wire_rise_connection == Some(*arm))
                && target
                    .as_ref()
                    .is_some_and(|b| b.kind == BlockKind::RedstoneWire);
            if *arm != WireConnection::Up && !side_rise {
                continue;
            }
            let ceiling = block_at(above);
            let requirements = [
                (
                    side,
                    support
                        .as_ref()
                        .map(|b| b.redstone_traits().wire_rise_connection == Some(*arm)),
                    "stored rise arm contradicts the adjacent support geometry",
                ),
                (
                    upper,
                    target.as_ref().map(|b| b.kind == BlockKind::RedstoneWire),
                    "stored rise arm has no upper wire at its target",
                ),
                (
                    above,
                    ceiling
                        .as_ref()
                        .map(|b| !b.redstone_traits().blocks_wire_rise_when_above),
                    "stored rise arm is obstructed above the lower wire",
                ),
            ];
            let contradictions: Vec<_> = requirements
                .iter()
                .filter(|(_, valid, _)| *valid == Some(false))
                .collect();
            if !contradictions.is_empty() {
                issues.extend(
                    contradictions
                        .into_iter()
                        .map(|(_, _, reason)| invalid(reason)),
                );
            } else {
                let required_positions: Vec<_> = requirements
                    .iter()
                    .filter(|(_, valid, _)| valid.is_none())
                    .map(|(pos, _, _)| *pos)
                    .collect();
                if !required_positions.is_empty() {
                    issues.push(WorldValidationIssue::UnknownWireConnection {
                        position: *position,
                        facing: *facing,
                        required_positions,
                    });
                }
            }
        }
    }
    issues
}
