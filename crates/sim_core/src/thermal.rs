//! Energy↔thermal unit conversion helpers.
//!
//! All sim-state arithmetic uses integer types:
//! - Temperature: `u32` milli-Kelvin
//! - Energy: `i64` Joules
//! - Power: `f32` Watts (content-defined, converted at boundary)
//!
//! Floats appear only in content definitions and these conversion boundaries.

use crate::Constants;

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
}
