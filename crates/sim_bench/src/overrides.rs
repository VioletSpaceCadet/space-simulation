use anyhow::{bail, Context, Result};
use sim_core::{Constants, GameContent, ModuleBehaviorDef};
use std::collections::HashMap;

pub fn apply_overrides(
    content: &mut GameContent,
    overrides: &HashMap<String, serde_json::Value>,
) -> Result<()> {
    for (key, value) in overrides {
        if let Some(rest) = key.strip_prefix("module.") {
            apply_module_override(&mut content.module_defs, rest, key, value)?;
        } else {
            apply_constant_override(&mut content.constants, key, value)?;
        }
    }
    Ok(())
}

fn apply_module_override(
    module_defs: &mut HashMap<String, sim_core::ModuleDef>,
    dotted: &str,
    full_key: &str,
    value: &serde_json::Value,
) -> Result<()> {
    let (behavior_type, field) = dotted.split_once('.').ok_or_else(|| {
        anyhow::anyhow!("invalid module override key '{full_key}': expected module.<type>.<field>")
    })?;

    let mut matched = false;

    for module_def in module_defs.values_mut() {
        match (&mut module_def.behavior, behavior_type) {
            (ModuleBehaviorDef::Processor(ref mut proc_def), "processor") => {
                match field {
                    "processing_interval_minutes" => proc_def.processing_interval_minutes = as_u64(full_key, value)?,
                    "wear_per_run" => module_def.wear_per_run = as_f32(full_key, value)?,
                    _ => bail!("unknown processor field '{field}' in override key '{full_key}'. Valid fields: processing_interval_minutes, wear_per_run"),
                }
                matched = true;
            }
            (ModuleBehaviorDef::Assembler(ref mut asm_def), "assembler") => {
                match field {
                    "assembly_interval_minutes" => asm_def.assembly_interval_minutes = as_u64(full_key, value)?,
                    "wear_per_run" => module_def.wear_per_run = as_f32(full_key, value)?,
                    "max_stock" => {
                        let map: std::collections::HashMap<String, u32> = serde_json::from_value(value.clone())
                            .with_context(|| format!("invalid max_stock value for '{full_key}'"))?;
                        asm_def.max_stock = map
                            .into_iter()
                            .map(|(k, v)| (sim_core::ComponentId(k), v))
                            .collect();
                    }
                    _ => bail!("unknown assembler field '{field}' in override key '{full_key}'. Valid fields: assembly_interval_minutes, wear_per_run, max_stock"),
                }
                matched = true;
            }
            (ModuleBehaviorDef::Lab(ref mut lab_def), "lab") => {
                match field {
                    "research_interval_minutes" => lab_def.research_interval_minutes = as_u64(full_key, value)?,
                    "data_consumption_per_run" => lab_def.data_consumption_per_run = as_f32(full_key, value)?,
                    "research_points_per_run" => lab_def.research_points_per_run = as_f32(full_key, value)?,
                    "wear_per_run" => module_def.wear_per_run = as_f32(full_key, value)?,
                    _ => bail!("unknown lab field '{field}' in override key '{full_key}'. Valid fields: research_interval_minutes, data_consumption_per_run, research_points_per_run, wear_per_run"),
                }
                matched = true;
            }
            (ModuleBehaviorDef::Maintenance(ref mut maint_def), "maintenance") => {
                match field {
                    "repair_interval_minutes" => maint_def.repair_interval_minutes = as_u64(full_key, value)?,
                    "wear_reduction_per_run" => maint_def.wear_reduction_per_run = as_f32(full_key, value)?,
                    "repair_kit_cost" => maint_def.repair_kit_cost = as_u32(full_key, value)?,
                    "repair_threshold" => maint_def.repair_threshold = as_f32(full_key, value)?,
                    _ => bail!("unknown maintenance field '{field}' in override key '{full_key}'. Valid fields: repair_interval_minutes, wear_reduction_per_run, repair_kit_cost, repair_threshold"),
                }
                matched = true;
            }
            (ModuleBehaviorDef::SensorArray(ref mut sensor_def), "sensor_array") => {
                match field {
                    "scan_interval_minutes" => sensor_def.scan_interval_minutes = as_u64(full_key, value)?,
                    "wear_per_run" => module_def.wear_per_run = as_f32(full_key, value)?,
                    _ => bail!("unknown sensor_array field '{field}' in override key '{full_key}'. Valid fields: scan_interval_minutes, wear_per_run"),
                }
                matched = true;
            }
            (ModuleBehaviorDef::SolarArray(ref mut solar_def), "solar_array") => {
                match field {
                    "base_output_kw" => solar_def.base_output_kw = as_f32(full_key, value)?,
                    "wear_per_run" => module_def.wear_per_run = as_f32(full_key, value)?,
                    _ => bail!("unknown solar_array field '{field}' in override key '{full_key}'. Valid fields: base_output_kw, wear_per_run"),
                }
                matched = true;
            }
            (ModuleBehaviorDef::Battery(ref mut battery_def), "battery") => {
                match field {
                    "capacity_kwh" => battery_def.capacity_kwh = as_f32(full_key, value)?,
                    "charge_rate_kw" => battery_def.charge_rate_kw = as_f32(full_key, value)?,
                    "discharge_rate_kw" => battery_def.discharge_rate_kw = as_f32(full_key, value)?,
                    _ => bail!("unknown battery field '{field}' in override key '{full_key}'. Valid fields: capacity_kwh, charge_rate_kw, discharge_rate_kw"),
                }
                matched = true;
            }
            (ModuleBehaviorDef::Radiator(ref mut radiator_def), "radiator") => {
                match field {
                    "cooling_capacity_w" => radiator_def.cooling_capacity_w = as_f32(full_key, value)?,
                    _ => bail!("unknown radiator field '{field}' in override key '{full_key}'. Valid fields: cooling_capacity_w"),
                }
                matched = true;
            }
            _ if behavior_type == "thermal" => {
                // Apply to the ThermalDef of ANY module that has one.
                if let Some(ref mut thermal_def) = module_def.thermal {
                    match field {
                        "heat_capacity_j_per_k" => thermal_def.heat_capacity_j_per_k = as_f32(full_key, value)?,
                        "passive_cooling_coefficient" => thermal_def.passive_cooling_coefficient = as_f32(full_key, value)?,
                        "max_temp_mk" => thermal_def.max_temp_mk = as_u32(full_key, value)?,
                        _ => bail!("unknown thermal field '{field}' in override key '{full_key}'. Valid fields: heat_capacity_j_per_k, passive_cooling_coefficient, max_temp_mk"),
                    }
                    matched = true;
                }
            }
            _ => {}
        }
    }

    if !matched {
        bail!("no modules matched behavior type '{behavior_type}' for override key '{full_key}'. Valid types: processor, assembler, lab, maintenance, sensor_array, solar_array, battery, radiator");
    }

    Ok(())
}

