//! Energy↔thermal unit conversion helpers.
//!
//! All sim-state arithmetic uses integer types:
//! - Temperature: `u32` milli-Kelvin
//! - Energy: `i64` Joules
//! - Power: `f32` Watts (content-defined, converted at boundary)
//!
//! Floats appear only in content definitions and these conversion boundaries.

use crate::{Constants, ElementDef, MaterialThermalProps, Phase, RecipeThermalReq};

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
/// - `min_temp_mk` → `optimal_min_mk`: linear ramp from `efficiency_floor` to 1.0
/// - `optimal_min_mk` and above: 1.0
#[inline]
pub fn thermal_efficiency(temp_mk: u32, req: &RecipeThermalReq) -> f32 {
    if temp_mk < req.min_temp_mk {
        return 0.0;
    }
    if temp_mk >= req.optimal_min_mk {
        return 1.0;
    }
    let range = req.optimal_min_mk - req.min_temp_mk;
    if range == 0 {
        return 1.0;
    }
    let progress = (temp_mk - req.min_temp_mk) as f32 / range as f32;
    req.efficiency_floor + (1.0 - req.efficiency_floor) * progress
}

/// Quality scaling based on temperature.
///
/// - Below `optimal_max_mk`: 1.0
/// - `optimal_max_mk` → `max_temp_mk`: linear ramp from 1.0 to `quality_at_max`
/// - Above `max_temp_mk`: `quality_floor`
#[inline]
pub fn thermal_quality_factor(temp_mk: u32, req: &RecipeThermalReq) -> f32 {
    if temp_mk <= req.optimal_max_mk {
        return 1.0;
    }
    if temp_mk > req.max_temp_mk {
        return req.quality_floor;
    }
    let range = req.max_temp_mk - req.optimal_max_mk;
    if range == 0 {
        return 1.0;
    }
    let progress = (temp_mk - req.optimal_max_mk) as f32 / range as f32;
    1.0 - (1.0 - req.quality_at_max) * progress
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
        crate::OverheatZone::Critical | crate::OverheatZone::Damage => {
            constants.thermal_wear_multiplier_critical
        }
    }
}

/// Hysteresis offset for solidification: material solidifies at
/// `melting_point - SOLIDIFICATION_HYSTERESIS_MK` to prevent oscillation.
pub const SOLIDIFICATION_HYSTERESIS_MK: u32 = 50_000; // 50K

