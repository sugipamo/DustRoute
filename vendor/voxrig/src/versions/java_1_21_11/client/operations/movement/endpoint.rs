//! One observation contract shared by actual admission and hypothetical aiming.
//! A prospective bound is never an observation or a send capability.
pub(super) const MAX_PACKET_ERROR: f64 = 1.0 / 4096.0;
pub(super) const MATCH_EPSILON: f64 = 1e-9;
pub(super) const MAX_DISCREPANCY: f64 = MAX_PACKET_ERROR + MATCH_EPSILON;
pub(super) const MAX_AIM_ERROR: f64 = MAX_PACKET_ERROR + MAX_DISCREPANCY;

pub(super) fn axis_matches(predicted: f64, observed: f64, packet_error: f64) -> bool {
    (0.0..=MAX_PACKET_ERROR).contains(&packet_error)
        && (predicted - observed).abs() <= packet_error + MATCH_EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admitted_axis_errors_fit_the_prospective_bound() {
        for packet_error in [0.0, MAX_PACKET_ERROR / 2.0, MAX_PACKET_ERROR] {
            for sign in [-1.0, 1.0] {
                let observed = sign * (packet_error + MATCH_EPSILON);
                assert!(axis_matches(0.0, observed, packet_error));
                assert!(packet_error + observed.abs() <= MAX_AIM_ERROR);
                assert!(!axis_matches(0.0, observed * 1.001, packet_error));
            }
        }
        for error in [-0.1, MAX_PACKET_ERROR * 1.001, f64::NAN, f64::INFINITY] {
            assert!(!axis_matches(0.0, 0.0, error));
        }
    }
}
