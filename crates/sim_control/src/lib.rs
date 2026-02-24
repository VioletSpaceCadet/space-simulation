use sim_core::{
    mine_duration, shortest_hop_count, AnomalyTag, AsteroidId, AsteroidState, Command,
    CommandEnvelope, CommandId, GameContent, GameState, InventoryItem, ModuleKindState, NodeId,
    PrincipalId, ShipId, ShipState, SiteId, TaskKind, TechId,
};

pub trait CommandSource {
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope>;
}

/// Drives ships automatically:
/// 1. Deposit cargo if hold is non-empty.
/// 2. Mine the best available deep-scanned asteroid.
/// 3. Deep-scan `IronRich` asteroids to unlock mining targets.
/// 4. Survey unscanned sites.
pub struct AutopilotController;

const AUTOPILOT_OWNER: &str = "principal_autopilot";

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Wraps `task` in a Transit if `from` and `to` are different nodes; else returns `task` as-is.
fn maybe_transit(task: TaskKind, from: &NodeId, to: &NodeId, content: &GameContent) -> TaskKind {
    match shortest_hop_count(from, to, &content.solar_system) {
        Some(0) | None => task,
        Some(hops) => TaskKind::Transit {
            destination: to.clone(),
            total_ticks: hops * content.constants.travel_ticks_per_hop,
            then: Box::new(task),
        },
    }
}

/// Allocates a command ID and builds a `CommandEnvelope`.
fn make_cmd(
    owner: &PrincipalId,
    tick: u64,
    next_id: &mut u64,
    command: Command,
) -> CommandEnvelope {
    let cmd_id = CommandId(format!("cmd_{:06}", *next_id));
    *next_id += 1;
    CommandEnvelope {
        id: cmd_id,
        issued_by: owner.clone(),
        issued_tick: tick,
        execute_at_tick: tick,
        command,
    }
}

/// Emits commands to install, enable, and configure station modules.
fn station_module_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();
    for station in state.stations.values() {
        for item in &station.inventory {
            if let InventoryItem::Module { item_id, .. } = item {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::InstallModule {
                        station_id: station.id.clone(),
                        module_item_id: item_id.clone(),
                    },
                ));
            }
        }
        for module in &station.modules {
            // Re-enable disabled modules, but not if auto-disabled due to max wear
            if !module.enabled && module.wear.wear < 1.0 {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::SetModuleEnabled {
                        station_id: station.id.clone(),
                        module_id: module.id.clone(),
                        enabled: true,
                    },
                ));
            }
            if let ModuleKindState::Processor(ps) = &module.kind_state {
                if ps.threshold_kg == 0.0 {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::SetModuleThreshold {
                            station_id: station.id.clone(),
                            module_id: module.id.clone(),
                            threshold_kg: content.constants.autopilot_refinery_threshold_kg,
                        },
                    ));
                }
            }
        }
    }
    commands
}

/// Returns idle autopilot ships sorted by ID for determinism.
fn collect_idle_ships(state: &GameState, owner: &PrincipalId) -> Vec<ShipId> {
    let mut ships: Vec<ShipId> = state
        .ships
        .values()
        .filter(|ship| {
            ship.owner == *owner
                && ship
                    .task
                    .as_ref()
                    .is_none_or(|t| matches!(t.kind, TaskKind::Idle))
        })
        .map(|ship| ship.id.clone())
        .collect();
    ships.sort_by(|a, b| a.0.cmp(&b.0));
    ships
}

/// Returns `IronRich` asteroid IDs above confidence threshold with unknown composition, sorted by ID.
fn collect_deep_scan_candidates(state: &GameState, content: &GameContent) -> Vec<AsteroidId> {
    let mut candidates: Vec<AsteroidId> = state
        .asteroids
        .values()
        .filter(|asteroid| {
            asteroid.knowledge.composition.is_none()
                && asteroid.knowledge.tag_beliefs.iter().any(|(tag, conf)| {
                    *tag == AnomalyTag::IronRich
                        && *conf > content.constants.autopilot_iron_rich_confidence_threshold
                })
        })
        .map(|a| a.id.clone())
        .collect();
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    candidates
}

