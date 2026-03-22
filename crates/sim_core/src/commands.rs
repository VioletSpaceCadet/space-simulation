//! Command handler functions for `apply_commands`.
//!
//! Each public function handles one `Command` variant. Handlers return `false`
//! when the command should be skipped (invalid target, insufficient resources,
//! etc.) — the caller uses this to `continue` the command loop.

use crate::tasks::{
    deep_scan_enabled, inventory_volume_m3, task_duration, task_kind_label, task_target,
};
use crate::{
    trade, EventEnvelope, GameContent, GameState, InventoryItem, ShipId, TaskKind, TaskState,
};
use rand::Rng;

use crate::engine::trade_unlock_tick;

/// Validate an `AssignShipTask` command and collect it into the assignments vec
/// for deferred processing. Returns `false` if the command should be skipped.
pub(crate) fn handle_assign_ship_task(
    state: &GameState,
    content: &GameContent,
    ship_id: &ShipId,
    task_kind: &TaskKind,
    issued_by: &crate::PrincipalId,
    assignments: &mut Vec<(ShipId, TaskKind)>,
) -> bool {
    let Some(ship) = state.ships.get(ship_id) else {
        return false;
    };
    if ship.owner != *issued_by {
        return false;
    }
    if matches!(task_kind, TaskKind::DeepScan { .. })
        && !deep_scan_enabled(&state.research, content)
    {
        return false;
    }
    assignments.push((ship_id.clone(), task_kind.clone()));
    true
}

/// Build the default `ModuleKindState` and `BehaviorType` for a module definition.
fn default_module_state(
    def: &crate::ModuleDef,
    content: &GameContent,
) -> (
    crate::ModuleKindState,
    crate::BehaviorType,
    Option<crate::ThermalState>,
) {
    let thermal_state = def.thermal.as_ref().map(|td| crate::ThermalState {
        temp_mk: content.constants.thermal_sink_temp_mk,
        thermal_group: td.thermal_group.clone(),
        ..Default::default()
    });
    let (kind_state, behavior_type) = match &def.behavior {
        crate::ModuleBehaviorDef::Processor(_) => (
            crate::ModuleKindState::Processor(crate::ProcessorState {
                threshold_kg: 0.0,
                ticks_since_last_run: 0,
                stalled: false,
            }),
            crate::BehaviorType::Processor,
        ),
        crate::ModuleBehaviorDef::Storage { .. } => (
            crate::ModuleKindState::Storage,
            crate::BehaviorType::Storage,
        ),
        crate::ModuleBehaviorDef::Maintenance(_) => (
            crate::ModuleKindState::Maintenance(crate::MaintenanceState {
                ticks_since_last_run: 0,
            }),
            crate::BehaviorType::Maintenance,
        ),
        crate::ModuleBehaviorDef::Assembler(_) => (
            crate::ModuleKindState::Assembler(crate::AssemblerState {
                ticks_since_last_run: 0,
                stalled: false,
                capped: false,
                cap_override: std::collections::HashMap::new(),
            }),
            crate::BehaviorType::Assembler,
        ),
        crate::ModuleBehaviorDef::Lab(_) => (
            crate::ModuleKindState::Lab(crate::LabState {
                ticks_since_last_run: 0,
                assigned_tech: None,
                starved: false,
            }),
            crate::BehaviorType::Lab,
        ),
        crate::ModuleBehaviorDef::SensorArray(_) => (
            crate::ModuleKindState::SensorArray(crate::SensorArrayState::default()),
            crate::BehaviorType::SensorArray,
        ),
        crate::ModuleBehaviorDef::SolarArray(_) => (
            crate::ModuleKindState::SolarArray(crate::SolarArrayState::default()),
            crate::BehaviorType::SolarArray,
        ),
        crate::ModuleBehaviorDef::Battery(_) => (
            crate::ModuleKindState::Battery(crate::BatteryState { charge_kwh: 0.0 }),
            crate::BehaviorType::Battery,
        ),
        crate::ModuleBehaviorDef::Radiator(_) => (
            crate::ModuleKindState::Radiator(crate::RadiatorState::default()),
            crate::BehaviorType::Radiator,
        ),
    };
    (kind_state, behavior_type, thermal_state)
}

/// Install a module from station inventory into the station's active modules.
pub(crate) fn handle_install_module(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    module_item_id: &crate::ModuleItemId,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let item_pos = station.inventory.iter().position(
        |i| matches!(i, InventoryItem::Module { item_id, .. } if item_id == module_item_id),
    );
    let Some(pos) = item_pos else { return false };
    let InventoryItem::Module {
        item_id,
        module_def_id,
    } = station.inventory.remove(pos)
    else {
        return false;
    };
    station.invalidate_volume_cache();

    let module_id_str = format!("module_inst_{:04}", state.counters.next_module_instance_id);
    state.counters.next_module_instance_id += 1;
    let module_id = crate::ModuleInstanceId(module_id_str);

    let Some(def) = content.module_defs.get(&module_def_id) else {
        return false;
    };
    let (kind_state, behavior_type, thermal) = default_module_state(def, content);

    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.modules.push(crate::ModuleState {
        id: module_id.clone(),
        def_id: module_def_id.clone(),
        enabled: false,
        kind_state,
        wear: crate::WearState::default(),
        thermal,
        power_stalled: false,
    });

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleInstalled {
            station_id: station_id.clone(),
            module_id,
            module_item_id: item_id,
            module_def_id,
            behavior_type,
        },
    ));
    true
}

