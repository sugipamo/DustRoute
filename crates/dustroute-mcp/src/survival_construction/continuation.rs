//! Fresh remaining-work contracts. Saved evidence never reconstructs a plan.
use super::*;
use dustroute_translate::snapshot::MinecraftSnapshotBlock;

#[derive(Debug, Serialize)]
pub(crate) struct SiteDiagnosis {
    pub completed_targets: Vec<[i32; 3]>,
    pub unbuilt_targets: Vec<[i32; 3]>,
    pub remaining_owned_temporary: Vec<TemporaryBlock>,
    pub conflicts: Vec<CellConflict>,
    pub ownership_is_conditional: bool,
}
#[derive(Debug, Serialize)]
pub(crate) struct CellConflict {
    pub position: [i32; 3],
    pub expected: NativeBlockState,
    pub actual: NativeBlockState,
}

pub(crate) fn scene_snapshot(scene: &CapturedSurvivalScene) -> Result<MinecraftSnapshot> {
    let bounds = scene.region();
    let scenario = scene.scenario();
    let mut blocks = Vec::new();
    for p in cells(Region::new(pos(bounds.min), pos(bounds.max))) {
        let state = scenario.block(xyz(p)).map_err(native_error)?;
        if state != air() {
            blocks.push(MinecraftSnapshotBlock {
                pos: p,
                name: state.name,
                properties: state.properties,
            });
        }
    }
    Ok(MinecraftSnapshot {
        min: pos(bounds.min),
        max: pos(bounds.max),
        blocks,
    })
}

/// Requires the original checked contract, a settled checkpoint and a new complete
/// scene. External changes are diagnosed and never made removal obligations.
pub(crate) fn assess(
    original: &ConstructionSite,
    checkpoint: &MinecraftSnapshot,
    owned: &[TemporaryBlock],
    fresh: &MinecraftSnapshot,
) -> Result<SiteDiagnosis> {
    let bad = |e: &str| ConstructionPlanningError::new("invalid_checkpoint_site", e);
    if Region::new(checkpoint.min, checkpoint.max) != original.scope.observed
        || checkpoint.min != fresh.min
        || checkpoint.max != fresh.max
    {
        return Err(bad(
            "checkpoint, fresh scene and original observation scope differ",
        ));
    }
    let saved = LiteralSnapshotIndex::new(checkpoint).map_err(|e| bad(&e.to_string()))?;
    let now = LiteralSnapshotIndex::new(fresh).map_err(|e| bad(&e.to_string()))?;
    let before = LiteralSnapshotIndex::new(&original.baseline).map_err(|e| bad(&e.to_string()))?;
    let mut ownership = BTreeMap::new();
    for block in owned {
        let p = pos(block.position);
        if !original.scope.temporary.iter().any(|r| r.contains(p))
            || !original.scope.edits.allows_change(p)
            || original.structure.contains_key(&block.position)
            || !matches!(
                block.state.name.as_str(),
                "minecraft:dirt" | "minecraft:stone"
            )
            || !block.state.properties.is_empty()
            || literal(&saved, p) != block.state
            || ownership
                .insert(block.position, block.state.clone())
                .is_some()
        {
            return Err(
                bad("invalid, duplicate or changed conditional temporary ownership")
                    .at(block.position),
            );
        }
    }
    // The checkpoint itself may contain only confirmed permanent additions or
    // owned temporary works inside the original Blueprint. Preserve its ground/air.
    for p in cells(Region::new(original.baseline.min, original.baseline.max)) {
        let initial = literal(&before, p);
        let expected = original
            .structure
            .get(&xyz(p))
            .or_else(|| ownership.get(&xyz(p)))
            .cloned()
            .unwrap_or_else(|| initial.clone());
        let actual = literal(&saved, p);
        if actual != expected && !(original.structure.contains_key(&xyz(p)) && actual == initial) {
            return Err(bad("checkpoint conflicts with original Blueprint obligations").at(xyz(p)));
        }
    }
    let conflicts = saved
        .changed_positions(&now)
        .map_err(|e| bad(&e.to_string()))?
        .into_iter()
        .map(|p| CellConflict {
            position: xyz(p),
            expected: literal(&saved, p),
            actual: literal(&now, p),
        })
        .collect();
    let mut completed_targets = Vec::new();
    let mut unbuilt_targets = Vec::new();
    for (&p, state) in &original.structure {
        if literal(&now, pos(p)) == *state {
            completed_targets.push(p);
        } else if literal(&now, pos(p)) == air() {
            unbuilt_targets.push(p);
        }
    }
    Ok(SiteDiagnosis {
        completed_targets,
        unbuilt_targets,
        remaining_owned_temporary: owned.to_vec(),
        conflicts,
        ownership_is_conditional: true,
    })
}

