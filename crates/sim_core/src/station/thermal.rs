//! Thermal tick step — passive cooling for modules with `ThermalDef`.
//!
//! Runs after maintenance in the station tick loop (step 3.6).
//! Modules are grouped by `ThermalGroupId` and processed in sorted order
//! (by group ID, then by module ID within each group) for determinism.

use std::collections::BTreeMap;

use crate::{thermal, GameContent, GameState, StationId};

/// Maximum absolute temperature in milli-Kelvin (10 000 K).
/// Hard ceiling to prevent unbounded growth from numerical errors.
const T_MAX_ABSOLUTE_MK: u32 = 10_000_000;

/// Tick the thermal system for a single station.
///
/// For every module that has a `ThermalDef`, apply passive cooling toward the
/// sink temperature. Modules are grouped by `ThermalGroupId` (ungrouped modules
/// use an empty-string key) and iterated in sorted order for determinism.
pub(crate) fn tick_thermal(state: &mut GameState, station_id: &StationId, content: &GameContent) {
    let dt_s = thermal::dt_seconds(&content.constants);
    let sink_temp_mk = content.constants.thermal_sink_temp_mk;

    let Some(station) = state.stations.get(station_id) else {
        return;
    };

    // Build a sorted map of (group_key -> sorted vec of module indices).
    // BTreeMap gives us deterministic group ordering. Within each group we
    // collect (module_id, index) pairs and sort by module_id.
    let mut groups: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();

    for (module_index, module) in station.modules.iter().enumerate() {
        // Only process modules that have a ThermalDef and ThermalState.
        let Some(ref _thermal_state) = module.thermal else {
            continue;
        };
        let Some(def) = content.module_defs.get(&module.def_id) else {
            continue;
        };
        let Some(ref _thermal_def) = def.thermal else {
            continue;
        };

        let group_key = module
            .thermal
            .as_ref()
            .and_then(|t| t.thermal_group.clone())
            .unwrap_or_default();

        groups
            .entry(group_key)
            .or_default()
            .push((module.id.0.clone(), module_index));
    }

    // Sort modules within each group by module ID for determinism.
    for modules in groups.values_mut() {
        modules.sort_by(|a, b| a.0.cmp(&b.0));
    }

    // Apply passive cooling to each module.
    for modules in groups.values() {
        for &(_, module_index) in modules {
            apply_passive_cooling(state, station_id, module_index, content, dt_s, sink_temp_mk);
        }
    }
}

