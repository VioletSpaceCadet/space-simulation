mod assembler;
mod boiloff;
mod lab;
mod maintenance;
mod processor;
mod sensor;
pub(crate) mod thermal;

use crate::instrumentation::{timed, TickTimings};
use crate::{
    tasks::element_density, Event, EventEnvelope, GameContent, GameState, InputFilter,
    InventoryItem, ItemKind, OutputSpec, RecipeDef, StationId, YieldFormula,
};
use std::collections::HashMap;

/// Estimate the total output volume (m3) a recipe would produce given the
/// consumed lots and their weighted-average composition.
fn estimate_output_volume_m3(
    recipe: &RecipeDef,
    avg_composition: &HashMap<String, f32>,
    consumed_kg: f32,
    content: &GameContent,
) -> f32 {
    let mut material_kg = 0.0_f32;
    let mut total_volume = 0.0_f32;

    for output in &recipe.outputs {
        match output {
            OutputSpec::Material {
                element,
                yield_formula,
                ..
            } => {
                let yield_frac = match yield_formula {
                    YieldFormula::ElementFraction { element: el } => {
                        avg_composition.get(el).copied().unwrap_or(0.0)
                    }
                    YieldFormula::FixedFraction(f) => *f,
                };
                material_kg = consumed_kg * yield_frac;
                let density = element_density(content, element);
                total_volume += material_kg / density;
            }
            OutputSpec::Slag { yield_formula } => {
                let yield_frac = match yield_formula {
                    YieldFormula::FixedFraction(f) => *f,
                    YieldFormula::ElementFraction { element } => {
                        avg_composition.get(element).copied().unwrap_or(0.0)
                    }
                };
                let slag_kg = (consumed_kg - material_kg) * yield_frac;
                let slag_density = element_density(content, crate::ELEMENT_SLAG);
                total_volume += slag_kg / slag_density;
            }
            OutputSpec::Component { component_id, .. } => {
                if let Some(comp_def) = content
                    .component_defs
                    .iter()
                    .find(|c| c.id == component_id.0)
                {
                    total_volume += comp_def.volume_m3;
                }
            }
            OutputSpec::Ship { .. } => {}
        }
    }
    total_volume
}

fn matches_input_filter(item: &InventoryItem, filter: Option<&InputFilter>) -> bool {
    match filter {
        Some(InputFilter::ItemKind(ItemKind::Ore)) => item.is_ore(),
        Some(InputFilter::ItemKind(ItemKind::Material)) => item.is_material(),
        Some(InputFilter::ItemKind(ItemKind::Slag)) => item.is_slag(),
        Some(InputFilter::Element(el)) => item.element_id() == Some(el.as_str()),
        _ => false,
    }
}

/// Ensure module type indices are initialized for all stations.
fn ensure_indices(state: &mut GameState, content: &GameContent) {
    for station in state.stations.values_mut() {
        if !station.core.module_type_index.is_initialized() {
            station.rebuild_module_index(content);
        }
    }
}

/// Ensure module type index is initialized for a single station.
fn ensure_station_index(state: &mut GameState, station_id: &StationId, content: &GameContent) {
    if let Some(station) = state.stations.get_mut(station_id) {
        if !station.core.module_type_index.is_initialized() {
            station.rebuild_module_index(content);
        }
    }
}

/// Check crew satisfaction transitions and emit Understaffed/FullyStaffed events.
/// Compares current `is_crew_satisfied` against `prev_crew_satisfied` stored on each module.
fn update_crew_satisfaction(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };
    let current_tick = state.meta.tick;
    let mut transitions: Vec<(crate::ModuleInstanceId, bool, bool)> = Vec::new();
    for module in &station.core.modules {
        let Some(def) = content.module_defs.get(&module.def_id) else {
            continue;
        };
        if def.crew_requirement.is_empty() {
            continue;
        }
        let now_satisfied = crate::is_crew_satisfied(&module.assigned_crew, &def.crew_requirement);
        if now_satisfied != module.prev_crew_satisfied {
            transitions.push((module.id.clone(), module.prev_crew_satisfied, now_satisfied));
        }
    }
    if !transitions.is_empty() {
        let station = state
            .stations
            .get_mut(station_id)
            .expect("station checked above");
        for (module_id, _, now) in &transitions {
            if let Some(module) = station.core.modules.iter_mut().find(|m| &m.id == module_id) {
                module.prev_crew_satisfied = *now;
            }
        }
    }
    for (module_id, was, now) in transitions {
        if was && !now {
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::ModuleUnderstaffed {
                    station_id: station_id.clone(),
                    module_id,
                },
            ));
        } else if !was && now {
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::ModuleFullyStaffed {
                    station_id: station_id.clone(),
                    module_id,
                },
            ));
        }
    }
}

