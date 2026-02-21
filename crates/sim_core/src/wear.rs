//! Wear math — generic across modules and (future) ships.

use crate::Constants;

/// Returns the efficiency multiplier for the given wear level.
/// Pure function — no mutation.
pub fn wear_efficiency(wear: f32, constants: &Constants) -> f32 {
    if wear >= constants.wear_band_critical_threshold {
        constants.wear_band_critical_efficiency
    } else if wear >= constants.wear_band_degraded_threshold {
        constants.wear_band_degraded_efficiency
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::base_content;

    #[test]
    fn nominal_band_full_efficiency() {
        let constants = &base_content().constants;
        assert!((wear_efficiency(0.0, constants) - 1.0).abs() < 1e-5);
        assert!((wear_efficiency(0.49, constants) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn degraded_band_reduced_efficiency() {
        let constants = &base_content().constants;
        assert!((wear_efficiency(0.5, constants) - 0.75).abs() < 1e-5);
        assert!((wear_efficiency(0.79, constants) - 0.75).abs() < 1e-5);
    }

    #[test]
    fn critical_band_heavily_reduced() {
        let constants = &base_content().constants;
        assert!((wear_efficiency(0.8, constants) - 0.5).abs() < 1e-5);
        assert!((wear_efficiency(1.0, constants) - 0.5).abs() < 1e-5);
    }
}
