//! Exact, version-pinned operation contract for an already built 1x2 door.
//! This is not a general placement proof and never creates ValidatedWorld.
use dustroute_translate::{MinecraftSnapshot, Pos, RegionBounds};
use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DoorState {
    Open,
    Closed,
}

#[derive(Clone, Debug)]
pub struct VerifiedDoor {
    origin: Pos,
    state: DoorState,
}
impl VerifiedDoor {
    pub fn state(&self) -> DoorState {
        self.state
    }
    pub fn lever(&self) -> Pos {
        self.origin.offset(2, 0, 4)
    }
    pub fn bounds(&self) -> RegionBounds {
        RegionBounds {
            min: self.origin.offset(-3, -2, -4),
            max: self.origin.offset(7, 3, 6),
        }
    }
}

#[derive(Deserialize)]
struct Contract {
    open: MinecraftSnapshot,
    closed: MinecraftSnapshot,
}
fn contract() -> Contract {
    serde_json::from_str(include_str!("piston_door_v1.json")).expect("embedded door contract")
}
type BlockMap = BTreeMap<Pos, (String, BTreeMap<String, String>)>;

fn blocks(snapshot: &MinecraftSnapshot, origin: Pos) -> Result<BlockMap, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut result = BTreeMap::new();
    for b in &snapshot.blocks {
        if !seen.insert(b.pos) {
            return Err("duplicate snapshot coordinate".into());
        }
        if b.pos.x < snapshot.min.x
            || b.pos.x > snapshot.max.x
            || b.pos.y < snapshot.min.y
            || b.pos.y > snapshot.max.y
            || b.pos.z < snapshot.min.z
            || b.pos.z > snapshot.max.z
        {
            return Err("block outside snapshot bounds".into());
        }
        if b.name == "minecraft:air" {
            continue;
        }
        let p = Pos::new(
            b.pos.x.checked_sub(origin.x).ok_or("coordinate overflow")?,
            b.pos.y.checked_sub(origin.y).ok_or("coordinate overflow")?,
            b.pos.z.checked_sub(origin.z).ok_or("coordinate overflow")?,
        );
        result.insert(p, (b.name.clone(), b.properties.clone()));
    }
    Ok(result)
}

/// Read-only reverse interpretation; stable output still passes `verify`
/// before an operation receives its private capability value.
pub fn inspect(
    snapshot: &MinecraftSnapshot,
    version: &str,
    complete: bool,
    known: Option<&VerifiedDoor>,
) -> dustroute_translate::PistonObservation {
    use dustroute_translate::{PistonObservation as Report, PistonObservationState as State};
    if version != "1.21.11" {
        return Report::unresolved(
            State::UnsupportedVersion,
            "unsupported_version",
            "door v1 requires Java 1.21.11",
        );
    }
    if !complete {
        return Report::unresolved(
            State::ObservationIncomplete,
            "incomplete_region",
            "complete observation required",
        );
    }
    let inferred = infer_origin(snapshot);
    let origin = match known
        .map(|d| d.origin)
        .or_else(|| inferred.as_ref().ok().copied())
    {
        Some(origin) => origin,
        None => {
            return Report::unresolved(
                State::ConfigurationMismatch,
                "unrecognized_anchor",
                &inferred.unwrap_err(),
            );
        }
    };
    let door = VerifiedDoor {
        origin,
        state: DoorState::Open,
    };
    let expected = contract();
    dustroute_translate::observe_piston_mechanism(
        snapshot,
        complete,
        version,
        &dustroute_translate::PistonObservationContract {
            version: "1.21.11",
            open: &expected.open,
            closed: &expected.closed,
            origin,
            required_bounds: door.bounds(),
        },
    )
}

fn infer_origin(snapshot: &MinecraftSnapshot) -> Result<Pos, String> {
    let levers: Vec<_> = snapshot
        .blocks
        .iter()
        .filter(|b| b.name == "minecraft:lever")
        .collect();
    if levers.len() != 1 {
        return Err("door v1 requires exactly one lever".into());
    }
    let p = levers[0].pos;
    let origin = Pos::new(
        p.x.checked_sub(2).ok_or("coordinate overflow")?,
        p.y,
        p.z.checked_sub(4).ok_or("coordinate overflow")?,
    );
    // Reserve arithmetic margin before calculating guard bounds.
    if [origin.x, origin.y, origin.z]
        .iter()
        .any(|v| *v < i32::MIN + 16 || *v > i32::MAX - 16)
    {
        return Err("coordinate overflow".into());
    }
    Ok(origin)
}

