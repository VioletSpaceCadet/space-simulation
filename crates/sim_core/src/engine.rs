use crate::research::advance_research;
use crate::station::tick_stations;
use crate::tasks::{
    deep_scan_enabled, inventory_volume_m3, resolve_deep_scan, resolve_deposit, resolve_mine,
    resolve_survey, resolve_transit, task_duration, task_kind_label, task_target,
};
use crate::trade;
use crate::{
    Command, CommandEnvelope, EventLevel, GameContent, GameState, InventoryItem, NodeId, ScanSite,
    ShipId, SiteId, TaskKind, TaskState,
};
use rand::Rng;

const MIN_UNSCANNED_SITES: usize = 5;
const REPLENISH_BATCH_SIZE: usize = 5;

/// Trade (import/export) unlocks after 1 simulated year (365 days × 24 h × 60 min).
pub const TRADE_UNLOCK_TICK: u64 = 525_600;

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
                let Some(station) = state.stations.get(station_id) else {
                    continue;
                };

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
                let current_volume = inventory_volume_m3(&station.inventory, content);
                if current_volume + new_volume > station.cargo_capacity_m3 {
                    continue; // no room
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

#[cfg(test)]
mod replenish_tests {
    use crate::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    #[allow(unused_imports)]
    use std::collections::{HashMap, HashSet};

    fn replenish_test_content() -> GameContent {
        let mut content = GameContent {
            content_version: "test".to_string(),
            techs: vec![],
            solar_system: SolarSystemDef {
                nodes: vec![NodeDef {
                    id: NodeId("node_test".to_string()),
                    name: "Test Node".to_string(),
                }],
                edges: vec![],
            },
            asteroid_templates: vec![AsteroidTemplateDef {
                id: "tmpl_iron_rich".to_string(),
                anomaly_tags: vec![AnomalyTag::IronRich],
                composition_ranges: HashMap::from([
                    ("Fe".to_string(), (0.7, 0.7)),
                    ("Si".to_string(), (0.3, 0.3)),
                ]),
            }],
            elements: vec![ElementDef {
                id: "ore".to_string(),
                density_kg_per_m3: 3000.0,
                display_name: "Raw Ore".to_string(),
                refined_name: None,
            }],
            module_defs: HashMap::new(),
            component_defs: vec![],
            pricing: PricingTable {
                import_surcharge_per_kg: 100.0,
                export_surcharge_per_kg: 50.0,
                items: HashMap::new(),
            },
            constants: Constants {
                survey_scan_ticks: 1,
                deep_scan_ticks: 1,
                travel_ticks_per_hop: 1,
                survey_tag_detection_probability: 1.0,
                asteroid_count_per_template: 1,
                asteroid_mass_min_kg: 500.0,
                asteroid_mass_max_kg: 500.0,
                ship_cargo_capacity_m3: 20.0,
                station_cargo_capacity_m3: 10_000.0,
                station_power_available_per_tick: 100.0,
                mining_rate_kg_per_tick: 50.0,
                deposit_ticks: 1,
                autopilot_iron_rich_confidence_threshold: 0.7,
                autopilot_refinery_threshold_kg: 500.0,
                research_roll_interval_ticks: 60,
                data_generation_peak: 100.0,
                data_generation_floor: 5.0,
                data_generation_decay_rate: 0.7,
                autopilot_slag_jettison_pct: 0.75,
                wear_band_degraded_threshold: 0.5,
                wear_band_critical_threshold: 0.8,
                wear_band_degraded_efficiency: 0.75,
                wear_band_critical_efficiency: 0.5,
            },
            density_map: HashMap::new(),
        };
        content.init_caches();
        content
    }

    fn empty_sites_state(content: &GameContent) -> GameState {
        GameState {
            meta: MetaState {
                tick: 0,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![],
            asteroids: HashMap::new(),
            ships: HashMap::new(),
            stations: HashMap::from([(
                StationId("station_test".to_string()),
                StationState {
                    id: StationId("station_test".to_string()),
                    location_node: NodeId("node_test".to_string()),
                    inventory: vec![],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![],
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
    fn replenish_spawns_sites_when_below_threshold() {
        let content = replenish_test_content();
        let mut state = empty_sites_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        assert_eq!(state.scan_sites.len(), 5); // REPLENISH_BATCH_SIZE
        let spawned_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event, Event::ScanSiteSpawned { .. }))
            .collect();
        assert_eq!(spawned_events.len(), 5);
    }

    #[test]
    fn replenish_does_not_spawn_when_at_threshold() {
        let content = replenish_test_content();
        let mut state = empty_sites_state(&content);
        // Pre-fill with MIN_UNSCANNED_SITES sites
        for i in 0..5 {
            state.scan_sites.push(ScanSite {
                id: SiteId(format!("site_existing_{i}")),
                node: NodeId("node_test".to_string()),
                template_id: "tmpl_iron_rich".to_string(),
            });
        }
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let spawned_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event, Event::ScanSiteSpawned { .. }))
            .collect();
        assert_eq!(spawned_events.len(), 0);
        assert_eq!(state.scan_sites.len(), 5);
    }

    #[test]
    fn replenish_site_ids_are_unique_uuids() {
        let content = replenish_test_content();
        let mut state = empty_sites_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let ids: Vec<_> = state.scan_sites.iter().map(|s| s.id.0.clone()).collect();
        // All start with "site_"
        for id in &ids {
            assert!(id.starts_with("site_"), "ID should start with site_: {id}");
        }
        // All unique
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "Site IDs should be unique");
    }

    #[test]
    fn jettison_slag_removes_all_slag_and_emits_event() {
        let content = replenish_test_content();
        let mut state = empty_sites_state(&content);
        // Pre-fill scan sites so replenish doesn't fire
        for i in 0..5 {
            state.scan_sites.push(ScanSite {
                id: SiteId(format!("site_existing_{i}")),
                node: NodeId("node_test".to_string()),
                template_id: "tmpl_iron_rich".to_string(),
            });
        }

        let station_id = StationId("station_test".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Slag {
            kg: 100.0,
            composition: HashMap::from([("slag".to_string(), 1.0)]),
        });
        station.inventory.push(InventoryItem::Slag {
            kg: 50.0,
            composition: HashMap::from([("slag".to_string(), 1.0)]),
        });
        station.inventory.push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 200.0,
            quality: 0.8,
        });

        let cmd = CommandEnvelope {
            id: crate::CommandId("cmd_000001".to_string()),
            issued_by: crate::PrincipalId("test".to_string()),
            issued_tick: 0,
            execute_at_tick: 0,
            command: Command::JettisonSlag {
                station_id: station_id.clone(),
            },
        };

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        // Slag should be gone, material should remain
        let station = &state.stations[&station_id];
        assert!(
            !station
                .inventory
                .iter()
                .any(|i| matches!(i, InventoryItem::Slag { .. })),
            "all slag should be removed"
        );
        assert_eq!(station.inventory.len(), 1, "material should remain");

        // Should have emitted SlagJettisoned event with total kg
        let jettison_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event, Event::SlagJettisoned { .. }))
            .collect();
        assert_eq!(jettison_events.len(), 1);
        if let Event::SlagJettisoned { kg, .. } = &jettison_events[0].event {
            assert!(
                (kg - 150.0).abs() < f32::EPSILON,
                "should jettison 150 kg total"
            );
        }
    }

    #[test]
    fn jettison_slag_no_event_when_no_slag() {
        let content = replenish_test_content();
        let mut state = empty_sites_state(&content);
        for i in 0..5 {
            state.scan_sites.push(ScanSite {
                id: SiteId(format!("site_existing_{i}")),
                node: NodeId("node_test".to_string()),
                template_id: "tmpl_iron_rich".to_string(),
            });
        }

        let station_id = StationId("station_test".to_string());

        let cmd = CommandEnvelope {
            id: crate::CommandId("cmd_000001".to_string()),
            issued_by: crate::PrincipalId("test".to_string()),
            issued_tick: 0,
            execute_at_tick: 0,
            command: Command::JettisonSlag {
                station_id: station_id.clone(),
            },
        };

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            !events
                .iter()
                .any(|e| matches!(e.event, Event::SlagJettisoned { .. })),
            "no event should be emitted when there is no slag"
        );
    }

    #[test]
    fn replenish_is_deterministic() {
        let content = replenish_test_content();

        let mut state1 = empty_sites_state(&content);
        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        tick(&mut state1, &[], &content, &mut rng1, EventLevel::Normal);

        let mut state2 = empty_sites_state(&content);
        let mut rng2 = ChaCha8Rng::seed_from_u64(42);
        tick(&mut state2, &[], &content, &mut rng2, EventLevel::Normal);

        let ids1: Vec<_> = state1.scan_sites.iter().map(|s| s.id.0.clone()).collect();
        let ids2: Vec<_> = state2.scan_sites.iter().map(|s| s.id.0.clone()).collect();
        assert_eq!(ids1, ids2);
    }
}

