use sim_core::{
    inventory_volume_m3, mine_duration, shortest_hop_count, trade, AnomalyTag, AsteroidId,
    AsteroidState, Command, CommandEnvelope, CommandId, ComponentId, DomainProgress, GameContent,
    GameState, InputAmount, InputFilter, InventoryItem, ModuleBehaviorDef, ModuleKindState, NodeId,
    PrincipalId, ShipId, ShipState, SiteId, TaskKind, TechDef, TechId, TradeItemSpec,
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

/// Geometric mean of per-domain ratios (accumulated / required), clamped to [0, 1].
fn compute_sufficiency(tech: &TechDef, progress: Option<&DomainProgress>) -> f32 {
    if tech.domain_requirements.is_empty() {
        return 1.0;
    }
    let ratios: Vec<f32> = tech
        .domain_requirements
        .iter()
        .map(|(domain, required)| {
            let accumulated =
                progress.map_or(0.0, |p| p.points.get(domain).copied().unwrap_or(0.0));
            (accumulated / required).min(1.0)
        })
        .collect();
    let product: f32 = ratios.iter().product();
    product.powf(1.0 / ratios.len() as f32)
}

/// Auto-assigns unassigned labs to the highest-priority eligible tech.
fn lab_assignment_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();

    for station in state.stations.values() {
        for module in &station.modules {
            let ModuleKindState::Lab(lab_state) = &module.kind_state else {
                continue;
            };
            // Skip labs that are already assigned to an eligible (non-unlocked) tech
            if let Some(ref tech_id) = lab_state.assigned_tech {
                if !state.research.unlocked.contains(tech_id) {
                    continue;
                }
            }

            // Find lab's domain from def
            let Some(def) = content.module_defs.get(&module.def_id) else {
                continue;
            };
            let ModuleBehaviorDef::Lab(lab_def) = &def.behavior else {
                continue;
            };

            // Find eligible techs that need this lab's domain
            let mut candidates: Vec<(TechId, f32)> = content
                .techs
                .iter()
                .filter(|tech| {
                    !state.research.unlocked.contains(&tech.id)
                        && tech
                            .prereqs
                            .iter()
                            .all(|p| state.research.unlocked.contains(p))
                        && tech.domain_requirements.contains_key(&lab_def.domain)
                })
                .map(|tech| {
                    let sufficiency =
                        compute_sufficiency(tech, state.research.evidence.get(&tech.id));
                    (tech.id.clone(), sufficiency)
                })
                .collect();
            // Highest sufficiency first (closest to unlock), then by ID for determinism
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0 .0.cmp(&b.0 .0)));

            if let Some((tech_id, _)) = candidates.first() {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::AssignLabTech {
                        station_id: station.id.clone(),
                        module_id: module.id.clone(),
                        tech_id: Some(tech_id.clone()),
                    },
                ));
            }
        }
    }
    commands
}

/// Maximum fleet size the autopilot will build toward.
/// Autopilot won't spend more than this fraction of balance on a single thruster import.
const AUTOPILOT_BUDGET_CAP_FRACTION: f64 = 0.05;