/// Compute and store efficiency for all modules on a station.
/// Call after `compute_power_budget` so `power_stalled` flags are set.
fn update_module_efficiencies(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    let current_tick = state.meta.tick;
    for module in &mut station.core.modules {
        if let Some(def) = content.module_defs.get(&module.def_id) {
            let old_efficiency = module.efficiency;
            module.efficiency = crate::compute_module_efficiency(module, def, &content.constants);
            if (module.efficiency - old_efficiency).abs() > f32::EPSILON {
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::ModuleEfficiencyChanged {
                        station_id: station_id.clone(),
                        module_id: module.id.clone(),
                        efficiency: module.efficiency,
                    },
                ));
            }
        }
    }
}

// timings is only used inside timed!() macro which is cfg-gated behind
// debug_assertions or the instrumentation feature.
#[allow(unused_mut, unused_variables)]
pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
    mut timings: Option<&mut TickTimings>,
) {
    // Ensure module type indices are initialized.
    ensure_indices(state, content);
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    let mut scratch_indices: Vec<usize> = Vec::new();
    for station_id in &station_ids {
        // Update crew satisfaction events (before efficiency recompute)
        update_crew_satisfaction(state, station_id, content, events);
        timed!(
            timings,
            power_budget,
            compute_power_budget(state, station_id, content, events)
        );
        // Compute combined efficiency after power budget sets power_stalled flags
        update_module_efficiencies(state, station_id, content, events);
        timed!(
            timings,
            processors,
            processor::tick_station_modules(
                state,
                station_id,
                content,
                events,
                &mut scratch_indices
            )
        );
        timed!(
            timings,
            assemblers,
            assembler::tick_assembler_modules(
                state,
                station_id,
                content,
                rng,
                events,
                &mut scratch_indices
            )
        );
        timed!(
            timings,
            sensors,
            sensor::tick_sensor_array_modules(state, station_id, content, events)
        );
        timed!(
            timings,
            labs,
            lab::tick_lab_modules(state, station_id, content, events)
        );
        timed!(
            timings,
            maintenance,
            maintenance::tick_maintenance_modules(state, station_id, content, events)
        );
        timed!(
            timings,
            thermal,
            thermal::tick_thermal(state, station_id, content, events)
        );
        // Step 3.7: Boiloff — uses post-thermal temperatures (Contract A)
        timed!(
            timings,
            boiloff,
            boiloff::apply_boiloff(state, station_id, content, events)
        );
    }
}

/// Apply battery charge/discharge. Returns (discharge, charge, stored) in kW/kWh.
fn apply_battery_buffering(
    state: &mut GameState,
    station_id: &StationId,
    batteries: &[(usize, crate::BatteryDef, f32, f32)],
    raw_surplus: f32,
    raw_deficit: f32,
) -> (f32, f32, f32) {
    let mut discharge_kw = 0.0_f32;
    let mut charge_kw = 0.0_f32;

    if raw_deficit > 0.0 {
        let mut remaining = raw_deficit;
        for (module_index, battery_def, current_charge, _efficiency) in batteries {
            if remaining <= 0.0 {
                break;
            }
            let available = current_charge.min(battery_def.discharge_rate_kw);
            let discharge = available.min(remaining);
            remaining -= discharge;
            discharge_kw += discharge;

            let Some(station) = state.stations.get_mut(station_id) else {
                continue;
            };
            if let crate::ModuleKindState::Battery(ref mut battery_state) =
                station.core.modules[*module_index].kind_state
            {
                battery_state.charge_kwh -= discharge;
            }
        }
    } else if raw_surplus > 0.0 {
        let mut remaining = raw_surplus;
        for (module_index, battery_def, current_charge, efficiency) in batteries {
            if remaining <= 0.0 {
                break;
            }
            let effective_capacity = battery_def.capacity_kwh * efficiency;
            let headroom = (effective_capacity - current_charge).max(0.0);
            let charge = headroom.min(battery_def.charge_rate_kw).min(remaining);
            remaining -= charge;
            charge_kw += charge;

            let Some(station) = state.stations.get_mut(station_id) else {
                continue;
            };
            if let crate::ModuleKindState::Battery(ref mut battery_state) =
                station.core.modules[*module_index].kind_state
            {
                battery_state.charge_kwh += charge;
            }
        }
    }

    // Sum total stored energy across all batteries after updates.
    let mut stored_kwh = 0.0_f32;
    let Some(station) = state.stations.get(station_id) else {
        return (discharge_kw, charge_kw, stored_kwh);
    };
    for module in &station.core.modules {
        if let crate::ModuleKindState::Battery(ref battery_state) = module.kind_state {
            stored_kwh += battery_state.charge_kwh;
        }
    }

    (discharge_kw, charge_kw, stored_kwh)
}

