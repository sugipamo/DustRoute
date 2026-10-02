//! Native MathHelper indexed trigonometry (angles in radians).
pub(super) fn trig(angle: f32, cosine: bool) -> f32 {
    let index = ((f64::from(angle) * 10430.378350470453 + if cosine { 16384.0 } else { 0.0 })
        as i64)
        & 65535;
    (index as f64 / 10430.378350470453).sin() as f32
}
