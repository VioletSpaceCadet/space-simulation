//! Thermal tick step — passive cooling for modules with `ThermalDef`.
//!
//! Runs after maintenance in the station tick loop (step 3.6).
//! Modules are grouped by `ThermalGroupId` and processed in sorted order
//! (by group ID, then by module ID within each group) for determinism.

use std::collections::BTreeMap;

use crate::{thermal, EventEnvelope, GameContent, GameState, OverheatZone, StationId};

/// Maximum absolute temperature in milli-Kelvin (10 000 K).
/// Hard ceiling to prevent unbounded growth from numerical errors.
const T_MAX_ABSOLUTE_MK: u32 = 10_000_000;

/// Tick the thermal system for a single station.
///
/// For every module that has a `ThermalDef`, apply passive cooling toward the
/// sink temperature. Modules are grouped by `ThermalGroupId` (ungrouped modules
/// use an empty-string key) and iterated in sorted order for determinism.
pub(crate) fn tick_thermal(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
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

    // Build a map of group_key → total radiator cooling capacity (W), adjusted for wear.
    // Radiators are identified by their ModuleBehaviorDef::Radiator variant.
    let mut radiator_cooling_by_group: BTreeMap<String, f32> = BTreeMap::new();
    for module in &station.modules {
        if !module.enabled {
            continue;
        }
        let Some(def) = content.module_defs.get(&module.def_id) else {
            continue;
        };
        let crate::ModuleBehaviorDef::Radiator(ref radiator_def) = def.behavior else {
            continue;
        };
        // Radiator must have thermal state to belong to a group.
        let group_key = module
            .thermal
            .as_ref()
            .and_then(|t| t.thermal_group.clone())
            .unwrap_or_default();

        let efficiency = crate::wear::wear_efficiency(module.wear.wear, &content.constants);
        *radiator_cooling_by_group.entry(group_key).or_default() +=
            radiator_def.cooling_capacity_w * efficiency;
    }

    // Apply passive cooling to each module.
    for modules in groups.values() {
        for &(_, module_index) in modules {
            apply_passive_cooling(state, station_id, module_index, content, dt_s, sink_temp_mk);
        }
    }

    // Apply radiator cooling per group: distribute total cooling energy evenly across
    // all thermal modules in the group.
    for (group_key, modules) in &groups {
        let total_radiator_w = radiator_cooling_by_group
            .get(group_key)
            .copied()
            .unwrap_or(0.0);
        if total_radiator_w <= 0.0 || modules.is_empty() {
            continue;
        }
        let total_cooling_j = f64::from(total_radiator_w) * dt_s;
        let per_module_cooling_j = total_cooling_j / modules.len() as f64;

        for &(_, module_index) in modules {
            apply_radiator_cooling(
                state,
                station_id,
                module_index,
                content,
                per_module_cooling_j,
                sink_temp_mk,
            );
        }
    }

    // Check overheat zones for all thermal modules after temperature updates.
    check_overheat_zones(state, station_id, content, events);
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

/// Apply radiator cooling to a single module.
///
/// Removes `cooling_j` of heat energy from the module, converting to a temperature
/// delta via the module's heat capacity. Temperature is clamped to \[`sink_temp`, `T_MAX`\].
fn apply_radiator_cooling(
    state: &mut GameState,
    station_id: &StationId,
    module_index: usize,
    content: &GameContent,
    cooling_j: f64,
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

    // No cooling below sink temperature.
    if current_temp_mk <= sink_temp_mk {
        return;
    }

    #[allow(clippy::cast_possible_truncation)]
    let cooling_j_i64 = cooling_j.clamp(0.0, i64::MAX as f64) as i64;
    let delta_mk =
        thermal::heat_to_temp_delta_mk(-cooling_j_i64, thermal_def.heat_capacity_j_per_k);

    let new_temp = if delta_mk < 0 {
        current_temp_mk.saturating_sub(delta_mk.unsigned_abs())
    } else {
        #[allow(clippy::cast_sign_loss)]
        current_temp_mk.saturating_add(delta_mk.cast_unsigned())
    };
    let clamped_temp = new_temp.clamp(sink_temp_mk, T_MAX_ABSOLUTE_MK);

    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    if let Some(ref mut thermal) = station.modules[module_index].thermal {
        thermal.temp_mk = clamped_temp;
    }
}

/// Classify a temperature into an overheat zone based on the module's `max_temp_mk`
/// and the global overheat offsets.
fn classify_overheat_zone(
    temp_mk: u32,
    max_temp_mk: u32,
    constants: &crate::Constants,
) -> OverheatZone {
    let critical_threshold =
        max_temp_mk.saturating_add(constants.thermal_overheat_critical_offset_mk);
    let warning_threshold =
        max_temp_mk.saturating_add(constants.thermal_overheat_warning_offset_mk);

    if temp_mk >= critical_threshold {
        OverheatZone::Critical
    } else if temp_mk >= warning_threshold {
        OverheatZone::Warning
    } else {
        OverheatZone::Nominal
    }
}

/// Check all thermal modules for overheat zone transitions and emit events.
///
/// Modules entering Warning zone: emit `OverheatWarning`.
/// Modules entering Critical zone: emit `OverheatCritical`, auto-disable module.
/// Modules returning to Nominal: emit `OverheatCleared`, re-enable if was auto-disabled by overheat.
fn check_overheat_zones(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };

    // Collect zone transitions before mutating state.
    let mut transitions: Vec<(usize, OverheatZone, OverheatZone, u32, u32)> = Vec::new();

    for (module_index, module) in station.modules.iter().enumerate() {
        let Some(ref thermal_state) = module.thermal else {
            continue;
        };
        let Some(def) = content.module_defs.get(&module.def_id) else {
            continue;
        };
        let Some(ref thermal_def) = def.thermal else {
            continue;
        };

        let new_zone = classify_overheat_zone(
            thermal_state.temp_mk,
            thermal_def.max_temp_mk,
            &content.constants,
        );

        if new_zone != thermal_state.overheat_zone {
            transitions.push((
                module_index,
                thermal_state.overheat_zone,
                new_zone,
                thermal_state.temp_mk,
                thermal_def.max_temp_mk,
            ));
        }
    }

    // Apply transitions.
    let current_tick = state.meta.tick;
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };

    for (module_index, old_zone, new_zone, temp_mk, max_temp_mk) in transitions {
        let module = &mut station.modules[module_index];
        let module_id = module.id.clone();

        // Update zone.
        if let Some(ref mut thermal) = module.thermal {
            thermal.overheat_zone = new_zone;
        }

        // Auto-disable on entering critical.
        if new_zone == OverheatZone::Critical {
            module.enabled = false;
        }

        // Re-enable on leaving critical (returning to warning or nominal).
        // Only re-enable if module was disabled by overheat (was in critical).
        if old_zone == OverheatZone::Critical && new_zone != OverheatZone::Critical {
            module.enabled = true;
        }

        // Emit events.
        match new_zone {
            OverheatZone::Warning => {
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    crate::Event::OverheatWarning {
                        station_id: station_id.clone(),
                        module_id,
                        temp_mk,
                        max_temp_mk,
                    },
                ));
            }
            OverheatZone::Critical => {
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    crate::Event::OverheatCritical {
                        station_id: station_id.clone(),
                        module_id,
                        temp_mk,
                        max_temp_mk,
                    },
                ));
            }
            OverheatZone::Nominal => {
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    crate::Event::OverheatCleared {
                        station_id: station_id.clone(),
                        module_id,
                        temp_mk,
                    },
                ));
            }
        }
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
                            ..Default::default()
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
            tick_thermal(&mut state, &station_id, &content, &mut Vec::new());
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

        tick_thermal(&mut state, &station_id, &content, &mut Vec::new());

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
            tick_thermal(&mut state1, &station_id, &content, &mut Vec::new());
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
            tick_thermal(&mut state2, &station_id, &content, &mut Vec::new());
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
        tick_thermal(&mut state, &station_id, &content, &mut Vec::new());
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

        tick_thermal(&mut state, &station_id, &content, &mut Vec::new());

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
                ..Default::default()
            }),
        });

        tick_thermal(&mut state, &station_id, &content, &mut Vec::new());

        // Both modules should have cooled (both above sink temp).
        let station = state.stations.get(&station_id).unwrap();
        let smelter_temp = station.modules[0].thermal.as_ref().unwrap().temp_mk;
        let reactor_temp = station.modules[1].thermal.as_ref().unwrap().temp_mk;
        assert!(smelter_temp < 600_000, "smelter should have cooled");
        assert!(reactor_temp < 600_000, "reactor should have cooled");
    }

    // ── Radiator cooling tests ───────────────────────────────────────

    /// Helper: add a radiator module def and instance to content + state.
    fn add_radiator(
        content: &mut GameContent,
        state: &mut GameState,
        station_id: &StationId,
        radiator_id: &str,
        cooling_capacity_w: f32,
        wear: f32,
    ) {
        let def_id = format!("module_radiator_{radiator_id}");
        content.module_defs.insert(
            def_id.clone(),
            ModuleDef {
                id: def_id.clone(),
                name: format!("Radiator {radiator_id}"),
                mass_kg: 200.0,
                volume_m3: 2.0,
                power_consumption_per_run: 0.0,
                wear_per_run: 0.0,
                behavior: ModuleBehaviorDef::Radiator(RadiatorDef { cooling_capacity_w }),
                thermal: Some(ThermalDef {
                    heat_capacity_j_per_k: 100.0,
                    passive_cooling_coefficient: 0.0,
                    max_temp_mk: 5_000_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: Some("smelting".to_string()),
                }),
            },
        );
        let station = state.stations.get_mut(station_id).unwrap();
        station.modules.push(ModuleState {
            id: ModuleInstanceId(format!("radiator_{radiator_id}")),
            def_id,
            enabled: true,
            kind_state: ModuleKindState::Radiator(RadiatorState::default()),
            wear: WearState { wear },
            power_stalled: false,
            thermal: Some(ThermalState {
                temp_mk: DEFAULT_AMBIENT_TEMP_MK,
                thermal_group: Some("smelting".to_string()),
                ..Default::default()
            }),
        });
    }

    #[test]
    fn radiator_cools_thermal_group() {
        let mut content = thermal_test_content();
        let station_id = StationId("station_test".to_string());

        // Smelter at 2000K (2_000_000 mK).
        let mut state_with_radiator = thermal_test_state(&content, 2_000_000);
        add_radiator(
            &mut content,
            &mut state_with_radiator,
            &station_id,
            "a",
            1000.0,
            0.0,
        );

        // Baseline: same setup without radiator.
        let content_baseline = thermal_test_content();
        let mut state_no_radiator = thermal_test_state(&content_baseline, 2_000_000);

        // Run one tick on each.
        tick_thermal(
            &mut state_with_radiator,
            &station_id,
            &content,
            &mut Vec::new(),
        );
        tick_thermal(
            &mut state_no_radiator,
            &station_id,
            &content_baseline,
            &mut Vec::new(),
        );

        let temp_with = state_with_radiator
            .stations
            .get(&station_id)
            .unwrap()
            .modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;
        let temp_without = state_no_radiator.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;

        assert!(
            temp_with < temp_without,
            "radiator should cool more than passive alone: with={temp_with}, without={temp_without}"
        );
    }

    #[test]
    fn multiple_radiators_stack() {
        let mut content_one = thermal_test_content();
        let station_id = StationId("station_test".to_string());

        let mut state_one = thermal_test_state(&content_one, 2_000_000);
        add_radiator(
            &mut content_one,
            &mut state_one,
            &station_id,
            "a",
            1000.0,
            0.0,
        );

        let mut content_two = thermal_test_content();
        let mut state_two = thermal_test_state(&content_two, 2_000_000);
        add_radiator(
            &mut content_two,
            &mut state_two,
            &station_id,
            "a",
            1000.0,
            0.0,
        );
        add_radiator(
            &mut content_two,
            &mut state_two,
            &station_id,
            "b",
            1000.0,
            0.0,
        );

        tick_thermal(&mut state_one, &station_id, &content_one, &mut Vec::new());
        tick_thermal(&mut state_two, &station_id, &content_two, &mut Vec::new());

        let temp_one = state_one.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;
        let temp_two = state_two.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;

        assert!(
            temp_two < temp_one,
            "two radiators should cool more than one: two={temp_two}, one={temp_one}"
        );
    }

    #[test]
    fn worn_radiator_less_effective() {
        let mut content_pristine = thermal_test_content();
        let station_id = StationId("station_test".to_string());
        let mut state_pristine = thermal_test_state(&content_pristine, 2_000_000);
        add_radiator(
            &mut content_pristine,
            &mut state_pristine,
            &station_id,
            "a",
            1000.0,
            0.0,
        );

        let mut content_worn = thermal_test_content();
        let mut state_worn = thermal_test_state(&content_worn, 2_000_000);
        add_radiator(
            &mut content_worn,
            &mut state_worn,
            &station_id,
            "a",
            1000.0,
            0.6, // degraded wear band
        );

        tick_thermal(
            &mut state_pristine,
            &station_id,
            &content_pristine,
            &mut Vec::new(),
        );
        tick_thermal(&mut state_worn, &station_id, &content_worn, &mut Vec::new());

        let temp_pristine = state_pristine.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;
        let temp_worn = state_worn.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;

        assert!(
            temp_worn > temp_pristine,
            "worn radiator should be less effective: worn={temp_worn}, pristine={temp_pristine}"
        );
    }

    #[test]
    fn disabled_radiator_contributes_zero_cooling() {
        let mut content = thermal_test_content();
        let station_id = StationId("station_test".to_string());

        // State without radiator (passive cooling only)
        let mut state_no_radiator = thermal_test_state(&content, 2_000_000);
        tick_thermal(
            &mut state_no_radiator,
            &station_id,
            &content,
            &mut Vec::new(),
        );
        let temp_no_radiator = state_no_radiator.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;

        // State with disabled radiator — should be identical to no radiator
        let mut state_disabled = thermal_test_state(&content, 2_000_000);
        add_radiator(
            &mut content,
            &mut state_disabled,
            &station_id,
            "a",
            1000.0,
            0.0,
        );
        // Disable the radiator
        let station = state_disabled.stations.get_mut(&station_id).unwrap();
        station.modules.last_mut().unwrap().enabled = false;

        tick_thermal(&mut state_disabled, &station_id, &content, &mut Vec::new());
        let temp_disabled = state_disabled.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;

        assert_eq!(
            temp_no_radiator, temp_disabled,
            "disabled radiator should not contribute cooling"
        );
    }

    // ── Overheat zone tests ────────────────────────────────────────

    #[test]
    fn classify_overheat_nominal() {
        let content = thermal_test_content();
        // max_temp_mk = 2_500_000, warning offset = 200_000, critical offset = 500_000
        // Nominal: below 2_700_000
        let zone = super::classify_overheat_zone(2_400_000, 2_500_000, &content.constants);
        assert_eq!(zone, OverheatZone::Nominal);
    }

    #[test]
    fn classify_overheat_warning() {
        let content = thermal_test_content();
        // Warning: >= 2_700_000 and < 3_000_000
        let zone = super::classify_overheat_zone(2_700_000, 2_500_000, &content.constants);
        assert_eq!(zone, OverheatZone::Warning);
    }

    #[test]
    fn classify_overheat_critical() {
        let content = thermal_test_content();
        // Critical: >= 3_000_000
        let zone = super::classify_overheat_zone(3_000_000, 2_500_000, &content.constants);
        assert_eq!(zone, OverheatZone::Critical);
    }

    #[test]
    fn overheat_warning_emits_event() {
        let content = thermal_test_content();
        let station_id = StationId("station_test".to_string());
        // Set temp in warning zone: max=2_500_000, warning threshold=2_700_000
        let mut state = thermal_test_state(&content, 2_800_000);
        let mut events = Vec::new();

        tick_thermal(&mut state, &station_id, &content, &mut events);

        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, Event::OverheatWarning { .. })),
            "should emit OverheatWarning event"
        );
        let station = state.stations.get(&station_id).unwrap();
        let zone = station.modules[0].thermal.as_ref().unwrap().overheat_zone;
        assert_eq!(zone, OverheatZone::Warning);
    }

    #[test]
    fn overheat_critical_emits_event_and_disables_module() {
        let content = thermal_test_content();
        let station_id = StationId("station_test".to_string());
        // Set temp in critical zone: max=2_500_000, critical threshold=3_000_000
        let mut state = thermal_test_state(&content, 3_100_000);
        let mut events = Vec::new();

        tick_thermal(&mut state, &station_id, &content, &mut events);

        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, Event::OverheatCritical { .. })),
            "should emit OverheatCritical event"
        );
        let station = state.stations.get(&station_id).unwrap();
        assert!(
            !station.modules[0].enabled,
            "module should be auto-disabled in critical zone"
        );
        let zone = station.modules[0].thermal.as_ref().unwrap().overheat_zone;
        assert_eq!(zone, OverheatZone::Critical);
    }

    #[test]
    fn overheat_cleared_when_cooling_below_threshold() {
        let content = thermal_test_content();
        let station_id = StationId("station_test".to_string());
        // Start in warning zone
        let mut state = thermal_test_state(&content, 2_800_000);

        // First tick: enter warning
        let mut events = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events);
        assert_eq!(
            state.stations.get(&station_id).unwrap().modules[0]
                .thermal
                .as_ref()
                .unwrap()
                .overheat_zone,
            OverheatZone::Warning
        );

        // Manually cool below warning threshold
        state.stations.get_mut(&station_id).unwrap().modules[0]
            .thermal
            .as_mut()
            .unwrap()
            .temp_mk = 2_500_000;

        let mut events2 = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events2);
        assert!(
            events2
                .iter()
                .any(|e| matches!(&e.event, Event::OverheatCleared { .. })),
            "should emit OverheatCleared when cooling below threshold"
        );
        assert_eq!(
            state.stations.get(&station_id).unwrap().modules[0]
                .thermal
                .as_ref()
                .unwrap()
                .overheat_zone,
            OverheatZone::Nominal
        );
    }

    #[test]
    fn overheat_no_duplicate_event_when_already_in_zone() {
        let content = thermal_test_content();
        let station_id = StationId("station_test".to_string());
        // Start in warning zone
        let mut state = thermal_test_state(&content, 2_800_000);

        // First tick: transition to warning
        let mut events1 = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events1);
        let warning_count1 = events1
            .iter()
            .filter(|e| matches!(&e.event, Event::OverheatWarning { .. }))
            .count();
        assert_eq!(warning_count1, 1);

        // Manually keep temp in warning range (cooling would drop it)
        state.stations.get_mut(&station_id).unwrap().modules[0]
            .thermal
            .as_mut()
            .unwrap()
            .temp_mk = 2_800_000;

        // Second tick: already in warning, no new event
        let mut events2 = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events2);
        let warning_count2 = events2
            .iter()
            .filter(|e| matches!(&e.event, Event::OverheatWarning { .. }))
            .count();
        assert_eq!(
            warning_count2, 0,
            "should not re-emit when already in warning zone"
        );
    }

    #[test]
    fn critical_module_re_enables_when_cooled() {
        let content = thermal_test_content();
        let station_id = StationId("station_test".to_string());
        // Start in critical zone
        let mut state = thermal_test_state(&content, 3_100_000);

        // Enter critical — module gets disabled
        let mut events = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events);
        assert!(!state.stations.get(&station_id).unwrap().modules[0].enabled);

        // Cool down to nominal
        {
            let module = &mut state.stations.get_mut(&station_id).unwrap().modules[0];
            module.thermal.as_mut().unwrap().temp_mk = 2_000_000;
            // Re-enable so tick_thermal processes it (disabled modules still have thermal state)
            // Actually, we don't need to re-enable — the check_overheat_zones function handles it.
        }

        let mut events2 = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events2);
        assert!(
            state.stations.get(&station_id).unwrap().modules[0].enabled,
            "module should be re-enabled after cooling below critical"
        );
        assert!(
            events2
                .iter()
                .any(|e| matches!(&e.event, Event::OverheatCleared { .. })),
            "should emit OverheatCleared on recovery"
        );
    }
}