/// Compute wear + environment + tech-adjusted solar output for one array.
fn resolve_solar_output(
    solar_def: &crate::SolarArrayDef,
    wear: f32,
    solar_intensity: f32,
    constants: &crate::Constants,
    global_modifiers: &crate::modifiers::ModifierSet,
) -> f32 {
    let mut power_mods = crate::modifiers::ModifierSet::new();
    power_mods.add(crate::modifiers::Modifier::pct_mult(
        crate::modifiers::StatId::SolarOutput,
        f64::from(solar_intensity),
        crate::modifiers::ModifierSource::Environment,
    ));
    power_mods.add(crate::modifiers::Modifier::pct_mult(
        crate::modifiers::StatId::SolarOutput,
        f64::from(crate::wear::wear_efficiency(wear, constants)),
        crate::modifiers::ModifierSource::Wear,
    ));
    power_mods.resolve_with_f32(
        crate::modifiers::StatId::SolarOutput,
        solar_def.base_output_kw,
        global_modifiers,
    )
}

/// Rebuild the cached power generation/consumption summary for a station.
/// Iterates all modules, looks up defs, computes wear-adjusted generation.
/// Only called when `power_budget_cache.is_valid()` is false.
fn rebuild_power_cache(
    station: &crate::StationState,
    content: &GameContent,
    global_modifiers: &crate::modifiers::ModifierSet,
) -> crate::PowerBudgetCache {
    let solar_intensity = content
        .solar_system
        .bodies
        .iter()
        .find(|b| b.id == station.position.parent_body)
        .map_or(1.0, |b| b.solar_intensity);

    let mut generated_kw = 0.0_f32;
    let mut consumed_kw = 0.0_f32;
    let mut has_power_infrastructure = false;
    let mut consumers: Vec<(usize, u8, f32)> = Vec::new();
    let mut battery_entries: Vec<(usize, crate::BatteryDef, f32)> = Vec::new();
    let mut solar_wear_targets: Vec<(usize, f32)> = Vec::new();
    let mut wear_band_snapshot: Vec<(usize, u8)> = Vec::new();

    // Resolve global power consumption multiplier (e.g. tech_electrolysis_efficiency).
    let power_consumption_mult =
        global_modifiers.resolve_f32(crate::modifiers::StatId::PowerConsumption, 1.0);

    // Resolve global battery capacity multiplier (e.g. tech_battery_storage).
    let battery_capacity_mult =
        global_modifiers.resolve_f32(crate::modifiers::StatId::BatteryCapacity, 1.0);

    for (module_index, module) in station.core.modules.iter().enumerate() {
        if !module.enabled {
            continue;
        }
        let Some(def) = content.module_defs.get(&module.def_id) else {
            continue;
        };

        match &def.behavior {
            crate::ModuleBehaviorDef::SolarArray(solar_def) => {
                has_power_infrastructure = true;
                generated_kw += resolve_solar_output(
                    solar_def,
                    module.wear.wear,
                    solar_intensity,
                    &content.constants,
                    global_modifiers,
                );
                consumed_kw += def.power_consumption_per_run * power_consumption_mult;
                if def.wear_per_run > 0.0 {
                    solar_wear_targets.push((module_index, def.wear_per_run));
                }
                wear_band_snapshot.push((
                    module_index,
                    crate::wear::wear_band(module.wear.wear, &content.constants),
                ));
            }
            crate::ModuleBehaviorDef::Battery(battery_def) => {
                has_power_infrastructure = true;
                let mut battery_mods = crate::modifiers::ModifierSet::new();
                battery_mods.add(crate::modifiers::Modifier::pct_mult(
                    crate::modifiers::StatId::PowerOutput,
                    f64::from(crate::wear::wear_efficiency(
                        module.wear.wear,
                        &content.constants,
                    )),
                    crate::modifiers::ModifierSource::Wear,
                ));
                let efficiency = battery_mods.resolve_with_f32(
                    crate::modifiers::StatId::PowerOutput,
                    1.0,
                    global_modifiers,
                );
                let mut scaled_battery = battery_def.clone();
                scaled_battery.capacity_kwh *= battery_capacity_mult;
                battery_entries.push((module_index, scaled_battery, efficiency));
                consumed_kw += def.power_consumption_per_run * power_consumption_mult;
                wear_band_snapshot.push((
                    module_index,
                    crate::wear::wear_band(module.wear.wear, &content.constants),
                ));
            }
            _ => {
                let effective_consumption = def.power_consumption_per_run * power_consumption_mult;
                consumed_kw += effective_consumption;
                if let Some(priority) = def.power_priority() {
                    consumers.push((module_index, priority, effective_consumption));
                }
            }
        }
    }

    // Pre-sort consumers by priority so we don't need to sort every tick.
    consumers.sort_by_key(|&(_, priority, _)| priority);

    let enabled_count = station.core.modules.iter().filter(|m| m.enabled).count();

    crate::PowerBudgetCache {
        generated_kw,
        consumed_kw,
        has_power_infrastructure,
        consumers,
        battery_entries,
        solar_wear_targets,
        wear_band_snapshot,
        global_modifier_generation: global_modifiers.generation(),
        module_enabled_snapshot: (station.core.modules.len(), enabled_count),
        ..Default::default()
    }
}