/// Emits Import commands for thrusters when a shipyard is ready and conditions are met.
///
/// Guards (VIO-41):
/// 1. Trade must be unlocked (tick >= `trade_unlock_tick()`).
/// 2. `tech_ship_construction` must be researched.
/// 3. Station must have fewer thrusters than the shipyard recipe requires.
/// 4. Budget cap: import cost must be < `AUTOPILOT_BUDGET_CAP_FRACTION` of current balance.
fn thruster_import_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();

    // Gate 1: Trade unlock
    if state.meta.tick < sim_core::trade_unlock_tick(content.constants.minutes_per_tick) {
        return commands;
    }

    // Gate 2: Tech requirement
    let tech_unlocked = state
        .research
        .unlocked
        .contains(&TechId("tech_ship_construction".to_string()));
    if !tech_unlocked {
        return commands;
    }

    let mut sorted_stations: Vec<_> = state.stations.values().collect();
    sorted_stations.sort_by(|a, b| a.id.0.cmp(&b.id.0));

    // Look up the shipyard recipe's thruster requirement from content.
    let required_thrusters = content
        .module_defs
        .get("module_shipyard")
        .and_then(|def| match &def.behavior {
            ModuleBehaviorDef::Assembler(asm) => asm.recipes.first(),
            _ => None,
        })
        .map_or(4, |recipe| {
            recipe
                .inputs
                .iter()
                .find_map(|input| match (&input.filter, &input.amount) {
                    (InputFilter::Component(cid), InputAmount::Count(n)) if cid.0 == "thruster" => {
                        Some(*n)
                    }
                    _ => None,
                })
                .unwrap_or(4)
        });

    for station in sorted_stations {
        // Find the shipyard module — must be enabled
        let has_shipyard = station
            .modules
            .iter()
            .any(|module| module.def_id == "module_shipyard" && module.enabled);
        if !has_shipyard {
            continue;
        }

        // Count current thrusters in inventory
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
        if thruster_count >= required_thrusters {
            continue; // Already have enough for the recipe
        }

        let needed = required_thrusters - thruster_count;
        let item_spec = TradeItemSpec::Component {
            component_id: ComponentId("thruster".to_string()),
            count: needed,
        };

        // Gate 5: Budget cap — cost must be < 5% of current balance
        let Some(cost) = trade::compute_import_cost(&item_spec, &content.pricing, content) else {
            continue;
        };
        if cost > state.balance * AUTOPILOT_BUDGET_CAP_FRACTION {
            continue;
        }

        commands.push(make_cmd(
            owner,
            state.meta.tick,
            next_id,
            Command::Import {
                station_id: station.id.clone(),
                item_spec,
            },
        ));
    }
    commands
}

/// Jettisons all slag from stations whose storage usage exceeds the threshold.
fn slag_jettison_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    let mut commands = Vec::new();
    let threshold = content.constants.autopilot_slag_jettison_pct;

    for station in state.stations.values() {
        let used_m3 = inventory_volume_m3(&station.inventory, content);
        let used_pct = used_m3 / station.cargo_capacity_m3;

        if used_pct >= threshold
            && station
                .inventory
                .iter()
                .any(|i| matches!(i, InventoryItem::Slag { .. }))
        {
            commands.push(make_cmd(
                owner,
                state.meta.tick,
                next_id,
                Command::JettisonSlag {
                    station_id: station.id.clone(),
                },
            ));
        }
    }
    commands
}

// ---------------------------------------------------------------------------
// AutopilotController
// ---------------------------------------------------------------------------