pub fn verify(snapshot: &MinecraftSnapshot, version: &str) -> Result<VerifiedDoor, String> {
    if version != "1.21.11" {
        return Err("door v1 requires Java 1.21.11".into());
    }
    let origin = infer_origin(snapshot)?;
    let mut door = VerifiedDoor {
        origin,
        state: DoorState::Open,
    };
    let bounds = door.bounds();
    if snapshot.min.x > bounds.min.x
        || snapshot.min.y > bounds.min.y
        || snapshot.min.z > bounds.min.z
        || snapshot.max.x < bounds.max.x
        || snapshot.max.y < bounds.max.y
        || snapshot.max.z < bounds.max.z
    {
        return Err("include the entire door and its one-block empty guard region".into());
    }
    let actual = blocks(snapshot, origin)?;
    let contract = contract();
    for (state, expected) in [
        (DoorState::Open, contract.open),
        (DoorState::Closed, contract.closed),
    ] {
        if actual == blocks(&expected, Pos::new(0, 0, 0))? {
            door.state = state;
            return Ok(door);
        }
    }
    Err("snapshot differs from the verified door v1 layout or is not fully settled".into())
}

#[cfg(test)]
pub(crate) fn sample(state: DoorState) -> MinecraftSnapshot {
    let c = contract();
    let mut s = if state == DoorState::Open {
        c.open
    } else {
        c.closed
    };
    s.min = Pos::new(-3, -2, -4);
    s.max = Pos::new(7, 3, 6);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_placement_requires_empty_complete_site_and_exact_open_undo() {
        let mut empty = sample(DoorState::Open);
        empty.blocks.clear();
        let proof = ValidatedDoorPlacement::new(Pos::new(0, 0, 0), &empty, "1.21.11").unwrap();
        assert!(
            proof
                .validate_built(&sample(DoorState::Open), "1.21.11")
                .is_ok()
        );
        assert!(
            proof
                .validate_built(&sample(DoorState::Closed), "1.21.11")
                .is_err()
        );
        assert!(
            ValidatedDoorPlacement::new(Pos::new(0, 0, 0), &sample(DoorState::Open), "1.21.11")
                .is_err()
        );
        assert!(ValidatedDoorPlacement::new(Pos::new(0, 0, 0), &empty, "1.21.10").is_err());
        let mut partial = empty.clone();
        partial.min.x += 1;
        assert!(ValidatedDoorPlacement::new(Pos::new(0, 0, 0), &partial, "1.21.11").is_err());
        assert!(ValidatedDoorPlacement::new(Pos::new(i32::MAX, 0, 0), &empty, "1.21.11").is_err());
        assert!(
            proof
                .writes(true)
                .as_array()
                .unwrap()
                .iter()
                .all(|b| b["state"] == "minecraft:air")
        );
        let world = dustroute_translate::world_from_snapshot(proof.initial()).unwrap();
        assert!(dustroute_translate::ValidatedWorld::try_from(world).is_err());
    }
    #[test]
    fn reverse_observation_distinguishes_evidence_without_authorizing_it() {
        use dustroute_translate::PistonObservationState as S;
        let open = sample(DoorState::Open);
        let closed = sample(DoorState::Closed);
        let known = verify(&open, "1.21.11").unwrap();
        let classify = |s: &MinecraftSnapshot| inspect(s, "1.21.11", true, Some(&known));
        assert_eq!(classify(&open).state, S::Open);
        assert_eq!(classify(&closed).state, S::Closed);
        assert_eq!(
            inspect(&open, "1.21.10", true, None).state,
            S::UnsupportedVersion
        );
        assert_eq!(
            inspect(&open, "1.21.11", false, None).state,
            S::ObservationIncomplete
        );
        let mut partial = open.clone();
        partial.min.x += 1;
        assert_eq!(classify(&partial).state, S::ObservationIncomplete);
        let mut missing = open.clone();
        missing
            .blocks
            .iter_mut()
            .find(|b| b.name == "minecraft:sticky_piston")
            .unwrap()
            .properties
            .remove("extended");
        assert_eq!(classify(&missing).state, S::ObservationIncomplete);
        let mut duplicate = open.clone();
        duplicate.blocks.push(duplicate.blocks[0].clone());
        assert_eq!(classify(&duplicate).state, S::ObservationIncomplete);
        let mut mixed = open.clone();
        mixed.blocks.retain(|b| b.pos.y != 1);
        mixed
            .blocks
            .extend(closed.blocks.iter().filter(|b| b.pos.y == 1).cloned());
        assert_eq!(classify(&mixed).state, S::Indeterminate);
        assert!(verify(&mixed, "1.21.11").is_err());
        let mut moving = open.clone();
        let block = moving
            .blocks
            .iter_mut()
            .find(|b| b.name == "minecraft:sticky_piston")
            .unwrap();
        let position = block.pos;
        block.name = "minecraft:moving_piston".into();
        block.properties.clear();
        let report = classify(&moving);
        assert_eq!(report.state, S::Moving);
        assert_eq!(report.issues[0].position, Some(position));
        assert!(verify(&moving, "1.21.11").is_err());
        moving.blocks.retain(|b| b.name != "minecraft:lever");
        assert_eq!(classify(&moving).state, S::ConfigurationMismatch);
        let mut extra = open.clone();
        let mut block = extra.blocks[0].clone();
        block.pos = Pos::new(-3, 0, 0);
        block.name = "minecraft:moving_piston".into();
        extra.blocks.push(block);
        assert_eq!(classify(&extra).state, S::ConfigurationMismatch);
        let mut translated = closed.clone();
        for b in &mut translated.blocks {
            b.pos = b.pos.offset(1100, 180, 1000);
        }
        translated.min = translated.min.offset(1100, 180, 1000);
        translated.max = translated.max.offset(1100, 180, 1000);
        assert_eq!(inspect(&translated, "1.21.11", true, None).state, S::Closed);
    }
    #[test]
    fn exact_states_and_translation_only() {
        for state in [DoorState::Open, DoorState::Closed] {
            let mut s = sample(state);
            for b in &mut s.blocks {
                b.pos = b.pos.offset(1100, 180, 1000);
            }
            s.min = s.min.offset(1100, 180, 1000);
            s.max = s.max.offset(1100, 180, 1000);
            let door = verify(&s, "1.21.11").unwrap();
            assert_eq!(door.state(), state);
            assert_eq!(door.lever(), Pos::new(1102, 180, 1004));
            assert!(
                dustroute_translate::ValidatedWorld::try_from(
                    dustroute_translate::world_from_snapshot(&s).unwrap()
                )
                .is_err()
            );
        }
    }
    #[test]
    fn every_required_block_is_checked() {
        for state in [DoorState::Open, DoorState::Closed] {
            let s = sample(state);
            for i in 0..s.blocks.len() {
                let mut changed = s.clone();
                changed.blocks.remove(i);
                assert!(
                    verify(&changed, "1.21.11").is_err(),
                    "missing {:?}",
                    s.blocks[i].pos
                );
            }
        }
    }
    #[test]
    fn rejects_wrong_version_guard_duplicates_and_signal_shape() {
        let s = sample(DoorState::Open);
        assert!(verify(&s, "1.21.10").is_err());
        let mut changed = s.clone();
        changed.min.x += 1;
        assert!(verify(&changed, "1.21.11").is_err());
        let mut changed = s.clone();
        changed.blocks.push(changed.blocks[0].clone());
        assert!(verify(&changed, "1.21.11").is_err());
        let mut changed = s.clone();
        let mut extra = changed.blocks[0].clone();
        extra.pos = Pos::new(-3, 0, 0);
        changed.blocks.push(extra);
        assert!(verify(&changed, "1.21.11").is_err());
        for (key, value) in [("power", "1"), ("north", "up")] {
            let mut changed = s.clone();
            changed
                .blocks
                .iter_mut()
                .find(|b| b.name == "minecraft:redstone_wire")
                .unwrap()
                .properties
                .insert(key.into(), value.into());
            assert!(verify(&changed, "1.21.11").is_err());
        }
        let mut changed = s.clone();
        changed
            .blocks
            .iter_mut()
            .find(|b| b.name == "minecraft:lever")
            .unwrap()
            .properties
            .insert("face".into(), "wall".into());
        assert!(verify(&changed, "1.21.11").is_err());
    }
}

