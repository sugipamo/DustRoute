//! Explicit state-change permission, independent of source claims/ownership.
use dustroute_minecraft::{Pos, Region};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldEditScope {
    pub editable: Vec<Region>,
    #[serde(default)]
    pub protected: Vec<Region>,
}
impl WorldEditScope {
    /// Omitted scope on older APIs means the complete observed work rectangle.
    /// It never implies ownership or permission beyond that rectangle.
    pub fn entire(region: Region) -> Self {
        Self {
            editable: vec![region],
            protected: vec![],
        }
    }
    pub fn allows_change(&self, position: Pos) -> bool {
        self.editable.iter().any(|r| r.contains(position))
            && !self.protected.iter().any(|r| r.contains(position))
    }
    /// Every observed cell outside editable regions is protected implicitly.
    /// Explicit protected regions document intent and cannot overlap editable.
    pub fn validate(&self, known: Region) -> Result<(), String> {
        let valid = |r: &Region| r.min.x <= r.max.x && r.min.y <= r.max.y && r.min.z <= r.max.z;
        if !valid(&known) {
            return Err("edit scope requires ordered complete observation bounds".into());
        }
        if self.editable.is_empty() || self.editable.len() > 64 || self.protected.len() > 64 {
            return Err(
                "edit scope requires 1..64 editable regions and at most 64 protected regions"
                    .into(),
            );
        }
        for r in self.editable.iter().chain(&self.protected) {
            if !valid(r) || !known.contains(r.min) || !known.contains(r.max) {
                return Err(format!(
                    "edit scope region {r:?} is invalid or outside the complete observation"
                ));
            }
        }
        let intersects = |a: &Region, b: &Region| {
            a.min.x <= b.max.x
                && b.min.x <= a.max.x
                && a.min.y <= b.max.y
                && b.min.y <= a.max.y
                && a.min.z <= b.max.z
                && b.min.z <= a.max.z
        };
        if let Some((a, b)) = self.editable.iter().find_map(|a| {
            self.protected
                .iter()
                .find(|b| intersects(a, b))
                .map(|b| (a, b))
        }) {
            return Err(format!(
                "editable region {a:?} overlaps protected region {b:?}"
            ));
        }
        Ok(())
    }
}