impl CommandSource for AutopilotController {
    #[allow(clippy::too_many_lines)]
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut commands = station_module_commands(state, content, &owner, next_command_id);
        commands.extend(lab_assignment_commands(
            state,
            content,
            &owner,
            next_command_id,
        ));
        commands.extend(thruster_import_commands(
            state,
            content,
            &owner,
            next_command_id,
        ));
        commands.extend(slag_jettison_commands(
            state,
            content,
            &owner,
            next_command_id,
        ));

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
        AsteroidId, AsteroidKnowledge, AsteroidState, InventoryItem, LotId, NodeId, ShipId,
        StationId,
    };
    use std::collections::HashMap;

    /// Autopilot tests disable research (no compute/power) and remove scan sites.
    fn autopilot_content() -> sim_core::GameContent {
        let mut content = base_content();
        content.techs.clear();
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
                power_stalled: false,
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
        content.module_defs.insert(
            "module_maintenance_bay".to_string(),
            sim_core::ModuleDef {
                id: "module_maintenance_bay".to_string(),
                name: "Maintenance Bay".to_string(),
                mass_kg: 2000.0,
                volume_m3: 5.0,
                power_consumption_per_run: 5.0,
                wear_per_run: 0.0,
                behavior: sim_core::ModuleBehaviorDef::Maintenance(sim_core::MaintenanceDef {
                    repair_interval_minutes: 30,
                    repair_interval_ticks: 30,
                    wear_reduction_per_run: 0.2,
                    repair_kit_cost: 1,
                    repair_threshold: 0.0,
                }),
            },
        );
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
                power_stalled: false,
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

    // --- Slag jettison tests ---

    #[test]
    fn test_autopilot_jettisons_slag_above_threshold() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        // Set small capacity so slag easily exceeds 75% threshold
        station.cargo_capacity_m3 = 100.0;
        // Add slag that takes up ~80% of capacity (slag density = 2500 kg/m3, 200kg = 0.08 m3)
        // Actually, let's use a volume that makes sense. We need volume > 75 m3.
        // Slag density is 2500 kg/m3. So 200_000 kg = 80 m3
        station.inventory.push(sim_core::InventoryItem::Slag {
            kg: 200_000.0,
            composition: HashMap::from([("slag".to_string(), 1.0)]),
        });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::JettisonSlag { .. })),
            "autopilot should issue JettisonSlag when storage usage exceeds threshold"
        );
    }

    #[test]
    fn test_autopilot_does_not_jettison_below_threshold() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        // Small amount of slag, well below 75% of 10,000 m3
        station.inventory.push(sim_core::InventoryItem::Slag {
            kg: 10.0,
            composition: HashMap::from([("slag".to_string(), 1.0)]),
        });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::JettisonSlag { .. })),
            "autopilot should NOT jettison slag when storage usage is below threshold"
        );
    }

    // --- Lab assignment tests ---

    fn lab_content_and_state() -> (sim_core::GameContent, sim_core::GameState) {
        let mut content = base_content();
        // Clear default techs and add one with domain requirement
        content.techs.clear();
        content.techs.push(sim_core::TechDef {
            id: TechId("tech_materials_v1".to_string()),
            name: "Materials Research".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::from([(sim_core::ResearchDomain::Materials, 100.0)]),
            accepted_data: vec![sim_core::DataKind::MiningData],
            difficulty: 10.0,
            effects: vec![],
        });
        // Add lab module def
        content.module_defs.insert(
            "module_materials_lab".to_string(),
            sim_core::ModuleDef {
                id: "module_materials_lab".to_string(),
                name: "Materials Lab".to_string(),
                mass_kg: 1000.0,
                volume_m3: 3.0,
                power_consumption_per_run: 2.0,
                wear_per_run: 0.01,
                behavior: sim_core::ModuleBehaviorDef::Lab(sim_core::LabDef {
                    domain: sim_core::ResearchDomain::Materials,
                    data_consumption_per_run: 5.0,
                    research_points_per_run: 10.0,
                    accepted_data: vec![sim_core::DataKind::MiningData],
                    research_interval_minutes: 10,
                    research_interval_ticks: 10,
                }),
            },
        );
        content.constants.station_power_available_per_tick = 0.0;
        let mut state = base_state(&content);
        state.scan_sites.clear();
        let station_id = StationId("station_earth_orbit".to_string());
        if let Some(station) = state.stations.get_mut(&station_id) {
            station.power_available_per_tick = 0.0;
        }
        (content, state)
    }

    #[test]
    fn test_autopilot_installs_lab_module() {
        let (content, mut state) = lab_content_and_state();

        let station_id = StationId("station_earth_orbit".to_string());
        state.stations.get_mut(&station_id).unwrap().inventory.push(
            sim_core::InventoryItem::Module {
                item_id: sim_core::ModuleItemId("module_item_lab_001".to_string()),
                module_def_id: "module_materials_lab".to_string(),
            },
        );

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::InstallModule { .. })),
            "autopilot should issue InstallModule for lab module in station inventory"
        );
    }

    #[test]
    fn test_autopilot_assigns_lab_to_eligible_tech() {
        let (content, mut state) = lab_content_and_state();

        let station_id = StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_lab_001".to_string()),
                def_id: "module_materials_lab".to_string(),
                enabled: true,
                kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                    ticks_since_last_run: 0,
                    assigned_tech: None,
                    starved: false,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignLabTech {
                    tech_id: Some(ref t),
                    ..
                } if t.0 == "tech_materials_v1"
            )),
            "autopilot should assign unassigned lab to eligible tech"
        );
    }

    #[test]
    fn test_autopilot_skips_assigned_lab() {
        let (content, mut state) = lab_content_and_state();

        let station_id = StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_lab_001".to_string()),
                def_id: "module_materials_lab".to_string(),
                enabled: true,
                kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                    ticks_since_last_run: 0,
                    assigned_tech: Some(TechId("tech_materials_v1".to_string())),
                    starved: false,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands
                .iter()
                .any(|cmd| matches!(&cmd.command, sim_core::Command::AssignLabTech { .. })),
            "autopilot should NOT issue AssignLabTech for already-assigned lab"
        );
    }

    #[test]
    fn test_autopilot_reassigns_lab_from_unlocked_tech() {
        let (mut content, mut state) = lab_content_and_state();

        // Add a second tech so there's something to reassign to
        content.techs.push(sim_core::TechDef {
            id: TechId("tech_materials_v2".to_string()),
            name: "Materials Research v2".to_string(),
            prereqs: vec![TechId("tech_materials_v1".to_string())],
            domain_requirements: HashMap::from([(sim_core::ResearchDomain::Materials, 200.0)]),
            accepted_data: vec![sim_core::DataKind::MiningData],
            difficulty: 10.0,
            effects: vec![],
        });

        // Mark tech_materials_v1 as unlocked (its prereq for v2)
        state
            .research
            .unlocked
            .insert(TechId("tech_materials_v1".to_string()));

        // Lab is assigned to the already-unlocked tech
        let station_id = StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_lab_001".to_string()),
                def_id: "module_materials_lab".to_string(),
                enabled: true,
                kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                    ticks_since_last_run: 0,
                    assigned_tech: Some(TechId("tech_materials_v1".to_string())),
                    starved: false,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignLabTech {
                    tech_id: Some(ref t),
                    ..
                } if t.0 == "tech_materials_v2"
            )),
            "autopilot should reassign lab from unlocked tech to next eligible tech"
        );
    }

    // --- Engineering lab assignment test ---

    #[test]
    fn test_lab_assignment_assigns_engineering_lab_to_ship_construction() {
        let mut content = base_content();
        content.techs.clear();
        content.techs.push(sim_core::TechDef {
            id: TechId("tech_ship_construction".to_string()),
            name: "Ship Construction".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::from([(sim_core::ResearchDomain::Engineering, 200.0)]),
            accepted_data: vec![sim_core::DataKind::EngineeringData],
            difficulty: 500.0,
            effects: vec![],
        });
        content.module_defs.insert(
            "module_engineering_lab".to_string(),
            sim_core::ModuleDef {
                id: "module_engineering_lab".to_string(),
                name: "Engineering Lab".to_string(),
                mass_kg: 4000.0,
                volume_m3: 8.0,
                power_consumption_per_run: 12.0,
                wear_per_run: 0.005,
                behavior: sim_core::ModuleBehaviorDef::Lab(sim_core::LabDef {
                    domain: sim_core::ResearchDomain::Engineering,
                    data_consumption_per_run: 10.0,
                    research_points_per_run: 5.0,
                    accepted_data: vec![sim_core::DataKind::EngineeringData],
                    research_interval_minutes: 1,
                    research_interval_ticks: 1,
                }),
            },
        );
        content.constants.station_power_available_per_tick = 0.0;

        let mut state = base_state(&content);
        state.scan_sites.clear();
        let station_id = StationId("station_earth_orbit".to_string());
        if let Some(station) = state.stations.get_mut(&station_id) {
            station.power_available_per_tick = 0.0;
        }

        // Install engineering lab module on the station
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .modules
            .push(sim_core::ModuleState {
                id: sim_core::ModuleInstanceId("module_inst_eng_lab_001".to_string()),
                def_id: "module_engineering_lab".to_string(),
                enabled: true,
                kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                    ticks_since_last_run: 0,
                    assigned_tech: None,
                    starved: false,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
            });

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignLabTech {
                    tech_id: Some(ref t),
                    ..
                } if t.0 == "tech_ship_construction"
            )),
            "autopilot should assign engineering lab to tech_ship_construction"
        );
    }

    // --- Thruster import tests ---

    /// Helper to set up state for thruster import tests.
    /// Shipyard recipe requires 4 thrusters, assembly interval is 1440 ticks.
    fn thruster_import_setup() -> (sim_core::GameContent, sim_core::GameState) {
        let mut content = base_content();
        content.techs.clear();
        content.constants.station_power_available_per_tick = 0.0;

        // Add shipyard module def with a recipe requiring 4 thrusters
        content.module_defs.insert(
            "module_shipyard".to_string(),
            sim_core::ModuleDef {
                id: "module_shipyard".to_string(),
                name: "Shipyard".to_string(),
                mass_kg: 5000.0,
                volume_m3: 20.0,
                power_consumption_per_run: 25.0,
                wear_per_run: 0.02,
                behavior: sim_core::ModuleBehaviorDef::Assembler(sim_core::AssemblerDef {
                    assembly_interval_minutes: 1440,
                    assembly_interval_ticks: 1440,
                    recipes: vec![sim_core::RecipeDef {
                        id: "recipe_test_ship".to_string(),
                        inputs: vec![
                            sim_core::RecipeInput {
                                filter: sim_core::InputFilter::Element("Fe".to_string()),
                                amount: sim_core::InputAmount::Kg(5000.0),
                            },
                            sim_core::RecipeInput {
                                filter: sim_core::InputFilter::Component(ComponentId(
                                    "thruster".to_string(),
                                )),
                                amount: sim_core::InputAmount::Count(4),
                            },
                        ],
                        outputs: vec![sim_core::OutputSpec::Ship {
                            cargo_capacity_m3: 50.0,
                        }],
                        efficiency: 1.0,
                    }],
                    max_stock: HashMap::new(),
                }),
            },
        );

        // Add thruster component def (needed for mass calculation)
        content.component_defs.push(sim_core::ComponentDef {
            id: "thruster".to_string(),
            name: "Thruster".to_string(),
            mass_kg: 200.0,
            volume_m3: 2.0,
        });

        // Set up pricing for thruster
        content.pricing = sim_core::PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: HashMap::from([(
                "thruster".to_string(),
                sim_core::PricingEntry {
                    base_price_per_unit: 50_000.0,
                    importable: true,
                    exportable: true,
                },
            )]),
        };

        let mut state = base_state(&content);
        state.scan_sites.clear();

        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.power_available_per_tick = 0.0;

        // Install enabled shipyard module
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("module_inst_shipyard_001".to_string()),
            def_id: "module_shipyard".to_string(),
            enabled: true,
            kind_state: sim_core::ModuleKindState::Assembler(sim_core::AssemblerState {
                ticks_since_last_run: 0,
                stalled: false,
                capped: false,
                cap_override: HashMap::new(),
            }),
            wear: sim_core::WearState::default(),
            power_stalled: false,
        });

        // Add 5000 kg Fe to station inventory
        station.inventory.push(sim_core::InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 5000.0,
            quality: 1.0,
        });

        // Unlock tech_ship_construction
        state
            .research
            .unlocked
            .insert(TechId("tech_ship_construction".to_string()));

        // Set high balance and advance past trade unlock
        state.balance = 10_000_000.0;
        state.meta.tick = sim_core::trade_unlock_tick(content.constants.minutes_per_tick);

        (content, state)
    }

    #[test]
    fn test_autopilot_imports_thrusters_when_shipyard_ready() {
        let (content, state) = thruster_import_setup();

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let import_cmd = commands.iter().find(|cmd| {
            matches!(
                &cmd.command,
                Command::Import {
                    item_spec: TradeItemSpec::Component {
                        component_id,
                        count: 4,
                    },
                    ..
                } if component_id.0 == "thruster"
            )
        });

        assert!(
            import_cmd.is_some(),
            "autopilot should import 4 thrusters when shipyard is ready and past interval"
        );
    }

    #[test]
    fn test_autopilot_no_thruster_import_when_balance_low() {
        let (content, mut state) = thruster_import_setup();
        state.balance = 100.0;

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Import {
                    item_spec: TradeItemSpec::Component { .. },
                    ..
                }
            )),
            "autopilot should NOT import thrusters when balance is too low"
        );
    }

    #[test]
    fn test_autopilot_no_thruster_import_when_tech_not_unlocked() {
        let (content, mut state) = thruster_import_setup();
        state.research.unlocked.clear();

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Import {
                    item_spec: TradeItemSpec::Component { .. },
                    ..
                }
            )),
            "autopilot should NOT import thrusters when tech_ship_construction is not unlocked"
        );
    }

    #[test]
    fn test_autopilot_no_thruster_import_exceeds_budget_cap() {
        let (content, mut state) = thruster_import_setup();
        // Set balance so that import cost > 5% of balance.
        // 4 thrusters: (50_000 * 4) + (200 * 4 * 100) = 200_000 + 80_000 = 280_000
        // 280_000 / 0.05 = 5_600_000 — so balance below that should block
        state.balance = 5_000_000.0;

        let mut autopilot = AutopilotController;
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Import {
                    item_spec: TradeItemSpec::Component { .. },
                    ..
                }
            )),
            "autopilot should NOT import thrusters when cost exceeds 5% budget cap"
        );
    }
}
