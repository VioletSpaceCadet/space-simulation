use crate::tasks::{
    deep_scan_enabled, inventory_volume_m3, task_duration, task_kind_label, task_target,
};
use crate::trade;
use crate::{
    Command, CommandEnvelope, GameContent, GameState, InventoryItem, ShipId, TaskKind, TaskState,
};
use rand::Rng;

/// Trade (import/export) unlocks after 1 simulated year (365 days × 24 h × 60 min).
pub const TRADE_UNLOCK_TICK: u64 = 525_600;

pub(crate) fn apply_commands(
    state: &mut GameState,
    commands: &[CommandEnvelope],
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Validate and collect assignments first to avoid split borrows.
    let mut assignments: Vec<(ShipId, TaskKind)> = Vec::new();

    for envelope in commands {
        if envelope.execute_at_tick != current_tick {
            continue;
        }
        match &envelope.command {
            Command::AssignShipTask { ship_id, task_kind } => {
                let Some(ship) = state.ships.get(ship_id) else {
                    continue;
                };
                if ship.owner != envelope.issued_by {
                    continue;
                }
                if matches!(task_kind, TaskKind::DeepScan { .. })
                    && !deep_scan_enabled(&state.research, content)
                {
                    continue;
                }
                assignments.push((ship_id.clone(), task_kind.clone()));
            }
            Command::InstallModule {
                station_id,
                module_item_id,
            } => {
                handle_install_module(state, station_id, module_item_id, content, events);
            }
            Command::UninstallModule {
                station_id,
                module_id,
            } => {
                handle_uninstall_module(state, station_id, module_id, events);
            }
            Command::SetModuleEnabled {
                station_id,
                module_id,
                enabled,
            } => {
                handle_set_module_enabled(state, station_id, module_id, *enabled, events);
            }
            Command::SetModuleThreshold {
                station_id,
                module_id,
                threshold_kg,
            } => {
                handle_set_module_threshold(state, station_id, module_id, *threshold_kg, events);
            }
            Command::AssignLabTech {
                station_id,
                module_id,
                tech_id,
            } => {
                handle_assign_lab_tech(state, station_id, module_id, tech_id.as_ref());
            }
            Command::SetAssemblerCap {
                station_id,
                module_id,
                component_id,
                max_stock,
            } => {
                handle_set_assembler_cap(state, station_id, module_id, component_id, *max_stock);
            }
            Command::Import {
                station_id,
                item_spec,
            } => {
                handle_import(state, station_id, item_spec, content, rng, events);
            }
            Command::Export {
                station_id,
                item_spec,
            } => {
                handle_export(state, station_id, item_spec, content, events);
            }
            Command::JettisonSlag { station_id } => {
                handle_jettison_slag(state, station_id, events);
            }
        }
    }

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

fn handle_install_module(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_item_id: &crate::ModuleItemId,
    content: &GameContent,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    let item_pos = station.inventory.iter().position(
        |i| matches!(i, InventoryItem::Module { item_id, .. } if item_id == module_item_id),
    );
    let Some(pos) = item_pos else { return };
    let InventoryItem::Module {
        item_id,
        module_def_id,
    } = station.inventory.remove(pos)
    else {
        return;
    };

    let module_id_str = format!("module_inst_{:04}", state.counters.next_module_instance_id);
    state.counters.next_module_instance_id += 1;
    let module_id = crate::ModuleInstanceId(module_id_str);

    let kind_state = match content.module_defs.iter().find(|d| d.id == module_def_id) {
        Some(def) => match &def.behavior {
            crate::ModuleBehaviorDef::Processor(_) => {
                crate::ModuleKindState::Processor(crate::ProcessorState {
                    threshold_kg: 0.0,
                    ticks_since_last_run: 0,
                    stalled: false,
                })
            }
            crate::ModuleBehaviorDef::Storage { .. } => crate::ModuleKindState::Storage,
            crate::ModuleBehaviorDef::Maintenance(_) => {
                crate::ModuleKindState::Maintenance(crate::MaintenanceState {
                    ticks_since_last_run: 0,
                })
            }
            crate::ModuleBehaviorDef::Assembler(_) => {
                crate::ModuleKindState::Assembler(crate::AssemblerState {
                    ticks_since_last_run: 0,
                    stalled: false,
                    capped: false,
                    cap_override: std::collections::HashMap::new(),
                })
            }
            crate::ModuleBehaviorDef::Lab(_) => crate::ModuleKindState::Lab(crate::LabState {
                ticks_since_last_run: 0,
                assigned_tech: None,
                starved: false,
            }),
            crate::ModuleBehaviorDef::SensorArray(_) => {
                crate::ModuleKindState::SensorArray(crate::SensorArrayState::default())
            }
        },
        None => return,
    };

    let station = state.stations.get_mut(station_id).unwrap();
    station.modules.push(crate::ModuleState {
        id: module_id.clone(),
        def_id: module_def_id.clone(),
        enabled: false,
        kind_state,
        wear: crate::WearState::default(),
    });

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleInstalled {
            station_id: station_id.clone(),
            module_id,
            module_item_id: item_id,
            module_def_id,
        },
    ));
}

