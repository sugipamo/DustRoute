//! Native EntityPosition + PositionFlag decoding shared by own/remote corrections.
use super::Reader;
use crate::versions::java_1_21_11::math::trig;
use anyhow::{Context, bail};

pub(super) struct Correction {
    pub position: [f64; 3],
    pub delta: [f64; 3],
    pub rotation: [f32; 2],
    pub flags: u32,
}
pub(super) struct Resolved {
    pub position: [f64; 3],
    pub rotation: [f32; 2],
    pub velocity: Option<[f64; 3]>,
}
impl Correction {
    pub fn read(r: &mut Reader<'_>) -> anyhow::Result<Self> {
        let value = Self {
            position: [r.f64()?, r.f64()?, r.f64()?],
            delta: [r.f64()?, r.f64()?, r.f64()?],
            rotation: [r.f32()?, r.f32()?],
            flags: r.u32()?,
        };
        if value.flags & !511 != 0 {
            bail!("unknown position flags");
        }
        Ok(value)
    }
    /// `velocity` must be the actual baseline being resolved, not a historical
    /// packet sample promoted to current simulated movement. Unknown stays None.
    pub fn resolve(
        &self,
        position: Option<[f64; 3]>,
        rotation: [f32; 2],
        velocity: Option<[f64; 3]>,
    ) -> anyhow::Result<Resolved> {
        let mut p = self.position;
        for (axis, value) in p.iter_mut().enumerate() {
            if self.flags & (1 << axis) != 0 {
                *value += position.context("relative position without baseline")?[axis];
            }
        }
        let mut r = self.rotation;
        for (axis, value) in r.iter_mut().enumerate() {
            if self.flags & (8 << axis) != 0 {
                *value += rotation[axis];
            }
        }
        if p.iter().any(|v| !v.is_finite() || v.abs() > 33_554_432.0)
            || r.iter().any(|v| !v.is_finite())
        {
            bail!("resolved correction outside finite world bounds");
        }
        r[1] = r[1].clamp(-90.0, 90.0);
        let v = if self.flags & 224 == 0 {
            Some(self.delta)
        } else if let Some(mut before) = velocity {
            if self.flags & 256 != 0 {
                let pitch = f64::from(rotation[1] - r[1]).to_radians() as f32;
                let yaw = f64::from(rotation[0] - r[0]).to_radians() as f32;
                let (s, c) = (f64::from(trig(pitch, false)), f64::from(trig(pitch, true)));
                before = [
                    before[0],
                    before[1] * c + before[2] * s,
                    before[2] * c - before[1] * s,
                ];
                let (s, c) = (f64::from(trig(yaw, false)), f64::from(trig(yaw, true)));
                before = [
                    before[0] * c + before[2] * s,
                    before[1],
                    before[2] * c - before[0] * s,
                ];
            }
            Some(std::array::from_fn(|axis| {
                self.delta[axis]
                    + if self.flags & (32 << axis) != 0 {
                        before[axis]
                    } else {
                        0.0
                    }
            }))
        } else {
            None
        };
        if v.is_some_and(|v| v.iter().any(|v| !v.is_finite())) {
            bail!("non-finite resolved velocity");
        }
        Ok(Resolved {
            position: p,
            rotation: r,
            velocity: v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_flag_combination_matches_native_resolution_and_wire_codec() {
        let scenarios: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../data/java_1_21_11/position_corrections.json"
        ))
        .unwrap();
        for scenario in scenarios.as_array().unwrap() {
            let position = serde_json::from_value(scenario["before"]["position"].clone()).unwrap();
            let rotation = serde_json::from_value(scenario["before"]["rotation"].clone()).unwrap();
            let velocity = serde_json::from_value(scenario["before"]["velocity"].clone()).unwrap();
            for case in scenario["cases"].as_array().unwrap() {
                let bytes = hex::decode(case["hex"].as_str().unwrap()).unwrap();
                let mut r = Reader::new(&bytes);
                assert_eq!(r.varint().unwrap(), 42);
                let correction = Correction::read(&mut r).unwrap();
                assert_eq!(correction.flags, case["flags"].as_u64().unwrap() as u32);
                assert!(r.bool().unwrap());
                r.end().unwrap();
                let result = correction
                    .resolve(Some(position), rotation, Some(velocity))
                    .unwrap();
                for axis in 0..3 {
                    assert!(
                        (result.position[axis]
                            - case["expected"]["position"][axis].as_f64().unwrap())
                        .abs()
                            < 1e-10
                    );
                    assert!(
                        (result.velocity.unwrap()[axis]
                            - case["expected"]["velocity"][axis].as_f64().unwrap())
                        .abs()
                            < 1e-10,
                        "flags {}",
                        correction.flags
                    );
                }
                for axis in 0..2 {
                    assert_eq!(
                        result.rotation[axis],
                        case["expected"]["rotation"][axis].as_f64().unwrap() as f32
                    );
                }
                let unknown = correction.resolve(Some(position), rotation, None).unwrap();
                assert_eq!(unknown.velocity.is_none(), correction.flags & 224 != 0);
            }
        }
    }
}