/// Ensure the power budget cache is up-to-date for a station.
/// Rebuilds if explicitly invalidated, global modifiers changed,
/// or module count/enabled state diverged (catches direct mutations in tests).
fn ensure_power_cache(state: &mut GameState, station_id: &StationId, content: &GameContent) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };
    let enabled_count = station.core.modules.iter().filter(|m| m.enabled).count();
    let needs_rebuild = !station.core.power_budget_cache.is_valid()
        || station.core.power_budget_cache.global_modifier_generation
            != state.modifiers.generation()
        || station.core.power_budget_cache.module_enabled_snapshot
            != (station.core.modules.len(), enabled_count);

    if needs_rebuild {
        let mut cache = rebuild_power_cache(station, content, &state.modifiers);
        cache.mark_valid();
        if let Some(station) = state.stations.get_mut(station_id) {
            station.core.power_budget_cache = cache;
        }
    }
}

/// Apply solar wear and check for band transitions that would invalidate the cache.
fn apply_solar_wear_and_check_bands(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };
    let wear_targets = station.core.power_budget_cache.solar_wear_targets.clone();
    for (module_idx, wear_per_run) in &wear_targets {
        apply_wear(state, station_id, *module_idx, *wear_per_run, events);
    }

    // Check if any power-related module crossed a wear band boundary.
    let Some(station) = state.stations.get(station_id) else {
        return;
    };
    let band_changed = station
        .core
        .power_budget_cache
        .wear_band_snapshot
        .iter()
        .any(|&(module_index, cached_band)| {
            module_index < station.core.modules.len()
                && crate::wear::wear_band(
                    station.core.modules[module_index].wear.wear,
                    &content.constants,
                ) != cached_band
        });
    if band_changed {
        if let Some(station) = state.stations.get_mut(station_id) {
            station.core.power_budget_cache.invalidate();
        }
    }
}

/// Compute the power budget for a station, store it in `PowerState`, and
/// mark modules as `power_stalled` when there is a deficit.
///
/// Uses a cached generation/consumption summary when available. The cache
/// is rebuilt only when modules change (install/uninstall/enable/disable)
/// or when a power-relevant module crosses a wear band boundary.
/// Battery buffering and stall logic run every tick regardless.
fn compute_power_budget(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };
    let prev_power = station.core.power.clone();

    ensure_power_cache(state, station_id, content);

    // Read cached values and build per-tick battery list with live charge.
    let Some(station) = state.stations.get(station_id) else {
        return;
    };
    let cache = &station.core.power_budget_cache;
    let generated_kw = cache.generated_kw;
    let consumed_kw = cache.consumed_kw;
    let has_power_infrastructure = cache.has_power_infrastructure;

    // Build per-tick battery list with live charge values.
    let batteries: Vec<(usize, crate::BatteryDef, f32, f32)> = cache
        .battery_entries
        .iter()
        .map(|(idx, def, eff)| {
            let charge = match &station.core.modules[*idx].kind_state {
                crate::ModuleKindState::Battery(bs) => bs.charge_kwh,
                _ => 0.0,
            };
            (*idx, def.clone(), charge, *eff)
        })
        .collect();

    // Clone consumer list for stall logic (small vec — typically 5-8 entries).
    let consumers = cache.consumers.clone();

    let raw_surplus = (generated_kw - consumed_kw).max(0.0);
    let raw_deficit = (consumed_kw - generated_kw).max(0.0);

    let (battery_discharge_kw, battery_charge_kw, battery_stored_kwh) =
        apply_battery_buffering(state, station_id, &batteries, raw_surplus, raw_deficit);
    let deficit_kw = (raw_deficit - battery_discharge_kw).max(0.0);

    // Apply stalls and update PowerState.
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    for module in &mut station.core.modules {
        module.power_stalled = false;
    }
    if deficit_kw > 0.0 && has_power_infrastructure {
        let mut remaining = deficit_kw;
        for &(module_index, _, consumption) in &consumers {
            if remaining <= 0.0 {
                break;
            }
            station.core.modules[module_index].power_stalled = true;
            remaining -= consumption;
        }
    }
    station.core.power = crate::PowerState {
        generated_kw,
        consumed_kw,
        deficit_kw,
        battery_discharge_kw,
        battery_charge_kw,
        battery_stored_kwh,
    };

    if station.core.power != prev_power {
        let current_tick = state.meta.tick;
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::PowerStateUpdated {
                station_id: station_id.clone(),
                power: station.core.power.clone(),
            },
        ));
    }

    apply_solar_wear_and_check_bands(state, station_id, content, events);
}

