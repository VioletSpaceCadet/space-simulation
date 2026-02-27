mod assembler;
mod lab;
mod maintenance;
mod processor;
mod sensor;

use crate::{
    tasks::element_density, Event, EventEnvelope, GameContent, GameState, InputFilter,
    InventoryItem, ItemKind, OutputSpec, RecipeDef, StationId, YieldFormula,
};
use std::collections::HashMap;

/// Minimum meaningful mass — amounts below this are discarded as rounding noise.
const MIN_MEANINGFUL_KG: f32 = 1e-3;

/// Tech ID required for ship construction recipes.
const TECH_SHIP_CONSTRUCTION: &str = "tech_ship_construction";

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
            OutputSpec::Component { .. } | OutputSpec::Ship { .. } => {}
        }
    }
    total_volume
}

fn matches_input_filter(item: &InventoryItem, filter: Option<&InputFilter>) -> bool {
    match filter {
        Some(InputFilter::ItemKind(ItemKind::Ore)) => matches!(item, InventoryItem::Ore { .. }),
        Some(InputFilter::ItemKind(ItemKind::Material)) => {
            matches!(item, InventoryItem::Material { .. })
        }
        Some(InputFilter::ItemKind(ItemKind::Slag)) => matches!(item, InventoryItem::Slag { .. }),
        Some(InputFilter::Element(el)) => {
            matches!(item, InventoryItem::Material { element, .. } if element == el)
        }
        _ => false,
    }
}

pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        compute_power_budget(state, station_id, content);
        processor::tick_station_modules(state, station_id, content, events);
        assembler::tick_assembler_modules(state, station_id, content, rng, events);
        sensor::tick_sensor_array_modules(state, station_id, content, events);
        lab::tick_lab_modules(state, station_id, content, events);
        maintenance::tick_maintenance_modules(state, station_id, content, events);
    }
}

/// Returns the power priority for a module behavior. Lower = stalled first.
/// Priority (highest first): Maintenance(4) > Processor(3) > Assembler(2) > Lab(1) > SensorArray(0).
/// Solar arrays and storage are never stalled (they generate power or are passive).
fn power_priority(behavior: &crate::ModuleBehaviorDef) -> Option<u8> {
    match behavior {
        crate::ModuleBehaviorDef::SensorArray(_) => Some(0),
        crate::ModuleBehaviorDef::Lab(_) => Some(1),
        crate::ModuleBehaviorDef::Assembler(_) => Some(2),
        crate::ModuleBehaviorDef::Processor(_) => Some(3),
        crate::ModuleBehaviorDef::Maintenance(_) => Some(4),
        crate::ModuleBehaviorDef::SolarArray(_)
        | crate::ModuleBehaviorDef::Storage { .. }
        | crate::ModuleBehaviorDef::Battery(_) => None,
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
        for (idx, battery_def, current_charge, _efficiency) in batteries {
            if remaining <= 0.0 {
                break;
            }
            let available = current_charge.min(battery_def.discharge_rate_kw);
            let discharge = available.min(remaining);
            remaining -= discharge;
            discharge_kw += discharge;

            let station = state.stations.get_mut(station_id).unwrap();
            if let crate::ModuleKindState::Battery(ref mut bs) = station.modules[*idx].kind_state {
                bs.charge_kwh -= discharge;
            }
        }
    } else if raw_surplus > 0.0 {
        let mut remaining = raw_surplus;
        for (idx, battery_def, current_charge, efficiency) in batteries {
            if remaining <= 0.0 {
                break;
            }
            let effective_capacity = battery_def.capacity_kwh * efficiency;
            let headroom = (effective_capacity - current_charge).max(0.0);
            let charge = headroom.min(battery_def.charge_rate_kw).min(remaining);
            remaining -= charge;
            charge_kw += charge;

            let station = state.stations.get_mut(station_id).unwrap();
            if let crate::ModuleKindState::Battery(ref mut bs) = station.modules[*idx].kind_state {
                bs.charge_kwh += charge;
            }
        }
    }

    // Sum total stored energy across all batteries after updates.
    let mut stored_kwh = 0.0_f32;
    let station = state.stations.get(station_id).unwrap();
    for module in &station.modules {
        if let crate::ModuleKindState::Battery(ref bs) = module.kind_state {
            stored_kwh += bs.charge_kwh;
        }
    }

    (discharge_kw, charge_kw, stored_kwh)
}