pub(crate) fn rebase(
    original: ConstructionSite,
    checkpoint: &MinecraftSnapshot,
    owned: &[TemporaryBlock],
    fresh: MinecraftSnapshot,
) -> Result<(ConstructionSite, SiteDiagnosis)> {
    let diagnosis = assess(&original, checkpoint, owned, &fresh)?;
    if !diagnosis.conflicts.is_empty() {
        return Err(ConstructionPlanningError::new(
            "checkpoint_site_changed",
            "fresh site differs; inspect conflicts before planning",
        ));
    }
    let index = LiteralSnapshotIndex::new(&fresh).expect("assessed snapshot");
    let structure = original
        .structure
        .into_iter()
        .filter(|(p, s)| literal(&index, pos(*p)) != *s)
        .collect();
    let mut final_blocks = index.to_owned_blocks();
    for temp in owned {
        final_blocks.remove(&pos(temp.position));
    }
    let final_index =
        LiteralSnapshotIndex::new(&original.expected).expect("checked original expected");
    for p in cells(Region::new(original.expected.min, original.expected.max)) {
        final_blocks.remove(&p);
        if let Some(block) = final_index.get(p) {
            final_blocks.insert(p, block.clone());
        }
    }
    let expected = MinecraftSnapshot {
        min: fresh.min,
        max: fresh.max,
        blocks: final_blocks.into_values().collect(),
    };
    Ok((
        ConstructionSite {
            baseline: fresh,
            expected,
            structure,
            scope: original.scope,
            initial_temporary: owned
                .iter()
                .map(|t| (t.position, t.state.clone()))
                .collect(),
        },
        diagnosis,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ConstructionSite, MinecraftSnapshot, Vec<TemporaryBlock>) {
        let design = super::super::tests::design();
        let original =
            ConstructionSite::from_grounded(&design, super::super::tests::scope()).unwrap();
        let mut snapshot = design.baseline;
        snapshot.min = original.scope.observed.min;
        snapshot.max = original.scope.observed.max;
        snapshot.blocks.push(MinecraftSnapshotBlock {
            pos: pos([0, 0, 0]),
            name: "minecraft:cobblestone".into(),
            properties: Default::default(),
        });
        // Preserve a surrounding block that is outside the Blueprint.
        snapshot.blocks.push(MinecraftSnapshotBlock {
            pos: pos([-5, -1, 0]),
            name: "minecraft:stone".into(),
            properties: Default::default(),
        });
        let temporary = TemporaryBlock {
            position: [-3, 0, 2],
            state: NativeBlockState {
                name: "minecraft:dirt".into(),
                properties: Default::default(),
            },
        };
        snapshot.blocks.push(MinecraftSnapshotBlock {
            pos: pos(temporary.position),
            name: temporary.state.name.clone(),
            properties: Default::default(),
        });
        (original, snapshot, vec![temporary])
    }
    #[test]
    fn remaining_contract_preserves_completed_work_and_requires_old_temporary_cleanup() {
        let (original, saved, owned) = fixture();
        let (site, diagnosis) = rebase(original, &saved, &owned, saved.clone()).unwrap();
        assert_eq!(diagnosis.completed_targets, vec![[0, 0, 0]]);
        assert_eq!(diagnosis.unbuilt_targets.len(), 48);
        assert!(!site.structure.contains_key(&[0, 0, 0]));
        assert_eq!(
            LiteralSnapshotIndex::new(&site.expected)
                .unwrap()
                .get(pos([-5, -1, 0]))
                .unwrap()
                .name,
            "minecraft:stone"
        );
        let mut ledger = Ledger::new(&site);
        assert_eq!(ledger.temporary.len(), 1);
        assert!(ledger.materials.required_supplied.is_empty());
        assert_eq!(
            ledger.materials.peak_temporary_in_world["minecraft:dirt"],
            1
        );
        super::super::tests::permanent(&mut ledger);
        let budget = BTreeMap::from([("minecraft:cobblestone".into(), 48)]);
        assert_eq!(
            ledger.clone().finish(&budget).unwrap_err().code,
            "incomplete_sequence"
        );
        ledger
            .remove(&HypotheticalBlockEdit {
                position: owned[0].position,
                before: owned[0].state.clone(),
                after: air(),
            })
            .unwrap();
        assert_eq!(
            ledger
                .clone()
                .finish(&BTreeMap::from([("minecraft:cobblestone".into(), 47)]))
                .unwrap_err()
                .missing_materials["minecraft:cobblestone"],
            1
        );
        assert_eq!(ledger.finish(&budget).unwrap().required_supplied, budget);
    }
    #[test]
    fn external_changes_are_diagnosed_without_becoming_removal_authority() {
        let (original, saved, owned) = fixture();
        let mut changed = saved.clone();
        changed
            .blocks
            .iter_mut()
            .find(|b| b.pos == pos(owned[0].position))
            .unwrap()
            .name = "minecraft:stone".into();
        let diagnosis = assess(&original, &saved, &owned, &changed).unwrap();
        assert_eq!(diagnosis.conflicts.len(), 1);
        assert_eq!(diagnosis.conflicts[0].position, owned[0].position);
        assert_eq!(
            rebase(original, &saved, &owned, changed).unwrap_err().code,
            "checkpoint_site_changed"
        );
    }
    #[test]
    fn checkpoint_cannot_invent_ownership_or_change_blueprint_air() {
        let (original, mut saved, owned) = fixture();
        assert!(
            assess(
                &original,
                &saved,
                &[owned[0].clone(), owned[0].clone()],
                &saved
            )
            .is_err()
        );
        let foreign = TemporaryBlock {
            position: [0, 0, 0],
            state: owned[0].state.clone(),
        };
        assert!(assess(&original, &saved, &[foreign], &saved).is_err());
        saved.blocks.push(MinecraftSnapshotBlock {
            pos: pos([2, 1, 2]),
            name: "minecraft:dirt".into(),
            properties: Default::default(),
        });
        assert_eq!(
            assess(&original, &saved, &owned, &saved).unwrap_err().code,
            "invalid_checkpoint_site"
        );
    }
}