/// Context extracted once per module, shared across the lifecycle.
pub(crate) struct ModuleTickContext<'a> {
    pub station_id: StationId,
    pub module_idx: usize,
    pub module_id: crate::ModuleInstanceId,
    pub def: &'a crate::ModuleDef,
    pub interval: u64,
    pub power_needed: f32,
    pub wear_per_run: f32,
    pub efficiency: f32,
}

/// Reason a module stalled (distinct from "skipped").
#[derive(Debug)]
pub(crate) enum StallReason {
    VolumeCap {
        shortfall_m3: f32,
    },
    StockCap,
    DataStarved,
    TooCold {
        current_temp_mk: u32,
        required_temp_mk: u32,
    },
}

/// Outcome of a module's `execute()` call.
#[derive(Debug)]
pub(crate) enum RunOutcome {
    /// Module ran successfully — framework resets timer, applies wear.
    Completed,
    /// Module can't run (no inputs, no target) — no wear.
    /// `reset_timer`: true = reset to 0, false = keep accumulating.
    /// Module specifies intent; framework executes.
    Skipped { reset_timer: bool },
    /// Module is stalled — framework resets timer, manages stall flag + events.
    Stalled(StallReason),
}

/// Extract shared module context. Returns None if the module should be skipped
/// entirely (disabled, power-stalled, passive type, missing def).
fn extract_context<'a>(
    state: &GameState,
    station_id: &StationId,
    module_idx: usize,
    content: &'a GameContent,
) -> Option<ModuleTickContext<'a>> {
    let station = state.stations.get(station_id)?;
    let module = &station.core.modules[module_idx];

    if !module.enabled || module.efficiency <= 0.0 {
        return None;
    }

    let def = content.module_defs.get(&module.def_id)?;

    let interval = def.behavior.interval_ticks()?;

    Some(ModuleTickContext {
        station_id: station_id.clone(),
        module_idx,
        module_id: module.id.clone(),
        def,
        interval,
        power_needed: def.power_consumption_per_run,
        wear_per_run: def.wear_per_run,
        efficiency: module.efficiency,
    })
}

fn apply_wear(
    state: &mut GameState,
    station_id: &StationId,
    module_idx: usize,
    wear_per_run: f32,
    events: &mut Vec<EventEnvelope>,
) {
    if wear_per_run <= 0.0 {
        return;
    }
    let current_tick = state.meta.tick;
    if let Some(station) = state.stations.get_mut(station_id) {
        let module = &mut station.core.modules[module_idx];
        let wear_before = module.wear.wear;
        module.wear.wear = (module.wear.wear + wear_per_run).min(1.0);
        let wear_after = module.wear.wear;
        let module_id = module.id.clone();
        let auto_disable = module.wear.wear >= 1.0;
        if auto_disable {
            module.enabled = false;
        }
        // Drop the module borrow before calling station methods.
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::WearAccumulated {
                station_id: station_id.clone(),
                module_id: module_id.clone(),
                wear_before,
                wear_after,
            },
        ));
        if auto_disable {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            station.invalidate_power_cache();
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::ModuleAutoDisabled {
                    station_id: station_id.clone(),
                    module_id,
                },
            ));
        }
    }
}

/// Increment timer and check if the module should run this tick.
/// Returns true if: timer >= interval AND station has enough power.
fn should_run(state: &mut GameState, ctx: &ModuleTickContext) -> bool {
    let ticks = {
        let Some(station) = state.stations.get_mut(&ctx.station_id) else {
            return false;
        };
        let module = &mut station.core.modules[ctx.module_idx];
        let Some(timer) = module.kind_state.ticks_since_last_run_mut() else {
            return false;
        };
        *timer += 1;
        *timer
    };

    if ticks < ctx.interval {
        return false;
    }

    let Some(station) = state.stations.get(&ctx.station_id) else {
        return false;
    };
    // Efficiency already incorporates crew and wear factors (checked in extract_context).
    // Here we only check per-run power availability.
    station.core.power_available_per_tick >= ctx.power_needed
}

/// Reset the `ticks_since_last_run` to 0 for any module kind.
fn reset_timer(state: &mut GameState, ctx: &ModuleTickContext) {
    let Some(station) = state.stations.get_mut(&ctx.station_id) else {
        return;
    };
    let module = &mut station.core.modules[ctx.module_idx];
    if let Some(timer) = module.kind_state.ticks_since_last_run_mut() {
        *timer = 0;
    }
}

