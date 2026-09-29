//! Literal differences, independent of runtime support and failure provenance.
//! Callers must establish complete, stable observation and label the reference.
use std::collections::BTreeSet;

use crate::snapshot::{MinecraftSnapshotBlock, index_literal_snapshot};
use crate::{MinecraftSnapshot, Pos};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferenceKind {
    Missing,
    Unexpected,
    DifferentBlock,
    Orientation,
    Configuration,
    State,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PropertyDifference {
    pub property: String,
    pub observed: Option<String>,
    pub target: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotDifference {
    pub position: Pos,
    pub observed: Option<MinecraftSnapshotBlock>,
    pub target: Option<MinecraftSnapshotBlock>,
    pub kinds: BTreeSet<DifferenceKind>,
    pub properties: Vec<PropertyDifference>,
}

pub fn differences(
    observed: &MinecraftSnapshot,
    target: &MinecraftSnapshot,
) -> Result<Vec<SnapshotDifference>, String> {
    if observed.min != target.min || observed.max != target.max {
        return Err("diagnosis requires the exact complete reference region".into());
    }
    let actual = index_literal_snapshot(observed)?;
    let expected = index_literal_snapshot(target)?;
    let positions: BTreeSet<_> = actual.keys().chain(expected.keys()).copied().collect();
    let mut findings = Vec::new();
    for position in positions {
        let observed = actual.get(&position);
        let target = expected.get(&position);
        if observed == target {
            continue;
        }
        let mut kinds = BTreeSet::new();
        let mut properties = Vec::new();
        match (observed, target) {
            (None, Some(_)) => {
                kinds.insert(DifferenceKind::Missing);
            }
            (Some(_), None) => {
                kinds.insert(DifferenceKind::Unexpected);
            }
            (Some(actual), Some(expected)) if actual.name != expected.name => {
                kinds.insert(DifferenceKind::DifferentBlock);
            }
            (Some(actual), Some(expected)) => {
                let keys: BTreeSet<_> = actual
                    .properties
                    .keys()
                    .chain(expected.properties.keys())
                    .collect();
                for key in keys {
                    let before = actual.properties.get(key);
                    let after = expected.properties.get(key);
                    if before == after {
                        continue;
                    }
                    kinds.insert(super::property_policy::role(key).difference_kind());
                    properties.push(PropertyDifference {
                        property: key.clone(),
                        observed: before.cloned(),
                        target: after.cloned(),
                    });
                }
            }
            (None, None) => unreachable!(),
        }
        findings.push(SnapshotDifference {
            position,
            observed: observed.cloned(),
            target: target.cloned(),
            kinds,
            properties,
        });
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn describes_missing_extra_replaced_and_multiple_property_changes_without_simulating_damage() {
        let target: MinecraftSnapshot = serde_json::from_value(json!({
            "min":{"x":0,"y":0,"z":0},"max":{"x":4,"y":0,"z":0},"blocks":[
                {"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone"},
                {"pos":{"x":1,"y":0,"z":0},"name":"minecraft:repeater","properties":{"facing":"north","delay":"4","powered":"false"}},
                {"pos":{"x":2,"y":0,"z":0},"name":"minecraft:sticky_piston","properties":{"facing":"up","extended":"false"}}
            ]})).unwrap();
        let mut actual = target.clone();
        actual.blocks.remove(0);
        actual.blocks[0].properties.extend([
            ("facing".into(), "south".into()),
            ("delay".into(), "1".into()),
            ("powered".into(), "true".into()),
        ]);
        actual.blocks[1].name = "minecraft:chest".into();
        actual.blocks.push(MinecraftSnapshotBlock {
            pos: Pos::new(3, 0, 0),
            name: "minecraft:stone".into(),
            properties: Default::default(),
        });
        let report = differences(&actual, &target).unwrap();
        assert_eq!(report.len(), 4);
        assert_eq!(report[0].kinds, BTreeSet::from([DifferenceKind::Missing]));
        assert_eq!(
            report[1].kinds,
            BTreeSet::from([
                DifferenceKind::Orientation,
                DifferenceKind::Configuration,
                DifferenceKind::State
            ])
        );
        assert_eq!(report[1].properties.len(), 3);
        assert_eq!(
            report[2].kinds,
            BTreeSet::from([DifferenceKind::DifferentBlock])
        );
        assert_eq!(
            report[3].kinds,
            BTreeSet::from([DifferenceKind::Unexpected])
        );
        assert_eq!(report[2].observed.as_ref().unwrap().name, "minecraft:chest");
        assert!(differences(&target, &target).unwrap().is_empty());
        actual.max.x += 1;
        assert!(differences(&actual, &target).is_err());
        actual.max = target.max;
        actual.blocks.push(actual.blocks[0].clone());
        assert!(differences(&actual, &target).is_err());
    }

    #[test]
    fn unsupported_block_properties_still_receive_literal_classification() {
        let target: MinecraftSnapshot = serde_json::from_value(json!({
            "min":{"x":0,"y":0,"z":0},"max":{"x":0,"y":0,"z":0},
            "blocks":[{"pos":{"x":0,"y":0,"z":0},"name":"example:unmodeled", "properties":{"new_setting":"a","axis":"x"}}]
        })).unwrap();
        let mut actual = target.clone();
        actual.blocks[0]
            .properties
            .insert("new_setting".into(), "b".into());
        actual.blocks[0]
            .properties
            .insert("axis".into(), "z".into());
        actual.blocks[0]
            .properties
            .insert("lit".into(), "true".into());
        let findings = differences(&actual, &target).unwrap();
        assert_eq!(
            findings[0].kinds,
            BTreeSet::from([
                DifferenceKind::Orientation,
                DifferenceKind::Configuration,
                DifferenceKind::State
            ])
        );
        assert_eq!(findings[0].properties.len(), 3);
    }
}