/// Mining value for sorting: `mass_kg × Fe_fraction`.
fn fe_mining_value(asteroid: &AsteroidState) -> f32 {
    asteroid.mass_kg
        * asteroid
            .knowledge
            .composition
            .as_ref()
            .and_then(|c| c.get("Fe"))
            .copied()
            .unwrap_or(0.0)
}

/// Priority 1: if ship has ore, return a Deposit (or Transit→Deposit) task to the nearest station.
fn deposit_priority(
    ship: &ShipState,
    state: &GameState,
    content: &GameContent,
) -> Option<TaskKind> {
    if !ship
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Ore { .. }))
    {
        return None;
    }
    let station = state.stations.values().min_by_key(|s| {
        shortest_hop_count(&ship.location_node, &s.location_node, &content.solar_system)
            .unwrap_or(u64::MAX)
    })?;
    Some(maybe_transit(
        TaskKind::Deposit {
            station: station.id.clone(),
            blocked: false,
        },
        &ship.location_node,
        &station.location_node,
        content,
    ))
}

// ---------------------------------------------------------------------------
// AutopilotController
// ---------------------------------------------------------------------------

impl CommandSource for AutopilotController {
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut commands = station_module_commands(state, content, &owner, next_command_id);

        let idle_ships = collect_idle_ships(state, &owner);
        let deep_scan_unlocked = state
            .research
            .unlocked
            .contains(&TechId("tech_deep_scan_v1".to_string()));
        let deep_scan_candidates = collect_deep_scan_candidates(state, content);
        let mut next_deep_scan = deep_scan_candidates.iter();
        let mut next_site = state.scan_sites.iter();

        let mut mine_candidates: Vec<&AsteroidState> = state
            .asteroids
            .values()
            .filter(|a| a.mass_kg > 0.0 && a.knowledge.composition.is_some())
            .collect();
        mine_candidates.sort_by(|a, b| fe_mining_value(b).total_cmp(&fe_mining_value(a)));
        let mut next_mine = mine_candidates.iter();

        for ship_id in idle_ships {
            let ship = &state.ships[&ship_id];

            // Priority 1: ship has ore → deposit at nearest station.
            if let Some(task) = deposit_priority(ship, state, content) {
                commands.push(make_cmd(
                    &ship.owner,
                    state.meta.tick,
                    next_command_id,
                    Command::AssignShipTask {
                        ship_id,
                        task_kind: task,
                    },
                ));
                continue;
            }

            // Priority 2: mine best available asteroid.
            if let Some(asteroid) = next_mine.next() {
                let task = maybe_transit(
                    TaskKind::Mine {
                        asteroid: asteroid.id.clone(),
                        duration_ticks: mine_duration(asteroid, ship, content),
                    },
                    &ship.location_node,
                    &asteroid.location_node,
                    content,
                );
                commands.push(make_cmd(
                    &ship.owner,
                    state.meta.tick,
                    next_command_id,
                    Command::AssignShipTask {
                        ship_id,
                        task_kind: task,
                    },
                ));
                continue;
            }

            // Priority 3: deep scan (enables future mining).
            if deep_scan_unlocked {
                if let Some(asteroid_id) = next_deep_scan.next() {
                    let node = state.asteroids[asteroid_id].location_node.clone();
                    let task = maybe_transit(
                        TaskKind::DeepScan {
                            asteroid: asteroid_id.clone(),
                        },
                        &ship.location_node,
                        &node,
                        content,
                    );
                    commands.push(make_cmd(
                        &ship.owner,
                        state.meta.tick,
                        next_command_id,
                        Command::AssignShipTask {
                            ship_id,
                            task_kind: task,
                        },
                    ));
                    continue;
                }
            }

            // Priority 4: survey unscanned sites.
            if let Some(site) = next_site.next() {
                let task = maybe_transit(
                    TaskKind::Survey {
                        site: SiteId(site.id.0.clone()),
                    },
                    &ship.location_node,
                    &site.node,
                    content,
                );
                commands.push(make_cmd(
                    &ship.owner,
                    state.meta.tick,
                    next_command_id,
                    Command::AssignShipTask {
                        ship_id,
                        task_kind: task,
                    },
                ));
            }
        }