/// Handle the stall transition: set the appropriate stall flag and emit an event
/// only on the transition from not-stalled to stalled.
fn handle_stall_transition(
    state: &mut GameState,
    ctx: &ModuleTickContext,
    reason: &StallReason,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Read old flags and set new flags in one borrow scope
    let transitioned = {
        let Some(station) = state.stations.get_mut(&ctx.station_id) else {
            return;
        };
        let module = &mut station.core.modules[ctx.module_idx];

        match reason {
            StallReason::VolumeCap { .. } => {
                let was_stalled = match &module.kind_state {
                    crate::ModuleKindState::Processor(s) => s.stalled,
                    crate::ModuleKindState::Assembler(s) => s.stalled,
                    _ => false,
                };
                match &mut module.kind_state {
                    crate::ModuleKindState::Processor(s) => s.stalled = true,
                    crate::ModuleKindState::Assembler(s) => s.stalled = true,
                    _ => {}
                }
                !was_stalled
            }
            StallReason::StockCap => {
                let was_capped = match &module.kind_state {
                    crate::ModuleKindState::Assembler(s) => s.capped,
                    _ => false,
                };
                if let crate::ModuleKindState::Assembler(s) = &mut module.kind_state {
                    s.capped = true;
                }
                !was_capped
            }
            StallReason::DataStarved => {
                let was_starved = match &module.kind_state {
                    crate::ModuleKindState::Lab(s) => s.starved,
                    _ => false,
                };
                if let crate::ModuleKindState::Lab(s) = &mut module.kind_state {
                    s.starved = true;
                }
                !was_starved
            }
            StallReason::TooCold { .. } => {
                let was_stalled = match &module.kind_state {
                    crate::ModuleKindState::Processor(s) => s.stalled,
                    _ => false,
                };
                if let crate::ModuleKindState::Processor(s) = &mut module.kind_state {
                    s.stalled = true;
                }
                !was_stalled
            }
        }
    };

    // Emit event outside the station borrow
    if transitioned {
        let event = match reason {
            StallReason::VolumeCap { shortfall_m3 } => Event::ModuleStalled {
                station_id: ctx.station_id.clone(),
                module_id: ctx.module_id.clone(),
                shortfall_m3: *shortfall_m3,
            },
            StallReason::StockCap => Event::AssemblerCapped {
                station_id: ctx.station_id.clone(),
                module_id: ctx.module_id.clone(),
            },
            StallReason::DataStarved => Event::LabStarved {
                station_id: ctx.station_id.clone(),
                module_id: ctx.module_id.clone(),
            },
            StallReason::TooCold {
                current_temp_mk,
                required_temp_mk,
            } => Event::ProcessorTooCold {
                station_id: ctx.station_id.clone(),
                module_id: ctx.module_id.clone(),
                current_temp_mk: *current_temp_mk,
                required_temp_mk: *required_temp_mk,
            },
        };
        events.push(crate::emit(&mut state.counters, current_tick, event));
    }
}

/// If the module was stalled, clear all stall flags and emit resume events.
fn handle_resume_if_stalled(
    state: &mut GameState,
    ctx: &ModuleTickContext,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Clear flags and collect which events to emit
    let mut emit_resumed = false;
    let mut emit_uncapped = false;
    let mut emit_lab_resumed = false;

    {
        let Some(station) = state.stations.get_mut(&ctx.station_id) else {
            return;
        };
        let module = &mut station.core.modules[ctx.module_idx];

        match &mut module.kind_state {
            crate::ModuleKindState::Processor(s) => {
                if s.stalled {
                    s.stalled = false;
                    emit_resumed = true;
                }
            }
            crate::ModuleKindState::Assembler(s) => {
                if s.stalled {
                    s.stalled = false;
                    emit_resumed = true;
                }
                if s.capped {
                    s.capped = false;
                    emit_uncapped = true;
                }
            }
            crate::ModuleKindState::Lab(s) => {
                if s.starved {
                    s.starved = false;
                    emit_lab_resumed = true;
                }
            }
            _ => {}
        }
    }

    // Emit events outside the station borrow
    if emit_resumed {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::ModuleResumed {
                station_id: ctx.station_id.clone(),
                module_id: ctx.module_id.clone(),
            },
        ));
    }
    if emit_uncapped {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::AssemblerUncapped {
                station_id: ctx.station_id.clone(),
                module_id: ctx.module_id.clone(),
            },
        ));
    }
    if emit_lab_resumed {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::LabResumed {
                station_id: ctx.station_id.clone(),
                module_id: ctx.module_id.clone(),
            },
        ));
    }
}

