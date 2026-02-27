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
        compute_power_budget(state, station_id, content, events);
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
fn compute_power_budget(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
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
    let mut wear_targets: Vec<(usize, f32)> = Vec::new();

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
                if def.wear_per_run > 0.0 {
                    wear_targets.push((idx, def.wear_per_run));
                }
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

    // Apply wear to solar arrays (and any other power infrastructure with wear).
    for (module_idx, wear_per_run) in wear_targets {
        apply_wear(state, station_id, module_idx, wear_per_run, events);
    }
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
    VolumeCap { shortfall_m3: f32 },
    StockCap,
    DataStarved,
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
    let module = &station.modules[module_idx];

    if !module.enabled || module.power_stalled {
        return None;
    }

    let def = content.module_defs.get(&module.def_id)?;

    let interval = match &def.behavior {
        crate::ModuleBehaviorDef::Processor(p) => p.processing_interval_ticks,
        crate::ModuleBehaviorDef::Assembler(a) => a.assembly_interval_ticks,
        crate::ModuleBehaviorDef::SensorArray(s) => s.scan_interval_ticks,
        crate::ModuleBehaviorDef::Lab(l) => l.research_interval_ticks,
        crate::ModuleBehaviorDef::Maintenance(m) => m.repair_interval_ticks,
        crate::ModuleBehaviorDef::Storage { .. }
        | crate::ModuleBehaviorDef::SolarArray(_)
        | crate::ModuleBehaviorDef::Battery(_) => return None,
    };

    let efficiency = crate::wear::wear_efficiency(module.wear.wear, &content.constants);

    Some(ModuleTickContext {
        station_id: station_id.clone(),
        module_idx,
        module_id: module.id.clone(),
        def,
        interval,
        power_needed: def.power_consumption_per_run,
        wear_per_run: def.wear_per_run,
        efficiency,
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

/// Increment timer and check if the module should run this tick.
/// Returns true if: timer >= interval AND station has enough power.
fn should_run(state: &mut GameState, ctx: &ModuleTickContext) -> bool {
    let ticks = {
        let Some(station) = state.stations.get_mut(&ctx.station_id) else {
            return false;
        };
        let module = &mut station.modules[ctx.module_idx];
        match &mut module.kind_state {
            crate::ModuleKindState::Processor(s) => {
                s.ticks_since_last_run += 1;
                s.ticks_since_last_run
            }
            crate::ModuleKindState::Assembler(s) => {
                s.ticks_since_last_run += 1;
                s.ticks_since_last_run
            }
            crate::ModuleKindState::SensorArray(s) => {
                s.ticks_since_last_run += 1;
                s.ticks_since_last_run
            }
            crate::ModuleKindState::Lab(s) => {
                s.ticks_since_last_run += 1;
                s.ticks_since_last_run
            }
            crate::ModuleKindState::Maintenance(s) => {
                s.ticks_since_last_run += 1;
                s.ticks_since_last_run
            }
            _ => return false,
        }
    };

    if ticks < ctx.interval {
        return false;
    }

    let Some(station) = state.stations.get(&ctx.station_id) else {
        return false;
    };
    station.power_available_per_tick >= ctx.power_needed
}

/// Reset the `ticks_since_last_run` to 0 for any module kind.
fn reset_timer(state: &mut GameState, ctx: &ModuleTickContext) {
    let Some(station) = state.stations.get_mut(&ctx.station_id) else {
        return;
    };
    let module = &mut station.modules[ctx.module_idx];
    match &mut module.kind_state {
        crate::ModuleKindState::Processor(s) => s.ticks_since_last_run = 0,
        crate::ModuleKindState::Assembler(s) => s.ticks_since_last_run = 0,
        crate::ModuleKindState::SensorArray(s) => s.ticks_since_last_run = 0,
        crate::ModuleKindState::Lab(s) => s.ticks_since_last_run = 0,
        crate::ModuleKindState::Maintenance(s) => s.ticks_since_last_run = 0,
        _ => {}
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
        let module = &mut station.modules[ctx.module_idx];

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
        let module = &mut station.modules[ctx.module_idx];

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
    events: &mut Vec<EventEnvelope>,
) {
    match outcome {
        RunOutcome::Completed => {
            // Clear stall flag if was stalled, emit resume event
            handle_resume_if_stalled(state, ctx, events);
            // Reset timer
            reset_timer(state, ctx);
            // Apply wear
            apply_wear(
                state,
                &ctx.station_id,
                ctx.module_idx,
                ctx.wear_per_run,
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
    use crate::*;
    use std::collections::{HashMap, HashSet};

    fn test_content_with_processor() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_refinery".to_string(),
            ModuleDef {
                id: "module_refinery".to_string(),
                name: "Refinery".to_string(),
                mass_kg: 5000.0,
                volume_m3: 10.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_minutes: 5,
                    processing_interval_ticks: 5,
                    recipes: vec![],
                }),
            },
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
                        id: ModuleInstanceId("refinery_inst_0001".to_string()),
                        def_id: "module_refinery".to_string(),
                        enabled: true,
                        kind_state,
                        wear: WearState::default(),
                        power_stalled: false,
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
    fn extract_context_returns_some_for_enabled_processor() {
        let content = test_content_with_processor();
        let state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 3,
                stalled: false,
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
            }),
        );
        let station_id = StationId("station_test".to_string());
        state.stations.get_mut(&station_id).unwrap().modules[0].enabled = false;
        assert!(extract_context(&state, &station_id, 0, &content).is_none());
    }

    #[test]
    fn extract_context_returns_none_for_power_stalled() {
        let content = test_content_with_processor();
        let mut state = test_state_with_module(
            &content,
            ModuleKindState::Processor(ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 0,
                stalled: false,
            }),
        );
        let station_id = StationId("station_test".to_string());
        state.stations.get_mut(&station_id).unwrap().modules[0].power_stalled = true;
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
            ModuleDef {
                id: "module_refinery".to_string(),
                name: "Storage".to_string(),
                mass_kg: 1000.0,
                volume_m3: 5.0,
                power_consumption_per_run: 0.0,
                wear_per_run: 0.0,
                behavior: ModuleBehaviorDef::Storage { capacity_m3: 500.0 },
            },
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
            }),
        );
        let station_id = StationId("station_test".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
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
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();
        apply_run_result(&mut state, &ctx, RunOutcome::Completed, &mut events);

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
            assert_eq!(ps.ticks_since_last_run, 0);
        }
        assert!((station.modules[0].wear.wear - 0.01).abs() < 1e-6);
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
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();
        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Skipped { reset_timer: false },
            &mut events,
        );

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
            assert_eq!(ps.ticks_since_last_run, 5);
        }
        assert!((station.modules[0].wear.wear).abs() < 1e-6);
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
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();
        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Skipped { reset_timer: true },
            &mut events,
        );

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
            assert_eq!(ps.ticks_since_last_run, 0);
        }
        assert!((station.modules[0].wear.wear).abs() < 1e-6);
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
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();

        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Stalled(StallReason::VolumeCap { shortfall_m3: 5.0 }),
            &mut events,
        );

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
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
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();

        apply_run_result(
            &mut state,
            &ctx,
            RunOutcome::Stalled(StallReason::VolumeCap { shortfall_m3: 5.0 }),
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
            }),
        );
        let station_id = StationId("station_test".to_string());
        let ctx = extract_context(&state, &station_id, 0, &content).unwrap();
        let mut events = Vec::new();

        apply_run_result(&mut state, &ctx, RunOutcome::Completed, &mut events);

        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
            assert!(!ps.stalled, "should be un-stalled after Completed");
        }
        assert!(events
            .iter()
            .any(|e| matches!(&e.event, Event::ModuleResumed { .. })));
    }
}
