use crate::research::advance_research;
use crate::station::tick_stations;
use crate::tasks::{
    deep_scan_enabled, resolve_deep_scan, resolve_deposit, resolve_mine, resolve_survey,
    resolve_transit, task_duration, task_kind_label, task_target,
};
use crate::{
    Command, CommandEnvelope, EventLevel, GameContent, GameState, InventoryItem, NodeId, ScanSite,
    ShipId, SiteId, TaskKind, TaskState,
};
use rand::Rng;

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

    apply_commands(state, commands, content, &mut events);
    resolve_ship_tasks(state, content, rng, &mut events);
    tick_stations(state, content, &mut events);
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
                            })
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
    use std::collections::{HashMap, HashSet};

    fn replenish_test_content() -> GameContent {
        GameContent {
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
            module_defs: vec![],
            component_defs: vec![],
            constants: Constants {
                survey_scan_ticks: 1,
                deep_scan_ticks: 1,
                travel_ticks_per_hop: 1,
                survey_scan_data_amount: 5.0,
                survey_scan_data_quality: 1.0,
                deep_scan_data_amount: 15.0,
                deep_scan_data_quality: 1.2,
                survey_tag_detection_probability: 1.0,
                asteroid_count_per_template: 1,
                asteroid_mass_min_kg: 500.0,
                asteroid_mass_max_kg: 500.0,
                ship_cargo_capacity_m3: 20.0,
                station_cargo_capacity_m3: 10_000.0,
                station_compute_units_total: 10,
                station_power_per_compute_unit_per_tick: 1.0,
                station_efficiency: 1.0,
                station_power_available_per_tick: 100.0,
                mining_rate_kg_per_tick: 50.0,
                deposit_ticks: 1,
                autopilot_iron_rich_confidence_threshold: 0.7,
                autopilot_refinery_threshold_kg: 500.0,
                wear_band_degraded_threshold: 0.5,
                wear_band_critical_threshold: 0.8,
                wear_band_degraded_efficiency: 0.75,
                wear_band_critical_efficiency: 0.5,
            },
        }
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
                    facilities: FacilitiesState {
                        compute_units_total: 0,
                        power_per_compute_unit_per_tick: 0.0,
                        efficiency: 1.0,
                    },
                    modules: vec![],
                },
            )]),
            research: ResearchState {
                unlocked: HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
            },
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