/// Apply the outcome of a module run: timer reset, wear, stall transitions, volume cache.
fn apply_run_result(
    state: &mut GameState,
    ctx: &ModuleTickContext,
    outcome: RunOutcome,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    match outcome {
        RunOutcome::Completed => {
            // Clear stall flag if was stalled, emit resume event
            handle_resume_if_stalled(state, ctx, events);
            // Reset timer
            reset_timer(state, ctx);
            // Compute wear through modifier system (heat zone multiplier).
            let mut wear_mods = crate::modifiers::ModifierSet::new();
            let heat_multiplier = state
                .stations
                .get(&ctx.station_id)
                .and_then(|s| s.core.modules.get(ctx.module_idx))
                .and_then(|m| m.thermal.as_ref())
                .map_or(1.0, |t| {
                    crate::thermal::heat_wear_multiplier(t.overheat_zone, &content.constants)
                });
            wear_mods.add(crate::modifiers::Modifier::pct_mult(
                crate::modifiers::StatId::WearRate,
                f64::from(heat_multiplier),
                crate::modifiers::ModifierSource::Thermal,
            ));
            let effective_wear = wear_mods.resolve_with_f32(
                crate::modifiers::StatId::WearRate,
                ctx.wear_per_run,
                &state.modifiers,
            );
            apply_wear(
                state,
                &ctx.station_id,
                ctx.module_idx,
                effective_wear,
                events,
            );
            // Invalidate volume cache (inventory may have changed)
            if let Some(station) = state.stations.get_mut(&ctx.station_id) {
                station.invalidate_volume_cache();
            }
        }
        RunOutcome::Skipped {
            reset_timer: should_reset,
        } => {
            if should_reset {
                reset_timer(state, ctx);
            }
            // No wear, no stall changes
        }
        RunOutcome::Stalled(reason) => {
            // Set stall flag, emit stall event on transition
            handle_stall_transition(state, ctx, &reason, events);
            // Reset timer
            reset_timer(state, ctx);
            // No wear
        }
    }
}

#[cfg(test)]
mod framework_tests {
    use super::*;
    use crate::test_fixtures::ModuleDefBuilder;
    use crate::AHashMap;
    use crate::*;
    use std::collections::{HashMap, HashSet};