        commands
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{
        test_fixtures::{base_content, base_state},
        AsteroidId, AsteroidKnowledge, AsteroidState, FacilitiesState, InventoryItem, LotId,
        NodeId, ShipId, StationId,
    };
    use std::collections::HashMap;

    /// Autopilot tests disable research (no compute/power) and remove scan sites.
    fn autopilot_content() -> sim_core::GameContent {
        let mut content = base_content();
        content.techs.clear();
        content.constants.station_compute_units_total = 0;
        content.constants.station_power_per_compute_unit_per_tick = 0.0;
        content.constants.station_power_available_per_tick = 0.0;
        content
    }

    fn autopilot_state(content: &sim_core::GameContent) -> sim_core::GameState {
        let mut state = base_state(content);
        state.scan_sites.clear();
        // Autopilot tests don't need research compute power on the station.
        let station_id = StationId("station_earth_orbit".to_string());
        if let Some(station) = state.stations.get_mut(&station_id) {
            station.power_available_per_tick = 0.0;
            station.facilities = FacilitiesState {
                compute_units_total: 0,
                power_per_compute_unit_per_tick: 0.0,
                efficiency: 1.0,
            };
        }
        state
    }

    #[test]
    fn test_autopilot_assigns_mine_when_asteroid_known() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let asteroid_id = AsteroidId("asteroid_0001".to_string());
        state.asteroids.insert(
            asteroid_id.clone(),
            AsteroidState {
                id: asteroid_id.clone(),
                location_node: NodeId("node_test".to_string()),
                true_composition: HashMap::from([("Fe".to_string(), 1.0)]),
                anomaly_tags: vec![],
                mass_kg: 500.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: Some(HashMap::from([("Fe".to_string(), 1.0)])),
                },
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Mine { .. },
                    ..
                }
            )),
            "autopilot should assign Mine task when deep-scanned asteroid is available"
        );
    }

    #[test]
    fn test_autopilot_assigns_deposit_when_ship_has_cargo() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let ship_id = ShipId("ship_0001".to_string());
        state
            .ships
            .get_mut(&ship_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Ore {
                lot_id: LotId("lot_test_0001".to_string()),
                asteroid_id: AsteroidId("asteroid_test".to_string()),
                kg: 100.0,
                composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Deposit { .. },
                    ..
                } | sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Transit { .. },
                    ..
                }
            )),
            "autopilot should assign Deposit (or Transit→Deposit) when ship has cargo"
        );
    }

    #[test]
    fn test_autopilot_installs_module_in_station_inventory() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = sim_core::StationId("station_earth_orbit".to_string());
        state.stations.get_mut(&station_id).unwrap().inventory.push(
            sim_core::InventoryItem::Module {
                item_id: sim_core::ModuleItemId("module_item_0001".to_string()),
                module_def_id: "module_basic_iron_refinery".to_string(),
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::InstallModule { .. })),
            "autopilot should issue InstallModule when Module item is in station inventory"
        );
    }

    #[test]
    fn test_autopilot_enables_disabled_module() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = sim_core::StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_0001".to_string()),
                def_id: "module_basic_iron_refinery".to_string(),
                enabled: false,
                kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                    threshold_kg: 0.0,
                    ticks_since_last_run: 0,
                    stalled: false,
                }),
                wear: sim_core::WearState::default(),
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::SetModuleEnabled { enabled: true, .. }
            )),
            "autopilot should enable a disabled installed module"
        );
    }

    #[test]
    fn test_autopilot_installs_maintenance_bay() {
        let mut content = autopilot_content();
        content.module_defs.push(sim_core::ModuleDef {
            id: "module_maintenance_bay".to_string(),
            name: "Maintenance Bay".to_string(),
            mass_kg: 2000.0,
            volume_m3: 5.0,
            power_consumption_per_run: 5.0,
            wear_per_run: 0.0,
            behavior: sim_core::ModuleBehaviorDef::Maintenance(sim_core::MaintenanceDef {
                repair_interval_ticks: 30,
                wear_reduction_per_run: 0.2,
                repair_kit_cost: 1,
            }),
        });
        let mut state = autopilot_state(&content);

        let station_id = StationId("station_earth_orbit".to_string());
        state.stations.get_mut(&station_id).unwrap().inventory.push(
            sim_core::InventoryItem::Module {
                item_id: sim_core::ModuleItemId("module_item_maint".to_string()),
                module_def_id: "module_maintenance_bay".to_string(),
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::InstallModule { .. })),
            "autopilot should install Maintenance Bay module"
        );
    }

    #[test]
    fn test_autopilot_surveys_when_no_asteroids_known() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        // Restore scan sites (autopilot_state clears them).
        state.scan_sites = vec![sim_core::ScanSite {
            id: SiteId("site_0001".to_string()),
            node: NodeId("node_test".to_string()),
            template_id: "tmpl_iron_rich".to_string(),
        }];
        // No asteroids (default), no cargo on ship → should fall through to Survey.

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Survey { .. },
                    ..
                }
            )),
            "autopilot should assign Survey when no asteroids are known"
        );
    }

    #[test]
    fn test_autopilot_handles_no_stations() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        // Give ship some cargo so deposit would normally fire.
        let ship_id = ShipId("ship_0001".to_string());
        state
            .ships
            .get_mut(&ship_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Ore {
                lot_id: LotId("lot_test_0001".to_string()),
                asteroid_id: AsteroidId("asteroid_test".to_string()),
                kg: 100.0,
                composition: HashMap::from([("Fe".to_string(), 1.0_f32)]),
            });

        // Remove all stations — deposit is impossible.
        state.stations.clear();

        // Add a scan site so ship has something to do.
        state.scan_sites = vec![sim_core::ScanSite {
            id: SiteId("site_0001".to_string()),
            node: NodeId("node_test".to_string()),
            template_id: "tmpl_iron_rich".to_string(),
        }];

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        // Should NOT crash, and should NOT issue a Deposit command.
        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Deposit { .. },
                    ..
                }
            )),
            "autopilot should not issue Deposit when no stations exist"
        );
    }

    #[test]
    fn test_autopilot_multiple_ships_get_different_assignments() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let owner = PrincipalId("principal_autopilot".to_string());

        // Add a second idle ship.
        let ship_2 = ShipId("ship_0002".to_string());
        state.ships.insert(
            ship_2.clone(),
            ShipState {
                id: ship_2,
                location_node: NodeId("node_test".to_string()),
                owner,
                inventory: vec![],
                cargo_capacity_m3: 20.0,
                task: None,
            },
        );

        // Provide two scan sites so each ship can get a different one.
        state.scan_sites = vec![
            sim_core::ScanSite {
                id: SiteId("site_0001".to_string()),
                node: NodeId("node_test".to_string()),
                template_id: "tmpl_iron_rich".to_string(),
            },
            sim_core::ScanSite {
                id: SiteId("site_0002".to_string()),
                node: NodeId("node_test".to_string()),
                template_id: "tmpl_iron_rich".to_string(),
            },
        ];

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        // Collect survey site targets from the commands.
        let survey_targets: Vec<&SiteId> = commands
            .iter()
            .filter_map(|cmd| match &cmd.command {
                sim_core::Command::AssignShipTask {
                    task_kind: TaskKind::Survey { site, .. },
                    ..
                } => Some(site),
                _ => None,
            })
            .collect();

        assert_eq!(
            survey_targets.len(),
            2,
            "both idle ships should receive Survey tasks"
        );
        assert_ne!(
            survey_targets[0], survey_targets[1],
            "each ship should survey a different site"
        );
    }

    #[test]
    fn test_autopilot_does_not_reenable_worn_out_module() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_0001".to_string()),
                def_id: "module_basic_iron_refinery".to_string(),
                enabled: false,
                kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                    threshold_kg: 500.0,
                    ticks_since_last_run: 0,
                    stalled: false,
                }),
                wear: sim_core::WearState { wear: 1.0 },
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::SetModuleEnabled { enabled: true, .. }
            )),
            "autopilot should NOT re-enable a module at max wear"
        );
    }
}
