use anyhow::{bail, Context, Result};
use sim_core::{Constants, GameContent, ModuleBehaviorDef};
use std::collections::HashMap;

pub fn apply_overrides(
    content: &mut GameContent,
    overrides: &HashMap<String, serde_json::Value>,
) -> Result<()> {
    // Split overrides into constant, module, autopilot, strategy, and scoring groups.
    let mut constant_overrides = Vec::new();
    let mut autopilot_overrides = Vec::new();
    let mut strategy_overrides = Vec::new();
    let mut scoring_overrides = Vec::new();
    for (key, value) in overrides {
        if let Some(rest) = key.strip_prefix("module.") {
            apply_module_override(&mut content.module_defs, rest, key, value)?;
        } else if let Some(rest) = key.strip_prefix("autopilot.") {
            autopilot_overrides.push((rest, value));
        } else if let Some(rest) = key.strip_prefix("strategy.") {
            strategy_overrides.push((rest, value, key.as_str()));
        } else if let Some(rest) = key.strip_prefix("scoring.") {
            scoring_overrides.push((rest, value, key.as_str()));
        } else {
            constant_overrides.push((key.as_str(), value));
        }
    }
    // Apply constant overrides in a single serialize→patch→deserialize pass.
    if !constant_overrides.is_empty() {
        apply_constant_overrides(&mut content.constants, &constant_overrides)?;
    }
    if !autopilot_overrides.is_empty() {
        apply_autopilot_overrides(&mut content.autopilot, &autopilot_overrides)?;
    }
    if !strategy_overrides.is_empty() {
        apply_strategy_overrides(&mut content.default_strategy, &strategy_overrides)?;
    }
    if !scoring_overrides.is_empty() {
        apply_scoring_overrides(&mut content.scoring, &scoring_overrides)?;
    }
    Ok(())
}

fn apply_module_override(
    module_defs: &mut sim_core::AHashMap<String, sim_core::ModuleDef>,
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
        bail!("no modules matched behavior type '{behavior_type}' for override key '{full_key}'. Valid types: processor, assembler, lab, maintenance, sensor_array, solar_array, battery, radiator, thermal");
    }

    Ok(())
}

/// Derived fields that are computed by `derive_tick_values()` after deserialization.
/// These use `#[serde(skip_deserializing)]` so overriding them has no effect —
/// reject them explicitly to avoid silent no-ops.
const DERIVED_CONSTANT_FIELDS: &[&str] = &[
    "survey_scan_ticks",
    "deep_scan_ticks",
    "mining_rate_kg_per_tick",
    "deposit_ticks",
    "station_power_available_per_tick",
];

/// Apply constant overrides via serde: serialize current values, patch, deserialize back.
/// New Constants fields are automatically overridable with zero additional code.
///
/// Callers must invoke `constants.derive_tick_values()` after this to recompute
/// derived tick fields from the (potentially overridden) game-time minute values.
fn apply_constant_overrides(
    constants: &mut Constants,
    overrides: &[(&str, &serde_json::Value)],
) -> Result<()> {
    let serde_json::Value::Object(mut map) =
        serde_json::to_value(&*constants).context("failed to serialize Constants")?
    else {
        unreachable!("Constants serializes to JSON object")
    };

    for &(key, value) in overrides {
        if DERIVED_CONSTANT_FIELDS.contains(&key) {
            bail!(
                "override key '{key}' targets a derived field that is recomputed from \
                 its source (e.g., override the _minutes variant instead)"
            );
        }
        if !map.contains_key(key) {
            bail!(
                "unknown override key '{key}'. \
                 Constant keys or module.<type>.<field> keys are supported."
            );
        }
        map.insert(key.to_string(), value.clone());
    }

    *constants = serde_json::from_value(serde_json::Value::Object(map))
        .context("failed to deserialize Constants after applying overrides")?;
    Ok(())
}