#[cfg(test)]
mod trade_tests {
    use crate::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::collections::HashMap;

    /// Build content with pricing entries for Fe, thruster, and a module.
    fn trade_content() -> GameContent {
        let mut content = test_fixtures::base_content();
        content.pricing = PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: HashMap::from([
                (
                    "Fe".to_string(),
                    PricingEntry {
                        base_price_per_unit: 50.0,
                        importable: true,
                        exportable: true,
                    },
                ),
                (
                    "thruster".to_string(),
                    PricingEntry {
                        base_price_per_unit: 500_000.0,
                        importable: true,
                        exportable: true,
                    },
                ),
                (
                    "repair_kit".to_string(),
                    PricingEntry {
                        base_price_per_unit: 8_000.0,
                        importable: true,
                        exportable: true,
                    },
                ),
                (
                    "ore".to_string(),
                    PricingEntry {
                        base_price_per_unit: 5.0,
                        importable: false,
                        exportable: false,
                    },
                ),
                (
                    "slag".to_string(),
                    PricingEntry {
                        base_price_per_unit: 1.0,
                        importable: false,
                        exportable: false,
                    },
                ),
                (
                    "module_basic_iron_refinery".to_string(),
                    PricingEntry {
                        base_price_per_unit: 2_000_000.0,
                        importable: true,
                        exportable: true,
                    },
                ),
            ]),
        };
        content.component_defs = vec![
            ComponentDef {
                id: "repair_kit".to_string(),
                name: "Repair Kit".to_string(),
                mass_kg: 50.0,
                volume_m3: 0.1,
            },
            ComponentDef {
                id: "thruster".to_string(),
                name: "Thruster".to_string(),
                mass_kg: 200.0,
                volume_m3: 0.5,
            },
        ];
        content.module_defs = HashMap::from([(
            "module_basic_iron_refinery".to_string(),
            ModuleDef {
                id: "module_basic_iron_refinery".to_string(),
                name: "Basic Iron Refinery".to_string(),
                mass_kg: 1000.0,
                volume_m3: 5.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_ticks: 10,
                    recipes: vec![],
                }),
            },
        )]);
        content
    }

    fn trade_state(content: &GameContent) -> GameState {
        let mut state = test_fixtures::base_state(content);
        state.balance = 10_000_000.0;
        state.meta.tick = TRADE_UNLOCK_TICK;
        // Pre-fill 5 scan sites to avoid replenish noise
        for index in 0..5 {
            state.scan_sites.push(ScanSite {
                id: SiteId(format!("site_pad_{index}")),
                node: NodeId("node_test".to_string()),
                template_id: "tmpl_iron_rich".to_string(),
            });
        }
        state
    }

    fn make_command(command: Command) -> CommandEnvelope {
        CommandEnvelope {
            id: CommandId("cmd_test".to_string()),
            issued_by: PrincipalId("principal_autopilot".to_string()),
            issued_tick: TRADE_UNLOCK_TICK,
            execute_at_tick: TRADE_UNLOCK_TICK,
            command,
        }
    }

    // ---- Import tests ----

    #[test]
    fn import_material_deducts_balance_and_adds_inventory() {
        let content = trade_content();
        let mut state = trade_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        let cmd = make_command(Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "Fe".to_string(),
                kg: 100.0,
            },
        });

        // Expected cost: 50.0 * 100 + 100.0 * 100.0 = 5000 + 10000 = 15000
        let expected_cost = 50.0 * 100.0 + 100.0 * 100.0;

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            (state.balance - (10_000_000.0 - expected_cost)).abs() < 0.01,
            "balance should be deducted by cost: got {} expected {}",
            state.balance,
            10_000_000.0 - expected_cost
        );

        let station = state.stations.get(&station_id).unwrap();
        let fe_kg: f32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        assert!(
            (fe_kg - 100.0).abs() < 0.01,
            "should have 100kg Fe, got {fe_kg}"
        );

        let imported = events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemImported { .. }));
        assert!(imported, "should emit ItemImported event");
    }

    #[test]
    fn import_component_deducts_balance_and_adds_inventory() {
        let content = trade_content();
        let mut state = trade_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        let cmd = make_command(Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Component {
                component_id: ComponentId("thruster".to_string()),
                count: 2,
            },
        });

        // cost: 500_000 * 2 + (200 * 2) * 100 = 1_000_000 + 40_000 = 1_040_000
        let expected_cost = 500_000.0 * 2.0 + (200.0 * 2.0) * 100.0;

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            (state.balance - (10_000_000.0 - expected_cost)).abs() < 0.01,
            "balance: got {} expected {}",
            state.balance,
            10_000_000.0 - expected_cost
        );

        let station = state.stations.get(&station_id).unwrap();
        let thruster_count: u32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "thruster" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(thruster_count, 2, "should have 2 thrusters");

        assert!(events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemImported { .. })));
    }

    #[test]
    fn import_module_deducts_balance_and_adds_with_unique_id() {
        let content = trade_content();
        let mut state = trade_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        let cmd = make_command(Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Module {
                module_def_id: "module_basic_iron_refinery".to_string(),
            },
        });

        // cost: 2_000_000 * 1 + 1000 * 100 = 2_000_000 + 100_000 = 2_100_000
        let expected_cost = 2_000_000.0 + 1000.0 * 100.0;

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            (state.balance - (10_000_000.0 - expected_cost)).abs() < 0.01,
            "balance: got {} expected {}",
            state.balance,
            10_000_000.0 - expected_cost
        );

        let station = state.stations.get(&station_id).unwrap();
        let module_items: Vec<_> = station
            .inventory
            .iter()
            .filter(|item| {
                matches!(item, InventoryItem::Module { module_def_id, .. }
                    if module_def_id == "module_basic_iron_refinery")
            })
            .collect();
        assert_eq!(module_items.len(), 1, "should have 1 module item");

        // Check the item_id starts with "module_item_"
        if let InventoryItem::Module { item_id, .. } = module_items[0] {
            assert!(
                item_id.0.starts_with("module_item_"),
                "module item_id should start with module_item_: {}",
                item_id.0
            );
        }

        assert!(events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemImported { .. })));
    }

    #[test]
    fn import_insufficient_funds_emits_event_no_change() {
        let content = trade_content();
        let mut state = trade_state(&content);
        state.balance = 100.0; // not enough for Fe import
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        let cmd = make_command(Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "Fe".to_string(),
                kg: 100.0,
            },
        });

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            (state.balance - 100.0).abs() < 0.01,
            "balance should not change"
        );

        let station = state.stations.get(&station_id).unwrap();
        let has_fe = station
            .inventory
            .iter()
            .any(|item| matches!(item, InventoryItem::Material { element, .. } if element == "Fe"));
        assert!(!has_fe, "should not have Fe in inventory");

        let insufficient = events
            .iter()
            .any(|e| matches!(&e.event, Event::InsufficientFunds { .. }));
        assert!(insufficient, "should emit InsufficientFunds event");
    }

    #[test]
    fn import_non_importable_is_rejected() {
        let content = trade_content();
        let mut state = trade_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        let cmd = make_command(Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "ore".to_string(),
                kg: 100.0,
            },
        });

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            (state.balance - 10_000_000.0).abs() < 0.01,
            "balance should not change for non-importable"
        );

        let imported = events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemImported { .. }));
        assert!(!imported, "should not emit ItemImported");
    }

    // ---- Export tests ----

    #[test]
    fn export_material_removes_from_inventory_and_adds_revenue() {
        let content = trade_content();
        let mut state = trade_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        // Pre-add 100 kg Fe to station
        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 100.0,
            quality: 1.0,
        });

        let cmd = make_command(Command::Export {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "Fe".to_string(),
                kg: 50.0,
            },
        });

        // revenue: 50.0 * 50 - 50.0 * 50.0 = 2500 - 2500 = 0.0 (floored)
        // Actually: base_price * quantity - mass * surcharge = 50 * 50 - 50 * 50 = 0
        let expected_revenue = (50.0_f64 * 50.0 - 50.0 * 50.0).max(0.0);

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            (state.balance - (10_000_000.0 + expected_revenue)).abs() < 0.01,
            "balance: got {} expected {}",
            state.balance,
            10_000_000.0 + expected_revenue
        );

        let station = state.stations.get(&station_id).unwrap();
        let fe_kg: f32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        assert!(
            (fe_kg - 50.0).abs() < 0.01,
            "should have 50kg Fe remaining, got {fe_kg}"
        );

        assert!(events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemExported { .. })));
    }

    #[test]
    fn export_component_removes_and_adds_revenue() {
        let content = trade_content();
        let mut state = trade_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: 5,
            quality: 1.0,
        });

        let cmd = make_command(Command::Export {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Component {
                component_id: ComponentId("repair_kit".to_string()),
                count: 2,
            },
        });

        // revenue: 8000 * 2 - (50 * 2) * 50 = 16000 - 5000 = 11000
        let expected_revenue = 8000.0 * 2.0 - (50.0 * 2.0) * 50.0;

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            (state.balance - (10_000_000.0 + expected_revenue)).abs() < 0.01,
            "balance: got {} expected {}",
            state.balance,
            10_000_000.0 + expected_revenue
        );

        let station = state.stations.get(&station_id).unwrap();
        let kit_count: u32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "repair_kit" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(kit_count, 3, "should have 3 repair kits remaining");

        assert!(events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemExported { .. })));
    }

    #[test]
    fn export_non_exportable_is_rejected() {
        let content = trade_content();
        let mut state = trade_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Slag {
            kg: 100.0,
            composition: HashMap::new(),
        });

        let cmd = make_command(Command::Export {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "slag".to_string(),
                kg: 50.0,
            },
        });

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            (state.balance - 10_000_000.0).abs() < 0.01,
            "balance should not change"
        );

        let exported = events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemExported { .. }));
        assert!(!exported, "should not emit ItemExported for non-exportable");
    }

    #[test]
    fn export_more_than_available_is_rejected() {
        let content = trade_content();
        let mut state = trade_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 100.0,
            quality: 1.0,
        });

        let cmd = make_command(Command::Export {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "Fe".to_string(),
                kg: 1000.0,
            },
        });

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            (state.balance - 10_000_000.0).abs() < 0.01,
            "balance should not change"
        );

        let station = state.stations.get(&station_id).unwrap();
        let fe_kg: f32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        assert!(
            (fe_kg - 100.0).abs() < 0.01,
            "Fe should still be 100kg, got {fe_kg}"
        );

        let exported = events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemExported { .. }));
        assert!(!exported, "should not emit ItemExported");
    }

    #[test]
    fn import_merges_material_with_existing() {
        let content = trade_content();
        let mut state = trade_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        // Pre-add 50 kg Fe with quality 1.0
        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 50.0,
            quality: 1.0,
        });

        let cmd = make_command(Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "Fe".to_string(),
                kg: 100.0,
            },
        });

        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        let station = state.stations.get(&station_id).unwrap();
        // Should merge into single entry
        let fe_entries: Vec<_> = station
            .inventory
            .iter()
            .filter(
                |item| matches!(item, InventoryItem::Material { element, .. } if element == "Fe"),
            )
            .collect();
        assert_eq!(
            fe_entries.len(),
            1,
            "should merge into one Fe entry, got {}",
            fe_entries.len()
        );
        if let InventoryItem::Material { kg, .. } = fe_entries[0] {
            assert!(
                (*kg - 150.0).abs() < 0.01,
                "merged Fe should be 150kg, got {kg}"
            );
        }
    }

    #[test]
    fn import_rejected_before_trade_unlock_tick() {
        let content = trade_content();
        let mut state = trade_state(&content);
        state.meta.tick = TRADE_UNLOCK_TICK - 1;
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());
        let balance_before = state.balance;

        let cmd = CommandEnvelope {
            id: CommandId("cmd_early".to_string()),
            issued_by: PrincipalId("principal_autopilot".to_string()),
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::Import {
                station_id: station_id.clone(),
                item_spec: TradeItemSpec::Material {
                    element: "Fe".to_string(),
                    kg: 100.0,
                },
            },
        };

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        assert!(
            (state.balance - balance_before).abs() < 0.01,
            "balance should be unchanged before trade unlock"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(&e.event, Event::ItemImported { .. })),
            "should not emit ItemImported before trade unlock"
        );
    }

    #[test]
    fn export_rejected_before_trade_unlock_tick() {
        let content = trade_content();
        let mut state = trade_state(&content);
        state.meta.tick = TRADE_UNLOCK_TICK - 1;
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        // Add exportable Fe
        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 500.0,
            quality: 0.7,
        });
        let balance_before = state.balance;

        let cmd = CommandEnvelope {
            id: CommandId("cmd_early_export".to_string()),
            issued_by: PrincipalId("principal_autopilot".to_string()),
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::Export {
                station_id: station_id.clone(),
                item_spec: TradeItemSpec::Material {
                    element: "Fe".to_string(),
                    kg: 100.0,
                },
            },
        };

        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        assert!(
            (state.balance - balance_before).abs() < 0.01,
            "balance should be unchanged before trade unlock"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(&e.event, Event::ItemExported { .. })),
            "should not emit ItemExported before trade unlock"
        );
    }
}