fn handle_uninstall_module(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    let pos = station.modules.iter().position(|m| &m.id == module_id);
    let Some(pos) = pos else { return };
    let module = station.modules.remove(pos);

    let item_id = crate::ModuleItemId(format!(
        "module_item_{:04}",
        state.counters.next_module_instance_id
    ));
    state.counters.next_module_instance_id += 1;

    let station = state.stations.get_mut(station_id).unwrap();
    station.inventory.push(InventoryItem::Module {
        item_id: item_id.clone(),
        module_def_id: module.def_id.clone(),
    });

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        crate::Event::ModuleUninstalled {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            module_item_id: item_id,
        },
    ));
}

fn handle_set_module_enabled(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    enabled: bool,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return;
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
}

fn handle_set_module_threshold(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    threshold_kg: f32,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return;
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
}

fn handle_assign_lab_tech(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    tech_id: Option<&crate::TechId>,
) {
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return;
    };
    if let crate::ModuleKindState::Lab(ls) = &mut module.kind_state {
        ls.assigned_tech = tech_id.cloned();
    }
}

fn handle_set_assembler_cap(
    state: &mut GameState,
    station_id: &crate::StationId,
    module_id: &crate::ModuleInstanceId,
    component_id: &crate::ComponentId,
    max_stock: u32,
) {
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
    };
    let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
        return;
    };
    if let crate::ModuleKindState::Assembler(asmb) = &mut module.kind_state {
        asmb.cap_override.insert(component_id.clone(), max_stock);
    }
}

fn handle_import(
    state: &mut GameState,
    station_id: &crate::StationId,
    item_spec: &crate::TradeItemSpec,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    if current_tick < TRADE_UNLOCK_TICK {
        return;
    }
    let Some(station) = state.stations.get(station_id) else {
        return;
    };

    // Look up pricing and compute cost
    let Some(cost) = trade::compute_import_cost(item_spec, &content.pricing, content) else {
        return; // not importable or unknown item
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
        return;
    }

    // Check cargo capacity
    let new_items = trade::create_inventory_items(item_spec, rng);
    let new_volume = inventory_volume_m3(&new_items, content);
    let current_volume = inventory_volume_m3(&station.inventory, content);
    if current_volume + new_volume > station.cargo_capacity_m3 {
        return; // no room
    }

    // Execute import
    state.balance -= cost;
    let station = state.stations.get_mut(station_id).unwrap();
    trade::merge_into_inventory(&mut station.inventory, new_items);

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
}

fn handle_export(
    state: &mut GameState,
    station_id: &crate::StationId,
    item_spec: &crate::TradeItemSpec,
    content: &GameContent,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    if current_tick < TRADE_UNLOCK_TICK {
        return;
    }
    let Some(station) = state.stations.get(station_id) else {
        return;
    };

    // Look up pricing and compute revenue
    let Some(revenue) = trade::compute_export_revenue(item_spec, &content.pricing, content) else {
        return; // not exportable or unknown item
    };

    // Check station has items
    if !trade::has_enough_for_export(&station.inventory, item_spec) {
        return;
    }

    // Execute export
    let station = state.stations.get_mut(station_id).unwrap();
    if !trade::remove_inventory_items(&mut station.inventory, item_spec) {
        return;
    }
    state.balance += revenue;

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
}

fn handle_jettison_slag(
    state: &mut GameState,
    station_id: &crate::StationId,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let Some(station) = state.stations.get_mut(station_id) else {
        return;
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
}
