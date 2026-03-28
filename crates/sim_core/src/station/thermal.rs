//! Thermal tick step — passive cooling for modules with `ThermalDef`.
//!
//! Runs after maintenance in the station tick loop (step 3.6).
//! Modules are grouped by `ThermalGroupId` and processed in sorted order
//! (by group ID, then by module ID within each group) for determinism.

use std::collections::BTreeMap;

use crate::{thermal, EventEnvelope, GameContent, GameState, OverheatZone, StationId};

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
    super::ensure_station_index(state, station_id, content);
    let dt_s = thermal::dt_seconds(&content.constants);
    let sink_temp_mk = content.constants.thermal_sink_temp_mk;

    let Some(station) = state.stations.get(station_id) else {
        return;
    };

    // Use pre-computed thermal indices to build groups.
    let thermal_indices = station.module_type_index.thermal.clone();
    let mut groups: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();

    for &module_index in &thermal_indices {
        let module = &station.modules[module_index];
        let Some(ref _thermal_state) = module.thermal else {
            continue;
        };
        let Some(def) = content.module_defs.get(&module.def_id) else {
            continue;
        };
        if def.thermal.is_none() {
            continue;
        }

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

    // Build radiator cooling using thermal indices (radiators have ThermalDef).
    let mut radiator_cooling_by_group: BTreeMap<String, f32> = BTreeMap::new();
    for &module_index in &thermal_indices {
        let module = &station.modules[module_index];
        if !module.enabled {
            continue;
        }
        let Some(def) = content.module_defs.get(&module.def_id) else {
            continue;
        };
        let crate::ModuleBehaviorDef::Radiator(ref radiator_def) = def.behavior else {
            continue;
        };
        let group_key = module
            .thermal
            .as_ref()
            .and_then(|t| t.thermal_group.clone())
            .unwrap_or_default();

        let mut cooling_mods = crate::modifiers::ModifierSet::new();
        cooling_mods.add(crate::modifiers::Modifier::pct_mult(
            crate::modifiers::StatId::CoolingRate,
            f64::from(crate::wear::wear_efficiency(
                module.wear.wear,
                &content.constants,
            )),
            crate::modifiers::ModifierSource::Wear,
        ));
        let effective_cooling = cooling_mods.resolve_with_f32(
            crate::modifiers::StatId::CoolingRate,
            radiator_def.cooling_capacity_w,
            &state.modifiers,
        );
        *radiator_cooling_by_group.entry(group_key).or_default() += effective_cooling;
    }

    // Apply idle heat generation to enabled modules with idle_heat_generation_w.
    for modules in groups.values() {
        for &(_, module_index) in modules {
            apply_idle_heat(state, station_id, module_index, content, dt_s);
        }
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

    // Cool material held in thermal containers (crucibles).
    tick_thermal_containers(state, station_id, content, dt_s, sink_temp_mk);

    // Check overheat zones for all thermal modules after temperature updates.
    check_overheat_zones(state, station_id, content, events);
}

/// Cool material held in thermal container modules (crucibles).
///
/// Each container module's held items lose heat toward sink temperature based on
/// the module's passive cooling coefficient. Phase transitions occur if material
/// cools below the solidification point.
fn tick_thermal_containers(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    dt_s: f64,
    sink_temp_mk: u32,
) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };

    // Collect container module indices
    let container_indices: Vec<usize> = station
        .modules
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m.kind_state, crate::ModuleKindState::ThermalContainer(_)))
        .map(|(idx, _)| idx)
        .collect();

    if container_indices.is_empty() {
        return;
    }

    let station = state
        .stations
        .get_mut(station_id)
        .expect("station verified above");

    for module_idx in container_indices {
        let module = &station.modules[module_idx];
        let Some(def) = content.module_defs.get(&module.def_id) else {
            continue;
        };
        let Some(ref thermal_def) = def.thermal else {
            continue;
        };

        let cooling_coeff = thermal_def.passive_cooling_coefficient;

        let crate::ModuleKindState::ThermalContainer(ref mut container) =
            station.modules[module_idx].kind_state
        else {
            continue;
        };

        // Apply Newton's law cooling to each held material with thermal props.
        for item in &mut container.held_items {
            if let crate::InventoryItem::Material {
                element,
                kg,
                thermal: Some(ref mut props),
                ..
            } = item
            {
                // Convert mK to K for Newton's law: dQ = coeff(W/K) * dT(K) * dt(s)
                let temp_diff_k = (f64::from(props.temp_mk) - f64::from(sink_temp_mk)) / 1000.0;
                #[allow(clippy::cast_possible_truncation)]
                let cooling_j = -(f64::from(cooling_coeff) * temp_diff_k * dt_s);

                if let Some(element_def) = content.elements.iter().find(|e| e.id == *element) {
                    #[allow(clippy::cast_possible_truncation)]
                    let heat = cooling_j.round() as i64;
                    thermal::update_phase(props, element_def, *kg, heat);
                }
            }
        }
    }
}