/// Compute the power budget for a station, store it in `PowerState`, and
/// mark modules as `power_stalled` when there is a deficit.
///
/// Generated power = sum of all enabled solar arrays:
///   `base_output_kw` * `solar_intensity` * `wear_efficiency`
///
/// Consumed power = sum of `power_consumption_per_run` for all enabled modules.
///
/// Batteries buffer power: surplus charges them, deficit discharges them.
/// Wear reduces effective battery capacity.
/// Modules are only stalled when batteries cannot cover the remaining deficit.
fn compute_power_budget(state: &mut GameState, station_id: &StationId, content: &GameContent) {
    let Some(station) = state.stations.get(station_id) else {
        return;
    };

    let solar_intensity = content
        .solar_system
        .nodes
        .iter()
        .find(|n| n.id == station.location_node)
        .map_or(1.0, |n| n.solar_intensity);

    let mut generated_kw = 0.0_f32;
    let mut consumed_kw = 0.0_f32;
    let mut has_power_infrastructure = false;
    let mut consumers: Vec<(usize, u8, f32)> = Vec::new();
    let mut batteries: Vec<(usize, crate::BatteryDef, f32, f32)> = Vec::new();

    for (idx, module) in station.modules.iter().enumerate() {
        if !module.enabled {
            continue;
        }
        let Some(def) = content.module_defs.get(&module.def_id) else {
            continue;
        };

        match &def.behavior {
            crate::ModuleBehaviorDef::SolarArray(solar_def) => {
                has_power_infrastructure = true;
                let efficiency = crate::wear::wear_efficiency(module.wear.wear, &content.constants);
                generated_kw += solar_def.base_output_kw * solar_intensity * efficiency;
                consumed_kw += def.power_consumption_per_run;
            }
            crate::ModuleBehaviorDef::Battery(battery_def) => {
                has_power_infrastructure = true;
                let efficiency = crate::wear::wear_efficiency(module.wear.wear, &content.constants);
                let current_charge =
                    if let crate::ModuleKindState::Battery(ref bs) = module.kind_state {
                        bs.charge_kwh
                    } else {
                        0.0
                    };
                batteries.push((idx, battery_def.clone(), current_charge, efficiency));
                consumed_kw += def.power_consumption_per_run;
            }
            _ => {
                consumed_kw += def.power_consumption_per_run;
                if let Some(priority) = power_priority(&def.behavior) {
                    consumers.push((idx, priority, def.power_consumption_per_run));
                }
            }
        }
    }

    let raw_surplus = (generated_kw - consumed_kw).max(0.0);
    let raw_deficit = (consumed_kw - generated_kw).max(0.0);

    let (battery_discharge_kw, battery_charge_kw, battery_stored_kwh) =
        apply_battery_buffering(state, station_id, &batteries, raw_surplus, raw_deficit);

    let deficit_kw = (raw_deficit - battery_discharge_kw).max(0.0);

    // Reset all power_stalled flags first.
    let station = state.stations.get_mut(station_id).unwrap();
    for module in &mut station.modules {
        module.power_stalled = false;
    }

    // Stall lowest-priority modules until budget balances.
    if deficit_kw > 0.0 && has_power_infrastructure {
        consumers.sort_by_key(|&(_, priority, _)| priority);
        let mut remaining_deficit = deficit_kw;
        for (idx, _, consumption) in &consumers {
            if remaining_deficit <= 0.0 {
                break;
            }
            station.modules[*idx].power_stalled = true;
            remaining_deficit -= consumption;
        }
    }

    station.power = crate::PowerState {
        generated_kw,
        consumed_kw,
        deficit_kw,
        battery_discharge_kw,
        battery_charge_kw,
        battery_stored_kwh,
    };
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
        let module = &mut station.modules[module_idx];
        let wear_before = module.wear.wear;
        module.wear.wear = (module.wear.wear + wear_per_run).min(1.0);
        let wear_after = module.wear.wear;

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::WearAccumulated {
                station_id: station_id.clone(),
                module_id: module.id.clone(),
                wear_before,
                wear_after,
            },
        ));
        if module.wear.wear >= 1.0 {
            let mid = module.id.clone();
            module.enabled = false;
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::ModuleAutoDisabled {
                    station_id: station_id.clone(),
                    module_id: mid,
                },
            ));
        }
    }
}
