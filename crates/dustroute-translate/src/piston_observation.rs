//! Read-only reverse interpretation of a pinned piston mechanism.
//! Reports evidence, never authorizes placement or predicts an unfinished move.
use crate::{MinecraftSnapshot, Pos, RegionBounds};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PistonObservationState {
    Open,
    Closed,
    Moving,
    Indeterminate,
    ConfigurationMismatch,
    ObservationIncomplete,
    UnsupportedVersion,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonObservationIssue {
    pub position: Option<Pos>,
    pub code: String,
    pub message: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonObservation {
    pub schema_version: String,
    pub state: PistonObservationState,
    pub issues: Vec<PistonObservationIssue>,
    pub next_action: String,
}
impl PistonObservation {
    pub fn unresolved(state: PistonObservationState, code: &str, message: &str) -> Self {
        report(state, vec![issue(None, code, message)])
    }
}
pub struct PistonObservationContract<'a> {
    pub version: &'a str,
    pub open: &'a MinecraftSnapshot,
    pub closed: &'a MinecraftSnapshot,
    pub origin: Pos,
    /// Absolute complete scan bounds, including the mechanism's guard.
    pub required_bounds: RegionBounds,
}
type BlockMap = BTreeMap<Pos, (String, BTreeMap<String, String>)>;
fn issue(position: Option<Pos>, code: &str, message: &str) -> PistonObservationIssue {
    PistonObservationIssue {
        position,
        code: code.into(),
        message: message.into(),
    }
}
fn report(state: PistonObservationState, issues: Vec<PistonObservationIssue>) -> PistonObservation {
    use PistonObservationState::*;
    PistonObservation {
        schema_version: "dustroute.piston-observation.v1".into(),
        state,
        issues,
        next_action: match state {
            Open | Closed => "create_previewed_operation",
            Moving | Indeterminate => "rescan_without_activation",
            ObservationIncomplete => "obtain_complete_observation",
            ConfigurationMismatch => "inspect_configuration",
            UnsupportedVersion => "use_supported_version",
        }
        .into(),
    }
}
fn map(snapshot: &MinecraftSnapshot, origin: Pos) -> Result<BlockMap, PistonObservationIssue> {
    let mut seen = BTreeSet::new();
    let mut blocks = BTreeMap::new();
    for b in &snapshot.blocks {
        if !seen.insert(b.pos) {
            return Err(issue(
                Some(b.pos),
                "duplicate_coordinate",
                "snapshot contains duplicate coordinates",
            ));
        }
        if b.pos.x < snapshot.min.x
            || b.pos.x > snapshot.max.x
            || b.pos.y < snapshot.min.y
            || b.pos.y > snapshot.max.y
            || b.pos.z < snapshot.min.z
            || b.pos.z > snapshot.max.z
        {
            return Err(issue(
                Some(b.pos),
                "outside_bounds",
                "block is outside declared observation",
            ));
        }
        if b.name == "minecraft:air" {
            continue;
        }
        let relative = b
            .pos
            .x
            .checked_sub(origin.x)
            .zip(b.pos.y.checked_sub(origin.y))
            .zip(b.pos.z.checked_sub(origin.z));
        let Some(((x, y), z)) = relative else {
            return Err(issue(
                Some(b.pos),
                "coordinate_overflow",
                "coordinate cannot be normalized",
            ));
        };
        blocks.insert(Pos::new(x, y, z), (b.name.clone(), b.properties.clone()));
    }
    Ok(blocks)
}

pub fn observe_piston_mechanism(
    snapshot: &MinecraftSnapshot,
    complete: bool,
    version: &str,
    contract: &PistonObservationContract<'_>,
) -> PistonObservation {
    use PistonObservationState::*;
    if version != contract.version {
        return PistonObservation::unresolved(
            UnsupportedVersion,
            "unsupported_version",
            "version differs from the verified contract",
        );
    }
    let b = contract.required_bounds;
    if !complete
        || snapshot.min.x > b.min.x
        || snapshot.min.y > b.min.y
        || snapshot.min.z > b.min.z
        || snapshot.max.x < b.max.x
        || snapshot.max.y < b.max.y
        || snapshot.max.z < b.max.z
    {
        return PistonObservation::unresolved(
            ObservationIncomplete,
            "incomplete_region",
            "the full mechanism and guard must be observed",
        );
    }
    let actual = match map(snapshot, contract.origin) {
        Ok(m) => m,
        Err(e) => return report(ObservationIncomplete, vec![e]),
    };
    let open = match map(contract.open, Pos::new(0, 0, 0)) {
        Ok(m) => m,
        Err(e) => return report(ConfigurationMismatch, vec![e]),
    };
    let closed = match map(contract.closed, Pos::new(0, 0, 0)) {
        Ok(m) => m,
        Err(e) => return report(ConfigurationMismatch, vec![e]),
    };
    if actual == open {
        return report(Open, vec![]);
    }
    if actual == closed {
        return report(Closed, vec![]);
    }
    let positions: BTreeSet<_> = actual
        .keys()
        .chain(open.keys())
        .chain(closed.keys())
        .copied()
        .collect();
    let air = ("minecraft:air".to_owned(), BTreeMap::new());
    let mut missing = Vec::new();
    let mut mismatch = Vec::new();
    let mut moving = Vec::new();
    for pos in positions {
        let a = actual.get(&pos).unwrap_or(&air);
        let o = open.get(&pos).unwrap_or(&air);
        let c = closed.get(&pos).unwrap_or(&air);
        let absolute = pos
            .x
            .checked_add(contract.origin.x)
            .zip(pos.y.checked_add(contract.origin.y))
            .zip(pos.z.checked_add(contract.origin.z))
            .map(|((x, y), z)| Pos::new(x, y, z));
        if a == o || a == c {
            continue;
        }
        let motion_cell = o != c
            && [o, c].iter().any(|v| {
                matches!(
                    v.0.as_str(),
                    "minecraft:piston"
                        | "minecraft:sticky_piston"
                        | "minecraft:piston_head"
                        | "minecraft:stone"
                )
            });
        if a.0 == "minecraft:moving_piston" && motion_cell {
            // Packet snapshots lack the carried block entity. Recognize only
            // the marker, without inventing payload, progress or destination.
            moving.push(issue(
                absolute,
                "moving_marker",
                "moving_piston observed; payload and progress are not inferred",
            ));
            continue;
        }
        let candidates: Vec<_> = [o, c].into_iter().filter(|v| v.0 == a.0).collect();
        if candidates.is_empty() {
            mismatch.push(issue(
                absolute,
                "unexpected_block",
                "block identity differs from both stable states",
            ));
            continue;
        }
        let keys: BTreeSet<_> = candidates
            .iter()
            .flat_map(|v| v.1.keys())
            .chain(a.1.keys())
            .collect();
        for key in keys {
            let Some(value) = a.1.get(key) else {
                missing.push(issue(
                    absolute,
                    "missing_property",
                    &format!("missing {key}"),
                ));
                continue;
            };
            if candidates.iter().any(|v| v.1.get(key) == Some(value)) {
                continue;
            }
            if a.0 == "minecraft:redstone_wire"
                && key == "power"
                && value.parse::<u8>().is_ok_and(|v| v <= 15)
            {
                continue;
            }
            mismatch.push(issue(
                absolute,
                "unexpected_property",
                &format!("unexpected {key}={value}"),
            ));
        }
    }
    if !mismatch.is_empty() {
        return report(ConfigurationMismatch, mismatch);
    }
    if !missing.is_empty() {
        return report(ObservationIncomplete, missing);
    }
    if !moving.is_empty() {
        return report(Moving, moving);
    }
    PistonObservation::unresolved(
        Indeterminate,
        "mixed_or_unsettled_state",
        "observed cells fit individual states but do not form a complete open or closed state; motion cannot be inferred",
    )
}