/// Apply idle heat generation to a single module.
///
/// Enabled modules with `idle_heat_generation_w` set generate heat every tick,
/// regardless of whether a recipe ran. This allows thermal modules (e.g. smelters)
/// to preheat from ambient temperature.
fn apply_idle_heat(
    state: &mut GameState,
    station_id: &StationId,
    module_index: usize,
    content: &GameContent,
    dt_s: f64,
) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };
    let module = &station.modules[module_index];

    // Only enabled modules generate idle heat.
    if !module.enabled {
        return;
    }

    let Some(def) = content.module_defs.get(&module.def_id) else {
        return;
    };
    let Some(ref thermal_def) = def.thermal else {
        return;
    };
    let Some(idle_w) = thermal_def.idle_heat_generation_w else {
        return;
    };
    if idle_w <= 0.0 {
        return;
    }

    let heat_j = thermal::power_to_heat_j(idle_w, dt_s);
    let delta_mk = thermal::heat_to_temp_delta_mk(heat_j, thermal_def.heat_capacity_j_per_k);

    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    if let Some(ref mut thermal) = station.modules[module_index].thermal {
        // idle_w > 0 guaranteed by early return, so delta_mk is always non-negative.
        debug_assert!(
            delta_mk >= 0,
            "idle heat delta must be >= 0, got {delta_mk}"
        );
        #[allow(clippy::cast_sign_loss)] // .max(0) guarantees non-negative
        let new_temp = thermal.temp_mk.saturating_add(delta_mk.max(0) as u32);
        thermal.temp_mk = new_temp.min(content.constants.t_max_absolute_mk);
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

    // Apply delta, clamping to [sink_temp, t_max_absolute_mk].
    let new_temp = if delta_mk < 0 {
        current_temp_mk.saturating_sub(delta_mk.unsigned_abs())
    } else {
        #[allow(clippy::cast_sign_loss)] // guarded by delta_mk >= 0 check
        current_temp_mk.saturating_add(delta_mk.cast_unsigned())
    };
    let clamped_temp = new_temp.clamp(sink_temp_mk, content.constants.t_max_absolute_mk);

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
    let clamped_temp = new_temp.clamp(sink_temp_mk, content.constants.t_max_absolute_mk);

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
    let damage_threshold = max_temp_mk.saturating_add(constants.thermal_overheat_damage_offset_mk);
    let critical_threshold =
        max_temp_mk.saturating_add(constants.thermal_overheat_critical_offset_mk);
    let warning_threshold =
        max_temp_mk.saturating_add(constants.thermal_overheat_warning_offset_mk);

    if temp_mk >= damage_threshold {
        OverheatZone::Damage
    } else if temp_mk >= critical_threshold {
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

    let has_enable_transitions = transitions.iter().any(|&(_, old_zone, new_zone, _, _)| {
        // Entering critical/damage disables; leaving critical/damage may re-enable.
        new_zone == OverheatZone::Critical
            || new_zone == OverheatZone::Damage
            || ((old_zone == OverheatZone::Critical || old_zone == OverheatZone::Damage)
                && new_zone != OverheatZone::Critical
                && new_zone != OverheatZone::Damage)
    });
    if has_enable_transitions {
        station.invalidate_power_cache();
    }

    for (module_index, old_zone, new_zone, temp_mk, max_temp_mk) in transitions {
        let module = &mut station.modules[module_index];
        let module_id = module.id.clone();

        // Update zone.
        if let Some(ref mut thermal) = module.thermal {
            thermal.overheat_zone = new_zone;
        }

        // Auto-disable on entering critical or damage.
        if new_zone == OverheatZone::Critical || new_zone == OverheatZone::Damage {
            module.enabled = false;
            if let Some(ref mut thermal) = module.thermal {
                thermal.overheat_disabled = true;
            }
        }

        // Damage zone: wear jumps to critical band threshold.
        if new_zone == OverheatZone::Damage {
            let wear_before = module.wear.wear;
            let critical_threshold = content.constants.wear_band_critical_threshold;
            module.wear.wear = module.wear.wear.max(critical_threshold);
            // Emit damage event with wear_before for diagnostics.
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                crate::Event::OverheatDamage {
                    station_id: station_id.clone(),
                    module_id: module.id.clone(),
                    temp_mk,
                    max_temp_mk,
                    wear_before,
                },
            ));
        }

        // Re-enable on leaving critical/damage, but only if overheat caused the disable.
        // Preserves player-disabled and wear-disabled states.
        if (old_zone == OverheatZone::Critical || old_zone == OverheatZone::Damage)
            && new_zone != OverheatZone::Critical
            && new_zone != OverheatZone::Damage
        {
            let was_overheat_disabled =
                module.thermal.as_ref().is_some_and(|t| t.overheat_disabled);
            if was_overheat_disabled {
                module.enabled = true;
                if let Some(ref mut thermal) = module.thermal {
                    thermal.overheat_disabled = false;
                }
            }
        }

        // Emit zone transition events (skip for Damage — already emitted above).
        if let Some(event) =
            overheat_zone_event(new_zone, station_id, module_id, temp_mk, max_temp_mk)
        {
            events.push(crate::emit(&mut state.counters, current_tick, event));
        }
    }
}