#[cfg(test)]
mod trade_integration_tests {
    use crate::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::collections::HashMap;

    /// Build content with pricing for Fe, thruster, and a shipyard module.
    fn economy_content() -> GameContent {
        let mut content = test_fixtures::base_content();

        // Pricing table for materials, components, and modules
        content.pricing = PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: HashMap::from([
                (
                    "Fe".to_string(),
                    PricingEntry {
                        base_price_per_unit: 50.0,
                        importable: true,
                        exportable: true,
                    },
                ),
                (
                    "thruster".to_string(),
                    PricingEntry {
                        base_price_per_unit: 500_000.0,
                        importable: true,
                        exportable: true,
                    },
                ),
                (
                    "module_shipyard".to_string(),
                    PricingEntry {
                        base_price_per_unit: 5_000_000.0,
                        importable: true,
                        exportable: true,
                    },
                ),
            ]),
        };

        // Component definition for thrusters
        content.component_defs = vec![ComponentDef {
            id: "thruster".to_string(),
            name: "Thruster".to_string(),
            mass_kg: 200.0,
            volume_m3: 0.5,
        }];

        // Shipyard assembler: consumes 100kg Fe + 2 thrusters => Ship (50 m3 cargo)
        // Use assembly_interval_ticks=2 so the test doesn't need thousands of ticks.
        content.module_defs = HashMap::from([(
            "module_shipyard".to_string(),
            ModuleDef {
                id: "module_shipyard".to_string(),
                name: "Shipyard".to_string(),
                mass_kg: 5000.0,
                volume_m3: 20.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.0,
                behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                    assembly_interval_ticks: 2,
                    recipes: vec![RecipeDef {
                        id: "recipe_build_ship".to_string(),
                        inputs: vec![
                            RecipeInput {
                                filter: InputFilter::Element("Fe".to_string()),
                                amount: InputAmount::Kg(100.0),
                            },
                            RecipeInput {
                                filter: InputFilter::Component(ComponentId("thruster".to_string())),
                                amount: InputAmount::Count(2),
                            },
                        ],
                        outputs: vec![OutputSpec::Ship {
                            cargo_capacity_m3: 50.0,
                        }],
                        efficiency: 1.0,
                    }],
                    max_stock: HashMap::new(),
                }),
            },
        )]);

        content
    }

    fn economy_state(content: &GameContent) -> GameState {
        let mut state = test_fixtures::base_state(content);
        state.balance = 1_000_000_000.0;
        state.meta.tick = TRADE_UNLOCK_TICK;
        // Pre-fill 5 scan sites to avoid replenish noise
        for index in 0..5 {
            state.scan_sites.push(ScanSite {
                id: SiteId(format!("site_pad_{index}")),
                node: NodeId("node_test".to_string()),
                template_id: "tmpl_iron_rich".to_string(),
            });
        }
        state
    }

    fn make_command(tick: u64, command: Command) -> CommandEnvelope {
        CommandEnvelope {
            id: CommandId(format!("cmd_test_{tick}")),
            issued_by: PrincipalId("principal_autopilot".to_string()),
            issued_tick: tick,
            execute_at_tick: tick,
            command,
        }
    }

    #[test]
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    fn economy_full_loop() {
        let content = economy_content();
        let mut state = economy_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let station_id = StationId("station_earth_orbit".to_string());

        // -----------------------------------------------------------
        // Step 1: Verify starting balance
        // -----------------------------------------------------------
        assert!(
            (state.balance - 1_000_000_000.0).abs() < 0.01,
            "starting balance should be 1B, got {}",
            state.balance
        );

        // -----------------------------------------------------------
        // Step 2: Import 4 thrusters
        // -----------------------------------------------------------
        let balance_before_thrusters = state.balance;
        let cmd_thrusters = make_command(
            state.meta.tick,
            Command::Import {
                station_id: station_id.clone(),
                item_spec: TradeItemSpec::Component {
                    component_id: ComponentId("thruster".to_string()),
                    count: 4,
                },
            },
        );
        let events = tick(
            &mut state,
            &[cmd_thrusters],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        // Verify balance decreased
        assert!(
            state.balance < balance_before_thrusters,
            "balance should decrease after importing thrusters: {} vs {}",
            state.balance,
            balance_before_thrusters
        );
        // Expected cost: 500_000 * 4 + (200 * 4) * 100 = 2_000_000 + 80_000 = 2_080_000
        let thruster_cost = 500_000.0 * 4.0 + (200.0 * 4.0) * 100.0;
        assert!(
            (state.balance - (balance_before_thrusters - thruster_cost)).abs() < 0.01,
            "balance after thruster import: expected {}, got {}",
            balance_before_thrusters - thruster_cost,
            state.balance
        );

        // Verify thrusters in inventory
        let station = state.stations.get(&station_id).unwrap();
        let thruster_count: u32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "thruster" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(thruster_count, 4, "should have 4 thrusters in inventory");

        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, Event::ItemImported { .. })),
            "should emit ItemImported for thrusters"
        );

        // -----------------------------------------------------------
        // Step 3: Import 5000 kg Fe
        // -----------------------------------------------------------
        let balance_before_fe = state.balance;
        let cmd_fe = make_command(
            state.meta.tick,
            Command::Import {
                station_id: station_id.clone(),
                item_spec: TradeItemSpec::Material {
                    element: "Fe".to_string(),
                    kg: 5000.0,
                },
            },
        );
        let events = tick(
            &mut state,
            &[cmd_fe],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        // Verify balance decreased
        assert!(
            state.balance < balance_before_fe,
            "balance should decrease after importing Fe"
        );
        // Expected cost: 50.0 * 5000 + 100.0 * 5000 = 250_000 + 500_000 = 750_000
        let fe_cost = 50.0 * 5000.0 + 100.0 * 5000.0;
        assert!(
            (state.balance - (balance_before_fe - fe_cost)).abs() < 0.01,
            "balance after Fe import: expected {}, got {}",
            balance_before_fe - fe_cost,
            state.balance
        );

        // Verify Fe in inventory
        let station = state.stations.get(&station_id).unwrap();
        let fe_kg: f32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        assert!(
            (fe_kg - 5000.0).abs() < 0.01,
            "should have 5000kg Fe, got {fe_kg}"
        );

        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, Event::ItemImported { .. })),
            "should emit ItemImported for Fe"
        );

        // -----------------------------------------------------------
        // Step 4: Import and install shipyard module, but WITHOUT tech
        //         Verify ModuleAwaitingTech and no ship spawned.
        // -----------------------------------------------------------
        let cmd_import_shipyard = make_command(
            state.meta.tick,
            Command::Import {
                station_id: station_id.clone(),
                item_spec: TradeItemSpec::Module {
                    module_def_id: "module_shipyard".to_string(),
                },
            },
        );
        tick(
            &mut state,
            &[cmd_import_shipyard],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        // Find the module item_id in inventory
        let station = state.stations.get(&station_id).unwrap();
        let module_item_id = station
            .inventory
            .iter()
            .find_map(|item| match item {
                InventoryItem::Module { item_id, .. } => Some(item_id.clone()),
                _ => None,
            })
            .expect("shipyard module should be in inventory after import");

        // Install the module
        let cmd_install = make_command(
            state.meta.tick,
            Command::InstallModule {
                station_id: station_id.clone(),
                module_item_id,
            },
        );
        tick(
            &mut state,
            &[cmd_install],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        // Enable the module
        let station = state.stations.get(&station_id).unwrap();
        let shipyard_module_id = station
            .modules
            .iter()
            .find(|m| m.def_id == "module_shipyard")
            .expect("shipyard should be installed")
            .id
            .clone();

        let cmd_enable = make_command(
            state.meta.tick,
            Command::SetModuleEnabled {
                station_id: station_id.clone(),
                module_id: shipyard_module_id.clone(),
                enabled: true,
            },
        );
        tick(
            &mut state,
            &[cmd_enable],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        // Tick forward enough for the assembler interval (2 ticks) without tech
        let ships_before = state.ships.len();
        let mut saw_awaiting_tech = false;
        for _ in 0..4 {
            let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
            if events
                .iter()
                .any(|e| matches!(&e.event, Event::ModuleAwaitingTech { .. }))
            {
                saw_awaiting_tech = true;
            }
        }

        assert!(
            saw_awaiting_tech,
            "should emit ModuleAwaitingTech when tech is not unlocked"
        );
        assert_eq!(
            state.ships.len(),
            ships_before,
            "no ship should be spawned without tech_ship_construction"
        );

        // -----------------------------------------------------------
        // Step 5: Unlock tech_ship_construction and tick until ship built
        // -----------------------------------------------------------
        state
            .research
            .unlocked
            .insert(TechId("tech_ship_construction".to_string()));

        // Tick enough times for the assembler to fire (interval=2).
        // The assembler may build multiple ships if enough materials exist and
        // enough ticks pass. We just need at least one ShipConstructed event.
        let ships_before = state.ships.len();
        let mut all_events = Vec::new();
        for _ in 0..4 {
            let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
            all_events.extend(events);
        }

        assert!(
            state.ships.len() > ships_before,
            "at least one ship should be constructed after unlocking tech"
        );
        let ships_built = state.ships.len() - ships_before;

        let ship_constructed = all_events
            .iter()
            .any(|e| matches!(&e.event, Event::ShipConstructed { .. }));
        assert!(
            ship_constructed,
            "should emit ShipConstructed after tech unlocked"
        );

        // Verify inputs were consumed proportional to ships built
        let station = state.stations.get(&station_id).unwrap();
        let fe_after: f32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        let expected_fe = 5000.0 - 100.0 * ships_built as f32;
        assert!(
            (fe_after - expected_fe).abs() < 0.01,
            "expected {expected_fe}kg Fe after {ships_built} ship builds, got {fe_after}"
        );

        let thruster_after: u32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "thruster" => Some(*count),
                _ => None,
            })
            .sum();
        let expected_thrusters = 4 - 2 * ships_built as u32;
        assert_eq!(
            thruster_after, expected_thrusters,
            "should have {expected_thrusters} thrusters remaining after {ships_built} ship builds"
        );

        // -----------------------------------------------------------
        // Step 6: Export some Fe and verify balance increases
        // -----------------------------------------------------------
        let balance_before_export = state.balance;
        let cmd_export = make_command(
            state.meta.tick,
            Command::Export {
                station_id: station_id.clone(),
                item_spec: TradeItemSpec::Material {
                    element: "Fe".to_string(),
                    kg: 1000.0,
                },
            },
        );
        let events = tick(
            &mut state,
            &[cmd_export],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        // Revenue: base_price * kg - surcharge * mass = 50 * 1000 - 50 * 1000 = 0
        // (Fe export revenue is 0 because surcharge equals price -- that's fine, we
        //  verify the mechanics work regardless.)
        let expected_revenue = (50.0_f64 * 1000.0 - 50.0 * 1000.0).max(0.0);
        assert!(
            (state.balance - (balance_before_export + expected_revenue)).abs() < 0.01,
            "balance after export: expected {}, got {}",
            balance_before_export + expected_revenue,
            state.balance
        );

        // Verify Fe reduced: started with expected_fe, exported 1000
        let station = state.stations.get(&station_id).unwrap();
        let fe_final: f32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        let expected_fe_final = expected_fe - 1000.0;
        assert!(
            (fe_final - expected_fe_final).abs() < 0.01,
            "expected {expected_fe_final}kg Fe after export, got {fe_final}"
        );

        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, Event::ItemExported { .. })),
            "should emit ItemExported for Fe"
        );
    }
}
