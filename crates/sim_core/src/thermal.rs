//! Energy↔thermal unit conversion helpers.
//!
//! All sim-state arithmetic uses integer types:
//! - Temperature: `u32` milli-Kelvin
//! - Energy: `i64` Joules
//! - Power: `f32` Watts (content-defined, converted at boundary)
//!
//! Floats appear only in content definitions and these conversion boundaries.

use crate::{Constants, RecipeThermalReq};

/// Seconds per tick, derived from `minutes_per_tick`.
#[inline]
pub fn dt_seconds(constants: &Constants) -> f64 {
    f64::from(constants.minutes_per_tick) * 60.0
}

/// Convert a power draw (Watts = J/s) over one tick into heat energy (Joules).
///
/// Clamps to `i64` range before casting to prevent undefined overflow.
#[inline]
#[allow(clippy::cast_possible_truncation)] // safe: clamped to i64 range
pub fn power_to_heat_j(watts: f32, dt_s: f64) -> i64 {
    let joules = f64::from(watts) * dt_s;
    joules.clamp(i64::MIN as f64, i64::MAX as f64) as i64
}

/// Convert heat energy (Joules) into a temperature delta (milli-Kelvin),
/// given a heat capacity in J/K.
///
/// Returns a signed delta: positive heat raises temperature, negative lowers it.
/// Panics in debug if `capacity_j_per_k` is zero.
/// Clamps to `i32` range before casting to prevent undefined overflow.
#[inline]
#[allow(clippy::cast_possible_truncation)] // safe: clamped to i32 range
pub fn heat_to_temp_delta_mk(heat_j: i64, capacity_j_per_k: f32) -> i32 {
    debug_assert!(
        capacity_j_per_k > 0.0,
        "heat capacity must be positive, got {capacity_j_per_k}"
    );
    // delta_K = heat_j / capacity_j_per_k
    // delta_mK = delta_K * 1000
    let delta_k = heat_j as f64 / f64::from(capacity_j_per_k);
    let delta_milli_kelvin = delta_k * 1000.0;
    delta_milli_kelvin.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// Efficiency scaling based on temperature (affects material yield).
///
/// - Below `min_temp_mk`: 0.0 (caller should stall instead of calling this)
/// - `min_temp_mk` → `optimal_min_mk`: linear ramp from 0.8 to 1.0
/// - `optimal_min_mk` and above: 1.0
#[inline]
pub fn thermal_efficiency(temp_mk: u32, req: &RecipeThermalReq) -> f32 {
    if temp_mk < req.min_temp_mk {
        return 0.0;
    }
    if temp_mk >= req.optimal_min_mk {
        return 1.0;
    }
    // Linear interpolation: min→optimal_min maps to 0.8→1.0
    let range = req.optimal_min_mk - req.min_temp_mk;
    if range == 0 {
        return 1.0;
    }
    let progress = (temp_mk - req.min_temp_mk) as f32 / range as f32;
    0.8 + 0.2 * progress
}

/// Quality scaling based on temperature.
///
/// - Below `optimal_max_mk`: 1.0
/// - `optimal_max_mk` → `max_temp_mk`: linear ramp from 1.0 to 0.6
/// - Above `max_temp_mk`: 0.3
#[inline]
pub fn thermal_quality_factor(temp_mk: u32, req: &RecipeThermalReq) -> f32 {
    if temp_mk <= req.optimal_max_mk {
        return 1.0;
    }
    if temp_mk > req.max_temp_mk {
        return 0.3;
    }
    // Linear interpolation: optimal_max→max maps to 1.0→0.6
    let range = req.max_temp_mk - req.optimal_max_mk;
    if range == 0 {
        return 1.0;
    }
    let progress = (temp_mk - req.optimal_max_mk) as f32 / range as f32;
    1.0 - 0.4 * progress
}

/// Returns the wear rate multiplier for a module's overheat zone.
///
/// - Nominal: 1.0 (no extra wear)
/// - Warning: `thermal_wear_multiplier_warning` (default 2.0)
/// - Critical: `thermal_wear_multiplier_critical` (default 4.0)
#[inline]
pub fn heat_wear_multiplier(zone: crate::OverheatZone, constants: &Constants) -> f32 {
    match zone {
        crate::OverheatZone::Nominal => 1.0,
        crate::OverheatZone::Warning => constants.thermal_wear_multiplier_warning,
        crate::OverheatZone::Critical => constants.thermal_wear_multiplier_critical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constants_with_minutes_per_tick(minutes: u32) -> Constants {
        let mut c = crate::test_fixtures::minimal_content().constants;
        c.minutes_per_tick = minutes;
        c
    }

    #[test]
    fn dt_seconds_one_minute_per_tick() {
        let constants = constants_with_minutes_per_tick(1);
        assert!((dt_seconds(&constants) - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dt_seconds_sixty_minutes_per_tick() {
        let constants = constants_with_minutes_per_tick(60);
        assert!((dt_seconds(&constants) - 3600.0).abs() < f64::EPSILON);
    }

    #[test]
    fn power_to_heat_100w_60s() {
        assert_eq!(power_to_heat_j(100.0, 60.0), 6_000);
    }

    #[test]
    fn power_to_heat_zero_power() {
        assert_eq!(power_to_heat_j(0.0, 60.0), 0);
    }

    #[test]
    fn heat_to_temp_delta_basic() {
        // 6000 J / 100 J/K = 60 K = 60_000 mK
        assert_eq!(heat_to_temp_delta_mk(6_000, 100.0), 60_000);
    }

    #[test]
    fn heat_to_temp_delta_negative() {
        // -6000 J / 100 J/K = -60 K = -60_000 mK
        assert_eq!(heat_to_temp_delta_mk(-6_000, 100.0), -60_000);
    }

    #[test]
    fn large_power_to_heat_no_overflow() {
        // 1 GW for 1 hour = 3.6 TJ — fits comfortably in i64
        let dt_s = 3600.0;
        let watts = 1_000_000_000.0_f32; // 1 GW
        let heat = power_to_heat_j(watts, dt_s);
        assert_eq!(heat, 3_600_000_000_000); // 3.6 TJ
    }

    #[test]
    fn realistic_smelter_delta() {
        // 10 kW smelter, 1-minute tick, 500 J/K capacity
        let heat = power_to_heat_j(10_000.0, 60.0); // 600_000 J
        assert_eq!(heat, 600_000);
        let delta = heat_to_temp_delta_mk(heat, 500.0);
        assert_eq!(delta, 1_200_000); // 1200 K rise per tick
    }

    // ── thermal_efficiency tests ─────────────────────────────────────

    fn smelter_req() -> RecipeThermalReq {
        RecipeThermalReq {
            min_temp_mk: 1_000_000,    // 1000K
            optimal_min_mk: 1_500_000, // 1500K
            optimal_max_mk: 2_000_000, // 2000K
            max_temp_mk: 2_500_000,    // 2500K
            heat_per_run_j: 50_000,
        }
    }

    #[test]
    fn efficiency_below_min_is_zero() {
        let req = smelter_req();
        assert!((thermal_efficiency(500_000, &req)).abs() < f32::EPSILON);
    }

    #[test]
    fn efficiency_at_min_is_80_percent() {
        let req = smelter_req();
        assert!((thermal_efficiency(1_000_000, &req) - 0.8).abs() < 1e-5);
    }

    #[test]
    fn efficiency_midway_ramp() {
        let req = smelter_req();
        // Midpoint of min→optimal_min = 1_250_000 → 0.9
        assert!((thermal_efficiency(1_250_000, &req) - 0.9).abs() < 1e-5);
    }

    #[test]
    fn efficiency_at_optimal_min_is_100_percent() {
        let req = smelter_req();
        assert!((thermal_efficiency(1_500_000, &req) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn efficiency_above_optimal_is_100_percent() {
        let req = smelter_req();
        assert!((thermal_efficiency(2_200_000, &req) - 1.0).abs() < f32::EPSILON);
    }

    // ── thermal_quality_factor tests ─────────────────────────────────

    #[test]
    fn quality_below_optimal_max_is_100_percent() {
        let req = smelter_req();
        assert!((thermal_quality_factor(1_800_000, &req) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn quality_at_optimal_max_is_100_percent() {
        let req = smelter_req();
        assert!((thermal_quality_factor(2_000_000, &req) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn quality_midway_degradation() {
        let req = smelter_req();
        // Midpoint of optimal_max→max = 2_250_000 → 0.8
        assert!((thermal_quality_factor(2_250_000, &req) - 0.8).abs() < 1e-5);
    }

    #[test]
    fn quality_at_max_is_60_percent() {
        let req = smelter_req();
        assert!((thermal_quality_factor(2_500_000, &req) - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn quality_above_max_is_30_percent() {
        let req = smelter_req();
        assert!((thermal_quality_factor(3_000_000, &req) - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn efficiency_zero_range_returns_one() {
        let req = RecipeThermalReq {
            min_temp_mk: 1_000_000,
            optimal_min_mk: 1_000_000, // same as min
            optimal_max_mk: 2_000_000,
            max_temp_mk: 2_500_000,
            heat_per_run_j: 0,
        };
        assert!((thermal_efficiency(1_000_000, &req) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn quality_zero_range_returns_one() {
        let req = RecipeThermalReq {
            min_temp_mk: 1_000_000,
            optimal_min_mk: 1_500_000,
            optimal_max_mk: 2_000_000,
            max_temp_mk: 2_000_000, // same as optimal_max
            heat_per_run_j: 0,
        };
        assert!((thermal_quality_factor(2_000_000, &req) - 1.0).abs() < f32::EPSILON);
    }

    // ── heat_wear_multiplier tests ────────────────────────────────────

    #[test]
    fn heat_wear_multiplier_nominal_is_1() {
        let constants = &crate::test_fixtures::base_content().constants;
        assert!(
            (heat_wear_multiplier(crate::OverheatZone::Nominal, constants) - 1.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn heat_wear_multiplier_warning_is_2() {
        let constants = &crate::test_fixtures::base_content().constants;
        assert!(
            (heat_wear_multiplier(crate::OverheatZone::Warning, constants) - 2.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn heat_wear_multiplier_critical_is_4() {
        let constants = &crate::test_fixtures::base_content().constants;
        assert!(
            (heat_wear_multiplier(crate::OverheatZone::Critical, constants) - 4.0).abs()
                < f32::EPSILON
        );
    }
}