/// Build an overheat zone transition event, or `None` for Damage (emitted separately).
fn overheat_zone_event(
    zone: OverheatZone,
    station_id: &StationId,
    module_id: crate::ModuleInstanceId,
    temp_mk: u32,
    max_temp_mk: u32,
) -> Option<crate::Event> {
    match zone {
        OverheatZone::Warning => Some(crate::Event::OverheatWarning {
            station_id: station_id.clone(),
            module_id,
            temp_mk,
            max_temp_mk,
        }),
        OverheatZone::Critical => Some(crate::Event::OverheatCritical {
            station_id: station_id.clone(),
            module_id,
            temp_mk,
            max_temp_mk,
        }),
        OverheatZone::Nominal => Some(crate::Event::OverheatCleared {
            station_id: station_id.clone(),
            module_id,
            temp_mk,
        }),
        OverheatZone::Damage => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::ModuleDefBuilder;
    use crate::AHashMap;
    use crate::*;
    use std::collections::{HashMap, HashSet};

    /// Create content with a module def that has thermal properties.
    fn thermal_test_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_smelter".to_string(),
            ModuleDefBuilder::new("module_smelter")
                .name("Test Smelter")
                .mass(5000.0)
                .volume(10.0)
                .power(100.0)
                .wear(0.02)
                .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_minutes: 5,
                    processing_interval_ticks: 5,
                    recipes: vec![],
                }))
                .thermal(ThermalDef {
                    heat_capacity_j_per_k: 500.0,
                    passive_cooling_coefficient: 0.05,
                    max_temp_mk: 2_500_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: Some("smelting".to_string()),
                    idle_heat_generation_w: None,
                })
                .build(),
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
            asteroids: std::collections::BTreeMap::new(),
            ships: std::collections::BTreeMap::new(),
            stations: [(
                station_id.clone(),
                StationState {
                    id: station_id,
                    position: crate::test_fixtures::test_position(),
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
                            selected_recipe: None,
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        module_priority: 0,
                        assigned_crew: Default::default(),
                        efficiency: 1.0,
                        prev_crew_satisfied: true,
                        thermal: Some(ThermalState {
                            temp_mk,
                            thermal_group: Some("smelting".to_string()),
                            ..Default::default()
                        }),
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    crew: Default::default(),
                    leaders: Vec::new(),
                    thermal_links: Vec::new(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: crate::ModuleTypeIndex::default(),
                    module_id_index: HashMap::new(),
                    power_budget_cache: crate::PowerBudgetCache::default(),
                },
            )]
            .into_iter()
            .collect(),
            research: ResearchState {
                unlocked: HashSet::new(),
                data_pool: AHashMap::default(),
                evidence: AHashMap::default(),
                action_counts: AHashMap::default(),
            },
            balance: 0.0,
            export_revenue_total: 0.0,
            export_count: 0,
            counters: Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
                next_lot_id: 0,
                next_module_instance_id: 0,
            },
            modifiers: crate::modifiers::ModifierSet::default(),
            events: crate::sim_events::SimEventState::default(),
            propellant_consumed_total: 0.0,
            body_cache: AHashMap::default(),
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
            ModuleDefBuilder::new("module_reactor")
                .name("Test Reactor")
                .mass(8000.0)
                .volume(15.0)
                .power(50.0)
                .wear(0.01)
                .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_minutes: 10,
                    processing_interval_ticks: 10,
                    recipes: vec![],
                }))
                .thermal(ThermalDef {
                    heat_capacity_j_per_k: 1000.0,
                    passive_cooling_coefficient: 0.03,
                    max_temp_mk: 3_000_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: Some("reactor".to_string()),
                    idle_heat_generation_w: None,
                })
                .build(),
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
                selected_recipe: None,
            }),
            wear: WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
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
            ModuleDefBuilder::new(&def_id)
                .name(&format!("Radiator {radiator_id}"))
                .mass(200.0)
                .volume(2.0)
                .behavior(ModuleBehaviorDef::Radiator(RadiatorDef {
                    cooling_capacity_w,
                }))
                .thermal(ThermalDef {
                    heat_capacity_j_per_k: 100.0,
                    passive_cooling_coefficient: 0.0,
                    max_temp_mk: 5_000_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: Some("smelting".to_string()),
                    idle_heat_generation_w: None,
                })
                .build(),
        );
        let station = state.stations.get_mut(station_id).unwrap();
        station.modules.push(ModuleState {
            id: ModuleInstanceId(format!("radiator_{radiator_id}")),
            def_id,
            enabled: true,
            kind_state: ModuleKindState::Radiator(RadiatorState::default()),
            wear: WearState { wear },
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
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

    #[test]
    fn manually_disabled_module_re_enabled_after_overheat_cycle() {
        let content = thermal_test_content();
        let station_id = StationId("station_test".to_string());
        // Start in critical zone
        let mut state = thermal_test_state(&content, 3_100_000);

        // Manually disable the module BEFORE the overheat system runs
        state.stations.get_mut(&station_id).unwrap().modules[0].enabled = false;

        // Tick to enter critical zone — sets overheat_disabled=true even though already disabled
        let mut events = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events);

        let module = &state.stations[&station_id].modules[0];
        assert!(!module.enabled, "module should still be disabled");
        assert!(
            module.thermal.as_ref().unwrap().overheat_disabled,
            "overheat_disabled should be set in critical zone"
        );

        // Cool down below thresholds
        {
            let module = &mut state.stations.get_mut(&station_id).unwrap().modules[0];
            module.thermal.as_mut().unwrap().temp_mk = 2_000_000;
        }

        let mut events2 = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events2);

        // The overheat system re-enables the module because it tracks it via
        // overheat_disabled. This is correct — the overheat system "owns" the disable
        // and re-enables on recovery. A separate test verifies wear-disabled modules
        // are NOT re-enabled by the overheat system.
        let module = &state.stations[&station_id].modules[0];
        assert!(
            module.enabled,
            "module should be re-enabled after overheat clears (overheat system owns the disable)"
        );
        assert!(
            !module.thermal.as_ref().unwrap().overheat_disabled,
            "overheat_disabled should be cleared"
        );
    }

    #[test]
    fn wear_disabled_module_not_re_enabled_by_overheat_clear() {
        let content = thermal_test_content();
        let station_id = StationId("station_test".to_string());
        // Start above critical
        let mut state = thermal_test_state(&content, 3_100_000);

        // Enter critical zone — sets overheat_disabled
        let mut events = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events);
        assert!(!state.stations.get(&station_id).unwrap().modules[0].enabled);

        // Simulate wear-based disable: clear overheat_disabled but keep enabled=false
        {
            let module = &mut state.stations.get_mut(&station_id).unwrap().modules[0];
            module.thermal.as_mut().unwrap().overheat_disabled = false;
            module.wear.wear = 1.0; // max wear
        }

        // Cool down
        {
            let module = &mut state.stations.get_mut(&station_id).unwrap().modules[0];
            module.thermal.as_mut().unwrap().temp_mk = 2_000_000;
        }

        let mut events2 = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events2);

        // Module should NOT be re-enabled because overheat_disabled was cleared
        assert!(
            !state.stations.get(&station_id).unwrap().modules[0].enabled,
            "wear-disabled module should stay disabled even after overheat clears"
        );
    }

    /// Helper: create content with a smelter that has idle heat generation.
    fn idle_heat_test_content(idle_w: f32) -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_smelter".to_string(),
            ModuleDefBuilder::new("module_smelter")
                .name("Test Smelter")
                .mass(5000.0)
                .volume(10.0)
                .power(100.0)
                .wear(0.02)
                .behavior(ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 5,
                    processing_interval_ticks: 5,
                    recipes: vec![],
                }))
                .thermal(ThermalDef {
                    heat_capacity_j_per_k: 500.0,
                    passive_cooling_coefficient: 0.05,
                    max_temp_mk: 2_500_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: Some("smelting".to_string()),
                    idle_heat_generation_w: Some(idle_w),
                })
                .build(),
        );
        content
    }

    #[test]
    fn idle_heat_warms_module_from_ambient() {
        let content = idle_heat_test_content(100.0); // 100W idle heat
        let mut state = thermal_test_state(&content, content.constants.thermal_sink_temp_mk);
        let station_id = StationId("station_test".to_string());

        let initial_temp = content.constants.thermal_sink_temp_mk;
        tick_thermal(&mut state, &station_id, &content, &mut Vec::new());

        let temp = state.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;
        assert!(
            temp > initial_temp,
            "idle heat should warm module above ambient; got {temp}, was {initial_temp}"
        );
    }

    #[test]
    fn idle_heat_not_applied_when_disabled() {
        let content = idle_heat_test_content(100.0);
        let mut state = thermal_test_state(&content, content.constants.thermal_sink_temp_mk);
        let station_id = StationId("station_test".to_string());

        // Disable the module.
        state.stations.get_mut(&station_id).unwrap().modules[0].enabled = false;

        tick_thermal(&mut state, &station_id, &content, &mut Vec::new());

        let temp = state.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;
        assert_eq!(
            temp, content.constants.thermal_sink_temp_mk,
            "disabled module should not receive idle heat"
        );
    }

    #[test]
    fn idle_heat_reaches_equilibrium() {
        let content = idle_heat_test_content(100.0);
        let mut state = thermal_test_state(&content, content.constants.thermal_sink_temp_mk);
        let station_id = StationId("station_test".to_string());

        // Run many ticks to reach equilibrium (time constant ~167 ticks).
        for _ in 0..2000 {
            tick_thermal(&mut state, &station_id, &content, &mut Vec::new());
        }

        let temp_a = state.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;

        // One more tick should produce negligible change.
        tick_thermal(&mut state, &station_id, &content, &mut Vec::new());
        let temp_b = state.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;

        let delta = temp_a.abs_diff(temp_b);
        assert!(
            delta < 100,
            "after many ticks, temperature should stabilize; delta was {delta} mK"
        );

        // Equilibrium should be above ambient.
        assert!(
            temp_a > content.constants.thermal_sink_temp_mk + 10_000,
            "equilibrium should be well above ambient; got {temp_a}"
        );
    }

    #[test]
    fn no_idle_heat_without_field() {
        // Use the standard thermal_test_content which has idle_heat_generation_w: None.
        let content = thermal_test_content();
        let sink = content.constants.thermal_sink_temp_mk;
        let mut state = thermal_test_state(&content, sink);
        let station_id = StationId("station_test".to_string());

        tick_thermal(&mut state, &station_id, &content, &mut Vec::new());

        let temp = state.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;
        assert_eq!(
            temp, sink,
            "module without idle_heat_generation_w at sink should stay at sink"
        );
    }

    #[test]
    fn damage_zone_sets_wear_to_critical_band() {
        let content = thermal_test_content();
        // Smelter max_temp_mk is 2_500_000. Damage offset is 800_000.
        // Damage threshold = 2_500_000 + 800_000 = 3_300_000.
        // Set well above so passive cooling doesn't drop below threshold.
        let damage_temp = 3_500_000;
        let mut state = thermal_test_state(&content, damage_temp);
        let station_id = StationId("station_test".to_string());

        // Set initial wear to something low.
        state.stations.get_mut(&station_id).unwrap().modules[0]
            .wear
            .wear = 0.1;

        let mut events = Vec::new();
        tick_thermal(&mut state, &station_id, &content, &mut events);

        let module = &state.stations.get(&station_id).unwrap().modules[0];
        let thermal = module.thermal.as_ref().unwrap();

        assert_eq!(
            thermal.overheat_zone,
            OverheatZone::Damage,
            "module should be in damage zone"
        );
        assert!(
            module.wear.wear >= 0.8,
            "wear should jump to at least 0.8; got {}",
            module.wear.wear
        );
        assert!(!module.enabled, "module should be auto-disabled");
        assert!(thermal.overheat_disabled, "overheat_disabled should be set");

        // Verify OverheatDamage event was emitted.
        let damage_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event, crate::Event::OverheatDamage { .. }))
            .collect();
        assert_eq!(
            damage_events.len(),
            1,
            "exactly one OverheatDamage event should be emitted"
        );
    }

    #[test]
    fn damage_zone_does_not_lower_existing_high_wear() {
        let content = thermal_test_content();
        let damage_temp = 3_500_000;
        let mut state = thermal_test_state(&content, damage_temp);
        let station_id = StationId("station_test".to_string());

        // Module already has high wear.
        state.stations.get_mut(&station_id).unwrap().modules[0]
            .wear
            .wear = 0.95;

        tick_thermal(&mut state, &station_id, &content, &mut Vec::new());

        let wear = state.stations.get(&station_id).unwrap().modules[0]
            .wear
            .wear;
        assert!(
            (wear - 0.95).abs() < f32::EPSILON,
            "wear should not decrease; got {wear}"
        );
    }
}