/// Uninstall a module from the station and return it to inventory.
pub(crate) fn handle_uninstall_module(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let pos = station.modules.iter().position(|m| &m.id == module_id);
    let Some(pos) = pos else { return false };
    let module = station.modules.remove(pos);

    let item_id = crate::ModuleItemId(format!(
        "module_item_{:04}",
        state.counters.next_module_instance_id
    ));
    state.counters.next_module_instance_id += 1;

    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    station.inventory.push(InventoryItem::Module {
        item_id: item_id.clone(),
        module_def_id: module.def_id.clone(),
    });
    station.invalidate_volume_cache();

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleUninstalled {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            module_item_id: item_id,
        },
    ));
    true
}

/// Toggle the enabled flag on a module.
pub(crate) fn handle_set_module_enabled(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    enabled: bool,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    module.enabled = enabled;
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleToggled {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            enabled,
        },
    ));
    true
}

/// Set the `threshold_kg` on a processor module.
pub(crate) fn handle_set_module_threshold(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    threshold_kg: f32,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    if let crate::ModuleKindState::Processor(ps) = &mut module.kind_state {
        ps.threshold_kg = threshold_kg;
    }
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleThresholdSet {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            threshold_kg,
        },
    ));
    true
}

/// Assign a tech to a lab module.
pub(crate) fn handle_assign_lab_tech(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    tech_id: Option<&crate::TechId>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    if let crate::ModuleKindState::Lab(ls) = &mut module.kind_state {
        ls.assigned_tech = tech_id.cloned();
    }
    true
}

/// Set the cap override on an assembler module.
pub(crate) fn handle_set_assembler_cap(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    component_id: &crate::ComponentId,
    max_stock: u32,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return false;
    };
    if let crate::ModuleKindState::Assembler(asmb) = &mut module.kind_state {
        asmb.cap_override.insert(component_id.clone(), max_stock);
    }
    true
}

/// Import items into a station via trade.
pub(crate) fn handle_import(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    item_spec: &crate::TradeItemSpec,
    current_tick: u64,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    if current_tick < trade_unlock_tick(content.constants.minutes_per_tick) {
        return false;
    }
    if !state.stations.contains_key(station_id) {
        return false;
    }

    // Look up pricing and compute cost
    let Some(cost) = trade::compute_import_cost(item_spec, &content.pricing, content) else {
        return false; // not importable or unknown item
    };

    // Check balance
    if state.balance < cost {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::InsufficientFunds {
                station_id: station_id.clone(),
                action: format!("import {}", trade::pricing_key(item_spec)),
                required: cost,
                available: state.balance,
            },
        ));
        return false;
    }

    // Check cargo capacity
    let new_items = trade::create_inventory_items(item_spec, rng);
    let new_volume = inventory_volume_m3(&new_items, content);
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let current_volume = station.used_volume_m3(content);
    let cargo_cap = station.cargo_capacity_m3;
    if current_volume + new_volume > cargo_cap {
        return false; // no room
    }

    // Execute import
    state.balance -= cost;
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    trade::merge_into_inventory(&mut station.inventory, new_items);
    station.invalidate_volume_cache();

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ItemImported {
            station_id: station_id.clone(),
            item_spec: item_spec.clone(),
            cost,
            balance_after: state.balance,
        },
    ));
    true
}

/// Export items from a station via trade.
pub(crate) fn handle_export(
    state: &mut GameState,
    content: &GameContent,
    station_id: &crate::StationId,
    item_spec: &crate::TradeItemSpec,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    if current_tick < trade_unlock_tick(content.constants.minutes_per_tick) {
        return false;
    }
    let Some(station) = state.stations.get(station_id) else {
        return false;
    };

    // Look up pricing and compute revenue
    let Some(revenue) = trade::compute_export_revenue(item_spec, &content.pricing, content) else {
        return false; // not exportable or unknown item
    };

    // Check station has items
    if !trade::has_enough_for_export(&station.inventory, item_spec) {
        return false;
    }

    // Execute export
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    if !trade::remove_inventory_items(&mut station.inventory, item_spec) {
        return false;
    }
    station.invalidate_volume_cache();
    state.balance += revenue;
    state.export_revenue_total += revenue;
    state.export_count += 1;

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ItemExported {
            station_id: station_id.clone(),
            item_spec: item_spec.clone(),
            revenue,
            balance_after: state.balance,
        },
    ));
    true
}

/// Jettison all slag from a station's inventory.
pub(crate) fn handle_jettison_slag(
    state: &mut GameState,
    station_id: &crate::StationId,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) -> bool {
    let Some(station) = state.stations.get_mut(station_id) else {
        return false;
    };
    let jettisoned_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Slag { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    station
        .inventory
        .retain(|i| !matches!(i, InventoryItem::Slag { .. }));
    station.invalidate_volume_cache();
    if jettisoned_kg > 0.0 {
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::SlagJettisoned {
                station_id: station_id.clone(),
                kg: jettisoned_kg,
            },
        ));
    }
    true
}

/// Apply deferred ship task assignments collected during the command loop.
pub(crate) fn apply_ship_assignments(
    state: &mut GameState,
    content: &GameContent,
    assignments: Vec<(ShipId, TaskKind)>,
    current_tick: u64,
    events: &mut Vec<EventEnvelope>,
) {
    for (ship_id, task_kind) in assignments {
        let duration = task_duration(&task_kind, &content.constants);
        let label = task_kind_label(&task_kind).to_string();
        let target = task_target(&task_kind);

        if let Some(ship) = state.ships.get_mut(&ship_id) {
            ship.task = Some(TaskState {
                kind: task_kind,
                started_tick: current_tick,
                eta_tick: current_tick + duration,
            });
        }

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::TaskStarted {
                ship_id,
                task_kind: label,
                target,
            },
        ));
    }
}