/// Update material phase based on heat input/output and element thermal properties.
///
/// When heating a solid through the melting point, energy goes into the latent heat
/// buffer instead of raising temperature. Phase flips to Liquid when the buffer is
/// fully charged (total latent heat = `latent_heat_j_per_kg * kg`).
///
/// When cooling a liquid, solidification occurs at `melting_point - 50K` (hysteresis)
/// to prevent oscillation. The buffer drains before temperature drops further.
///
/// Elements without thermal properties (`melting_point_mk = None`) are unaffected.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn update_phase(props: &mut MaterialThermalProps, element: &ElementDef, kg: f32, heat_j: i64) {
    let Some(melting_point_mk) = element.melting_point_mk else {
        // Non-thermal element: just apply heat as temperature change.
        if let Some(specific_heat) = element.specific_heat_j_per_kg_k {
            let capacity = f64::from(specific_heat) * f64::from(kg);
            if capacity > 0.0 {
                apply_temp_change(props, heat_j, capacity);
            }
        }
        return;
    };

    let latent_heat_per_kg = element.latent_heat_j_per_kg.unwrap_or(0);
    let specific_heat = element.specific_heat_j_per_kg_k.unwrap_or(449);
    let kg_u32 = kg.max(0.0) as u32; // safe: kg is non-negative mass
    let total_latent = i64::from(latent_heat_per_kg) * i64::from(kg_u32);
    let capacity_j_per_k = f64::from(specific_heat) * f64::from(kg);

    if capacity_j_per_k <= 0.0 {
        return;
    }

    let solidification_point = melting_point_mk.saturating_sub(SOLIDIFICATION_HYSTERESIS_MK);
    let mut remaining_heat = heat_j;

    match props.phase {
        Phase::Solid => {
            if remaining_heat > 0 {
                let heat_to_melt =
                    temp_distance_to_heat(props.temp_mk, melting_point_mk, capacity_j_per_k);
                if remaining_heat <= heat_to_melt {
                    apply_temp_change(props, remaining_heat, capacity_j_per_k);
                } else {
                    remaining_heat -= heat_to_melt;
                    props.temp_mk = melting_point_mk;
                    let buffer_remaining = total_latent - props.latent_heat_buffer_j;
                    if remaining_heat < buffer_remaining {
                        props.latent_heat_buffer_j += remaining_heat;
                    } else {
                        remaining_heat -= buffer_remaining;
                        props.phase = Phase::Liquid;
                        props.latent_heat_buffer_j = total_latent;
                        apply_temp_change(props, remaining_heat, capacity_j_per_k);
                    }
                }
            } else {
                apply_temp_change(props, remaining_heat, capacity_j_per_k);
            }
        }
        Phase::Liquid => {
            if remaining_heat >= 0 {
                apply_temp_change(props, remaining_heat, capacity_j_per_k);
            } else {
                let heat_to_solidify =
                    temp_distance_to_heat(props.temp_mk, solidification_point, capacity_j_per_k);
                let cooling = -remaining_heat;
                if cooling <= heat_to_solidify {
                    apply_temp_change(props, remaining_heat, capacity_j_per_k);
                } else {
                    remaining_heat += heat_to_solidify;
                    props.temp_mk = solidification_point;
                    let drain_needed = props.latent_heat_buffer_j;
                    let cooling_left = -remaining_heat;
                    if cooling_left < drain_needed {
                        props.latent_heat_buffer_j -= cooling_left;
                    } else {
                        remaining_heat += drain_needed;
                        props.phase = Phase::Solid;
                        props.latent_heat_buffer_j = 0;
                        apply_temp_change(props, remaining_heat, capacity_j_per_k);
                    }
                }
            }
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn temp_distance_to_heat(from_mk: u32, to_mk: u32, capacity_j_per_k: f64) -> i64 {
    if to_mk <= from_mk {
        return 0;
    }
    let delta_k = f64::from(to_mk - from_mk) / 1000.0;
    (delta_k * capacity_j_per_k).round() as i64
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn apply_temp_change(props: &mut MaterialThermalProps, heat_j: i64, capacity_j_per_k: f64) {
    let delta_mk = (heat_j as f64 / capacity_j_per_k * 1000.0)
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
    let new_temp = (i64::from(props.temp_mk) + i64::from(delta_mk)).max(0);
    props.temp_mk = new_temp as u32;
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
            efficiency_floor: 0.8,
            quality_floor: 0.3,
            quality_at_max: 0.6,
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
            efficiency_floor: 0.8,
            quality_floor: 0.3,
            quality_at_max: 0.6,
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
            efficiency_floor: 0.8,
            quality_floor: 0.3,
            quality_at_max: 0.6,
        };
        assert!((thermal_quality_factor(2_000_000, &req) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn custom_efficiency_floor() {
        let req = RecipeThermalReq {
            min_temp_mk: 1_000_000,
            optimal_min_mk: 1_500_000,
            optimal_max_mk: 2_000_000,
            max_temp_mk: 2_500_000,
            heat_per_run_j: 0,
            efficiency_floor: 0.5, // custom: 50% instead of 80%
            quality_floor: 0.3,
            quality_at_max: 0.6,
        };
        // At min_temp: should be 0.5
        assert!((thermal_efficiency(1_000_000, &req) - 0.5).abs() < 1e-5);
        // Midway: 0.5 + 0.5*0.5 = 0.75
        assert!((thermal_efficiency(1_250_000, &req) - 0.75).abs() < 1e-5);
        // At optimal_min: still 1.0
        assert!((thermal_efficiency(1_500_000, &req) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn custom_quality_curves() {
        let req = RecipeThermalReq {
            min_temp_mk: 1_000_000,
            optimal_min_mk: 1_500_000,
            optimal_max_mk: 2_000_000,
            max_temp_mk: 2_500_000,
            heat_per_run_j: 0,
            efficiency_floor: 0.8,
            quality_floor: 0.1,  // custom: 10% above max
            quality_at_max: 0.4, // custom: 40% at max
        };
        // At max_temp: should be quality_at_max = 0.4
        assert!((thermal_quality_factor(2_500_000, &req) - 0.4).abs() < f32::EPSILON);
        // Above max_temp: should be quality_floor = 0.1
        assert!((thermal_quality_factor(3_000_000, &req) - 0.1).abs() < f32::EPSILON);
        // Midway: 1.0 - (1.0 - 0.4) * 0.5 = 1.0 - 0.3 = 0.7
        assert!((thermal_quality_factor(2_250_000, &req) - 0.7).abs() < 1e-5);
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

    // ── phase transition tests ────────────────────────────────────────

    fn fe_element() -> ElementDef {
        ElementDef {
            id: "Fe".to_string(),
            density_kg_per_m3: 7874.0,
            display_name: "Iron".to_string(),
            refined_name: None,
            category: "material".to_string(),
            melting_point_mk: Some(1_811_000),
            latent_heat_j_per_kg: Some(247_000),
            specific_heat_j_per_kg_k: Some(449),
            boiloff_rate_per_day_at_293k: None,
            boiling_point_mk: None,
            boiloff_curve: None,
        }
    }

    fn solid_fe_props(temp_mk: u32) -> MaterialThermalProps {
        MaterialThermalProps {
            temp_mk,
            phase: Phase::Solid,
            latent_heat_buffer_j: 0,
        }
    }

    fn liquid_fe_props(temp_mk: u32, latent_buffer: i64) -> MaterialThermalProps {
        MaterialThermalProps {
            temp_mk,
            phase: Phase::Liquid,
            latent_heat_buffer_j: latent_buffer,
        }
    }

    #[test]
    fn heat_solid_fe_below_melting_point() {
        let element = fe_element();
        let mut props = solid_fe_props(1_500_000);
        update_phase(&mut props, &element, 100.0, 1_000_000);
        assert_eq!(props.phase, Phase::Solid);
        assert!(props.temp_mk > 1_500_000);
        assert!(props.temp_mk < 1_811_000);
    }

    #[test]
    fn heat_solid_fe_fills_latent_buffer() {
        let element = fe_element();
        let mut props = solid_fe_props(1_811_000);
        let total_latent = 247_000_i64 * 100;
        update_phase(&mut props, &element, 100.0, total_latent / 2);
        assert_eq!(props.phase, Phase::Solid);
        assert_eq!(props.temp_mk, 1_811_000);
        assert!(props.latent_heat_buffer_j > 0);
        assert!(props.latent_heat_buffer_j < total_latent);
    }

    #[test]
    fn heat_solid_fe_completes_transition() {
        let element = fe_element();
        let mut props = solid_fe_props(1_811_000);
        let total_latent = 247_000_i64 * 100;
        update_phase(&mut props, &element, 100.0, total_latent + 1_000_000);
        assert_eq!(props.phase, Phase::Liquid);
        assert!(props.temp_mk > 1_811_000);
        assert_eq!(props.latent_heat_buffer_j, total_latent);
    }

    #[test]
    fn heat_solid_fe_from_cold_through_melting() {
        let element = fe_element();
        let mut props = solid_fe_props(1_700_000);
        update_phase(&mut props, &element, 100.0, 100_000_000);
        assert_eq!(props.phase, Phase::Liquid);
        assert!(props.temp_mk > 1_811_000);
    }

    #[test]
    fn cool_liquid_fe_solidifies_with_hysteresis() {
        let element = fe_element();
        let total_latent = 247_000_i64 * 100;
        let mut props = liquid_fe_props(1_900_000, total_latent);
        update_phase(&mut props, &element, 100.0, -200_000_000);
        assert_eq!(props.phase, Phase::Solid);
        assert!(props.temp_mk < 1_761_000);
        assert_eq!(props.latent_heat_buffer_j, 0);
    }

    #[test]
    fn cool_liquid_fe_partial_drain_stays_liquid() {
        let element = fe_element();
        let total_latent = 247_000_i64 * 100;
        let solidification_point = 1_811_000 - SOLIDIFICATION_HYSTERESIS_MK;
        let mut props = liquid_fe_props(solidification_point, total_latent);
        update_phase(&mut props, &element, 100.0, -(total_latent / 4));
        assert_eq!(props.phase, Phase::Liquid);
        assert_eq!(props.temp_mk, solidification_point);
        assert!(props.latent_heat_buffer_j > 0);
    }

    #[test]
    fn zero_heat_does_nothing() {
        let element = fe_element();
        let mut props = solid_fe_props(1_000_000);
        let original = props.clone();
        update_phase(&mut props, &element, 100.0, 0);
        assert_eq!(props, original);
    }

    #[test]
    fn phase_transition_determinism() {
        let element = fe_element();
        let heat_sequence = [
            5_000_000_i64,
            -2_000_000,
            30_000_000,
            -50_000_000,
            10_000_000,
        ];
        let run = || {
            let mut props = solid_fe_props(1_000_000);
            for &heat in &heat_sequence {
                update_phase(&mut props, &element, 100.0, heat);
            }
            props
        };
        assert_eq!(run(), run(), "phase transitions must be deterministic");
    }

    #[test]
    fn non_thermal_element_ignores_phase() {
        let ore = ElementDef {
            id: "ore".to_string(),
            density_kg_per_m3: 3000.0,
            display_name: "Ore".to_string(),
            refined_name: None,
            category: "raw_ore".to_string(),
            melting_point_mk: None,
            latent_heat_j_per_kg: None,
            specific_heat_j_per_kg_k: None,
            boiloff_rate_per_day_at_293k: None,
            boiling_point_mk: None,
            boiloff_curve: None,
        };
        let mut props = solid_fe_props(1_000_000);
        update_phase(&mut props, &ore, 100.0, 10_000_000);
        assert_eq!(props.phase, Phase::Solid);
        assert_eq!(props.temp_mk, 1_000_000);
    }
}