    fn test_content_with_processor() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_refinery".to_string(),
            ModuleDefBuilder::new("module_refinery")
                .name("Refinery")
                .mass(5000.0)
                .volume(10.0)
                .power(10.0)
                .wear(0.01)
                .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_minutes: 5,
                    processing_interval_ticks: 5,
                    recipes: vec![],
                }))
                .build(),
        );
        content
    }

    fn test_state_with_module(content: &GameContent, kind_state: ModuleKindState) -> GameState {
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
                    core: FacilityCore {
                        inventory: vec![],
                        cargo_capacity_m3: 10_000.0,
                        power_available_per_tick: 100.0,
                        modules: vec![ModuleState {
                            id: ModuleInstanceId("refinery_inst_0001".to_string()),
                            def_id: "module_refinery".to_string(),
                            enabled: true,
                            kind_state,
                            wear: WearState::default(),
                            power_stalled: false,
                            module_priority: 0,
                            assigned_crew: Default::default(),
                            efficiency: 1.0,
                            prev_crew_satisfied: true,
                            thermal: None,
                        }],
                        modifiers: crate::modifiers::ModifierSet::default(),
                        crew: Default::default(),
                        thermal_links: Vec::new(),
                        power: PowerState::default(),
                        cached_inventory_volume_m3: None,
                        module_type_index: crate::ModuleTypeIndex::default(),
                        module_id_index: HashMap::new(),
                        power_budget_cache: crate::PowerBudgetCache::default(),
                    },
                    leaders: Vec::new(),
                },
            )]
            .into_iter()
            .collect(),
            ground_facilities: std::collections::BTreeMap::new(),
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
            progression: Default::default(),
            body_cache: AHashMap::default(),
        }
    }

    #[test]
    fn extract_context_returns_some_for_enabled_processor() {
        let content = test_content_with_processor();
        let state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 3,
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content);
        assert!(ctx.is_some(), "should return context for enabled processor");
        let ctx = ctx.unwrap();
        assert_eq!(ctx.module_idx, 0);
        assert_eq!(ctx.interval, 5);
        assert!((ctx.power_needed - 10.0).abs() < 1e-3);
        assert!((ctx.wear_per_run - 0.01).abs() < 1e-3);
        assert!((ctx.efficiency - 1.0).abs() < 1e-3);
    }

    #[test]
    fn extract_context_returns_none_for_disabled_module() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 0,
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        state.stations.get_mut(&station_id).unwrap().core.modules[0].enabled = false;
        assert!(extract_context(&state, &station_id, 0, &content).is_none());
    }

    #[test]
    fn extract_context_returns_none_for_zero_efficiency() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 0,
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        // Simulate power stall via efficiency (power_stalled folds into efficiency)
        state.stations.get_mut(&station_id).unwrap().core.modules[0].efficiency = 0.0;
        assert!(extract_context(&state, &station_id, 0, &content).is_none());
    }

    #[test]
    fn extract_context_returns_none_for_storage() {
        let content = test_content_with_processor();
        let state = test_state_with_module(&content, ModuleKindState::Storage);
        let station_id = StationId("station_test".to_string());
        let mut content2 = content.clone();
        content2.module_defs.insert(
            "module_refinery".to_string(),
            ModuleDefBuilder::new("module_refinery")
                .name("Storage")
                .mass(1000.0)
                .volume(5.0)
                .behavior(ModuleBehaviorDef::Storage { capacity_m3: 500.0 })
                .build(),
        );
        assert!(extract_context(&state, &station_id, 0, &content2).is_none());
    }

    // ── Task 2: should_run() tests ──────────────────────────────────────

    #[test]
    fn should_run_returns_false_before_interval() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 2, // interval is 5, after increment = 3
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        assert!(!should_run(&mut state, &ctx));
    }

    #[test]
    fn should_run_returns_true_at_interval() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 4, // after increment = 5 = interval
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        assert!(should_run(&mut state, &ctx));
    }

    #[test]
    fn should_run_returns_false_when_insufficient_power() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 4,
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .core
            .power_available_per_tick = 5.0;
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        assert!(!should_run(&mut state, &ctx));
    }

    // ── Task 2: apply_run_result() tests ────────────────────────────────

    #[test]
    fn apply_run_result_completed_resets_timer_and_applies_wear() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 5,
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();
        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Completed,
            &content,
            &mut events,
        );

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.core.modules[0].kind_state {
            assert_eq!(ps.ticks_since_last_run, 0);
        }
        assert!((station.core.modules[0].wear.wear - 0.01).abs() < 1e-6);
        assert!(events
            .iter()
            .any(|e| matches!(&e.event, Event::WearAccumulated { .. })));
    }

    #[test]
    fn apply_run_result_skipped_keep_does_not_reset_timer_or_wear() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 5,
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();
        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Skipped { reset_timer: false },
            &content,
            &mut events,
        );

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.core.modules[0].kind_state {
            assert_eq!(ps.ticks_since_last_run, 5);
        }
        assert!((station.core.modules[0].wear.wear).abs() < 1e-6);
        assert!(events.is_empty());
    }

    #[test]
    fn apply_run_result_skipped_reset_resets_timer_but_no_wear() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 5,
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();
        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Skipped { reset_timer: true },
            &content,
            &mut events,
        );

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.core.modules[0].kind_state {
            assert_eq!(ps.ticks_since_last_run, 0);
        }
        assert!((station.core.modules[0].wear.wear).abs() < 1e-6);
        assert!(events.is_empty());
    }

    // ── Task 3: stall transition tests ──────────────────────────────────

    #[test]
    fn stall_transition_emits_module_stalled_event() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 5,
                stalled: false,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();

        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Stalled(StallReason::VolumeCap { shortfall_m3: 5.0 }),
            &content,
            &mut events,
        );

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.core.modules[0].kind_state {
            assert!(ps.stalled, "should be stalled after VolumeCap");
        }
        assert!(events
            .iter()
            .any(|e| matches!(&e.event, Event::ModuleStalled { .. })));
    }

    #[test]
    fn stall_does_not_re_emit_when_already_stalled() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 5,
                stalled: true,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();

        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Stalled(StallReason::VolumeCap { shortfall_m3: 5.0 }),
            &content,
            &mut events,
        );

        assert!(!events
            .iter()
            .any(|e| matches!(&e.event, Event::ModuleStalled { .. })));
    }

    #[test]
    fn completed_after_stall_emits_resumed_and_clears_flag() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 5,
                stalled: true,
                selected_recipe: None,
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();

        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Completed,
            &content,
            &mut events,
        );

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.core.modules[0].kind_state {
            assert!(!ps.stalled, "should be un-stalled after Completed");
        }
        assert!(events
            .iter()
            .any(|e| matches!(&e.event, Event::ModuleResumed { .. })));
    }

    #[test]
    fn apply_run_result_completed_with_overheat_warning_doubles_wear() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 5,
                stalled: false,
                selected_recipe: None,
            }),
        );
        // Give the module a thermal state in the Warning zone.
        let station_id = StationId("station_test".to_string());
        state.stations.get_mut(&station_id).unwrap().core.modules[0].thermal = Some(ThermalState {
            temp_mk: 300_000,
            thermal_group: None,
            overheat_zone: crate::OverheatZone::Warning,
            overheat_disabled: false,
        });
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();
        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Completed,
            &content,
            &mut events,
        );

        let station = state.stations.get(&station_id).unwrap();
        // wear_per_run = 0.01, warning multiplier = 2.0 → effective wear = 0.02
        let expected_wear = ctx.wear_per_run * content.constants.thermal_wear_multiplier_warning;
        assert!(
            (station.core.modules[0].wear.wear - expected_wear).abs() < 1e-6,
            "expected wear {expected_wear}, got {}",
            station.core.modules[0].wear.wear,
        );
    }
}