fn apply_constant_override(
    constants: &mut Constants,
    key: &str,
    value: &serde_json::Value,
) -> Result<()> {
    match key {
        "survey_scan_minutes" => constants.survey_scan_minutes = as_u64(key, value)?,
        "deep_scan_minutes" => constants.deep_scan_minutes = as_u64(key, value)?,
        "travel_minutes_per_hop" => constants.travel_minutes_per_hop = as_u64(key, value)?,
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
        "mining_rate_kg_per_minute" => constants.mining_rate_kg_per_minute = as_f32(key, value)?,
        "deposit_minutes" => constants.deposit_minutes = as_u64(key, value)?,
        "station_power_available_per_minute" => {
            constants.station_power_available_per_minute = as_f32(key, value)?;
        }
        "autopilot_iron_rich_confidence_threshold" => {
            constants.autopilot_iron_rich_confidence_threshold = as_f32(key, value)?;
        }
        "autopilot_refinery_threshold_kg" => {
            constants.autopilot_refinery_threshold_kg = as_f32(key, value)?;
        }
        "autopilot_slag_jettison_pct" => {
            constants.autopilot_slag_jettison_pct = as_f32(key, value)?;
        }
        "research_roll_interval_minutes" => {
            constants.research_roll_interval_minutes = as_u64(key, value)?;
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
        "minutes_per_tick" => {
            constants.minutes_per_tick = as_u32(key, value)?;
        }
        "thermal_sink_temp_mk" => {
            constants.thermal_sink_temp_mk = as_u32(key, value)?;
        }
        "thermal_overheat_warning_offset_mk" => {
            constants.thermal_overheat_warning_offset_mk = as_u32(key, value)?;
        }
        "thermal_overheat_critical_offset_mk" => {
            constants.thermal_overheat_critical_offset_mk = as_u32(key, value)?;
        }
        "thermal_wear_multiplier_warning" => {
            constants.thermal_wear_multiplier_warning = as_f32(key, value)?;
        }
        "thermal_wear_multiplier_critical" => {
            constants.thermal_wear_multiplier_critical = as_f32(key, value)?;
        }
        _ => bail!(
            "unknown override key '{key}'. Constant keys or module.<type>.<field> keys are supported."
        ),
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

    fn test_content() -> GameContent {
        sim_world::load_content("../../content").unwrap()
    }

    #[test]
    fn test_apply_constant_override() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "station_cargo_capacity_m3".to_string(),
            serde_json::json!(200.0),
        )]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert!((content.constants.station_cargo_capacity_m3 - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_apply_u64_override() {
        let mut content = test_content();
        let overrides = HashMap::from([("survey_scan_minutes".to_string(), serde_json::json!(99))]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert_eq!(content.constants.survey_scan_minutes, 99);
    }

    #[test]
    fn test_apply_research_override() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "research_roll_interval_minutes".to_string(),
            serde_json::json!(120),
        )]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert_eq!(content.constants.research_roll_interval_minutes, 120);
    }

    #[test]
    fn test_unknown_key_errors() {
        let mut content = test_content();
        let overrides = HashMap::from([("nonexistent_field".to_string(), serde_json::json!(1.0))]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown override key"));
        assert!(err.contains("nonexistent_field"));
    }

    #[test]
    fn test_type_mismatch_errors() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "survey_scan_minutes".to_string(),
            serde_json::json!("not_a_number"),
        )]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
    }

    #[test]
    fn test_module_processor_override() {
        let mut content = test_content();
        let overrides = HashMap::from([
            (
                "module.processor.processing_interval_minutes".to_string(),
                serde_json::json!(180),
            ),
            (
                "module.processor.wear_per_run".to_string(),
                serde_json::json!(0.02),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();

        for module_def in content.module_defs.values() {
            if let ModuleBehaviorDef::Processor(ref proc_def) = module_def.behavior {
                assert_eq!(proc_def.processing_interval_minutes, 180);
                assert!((module_def.wear_per_run - 0.02).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_module_lab_override() {
        let mut content = test_content();
        let overrides = HashMap::from([
            (
                "module.lab.research_interval_minutes".to_string(),
                serde_json::json!(10),
            ),
            (
                "module.lab.data_consumption_per_run".to_string(),
                serde_json::json!(5.0),
            ),
            (
                "module.lab.research_points_per_run".to_string(),
                serde_json::json!(2.5),
            ),
            (
                "module.lab.wear_per_run".to_string(),
                serde_json::json!(0.002),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();

        for module_def in content.module_defs.values() {
            if let ModuleBehaviorDef::Lab(ref lab_def) = module_def.behavior {
                assert_eq!(lab_def.research_interval_minutes, 10);
                assert!((lab_def.data_consumption_per_run - 5.0).abs() < f32::EPSILON);
                assert!((lab_def.research_points_per_run - 2.5).abs() < f32::EPSILON);
                assert!((module_def.wear_per_run - 0.002).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_module_assembler_override() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "module.assembler.assembly_interval_minutes".to_string(),
            serde_json::json!(240),
        )]);
        apply_overrides(&mut content, &overrides).unwrap();

        for module_def in content.module_defs.values() {
            if let ModuleBehaviorDef::Assembler(ref asm_def) = module_def.behavior {
                assert_eq!(asm_def.assembly_interval_minutes, 240);
            }
        }
    }

    #[test]
    fn test_module_assembler_max_stock_override() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "module.assembler.max_stock".to_string(),
            serde_json::json!({"repair_kit": 25}),
        )]);
        apply_overrides(&mut content, &overrides).unwrap();

        for module_def in content.module_defs.values() {
            if let ModuleBehaviorDef::Assembler(ref asm_def) = module_def.behavior {
                assert_eq!(
                    asm_def
                        .max_stock
                        .get(&sim_core::ComponentId("repair_kit".to_string())),
                    Some(&25)
                );
            }
        }
    }

    #[test]
    fn test_module_maintenance_override() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "module.maintenance.wear_reduction_per_run".to_string(),
            serde_json::json!(0.3),
        )]);
        apply_overrides(&mut content, &overrides).unwrap();

        for module_def in content.module_defs.values() {
            if let ModuleBehaviorDef::Maintenance(ref maint_def) = module_def.behavior {
                assert!((maint_def.wear_reduction_per_run - 0.3).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_module_unknown_type_errors() {
        let mut content = test_content();
        let overrides =
            HashMap::from([("module.turret.fire_rate".to_string(), serde_json::json!(10))]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("turret"));
    }

    #[test]
    fn test_module_unknown_field_errors() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "module.processor.nonexistent".to_string(),
            serde_json::json!(10),
        )]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("nonexistent"));
    }

    #[test]
    fn test_module_solar_array_override() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "module.solar_array.base_output_kw".to_string(),
            serde_json::json!(75.0),
        )]);
        apply_overrides(&mut content, &overrides).unwrap();

        for module_def in content.module_defs.values() {
            if let ModuleBehaviorDef::SolarArray(ref solar_def) = module_def.behavior {
                assert!((solar_def.base_output_kw - 75.0).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_module_battery_override() {
        let mut content = test_content();
        let overrides = HashMap::from([
            (
                "module.battery.capacity_kwh".to_string(),
                serde_json::json!(200.0),
            ),
            (
                "module.battery.charge_rate_kw".to_string(),
                serde_json::json!(40.0),
            ),
            (
                "module.battery.discharge_rate_kw".to_string(),
                serde_json::json!(60.0),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();

        for module_def in content.module_defs.values() {
            if let ModuleBehaviorDef::Battery(ref battery_def) = module_def.behavior {
                assert!((battery_def.capacity_kwh - 200.0).abs() < f32::EPSILON);
                assert!((battery_def.charge_rate_kw - 40.0).abs() < f32::EPSILON);
                assert!((battery_def.discharge_rate_kw - 60.0).abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_minutes_per_tick_override() {
        let mut content = test_content();
        let overrides = HashMap::from([("minutes_per_tick".to_string(), serde_json::json!(1))]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert_eq!(content.constants.minutes_per_tick, 1);
    }

    #[test]
    fn test_thermal_constant_overrides() {
        let mut content = test_content();
        let overrides = HashMap::from([
            (
                "thermal_sink_temp_mk".to_string(),
                serde_json::json!(200_000),
            ),
            (
                "thermal_overheat_warning_offset_mk".to_string(),
                serde_json::json!(100_000),
            ),
            (
                "thermal_overheat_critical_offset_mk".to_string(),
                serde_json::json!(300_000),
            ),
            (
                "thermal_wear_multiplier_warning".to_string(),
                serde_json::json!(3.0),
            ),
            (
                "thermal_wear_multiplier_critical".to_string(),
                serde_json::json!(6.0),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();

        assert_eq!(content.constants.thermal_sink_temp_mk, 200_000);
        assert_eq!(
            content.constants.thermal_overheat_warning_offset_mk,
            100_000
        );
        assert_eq!(
            content.constants.thermal_overheat_critical_offset_mk,
            300_000
        );
        assert!((content.constants.thermal_wear_multiplier_warning - 3.0).abs() < f32::EPSILON);
        assert!((content.constants.thermal_wear_multiplier_critical - 6.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_module_thermal_override() {
        let mut content = test_content();
        let overrides = HashMap::from([
            (
                "module.thermal.heat_capacity_j_per_k".to_string(),
                serde_json::json!(1000.0),
            ),
            (
                "module.thermal.max_temp_mk".to_string(),
                serde_json::json!(3_000_000),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();

        // Verify all modules with thermal defs got updated
        let thermal_modules: Vec<_> = content
            .module_defs
            .values()
            .filter(|m| m.thermal.is_some())
            .collect();
        assert!(
            !thermal_modules.is_empty(),
            "should have at least one thermal module"
        );
        for module_def in &thermal_modules {
            let thermal = module_def.thermal.as_ref().unwrap();
            assert!(
                (thermal.heat_capacity_j_per_k - 1000.0).abs() < f32::EPSILON,
                "heat_capacity should be 1000.0, got {}",
                thermal.heat_capacity_j_per_k,
            );
            assert_eq!(thermal.max_temp_mk, 3_000_000);
        }
    }

    #[test]
    fn test_module_thermal_unknown_field_errors() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "module.thermal.nonexistent_field".to_string(),
            serde_json::json!(10.0),
        )]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("nonexistent_field"));
    }

    #[test]
    fn test_mixed_constant_and_module_overrides() {
        let mut content = test_content();
        let overrides = HashMap::from([
            (
                "station_cargo_capacity_m3".to_string(),
                serde_json::json!(500.0),
            ),
            (
                "module.processor.processing_interval_minutes".to_string(),
                serde_json::json!(90),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();

        assert!((content.constants.station_cargo_capacity_m3 - 500.0).abs() < f32::EPSILON);
        for module_def in content.module_defs.values() {
            if let ModuleBehaviorDef::Processor(ref proc_def) = module_def.behavior {
                assert_eq!(proc_def.processing_interval_minutes, 90);
            }
        }
    }
}