/// Apply passive cooling to a single module.
///
/// Formula: `Q_loss = passive_cooling_coefficient * dt_s * (T - T_sink) / 1000`
/// The division by 1000 converts the temperature difference from milli-Kelvin to Kelvin
/// for the energy calculation, keeping the coefficient's units as W/K.
fn apply_passive_cooling(
    state: &mut GameState,
    station_id: &StationId,
    module_index: usize,
    content: &GameContent,
    dt_s: f64,
    sink_temp_mk: u32,
) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };
    let module = &station.modules[module_index];

    let Some(ref thermal_state) = module.thermal else {
        return;
    };
    let Some(def) = content.module_defs.get(&module.def_id) else {
        return;
    };
    let Some(ref thermal_def) = def.thermal else {
        return;
    };

    let current_temp_mk = thermal_state.temp_mk;

    // No cooling needed if at or below sink temperature.
    if current_temp_mk <= sink_temp_mk {
        return;
    }

    // Q_loss (Joules) = coeff * dt_s * (T_current - T_sink) / 1000
    // The /1000 converts mK difference to K for the coefficient (W/K).
    let temp_diff_mk = current_temp_mk.saturating_sub(sink_temp_mk);
    let cooling_j =
        f64::from(thermal_def.passive_cooling_coefficient) * dt_s * f64::from(temp_diff_mk)
            / 1000.0;

    // Convert cooling energy to temperature delta (negative = cooling).
    #[allow(clippy::cast_possible_truncation)] // safe: clamped to i64 range
    let cooling_j_i64 = cooling_j.clamp(0.0, i64::MAX as f64) as i64;
    let delta_mk =
        thermal::heat_to_temp_delta_mk(-cooling_j_i64, thermal_def.heat_capacity_j_per_k);

    // Passive cooling always produces a non-positive delta.
    debug_assert!(
        delta_mk <= 0,
        "passive cooling delta must be <= 0, got {delta_mk}"
    );

    // Apply delta, clamping to [sink_temp, T_MAX_ABSOLUTE].
    let new_temp = if delta_mk < 0 {
        current_temp_mk.saturating_sub(delta_mk.unsigned_abs())
    } else {
        #[allow(clippy::cast_sign_loss)] // guarded by delta_mk >= 0 check
        current_temp_mk.saturating_add(delta_mk.cast_unsigned())
    };
    let clamped_temp = new_temp.clamp(sink_temp_mk, T_MAX_ABSOLUTE_MK);

    // Write the updated temperature.
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    if let Some(ref mut thermal) = station.modules[module_index].thermal {
        thermal.temp_mk = clamped_temp;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use std::collections::{HashMap, HashSet};

    /// Create content with a module def that has thermal properties.
    fn thermal_test_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_smelter".to_string(),
            ModuleDef {
                id: "module_smelter".to_string(),
                name: "Test Smelter".to_string(),
                mass_kg: 5000.0,
                volume_m3: 10.0,
                power_consumption_per_run: 100.0,
                wear_per_run: 0.02,
                behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_minutes: 5,
                    processing_interval_ticks: 5,
                    recipes: vec![],
                }),
                thermal: Some(ThermalDef {
                    heat_capacity_j_per_k: 500.0,
                    passive_cooling_coefficient: 0.05,
                    max_temp_mk: 2_500_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: Some("smelting".to_string()),
                }),
            },
        );
        content
    }

    /// Create a state with a single thermal module at the given temperature.
    fn thermal_test_state(content: &GameContent, temp_mk: u32) -> GameState {
        let station_id = StationId("station_test".to_string());
        GameState {
            meta: MetaState {
                tick: 10,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![],
            asteroids: HashMap::new(),
            ships: HashMap::new(),
            stations: HashMap::from([(
                station_id.clone(),
                StationState {
                    id: station_id,
                    location_node: NodeId("node_test".to_string()),
                    inventory: vec![],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("smelter_0001".to_string()),
                        def_id: "module_smelter".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::Processor(ProcessorState {
                            threshold_kg: 0.0,
                            ticks_since_last_run: 0,
                            stalled: false,
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        thermal: Some(ThermalState {
                            temp_mk,
                            thermal_group: Some("smelting".to_string()),
                        }),
                    }],
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                },
            )]),
            research: ResearchState {
                unlocked: HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
                action_counts: HashMap::new(),
            },
            balance: 0.0,
            counters: Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
                next_lot_id: 0,
                next_module_instance_id: 0,
            },
        }
    }

    #[test]
    fn cooling_toward_sink_temp() {
        let content = thermal_test_content();
        // Start at 500K (500_000 mK), well above sink temp (293K).
        let mut state = thermal_test_state(&content, 500_000);
        let station_id = StationId("station_test".to_string());

        // Run several ticks and verify temperature decreases monotonically.
        let mut prev_temp = 500_000_u32;
        for _ in 0..10 {
            tick_thermal(&mut state, &station_id, &content);
            let station = state.stations.get(&station_id).unwrap();
            let temp = station.modules[0].thermal.as_ref().unwrap().temp_mk;
            assert!(
                temp < prev_temp,
                "temperature should decrease: was {prev_temp}, now {temp}"
            );
            assert!(
                temp >= content.constants.thermal_sink_temp_mk,
                "temperature should not go below sink"
            );
            prev_temp = temp;
        }
    }

    #[test]
    fn stable_at_sink_temp() {
        let content = thermal_test_content();
        let sink = content.constants.thermal_sink_temp_mk;
        let mut state = thermal_test_state(&content, sink);
        let station_id = StationId("station_test".to_string());

        tick_thermal(&mut state, &station_id, &content);

        let station = state.stations.get(&station_id).unwrap();
        let temp = station.modules[0].thermal.as_ref().unwrap().temp_mk;
        assert_eq!(temp, sink, "temperature should remain at sink temp");
    }

    #[test]
    fn deterministic_temp_history() {
        let content = thermal_test_content();
        let station_id = StationId("station_test".to_string());

        // Run 1
        let mut state1 = thermal_test_state(&content, 800_000);
        let mut history1 = Vec::new();
        for _ in 0..20 {
            tick_thermal(&mut state1, &station_id, &content);
            let temp = state1.stations.get(&station_id).unwrap().modules[0]
                .thermal
                .as_ref()
                .unwrap()
                .temp_mk;
            history1.push(temp);
        }

        // Run 2 — identical setup
        let mut state2 = thermal_test_state(&content, 800_000);
        let mut history2 = Vec::new();
        for _ in 0..20 {
            tick_thermal(&mut state2, &station_id, &content);
            let temp = state2.stations.get(&station_id).unwrap().modules[0]
                .thermal
                .as_ref()
                .unwrap()
                .temp_mk;
            history2.push(temp);
        }

        assert_eq!(history1, history2, "temperature histories must match");
    }

    #[test]
    fn module_without_thermal_state_is_skipped() {
        let content = thermal_test_content();
        let mut state = thermal_test_state(&content, 500_000);
        let station_id = StationId("station_test".to_string());

        // Remove thermal state from the module.
        state.stations.get_mut(&station_id).unwrap().modules[0].thermal = None;

        // Should not panic or error.
        tick_thermal(&mut state, &station_id, &content);
    }

    #[test]
    fn module_without_thermal_def_is_skipped() {
        let mut content = thermal_test_content();
        let mut state = thermal_test_state(&content, 500_000);
        let station_id = StationId("station_test".to_string());

        // Remove ThermalDef from the module definition.
        content
            .module_defs
            .get_mut("module_smelter")
            .unwrap()
            .thermal = None;

        tick_thermal(&mut state, &station_id, &content);

        // Temperature should be unchanged since no ThermalDef means no processing.
        let temp = state.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;
        assert_eq!(temp, 500_000);
    }

    #[test]
    fn sorted_group_order() {
        let mut content = thermal_test_content();
        // Add a second module def in a different group.
        content.module_defs.insert(
            "module_reactor".to_string(),
            ModuleDef {
                id: "module_reactor".to_string(),
                name: "Test Reactor".to_string(),
                mass_kg: 8000.0,
                volume_m3: 15.0,
                power_consumption_per_run: 50.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_minutes: 10,
                    processing_interval_ticks: 10,
                    recipes: vec![],
                }),
                thermal: Some(ThermalDef {
                    heat_capacity_j_per_k: 1000.0,
                    passive_cooling_coefficient: 0.03,
                    max_temp_mk: 3_000_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: Some("reactor".to_string()),
                }),
            },
        );

        let station_id = StationId("station_test".to_string());
        let mut state = thermal_test_state(&content, 600_000);

        // Add a reactor module to the station (different group, "reactor" < "smelting").
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(ModuleState {
            id: ModuleInstanceId("reactor_0001".to_string()),
            def_id: "module_reactor".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Processor(ProcessorState {
                threshold_kg: 0.0,
                ticks_since_last_run: 0,
                stalled: false,
            }),
            wear: WearState::default(),
            power_stalled: false,
            thermal: Some(ThermalState {
                temp_mk: 600_000,
                thermal_group: Some("reactor".to_string()),
            }),
        });

        tick_thermal(&mut state, &station_id, &content);

        // Both modules should have cooled (both above sink temp).
        let station = state.stations.get(&station_id).unwrap();
        let smelter_temp = station.modules[0].thermal.as_ref().unwrap().temp_mk;
        let reactor_temp = station.modules[1].thermal.as_ref().unwrap().temp_mk;
        assert!(smelter_temp < 600_000, "smelter should have cooled");
        assert!(reactor_temp < 600_000, "reactor should have cooled");
    }
}