/// Apply overrides to `StrategyConfig` (`GameContent.default_strategy`) using
/// the same serialize→patch→deserialize pattern as other override groups.
/// Supports nested dotted paths for the `priorities.*` fields, e.g.
/// `strategy.priorities.mining = 0.9`.
///
/// This applies to `content.default_strategy` BEFORE `build_initial_state()`
/// seeds `GameState.strategy_config`, so the lifecycle matches constant
/// overrides: applied once at scenario start, then frozen for the run.
fn apply_strategy_overrides(
    strategy: &mut sim_core::StrategyConfig,
    // (stripped key, value, original key including prefix)
    overrides: &[(&str, &serde_json::Value, &str)],
) -> Result<()> {
    let serde_json::Value::Object(mut map) =
        serde_json::to_value(&*strategy).context("failed to serialize StrategyConfig")?
    else {
        unreachable!("StrategyConfig serializes to JSON object")
    };

    // Apply longer (more specific) paths last so a scenario mixing
    // `strategy.priorities` with a sibling top-level key ends with the
    // nested override winning regardless of HashMap iteration order.
    let mut sorted_overrides: Vec<&(&str, &serde_json::Value, &str)> = overrides.iter().collect();
    sorted_overrides.sort_by_key(|o| o.0.matches('.').count());

    for &&(stripped_key, value, original_key) in &sorted_overrides {
        set_nested_path(&mut map, stripped_key, value.clone())
            .with_context(|| format!("invalid strategy override key '{original_key}'"))?;
    }

    // Preserve the applied-key list so a deserialize failure can still name
    // which override corrupted the shape. For example, `strategy.priorities
    // = 0.5` passes set_nested_path (valid top-level key) but breaks the
    // `PriorityWeights` struct shape when we deserialize back.
    let applied_keys: Vec<&str> = sorted_overrides.iter().map(|o| o.2).collect();
    *strategy = serde_json::from_value(serde_json::Value::Object(map)).with_context(|| {
        format!(
            "failed to deserialize StrategyConfig after applying overrides [{}]",
            applied_keys.join(", ")
        )
    })?;
    Ok(())
}