/// Immutable proof for the one pinned, empty-site initial placement. This is
/// deliberately not a general ValidatedWorld or an arbitrary block-write API.
#[derive(Clone, Debug)]
pub struct ValidatedDoorPlacement {
    door: VerifiedDoor,
    initial: MinecraftSnapshot,
}
impl ValidatedDoorPlacement {
    pub fn new(origin: Pos, baseline: &MinecraftSnapshot, version: &str) -> Result<Self, String> {
        if [origin.x, origin.y, origin.z]
            .iter()
            .any(|v| *v < i32::MIN + 16 || *v > i32::MAX - 16)
        {
            return Err("coordinate overflow".into());
        }
        let door = VerifiedDoor {
            origin,
            state: DoorState::Open,
        };
        let bounds = door.bounds();
        let mut initial = contract().open;
        initial.min = bounds.min;
        initial.max = bounds.max;
        for block in &mut initial.blocks {
            block.pos = block.pos.offset(origin.x, origin.y, origin.z);
        }
        verify(&initial, version)?;
        // Retain all ordinary structural/state checks. Only this exact fixed
        // template may carry the already verified pistons through this proof.
        let world =
            dustroute_translate::world_from_snapshot(&initial).map_err(|e| e.to_string())?;
        if world.placement_issues().iter().any(|issue| {
            !matches!(
                issue,
                dustroute_translate::WorldValidationIssue::UnsupportedPlacement {
                    kind: dustroute_translate::BlockKind::Piston,
                    ..
                }
            )
        }) {
            return Err("fixed placement failed structural validation".into());
        }
        let proof = Self { door, initial };
        proof.validate_empty(baseline, version)?;
        Ok(proof)
    }
    pub fn bounds(&self) -> RegionBounds {
        self.door.bounds()
    }
    pub fn origin(&self) -> Pos {
        self.door.origin
    }
    pub fn initial(&self) -> &MinecraftSnapshot {
        &self.initial
    }
    pub fn validate_empty(
        &self,
        snapshot: &MinecraftSnapshot,
        version: &str,
    ) -> Result<(), String> {
        if version != "1.21.11" {
            return Err("door placement requires Java 1.21.11".into());
        }
        let bounds = self.bounds();
        if snapshot.min != bounds.min || snapshot.max != bounds.max {
            return Err("exact complete placement guard scan required".into());
        }
        if !blocks(snapshot, self.origin())?.is_empty() {
            return Err("placement and guard must be entirely empty".into());
        }
        Ok(())
    }
    pub fn validate_built(
        &self,
        snapshot: &MinecraftSnapshot,
        version: &str,
    ) -> Result<(), String> {
        let actual = verify(snapshot, version)?;
        if actual.origin != self.origin() || actual.state != DoorState::Open {
            return Err("restore the exact open state before undoing placement".into());
        }
        Ok(())
    }
    pub fn writes(&self, undo: bool) -> serde_json::Value {
        let mut blocks = self.initial.blocks.iter().collect::<Vec<_>>();
        blocks.sort_by_key(|b| {
            (
                match b.name.as_str() {
                    "minecraft:stone" => 0,
                    "minecraft:sticky_piston" | "minecraft:piston" => 1,
                    "minecraft:lever" => 3,
                    _ => 2,
                },
                b.pos.y,
                b.pos.x,
                b.pos.z,
            )
        });
        if undo {
            blocks.reverse();
        }
        serde_json::json!(
            blocks
                .into_iter()
                .map(|b| {
                    let props = b
                        .properties
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    let state = if undo {
                        "minecraft:air".into()
                    } else if props.is_empty() {
                        b.name.clone()
                    } else {
                        format!("{}[{props}]", b.name)
                    };
                    serde_json::json!({"pos":b.pos,"state":state})
                })
                .collect::<Vec<_>>()
        )
    }
}
