use crate::research::advance_research;
use crate::station::tick_stations;
use crate::tasks::{
    deep_scan_enabled, inventory_volume_m3, resolve_deep_scan, resolve_deposit, resolve_mine,
    resolve_survey, resolve_transit, task_duration, task_kind_label, task_target,
};
use crate::{
    trade, Command, CommandEnvelope, EventLevel, GameContent, GameState, InventoryItem, NodeId,
    ScanSite, ShipId, SiteId, TaskKind, TaskState,
};
use rand::Rng;

/// Trade (import/export) unlocks after 1 simulated year (365 days × 24 h × 60 min).
pub const TRADE_UNLOCK_TICK: u64 = 525_600;

const MIN_UNSCANNED_SITES: usize = 5;
const REPLENISH_BATCH_SIZE: usize = 5;

/// Advance the simulation by one tick.
///
/// Order of operations:
/// 1. Apply commands scheduled for this tick.
/// 2. Resolve ship tasks whose eta has arrived.
/// 3. Tick station modules (refinery processors).
/// 4. Advance station research on all eligible techs.
/// 5. Replenish scan sites if below threshold.
/// 6. Increment tick counter.
///
/// Returns all events produced this tick.
pub fn tick(
    state: &mut GameState,
    commands: &[CommandEnvelope],
    content: &GameContent,
    rng: &mut impl Rng,
    event_level: EventLevel,
) -> Vec<crate::EventEnvelope> {
    let mut events = Vec::new();

    apply_commands(state, commands, content, rng, &mut events);
    resolve_ship_tasks(state, content, rng, &mut events);
    tick_stations(state, content, rng, &mut events);
    advance_research(state, content, rng, event_level, &mut events);
    replenish_scan_sites(state, content, rng, &mut events);

    state.meta.tick += 1;
    events
}