/// Walk a dotted path into a serde JSON object and replace the leaf value.
/// Every intermediate segment must already exist in the object and be an
/// object itself — this is what rejects unknown keys with a descriptive
/// error instead of silently creating new fields.
fn set_nested_path(
    root: &mut serde_json::Map<String, serde_json::Value>,
    dotted_path: &str,
    value: serde_json::Value,
) -> Result<()> {
    let segments: Vec<&str> = dotted_path.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        bail!("empty path segment in '{dotted_path}'");
    }

    let mut current = root;
    for (i, segment) in segments.iter().enumerate() {
        let is_leaf = i == segments.len() - 1;
        if is_leaf {
            if !current.contains_key(*segment) {
                bail!(
                    "unknown field '{segment}'. Valid keys at this level: {}",
                    current
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            current.insert((*segment).to_string(), value);
            return Ok(());
        }
        // Intermediate segment — must be an object we can descend into.
        let child = current
            .get_mut(*segment)
            .ok_or_else(|| anyhow::anyhow!("unknown field '{segment}' in path"))?;
        let serde_json::Value::Object(child_map) = child else {
            bail!("field '{segment}' is not an object; cannot descend for nested override");
        };
        current = child_map;
    }
    unreachable!("loop returns on leaf");
}

/// Apply overrides to `AutopilotConfig` using the same serialize→patch→deserialize
/// pattern as constant overrides. Keys are bare field names (without the `autopilot.` prefix).
fn apply_autopilot_overrides(
    autopilot: &mut sim_core::AutopilotConfig,
    overrides: &[(&str, &serde_json::Value)],
) -> Result<()> {
    let serde_json::Value::Object(mut map) =
        serde_json::to_value(&*autopilot).context("failed to serialize AutopilotConfig")?
    else {
        unreachable!("AutopilotConfig serializes to JSON object")
    };

    for &(key, value) in overrides {
        if !map.contains_key(key) {
            bail!(
                "unknown autopilot override key 'autopilot.{key}'. \
                 Valid keys: {}",
                map.keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        map.insert(key.to_string(), value.clone());
    }

    *autopilot = serde_json::from_value(serde_json::Value::Object(map))
        .context("failed to deserialize AutopilotConfig after applying overrides")?;
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

/// Apply overrides to `ScoringConfig`. Supports dotted paths:
/// - `scoring.scale_factor` → top-level field
/// - `scoring.computation_interval_ticks` → top-level field
/// - `scoring.dimensions.DIMID.weight` → dimension field by ID
/// - `scoring.dimensions.DIMID.ceiling` → dimension field by ID
/// - `scoring.dimensions.DIMID.signals.SOURCE.blend` → signal field by source name
/// - `scoring.dimensions.DIMID.signals.SOURCE.saturation` → signal field by source name
fn apply_scoring_overrides(
    scoring: &mut sim_core::ScoringConfig,
    overrides: &[(&str, &serde_json::Value, &str)],
) -> Result<()> {
    for &(key, value, full_key) in overrides {
        let parts: Vec<&str> = key.split('.').collect();
        match parts.as_slice() {
            ["scale_factor"] => {
                scoring.scale_factor = value
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("'{full_key}': expected number"))?;
            }
            ["computation_interval_ticks"] => {
                scoring.computation_interval_ticks = as_u64(full_key, value)?;
            }
            ["dimensions", dim_id, field] => {
                let dim = scoring
                    .dimensions
                    .iter_mut()
                    .find(|d| d.id == *dim_id)
                    .ok_or_else(|| anyhow::anyhow!("'{full_key}': unknown dimension '{dim_id}'"))?;
                match *field {
                    "weight" => {
                        dim.weight = value
                            .as_f64()
                            .ok_or_else(|| anyhow::anyhow!("'{full_key}': expected number"))?;
                    }
                    "ceiling" => {
                        dim.ceiling = value
                            .as_f64()
                            .ok_or_else(|| anyhow::anyhow!("'{full_key}': expected number"))?;
                    }
                    _ => bail!("'{full_key}': unknown dimension field '{field}'"),
                }
            }
            ["dimensions", dim_id, "signals", source, field] => {
                let dim = scoring
                    .dimensions
                    .iter_mut()
                    .find(|d| d.id == *dim_id)
                    .ok_or_else(|| anyhow::anyhow!("'{full_key}': unknown dimension '{dim_id}'"))?;
                let signal = dim
                    .signals
                    .iter_mut()
                    .find(|s| s.source == *source)
                    .ok_or_else(|| {
                        anyhow::anyhow!("'{full_key}': unknown signal source '{source}'")
                    })?;
                let num = value
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("'{full_key}': expected number"))?;
                match *field {
                    "blend" => signal.blend = num,
                    "saturation" => signal.saturation = Some(num),
                    "band_low" => signal.band_low = Some(num),
                    "band_high" => signal.band_high = Some(num),
                    "clamp_max" => signal.clamp_max = Some(num),
                    _ => bail!("'{full_key}': unknown signal field '{field}'"),
                }
            }
            _ => bail!(
                "unknown scoring override key '{full_key}'. \
                 Use scoring.scale_factor, scoring.dimensions.<id>.weight, \
                 scoring.dimensions.<id>.signals.<source>.<field>"
            ),
        }
    }
    Ok(())
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
    fn test_apply_spatial_overrides() {
        let mut content = test_content();
        let overrides = HashMap::from([
            ("ticks_per_au".to_string(), serde_json::json!(5000)),
            ("min_transit_ticks".to_string(), serde_json::json!(2)),
            ("docking_range_au_um".to_string(), serde_json::json!(20000)),
            (
                "replenish_check_interval_ticks".to_string(),
                serde_json::json!(48),
            ),
            ("replenish_target_count".to_string(), serde_json::json!(15)),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert_eq!(content.constants.ticks_per_au, 5000);
        assert_eq!(content.constants.min_transit_ticks, 2);
        assert_eq!(content.constants.docking_range_au_um, 20000);
        assert_eq!(content.constants.replenish_check_interval_ticks, 48);
        assert_eq!(content.constants.replenish_target_count, 15);
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
    fn test_autopilot_export_overrides() {
        let mut content = test_content();
        let overrides = HashMap::from([
            (
                "autopilot_export_batch_size_kg".to_string(),
                serde_json::json!(250.0),
            ),
            (
                "autopilot_export_min_revenue".to_string(),
                serde_json::json!(5000.0),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();

        assert!((content.constants.autopilot_export_batch_size_kg - 250.0).abs() < f32::EPSILON);
        assert!((content.constants.autopilot_export_min_revenue - 5000.0).abs() < f64::EPSILON);
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
                "module.thermal.passive_cooling_coefficient".to_string(),
                serde_json::json!(5.0),
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
            assert!(
                (thermal.passive_cooling_coefficient - 5.0).abs() < f32::EPSILON,
                "passive_cooling should be 5.0, got {}",
                thermal.passive_cooling_coefficient,
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

    #[test]
    fn test_autopilot_overrides() {
        let mut content = test_content();
        let original_pct = content.autopilot.slag_jettison_pct;
        let overrides = HashMap::from([(
            "autopilot.slag_jettison_pct".to_string(),
            serde_json::json!(0.9),
        )]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert!(
            (content.autopilot.slag_jettison_pct - 0.9).abs() < f32::EPSILON,
            "autopilot.slag_jettison_pct should be overridden to 0.9"
        );
        assert!(
            (original_pct - 0.75).abs() < f32::EPSILON,
            "original value should have been 0.75"
        );
    }

    #[test]
    fn test_autopilot_override_unknown_key_errors() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "autopilot.nonexistent_field".to_string(),
            serde_json::json!(42),
        )]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown autopilot override key"),
            "error should mention unknown key: {err}"
        );
    }

    #[test]
    fn test_strategy_override_top_level_fields() {
        let mut content = test_content();
        let overrides = HashMap::from([
            ("strategy.mode".to_string(), serde_json::json!("Expand")),
            (
                "strategy.fleet_size_target".to_string(),
                serde_json::json!(8),
            ),
            (
                "strategy.refuel_threshold_pct".to_string(),
                serde_json::json!(0.6),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert_eq!(
            content.default_strategy.mode,
            sim_core::StrategyMode::Expand
        );
        assert_eq!(content.default_strategy.fleet_size_target, 8);
        assert!((content.default_strategy.refuel_threshold_pct - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn test_strategy_override_nested_priorities() {
        let mut content = test_content();
        let original = content.default_strategy.priorities.research;
        let overrides = HashMap::from([
            (
                "strategy.priorities.mining".to_string(),
                serde_json::json!(0.95),
            ),
            (
                "strategy.priorities.fleet_expansion".to_string(),
                serde_json::json!(0.8),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert!((content.default_strategy.priorities.mining - 0.95).abs() < f32::EPSILON);
        assert!((content.default_strategy.priorities.fleet_expansion - 0.8).abs() < f32::EPSILON);
        // Unspecified nested fields are preserved, not reset to default.
        assert!(
            (content.default_strategy.priorities.research - original).abs() < f32::EPSILON,
            "untouched priorities field should be preserved",
        );
    }

    #[test]
    fn test_strategy_override_unknown_top_level_errors() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "strategy.nonexistent_field".to_string(),
            serde_json::json!(42),
        )]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("strategy.nonexistent_field") || err.contains("nonexistent_field"),
            "error should mention the bad key: {err}",
        );
    }

    #[test]
    fn test_strategy_override_unknown_nested_key_errors() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "strategy.priorities.bogus_concern".to_string(),
            serde_json::json!(0.5),
        )]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("bogus_concern"),
            "error should name the invalid nested key: {err}",
        );
    }

    #[test]
    fn test_strategy_override_type_mismatch_errors() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "strategy.fleet_size_target".to_string(),
            serde_json::json!("not_a_number"),
        )]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
    }

    #[test]
    fn test_strategy_override_invalid_mode_errors() {
        let mut content = test_content();
        let overrides = HashMap::from([(
            "strategy.mode".to_string(),
            serde_json::json!("NotARealMode"),
        )]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
    }

    #[test]
    fn test_strategy_override_mixed_with_other_groups() {
        let mut content = test_content();
        let overrides = HashMap::from([
            (
                "station_cargo_capacity_m3".to_string(),
                serde_json::json!(500.0),
            ),
            (
                "strategy.mode".to_string(),
                serde_json::json!("Consolidate"),
            ),
            (
                "strategy.priorities.research".to_string(),
                serde_json::json!(1.0),
            ),
            (
                "autopilot.slag_jettison_pct".to_string(),
                serde_json::json!(0.5),
            ),
        ]);
        apply_overrides(&mut content, &overrides).unwrap();
        assert!((content.constants.station_cargo_capacity_m3 - 500.0).abs() < f32::EPSILON);
        assert_eq!(
            content.default_strategy.mode,
            sim_core::StrategyMode::Consolidate
        );
        assert!((content.default_strategy.priorities.research - 1.0).abs() < f32::EPSILON);
        assert!((content.autopilot.slag_jettison_pct - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_strategy_override_object_to_scalar_fails_with_key_context() {
        // Fat-fingering `strategy.priorities = 0.5` (instead of
        // `strategy.priorities.mining = 0.5`) must fail with a message that
        // still names the offending key — otherwise the user sees a generic
        // "failed to deserialize StrategyConfig" with no diagnostic value.
        let mut content = test_content();
        let overrides =
            HashMap::from([("strategy.priorities".to_string(), serde_json::json!(0.5))]);
        let err = apply_overrides(&mut content, &overrides).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("strategy.priorities"),
            "error should name the bad override key: {msg}",
        );
    }

    #[test]
    fn test_strategy_override_deeply_nested_beyond_valid_path_errors() {
        // `strategy.priorities.mining` is valid; descending further into a
        // scalar (`strategy.priorities.mining.subfield`) must bail.
        let mut content = test_content();
        let overrides = HashMap::from([(
            "strategy.priorities.mining.subfield".to_string(),
            serde_json::json!(0.5),
        )]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
        let err = format!("{:#}", result.unwrap_err());
        assert!(
            err.contains("mining") || err.contains("not an object"),
            "error should explain the descent-into-scalar failure: {err}",
        );
    }

    #[test]
    fn test_strategy_override_empty_prefix_errors() {
        // Just `strategy.` with an empty path segment must fail — not silently
        // succeed or walk into the root.
        let mut content = test_content();
        let overrides = HashMap::from([("strategy.".to_string(), serde_json::json!(42))]);
        let result = apply_overrides(&mut content, &overrides);
        assert!(result.is_err());
        let err = format!("{:#}", result.unwrap_err());
        assert!(
            err.contains("empty") || err.contains("strategy."),
            "error should explain the empty-segment failure: {err}",
        );
    }

    #[test]
    fn test_strategy_override_specific_wins_over_general_regardless_of_map_order() {
        // Scenario authors may combine a top-level field override with a
        // sibling nested one. Both apply correctly regardless of HashMap
        // iteration order. Repeated runs stress the sort_by_key(depth)
        // discipline in apply_strategy_overrides.
        for _ in 0..16 {
            let mut content = test_content();
            let overrides = HashMap::from([
                (
                    "strategy.priorities.mining".to_string(),
                    serde_json::json!(0.99),
                ),
                (
                    "strategy.fleet_size_target".to_string(),
                    serde_json::json!(7),
                ),
            ]);
            apply_overrides(&mut content, &overrides).unwrap();
            assert!((content.default_strategy.priorities.mining - 0.99).abs() < f32::EPSILON);
            assert_eq!(content.default_strategy.fleet_size_target, 7);
        }
    }

    mod scoring_overrides {
        use super::*;

        #[test]
        fn test_scoring_scale_factor_override() {
            let mut content = test_content();
            let overrides = HashMap::from([(
                "scoring.scale_factor".to_string(),
                serde_json::json!(5000.0),
            )]);
            apply_overrides(&mut content, &overrides).unwrap();
            assert!((content.scoring.scale_factor - 5000.0).abs() < f64::EPSILON);
        }

        #[test]
        fn test_scoring_dimension_weight_override() {
            let mut content = test_content();
            let overrides = HashMap::from([(
                "scoring.dimensions.research_progress.weight".to_string(),
                serde_json::json!(0.30),
            )]);
            apply_overrides(&mut content, &overrides).unwrap();
            let dim = content
                .scoring
                .dimensions
                .iter()
                .find(|d| d.id == "research_progress")
                .unwrap();
            assert!((dim.weight - 0.30).abs() < f64::EPSILON);
        }

        #[test]
        fn test_scoring_signal_blend_override() {
            let mut content = test_content();
            let overrides = HashMap::from([(
                "scoring.dimensions.research_progress.signals.tech_fraction.blend".to_string(),
                serde_json::json!(0.8),
            )]);
            apply_overrides(&mut content, &overrides).unwrap();
            let dim = content
                .scoring
                .dimensions
                .iter()
                .find(|d| d.id == "research_progress")
                .unwrap();
            let signal = dim
                .signals
                .iter()
                .find(|s| s.source == "tech_fraction")
                .unwrap();
            assert!((signal.blend - 0.8).abs() < f64::EPSILON);
        }

        #[test]
        fn test_scoring_signal_saturation_override() {
            let mut content = test_content();
            let overrides = HashMap::from([(
                "scoring.dimensions.research_progress.signals.total_raw_data.saturation"
                    .to_string(),
                serde_json::json!(2000.0),
            )]);
            apply_overrides(&mut content, &overrides).unwrap();
            let dim = content
                .scoring
                .dimensions
                .iter()
                .find(|d| d.id == "research_progress")
                .unwrap();
            let signal = dim
                .signals
                .iter()
                .find(|s| s.source == "total_raw_data")
                .unwrap();
            assert!((signal.saturation.unwrap() - 2000.0).abs() < f64::EPSILON);
        }

        #[test]
        fn test_unknown_scoring_dimension_errors() {
            let mut content = test_content();
            let overrides = HashMap::from([(
                "scoring.dimensions.nonexistent.weight".to_string(),
                serde_json::json!(0.5),
            )]);
            let err = apply_overrides(&mut content, &overrides)
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown dimension"), "{err}");
        }

        #[test]
        fn test_unknown_scoring_signal_errors() {
            let mut content = test_content();
            let overrides = HashMap::from([(
                "scoring.dimensions.research_progress.signals.nonexistent.blend".to_string(),
                serde_json::json!(0.5),
            )]);
            let err = apply_overrides(&mut content, &overrides)
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown signal source"), "{err}");
        }
    }
}
