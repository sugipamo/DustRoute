//! Typed native own-player attributes; registry/default facts are oracle-checked.
use super::*;

#[derive(Clone, Copy)]
enum Field {
    Scale,
    BlockBreakSpeed,
    MiningEfficiency,
    SubmergedMiningSpeed,
    MovementSpeed,
    Gravity,
    JumpStrength,
    StepHeight,
    MovementEfficiency,
    SneakingSpeed,
    SafeFallDistance,
    FallDamageMultiplier,
}
impl Field {
    fn slot(self, player: &mut LocalPlayerState) -> &mut Option<AttributeValue> {
        match self {
            Self::Scale => &mut player.scale,
            Self::BlockBreakSpeed => &mut player.block_break_speed,
            Self::MiningEfficiency => &mut player.mining_efficiency,
            Self::SubmergedMiningSpeed => &mut player.submerged_mining_speed,
            Self::MovementSpeed => &mut player.movement_speed,
            Self::Gravity => &mut player.gravity,
            Self::JumpStrength => &mut player.jump_strength,
            Self::StepHeight => &mut player.step_height,
            Self::MovementEfficiency => &mut player.movement_efficiency,
            Self::SneakingSpeed => &mut player.sneaking_speed,
            Self::SafeFallDistance => &mut player.safe_fall_distance,
            Self::FallDamageMultiplier => &mut player.fall_damage_multiplier,
        }
    }
}
struct Definition {
    id: i32,
    initial: f64,
    min: f64,
    max: f64,
    field: Field,
}
const DEFINITIONS: &[Definition] = &[
    Definition {
        id: ids::SCALE_ATTRIBUTE,
        initial: 1.0,
        min: 0.0625,
        max: 16.0,
        field: Field::Scale,
    },
    Definition {
        id: 5,
        initial: 1.0,
        min: 0.0,
        max: 1024.0,
        field: Field::BlockBreakSpeed,
    },
    Definition {
        id: 20,
        initial: 0.0,
        min: 0.0,
        max: 1024.0,
        field: Field::MiningEfficiency,
    },
    Definition {
        id: 29,
        initial: 0.2,
        min: 0.0,
        max: 20.0,
        field: Field::SubmergedMiningSpeed,
    },
    Definition {
        id: 22,
        initial: 0.1_f32 as f64,
        min: 0.0,
        max: 1024.0,
        field: Field::MovementSpeed,
    },
    Definition {
        id: 14,
        initial: 0.08,
        min: -1.0,
        max: 1.0,
        field: Field::Gravity,
    },
    Definition {
        id: 15,
        initial: 0.42_f32 as f64,
        min: 0.0,
        max: 32.0,
        field: Field::JumpStrength,
    },
    Definition {
        id: 28,
        initial: 0.6,
        min: 0.0,
        max: 10.0,
        field: Field::StepHeight,
    },
    Definition {
        id: 21,
        initial: 0.0,
        min: 0.0,
        max: 1.0,
        field: Field::MovementEfficiency,
    },
    Definition {
        id: 26,
        initial: 0.3,
        min: 0.0,
        max: 1.0,
        field: Field::SneakingSpeed,
    },
    Definition {
        id: 24,
        initial: 3.0,
        min: -1024.0,
        max: 1024.0,
        field: Field::SafeFallDistance,
    },
    Definition {
        id: 11,
        initial: 1.0,
        min: 0.0,
        max: 100.0,
        field: Field::FallDamageMultiplier,
    },
];
pub(super) fn initialize(player: &mut LocalPlayerState) {
    for d in DEFINITIONS {
        *d.field.slot(player) = Some(AttributeValue {
            value: d.initial,
            basis: ValueBasis::NativeReset,
        });
    }
}
pub(super) fn received(player: &mut LocalPlayerState, values: &BTreeMap<i32, f64>, sequence: u64) {
    for d in DEFINITIONS {
        if let Some(value) = values.get(&d.id) {
            *d.field.slot(player) = Some(AttributeValue {
                value: value.clamp(d.min, d.max),
                basis: ValueBasis::Received { sequence },
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_typed_attributes_match_native_registry_defaults_and_limits() {
        let sources: [serde_json::Value; 2] = [
            serde_json::from_str(include_str!(
                "../../../../../../data/java_1_21_11/survival_foundation.json"
            ))
            .unwrap(),
            serde_json::from_str(include_str!(
                "../../../../../../data/java_1_21_11/survival_movement.json"
            ))
            .unwrap(),
        ];
        let native: Vec<_> = sources
            .iter()
            .flat_map(|s| s["attributes"].as_array().unwrap())
            .collect();
        assert_eq!(native.len(), DEFINITIONS.len());
        let mut player = LocalPlayerState::spawned(42);
        let mut seen = std::collections::BTreeSet::new();
        for d in DEFINITIONS {
            assert!(seen.insert(d.id), "duplicate attribute dispatch");
            let n = native
                .iter()
                .find(|n| n["id"].as_i64() == Some(i64::from(d.id)))
                .unwrap();
            assert_eq!(n["default"].as_f64().unwrap(), d.initial);
            assert_eq!(n["min"].as_f64().unwrap(), d.min);
            assert_eq!(n["max"].as_f64().unwrap(), d.max);
            let v = d.field.slot(&mut player).unwrap();
            assert_eq!(v.value, d.initial);
            assert_eq!(v.basis, ValueBasis::NativeReset);
        }
        assert!(
            sources[1]["attributes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|a| a["tracked"] == true)
        );
    }
}
