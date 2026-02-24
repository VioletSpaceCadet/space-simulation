use anyhow::{bail, Result};
use sim_core::Constants;
use std::collections::HashMap;

const VALID_KEYS: &[&str] = &[
    "survey_scan_ticks",
    "deep_scan_ticks",
    "travel_ticks_per_hop",
    "survey_tag_detection_probability",
    "asteroid_count_per_template",
    "asteroid_mass_min_kg",
    "asteroid_mass_max_kg",
    "ship_cargo_capacity_m3",
    "station_cargo_capacity_m3",
    "mining_rate_kg_per_tick",
    "deposit_ticks",
    "station_power_available_per_tick",
    "autopilot_iron_rich_confidence_threshold",
    "autopilot_refinery_threshold_kg",
    "research_roll_interval_ticks",
    "data_generation_peak",
    "data_generation_floor",
    "data_generation_decay_rate",
    "wear_band_degraded_threshold",
    "wear_band_critical_threshold",
    "wear_band_degraded_efficiency",
    "wear_band_critical_efficiency",
];

pub fn apply_overrides(
    constants: &mut Constants,
    overrides: &HashMap<String, serde_json::Value>,
) -> Result<()> {
    for (key, value) in overrides {
        match key.as_str() {
            "survey_scan_ticks" => constants.survey_scan_ticks = as_u64(key, value)?,
            "deep_scan_ticks" => constants.deep_scan_ticks = as_u64(key, value)?,
            "travel_ticks_per_hop" => constants.travel_ticks_per_hop = as_u64(key, value)?,
            "survey_tag_detection_probability" => {
                constants.survey_tag_detection_probability = as_f32(key, value)?;
            }
            "asteroid_count_per_template" => {
                constants.asteroid_count_per_template = as_u32(key, value)?;
            }
            "asteroid_mass_min_kg" => constants.asteroid_mass_min_kg = as_f32(key, value)?,
            "asteroid_mass_max_kg" => constants.asteroid_mass_max_kg = as_f32(key, value)?,
            "ship_cargo_capacity_m3" => constants.ship_cargo_capacity_m3 = as_f32(key, value)?,
            "station_cargo_capacity_m3" => {
                constants.station_cargo_capacity_m3 = as_f32(key, value)?;
            }
            "mining_rate_kg_per_tick" => constants.mining_rate_kg_per_tick = as_f32(key, value)?,
            "deposit_ticks" => constants.deposit_ticks = as_u64(key, value)?,
            "station_power_available_per_tick" => {
                constants.station_power_available_per_tick = as_f32(key, value)?;
            }
            "autopilot_iron_rich_confidence_threshold" => {
                constants.autopilot_iron_rich_confidence_threshold = as_f32(key, value)?;
            }
            "autopilot_refinery_threshold_kg" => {
                constants.autopilot_refinery_threshold_kg = as_f32(key, value)?;
            }
            "research_roll_interval_ticks" => {
                constants.research_roll_interval_ticks = as_u64(key, value)?;
            }
            "data_generation_peak" => constants.data_generation_peak = as_f32(key, value)?,
            "data_generation_floor" => constants.data_generation_floor = as_f32(key, value)?,
            "data_generation_decay_rate" => {
                constants.data_generation_decay_rate = as_f32(key, value)?;
            }
            "wear_band_degraded_threshold" => {
                constants.wear_band_degraded_threshold = as_f32(key, value)?;
            }
            "wear_band_critical_threshold" => {
                constants.wear_band_critical_threshold = as_f32(key, value)?;
            }
            "wear_band_degraded_efficiency" => {
                constants.wear_band_degraded_efficiency = as_f32(key, value)?;
            }
            "wear_band_critical_efficiency" => {
                constants.wear_band_critical_efficiency = as_f32(key, value)?;
            }
            _ => bail!(
                "unknown override key '{key}'. Valid keys: {}",
                VALID_KEYS.join(", ")
            ),
        }
    }
    Ok(())
}

#[allow(clippy::cast_possible_truncation)] // JSON f64→f32 is intentional
fn as_f32(key: &str, value: &serde_json::Value) -> Result<f32> {
    value
        .as_f64()
        .map(|v| v as f32)
        .ok_or_else(|| anyhow::anyhow!("override '{key}': expected a number, got {value}"))
}

fn as_u64(key: &str, value: &serde_json::Value) -> Result<u64> {
    value.as_u64().ok_or_else(|| {
        anyhow::anyhow!("override '{key}': expected a positive integer, got {value}")
    })
}

fn as_u32(key: &str, value: &serde_json::Value) -> Result<u32> {
    let val = as_u64(key, value)?;
    u32::try_from(val)
        .map_err(|_| anyhow::anyhow!("override '{key}': value {val} exceeds u32 range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_constants() -> Constants {
        serde_json::from_str(include_str!("../../../content/constants.json")).unwrap()
    }

    #[test]
    fn test_apply_f32_override() {
        let mut constants = default_constants();
        let overrides = HashMap::from([(
            "station_cargo_capacity_m3".to_string(),
            serde_json::json!(200.0),
        )]);
        apply_overrides(&mut constants, &overrides).unwrap();
        assert!((constants.station_cargo_capacity_m3 - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_apply_u64_override() {
        let mut constants = default_constants();
        let overrides = HashMap::from([("survey_scan_ticks".to_string(), serde_json::json!(99))]);
        apply_overrides(&mut constants, &overrides).unwrap();
        assert_eq!(constants.survey_scan_ticks, 99);
    }

    #[test]
    fn test_apply_research_override() {
        let mut constants = default_constants();
        let overrides = HashMap::from([(
            "research_roll_interval_ticks".to_string(),
            serde_json::json!(120),
        )]);
        apply_overrides(&mut constants, &overrides).unwrap();
        assert_eq!(constants.research_roll_interval_ticks, 120);
    }

    #[test]
    fn test_unknown_key_errors() {
        let mut constants = default_constants();
        let overrides = HashMap::from([("nonexistent_field".to_string(), serde_json::json!(1.0))]);
        let result = apply_overrides(&mut constants, &overrides);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown override key"));
        assert!(err.contains("nonexistent_field"));
    }

    #[test]
    fn test_type_mismatch_errors() {
        let mut constants = default_constants();
        let overrides = HashMap::from([(
            "survey_scan_ticks".to_string(),
            serde_json::json!("not_a_number"),
        )]);
        let result = apply_overrides(&mut constants, &overrides);
        assert!(result.is_err());
    }
}