#[allow(clippy::too_many_lines)]
fn apply_commands(
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
                let Some(station) = state.stations.get_mut(station_id) else {
                    continue;
                };
                let item_pos = station.inventory.iter().position(|i| {
                    matches!(i, InventoryItem::Module { item_id, .. } if item_id == module_item_id)
                });
                let Some(pos) = item_pos else { continue };
                let InventoryItem::Module {
                    item_id,
                    module_def_id,
                } = station.inventory.remove(pos)
                else {
                    continue;
                };
                station.invalidate_volume_cache();

                let module_id_str =
                    format!("module_inst_{:04}", state.counters.next_module_instance_id);
                state.counters.next_module_instance_id += 1;
                let module_id = crate::ModuleInstanceId(module_id_str);

                let kind_state = match content.module_defs.get(&module_def_id) {
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
                        crate::ModuleBehaviorDef::Lab(_) => {
                            crate::ModuleKindState::Lab(crate::LabState {
                                ticks_since_last_run: 0,
                                assigned_tech: None,
                                starved: false,
                            })
                        }
                        crate::ModuleBehaviorDef::SensorArray(_) => {
                            crate::ModuleKindState::SensorArray(crate::SensorArrayState::default())
                        }
                        crate::ModuleBehaviorDef::SolarArray(_) => {
                            crate::ModuleKindState::SolarArray(crate::SolarArrayState::default())
                        }
                    },
                    None => continue,
                };

                let station = state.stations.get_mut(station_id).unwrap();
                station.modules.push(crate::ModuleState {
                    id: module_id.clone(),
                    def_id: module_def_id.clone(),
                    enabled: false,
                    kind_state,
                    wear: crate::WearState::default(),
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
                    },
                ));
            }
            Command::UninstallModule {
                station_id,
                module_id,
            } => {
                let Some(station) = state.stations.get_mut(station_id) else {
                    continue;
                };
                let pos = station.modules.iter().position(|m| &m.id == module_id);
                let Some(pos) = pos else { continue };
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
            }
            Command::SetModuleEnabled {
                station_id,
                module_id,
                enabled,
            } => {
                let Some(station) = state.stations.get_mut(station_id) else {
                    continue;
                };
                let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
                    continue;
                };
                module.enabled = *enabled;
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    crate::Event::ModuleToggled {
                        station_id: station_id.clone(),
                        module_id: module_id.clone(),
                        enabled: *enabled,
                    },
                ));
            }
            Command::SetModuleThreshold {
                station_id,
                module_id,
                threshold_kg,
            } => {
                let Some(station) = state.stations.get_mut(station_id) else {
                    continue;
                };
                let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
                    continue;
                };
                if let crate::ModuleKindState::Processor(ps) = &mut module.kind_state {
                    ps.threshold_kg = *threshold_kg;
                }
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    crate::Event::ModuleThresholdSet {
                        station_id: station_id.clone(),
                        module_id: module_id.clone(),
                        threshold_kg: *threshold_kg,
                    },
                ));
            }
            Command::AssignLabTech {
                station_id,
                module_id,
                tech_id,
            } => {
                let Some(station) = state.stations.get_mut(station_id) else {
                    continue;
                };
                let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
                    continue;
                };
                if let crate::ModuleKindState::Lab(ls) = &mut module.kind_state {
                    ls.assigned_tech.clone_from(tech_id);
                }
            }
            Command::SetAssemblerCap {
                station_id,
                module_id,
                component_id,
                max_stock,
            } => {
                let Some(station) = state.stations.get_mut(station_id) else {
                    continue;
                };
                let Some(module) = station.modules.iter_mut().find(|m| &m.id == module_id) else {
                    continue;
                };
                if let crate::ModuleKindState::Assembler(asmb) = &mut module.kind_state {
                    asmb.cap_override.insert(component_id.clone(), *max_stock);
                }
            }
            Command::Import {
                station_id,
                item_spec,
            } => {
                if current_tick < TRADE_UNLOCK_TICK {
                    continue;
                }
                if !state.stations.contains_key(station_id) {
                    continue;
                }

                // Look up pricing and compute cost
                let Some(cost) = trade::compute_import_cost(item_spec, &content.pricing, content)
                else {
                    continue; // not importable or unknown item
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
                    continue;
                }

                // Check cargo capacity
                let new_items = trade::create_inventory_items(item_spec, rng);
                let new_volume = inventory_volume_m3(&new_items, content);
                let station = state.stations.get_mut(station_id).unwrap();
                let current_volume = station.used_volume_m3(content);
                let cargo_cap = station.cargo_capacity_m3;
                if current_volume + new_volume > cargo_cap {
                    continue; // no room
                }

                // Execute import
                state.balance -= cost;
                let station = state.stations.get_mut(station_id).unwrap();
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
            }
            Command::Export {
                station_id,
                item_spec,
            } => {
                if current_tick < TRADE_UNLOCK_TICK {
                    continue;
                }
                let Some(station) = state.stations.get(station_id) else {
                    continue;
                };

                // Look up pricing and compute revenue
                let Some(revenue) =
                    trade::compute_export_revenue(item_spec, &content.pricing, content)
                else {
                    continue; // not exportable or unknown item
                };

                // Check station has items
                if !trade::has_enough_for_export(&station.inventory, item_spec) {
                    continue;
                }

                // Execute export
                let station = state.stations.get_mut(station_id).unwrap();
                if !trade::remove_inventory_items(&mut station.inventory, item_spec) {
                    continue;
                }
                station.invalidate_volume_cache();
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
            Command::JettisonSlag { station_id } => {
                let Some(station) = state.stations.get_mut(station_id) else {
                    continue;
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

fn resolve_ship_tasks(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Collect ships whose task eta has arrived, sorted for determinism.
    let mut ship_ids: Vec<ShipId> = state
        .ships
        .values()
        .filter(|ship| {
            matches!(&ship.task, Some(task)
                if task.eta_tick == current_tick
                && !matches!(task.kind, TaskKind::Idle))
        })
        .map(|ship| ship.id.clone())
        .collect();
    ship_ids.sort_by(|a, b| a.0.cmp(&b.0));

    for ship_id in ship_ids {
        // Clone the task kind to release the borrow on state.ships.
        let Some(task_kind) = state
            .ships
            .get(&ship_id)
            .and_then(|ship| ship.task.as_ref())
            .map(|task| task.kind.clone())
        else {
            continue;
        };

        match task_kind {
            TaskKind::Transit {
                ref destination,
                ref then,
                ..
            } => {
                resolve_transit(state, &ship_id, destination, then, content, events);
            }
            TaskKind::Survey { ref site } => {
                resolve_survey(state, &ship_id, site, content, rng, events);
            }
            TaskKind::DeepScan { ref asteroid } => {
                resolve_deep_scan(state, &ship_id, asteroid, content, rng, events);
            }
            TaskKind::Mine { ref asteroid, .. } => {
                resolve_mine(state, &ship_id, asteroid, content, events);
            }
            TaskKind::Deposit { ref station, .. } => {
                resolve_deposit(state, &ship_id, station, content, events);
            }
            TaskKind::Idle => {}
        }
    }
}

fn replenish_scan_sites(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    if state.scan_sites.len() >= MIN_UNSCANNED_SITES {
        return;
    }

    let node_ids: Vec<&NodeId> = content.solar_system.nodes.iter().map(|n| &n.id).collect();
    let templates = &content.asteroid_templates;

    if node_ids.is_empty() || templates.is_empty() {
        return;
    }

    let current_tick = state.meta.tick;

    for _ in 0..REPLENISH_BATCH_SIZE {
        let template = &templates[rng.gen_range(0..templates.len())];
        let node = node_ids[rng.gen_range(0..node_ids.len())].clone();
        let uuid = crate::generate_uuid(rng);
        let site_id = SiteId(format!("site_{uuid}"));

        state.scan_sites.push(ScanSite {
            id: site_id.clone(),
            node: node.clone(),
            template_id: template.id.clone(),
        });

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::ScanSiteSpawned {
                site_id,
                node,
                template_id: template.id.clone(),
            },
        ));
    }
}
