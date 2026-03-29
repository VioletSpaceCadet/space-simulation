mod agents;
mod behaviors;
mod objectives;

use std::collections::BTreeMap;

use agents::ship_agent::ShipAgent;
use agents::station_agent::StationAgent;
use agents::Agent;
use behaviors::{AutopilotBehavior, AUTOPILOT_OWNER};
use sim_core::{CommandEnvelope, GameContent, GameState, PrincipalId, ShipId, StationId};

pub trait CommandSource {
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope>;
}

/// Drives ships automatically via flat behaviors (station management, labs,
/// crew, exports, etc.) plus hierarchical ship agents that convert objectives
/// into tactical commands.
pub struct AutopilotController {
    behaviors: Vec<Box<dyn AutopilotBehavior>>,
    ship_agents: BTreeMap<ShipId, ShipAgent>,
    station_agents: BTreeMap<StationId, StationAgent>,
    /// Cached owner ID — avoids per-tick String allocation.
    owner: PrincipalId,
}

impl AutopilotController {
    pub fn new() -> Self {
        Self {
            behaviors: behaviors::default_behaviors(),
            ship_agents: BTreeMap::new(),
            station_agents: BTreeMap::new(),
            owner: PrincipalId(AUTOPILOT_OWNER.to_string()),
        }
    }
}

impl Default for AutopilotController {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandSource for AutopilotController {
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();

        // 1. Run flat behaviors (station management, labs, crew, etc.)
        for behavior in &mut self.behaviors {
            commands.extend(behavior.generate(state, content, &self.owner, next_command_id));
        }

        // 2. Sync agent lifecycle — create for new entities, remove for deleted
        for (ship_id, ship) in &state.ships {
            if ship.owner == self.owner && !self.ship_agents.contains_key(ship_id) {
                self.ship_agents
                    .insert(ship_id.clone(), ShipAgent::new(ship_id.clone()));
            }
        }
        self.ship_agents
            .retain(|id, _| state.ships.contains_key(id));

        for station_id in state.stations.keys() {
            if !self.station_agents.contains_key(station_id) {
                self.station_agents
                    .insert(station_id.clone(), StationAgent::new(station_id.clone()));
            }
        }
        self.station_agents
            .retain(|id, _| state.stations.contains_key(id));

        // 3. Station agents assign objectives to co-located idle ships (AD1).
        // Note: deduplication is per-station (each station has its own shared
        // iterators). With multiple stations, two stations could theoretically
        // assign the same asteroid. This is acceptable — the current game has
        // one station, and multi-station deduplication belongs in the strategic
        // layer (future work).
        for station_agent in self.station_agents.values() {
            station_agent.assign_ship_objectives(
                &mut self.ship_agents,
                state,
                content,
                &self.owner,
            );
        }

        // 4. Ship agents run in BTreeMap order (deterministic by ShipId, AD2)
        for agent in self.ship_agents.values_mut() {
            commands.extend(agent.generate(state, content, &self.owner, next_command_id));
        }

        commands
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviors::AUTOPILOT_OWNER;
    use sim_core::{
        test_fixtures::{base_content, base_state, test_position, ModuleDefBuilder},
        AnomalyTag, AsteroidId, AsteroidKnowledge, AsteroidState, Command, ComponentDef,
        ComponentId, InventoryItem, LotId, PricingEntry, ShipId, ShipState, SiteId, StationId,
        TaskKind, TechId, TradeItemSpec,
    };
    use std::collections::HashMap;

    /// Autopilot tests disable research (no compute/power) and remove scan sites.
    fn autopilot_content() -> sim_core::GameContent {
        let mut content = base_content();
        content.techs.clear();
        content.constants.station_power_available_per_tick = 0.0;
        // Add minimal module defs for role-based autopilot lookups
        add_role_module_defs(&mut content);
        content
    }

    /// Insert stub module defs with roles for autopilot tests.
    fn add_role_module_defs(content: &mut sim_core::GameContent) {
        let stub = |id: &str, roles: Vec<&str>| ModuleDefBuilder::new(id).roles(roles).build();
        content.module_defs.insert(
            "module_electrolysis_unit".to_string(),
            stub(
                "module_electrolysis_unit",
                vec!["propellant", "propellant_support"],
            ),
        );
        content.module_defs.insert(
            "module_heating_unit".to_string(),
            stub("module_heating_unit", vec!["propellant_support"]),
        );
    }

    /// Rebuild module indexes for all stations (needed after adding modules in tests).
    fn rebuild_station_indexes(state: &mut sim_core::GameState, content: &sim_core::GameContent) {
        for station in state.stations.values_mut() {
            station.rebuild_module_index(content);
        }
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
                position: test_position(),
                true_composition: HashMap::from([("Fe".to_string(), 1.0)]),
                anomaly_tags: vec![],
                mass_kg: 500.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: Some(HashMap::from([("Fe".to_string(), 1.0)])),
                },
            },
        );

        let mut autopilot = AutopilotController::new();
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

        let mut autopilot = AutopilotController::new();
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

        let mut autopilot = AutopilotController::new();
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
                    selected_recipe: None,
                }),
                wear: sim_core::WearState::default(),
                power_stalled: false,
                module_priority: 0,
                assigned_crew: Default::default(),
                efficiency: 1.0,
                prev_crew_satisfied: true,
                thermal: None,
            });

        let mut autopilot = AutopilotController::new();
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
            ModuleDefBuilder::new("module_maintenance_bay")
                .name("Maintenance Bay")
                .mass(2000.0)
                .volume(5.0)
                .power(5.0)
                .behavior(sim_core::ModuleBehaviorDef::Maintenance(
                    sim_core::MaintenanceDef {
                        repair_interval_minutes: 30,
                        repair_interval_ticks: 30,
                        wear_reduction_per_run: 0.2,
                        repair_kit_cost: 1,
                        repair_threshold: 0.0,
                        maintenance_component_id: "repair_kit".to_string(),
                    },
                ))
                .build(),
        );
        let mut state = autopilot_state(&content);

        let station_id = StationId("station_earth_orbit".to_string());
        state.stations.get_mut(&station_id).unwrap().inventory.push(
            sim_core::InventoryItem::Module {
                item_id: sim_core::ModuleItemId("module_item_maint".to_string()),
                module_def_id: "module_maintenance_bay".to_string(),
            },
        );

        let mut autopilot = AutopilotController::new();
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
            position: test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        }];
        // No asteroids (default), no cargo on ship → should fall through to Survey.

        let mut autopilot = AutopilotController::new();
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
            position: test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        }];

        let mut autopilot = AutopilotController::new();
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
                position: test_position(),
                owner,
                inventory: vec![],
                cargo_capacity_m3: 20.0,
                task: None,
                speed_ticks_per_au: None,
                modifiers: sim_core::modifiers::ModifierSet::default(),
                hull_id: sim_core::HullId("hull_general_purpose".to_string()),
                fitted_modules: vec![],
                propellant_kg: 0.0,
                propellant_capacity_kg: 0.0,
                crew: Default::default(),
                leaders: Vec::new(),
            },
        );

        // Provide two scan sites so each ship can get a different one.
        state.scan_sites = vec![
            sim_core::ScanSite {
                id: SiteId("site_0001".to_string()),
                position: test_position(),
                template_id: "tmpl_iron_rich".to_string(),
            },
            sim_core::ScanSite {
                id: SiteId("site_0002".to_string()),
                position: test_position(),
                template_id: "tmpl_iron_rich".to_string(),
            },
        ];

        let mut autopilot = AutopilotController::new();
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
                    selected_recipe: None,
                }),
                wear: sim_core::WearState { wear: 1.0 },
                power_stalled: false,
                module_priority: 0,
                assigned_crew: Default::default(),
                efficiency: 1.0,
                prev_crew_satisfied: true,
                thermal: None,
            });

        let mut autopilot = AutopilotController::new();
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

        let mut autopilot = AutopilotController::new();
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

        let mut autopilot = AutopilotController::new();
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
            accepted_data: vec![sim_core::DataKind::AssayData],
            effects: vec![],
        });
        // Add lab module def
        content.module_defs.insert(
            "module_materials_lab".to_string(),
            ModuleDefBuilder::new("module_materials_lab")
                .name("Materials Lab")
                .mass(1000.0)
                .volume(3.0)
                .power(2.0)
                .wear(0.01)
                .behavior(sim_core::ModuleBehaviorDef::Lab(sim_core::LabDef {
                    domain: sim_core::ResearchDomain::Materials,
                    data_consumption_per_run: 5.0,
                    research_points_per_run: 10.0,
                    accepted_data: vec![sim_core::DataKind::AssayData],
                    research_interval_minutes: 10,
                    research_interval_ticks: 10,
                }))
                .build(),
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

        let mut autopilot = AutopilotController::new();
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
                module_priority: 0,
                assigned_crew: Default::default(),
                efficiency: 1.0,
                prev_crew_satisfied: true,
                thermal: None,
            });

        let mut autopilot = AutopilotController::new();
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
                module_priority: 0,
                assigned_crew: Default::default(),
                efficiency: 1.0,
                prev_crew_satisfied: true,
                thermal: None,
            });

        let mut autopilot = AutopilotController::new();
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
            accepted_data: vec![sim_core::DataKind::AssayData],
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
                module_priority: 0,
                assigned_crew: Default::default(),
                efficiency: 1.0,
                prev_crew_satisfied: true,
                thermal: None,
            });

        let mut autopilot = AutopilotController::new();
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
            domain_requirements: HashMap::from([(sim_core::ResearchDomain::Manufacturing, 200.0)]),
            accepted_data: vec![sim_core::DataKind::ManufacturingData],
            effects: vec![],
        });
        content.module_defs.insert(
            "module_engineering_lab".to_string(),
            ModuleDefBuilder::new("module_engineering_lab")
                .name("Engineering Lab")
                .mass(4000.0)
                .volume(8.0)
                .power(12.0)
                .wear(0.005)
                .behavior(sim_core::ModuleBehaviorDef::Lab(sim_core::LabDef {
                    domain: sim_core::ResearchDomain::Manufacturing,
                    data_consumption_per_run: 10.0,
                    research_points_per_run: 5.0,
                    accepted_data: vec![sim_core::DataKind::ManufacturingData],
                    research_interval_minutes: 1,
                    research_interval_ticks: 1,
                }))
                .build(),
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
                module_priority: 0,
                assigned_crew: Default::default(),
                efficiency: 1.0,
                prev_crew_satisfied: true,
                thermal: None,
            });

        let mut autopilot = AutopilotController::new();
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

    #[test]
    fn test_lab_assignment_cache_rebuilds_on_tech_unlock() {
        let mut content = base_content();
        content.techs.clear();
        // Two techs: basic (no prereqs) and advanced (requires basic)
        content.techs.push(sim_core::TechDef {
            id: TechId("tech_basic".to_string()),
            name: "Basic".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::from([(sim_core::ResearchDomain::Manufacturing, 10.0)]),
            accepted_data: vec![sim_core::DataKind::ManufacturingData],
            effects: vec![],
        });
        content.techs.push(sim_core::TechDef {
            id: TechId("tech_advanced".to_string()),
            name: "Advanced".to_string(),
            prereqs: vec![TechId("tech_basic".to_string())],
            domain_requirements: HashMap::from([(sim_core::ResearchDomain::Manufacturing, 50.0)]),
            accepted_data: vec![sim_core::DataKind::ManufacturingData],
            effects: vec![],
        });
        content.module_defs.insert(
            "module_mfg_lab".to_string(),
            ModuleDefBuilder::new("module_mfg_lab")
                .name("Mfg Lab")
                .mass(4000.0)
                .volume(8.0)
                .power(12.0)
                .wear(0.005)
                .behavior(sim_core::ModuleBehaviorDef::Lab(sim_core::LabDef {
                    domain: sim_core::ResearchDomain::Manufacturing,
                    data_consumption_per_run: 10.0,
                    research_points_per_run: 5.0,
                    accepted_data: vec![sim_core::DataKind::ManufacturingData],
                    research_interval_minutes: 1,
                    research_interval_ticks: 1,
                }))
                .build(),
        );
        content.constants.station_power_available_per_tick = 0.0;

        let mut state = base_state(&content);
        state.scan_sites.clear();
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.power_available_per_tick = 0.0;
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("lab_inst_001".to_string()),
            def_id: "module_mfg_lab".to_string(),
            enabled: true,
            kind_state: sim_core::ModuleKindState::Lab(sim_core::LabState {
                ticks_since_last_run: 0,
                assigned_tech: None,
                starved: false,
            }),
            wear: sim_core::WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            thermal: None,
        });

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;

        // First call: only tech_basic is eligible (tech_advanced prereq not met)
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);
        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignLabTech { tech_id: Some(ref t), .. }
                if t.0 == "tech_basic"
            )),
            "should assign to tech_basic (only eligible tech)"
        );

        // Unlock tech_basic — now tech_advanced becomes eligible
        state
            .research
            .unlocked
            .insert(TechId("tech_basic".to_string()));
        // Clear lab assignment so it needs reassignment
        let station = state.stations.get_mut(&station_id).unwrap();
        if let sim_core::ModuleKindState::Lab(ref mut lab) = station.modules[0].kind_state {
            lab.assigned_tech = Some(TechId("tech_basic".to_string()));
        }

        let commands = autopilot.generate_commands(&state, &content, &mut next_id);
        assert!(
            commands.iter().any(|cmd| matches!(
                &cmd.command,
                sim_core::Command::AssignLabTech { tech_id: Some(ref t), .. }
                if t.0 == "tech_advanced"
            )),
            "after unlocking tech_basic, cache should rebuild and assign to tech_advanced"
        );
    }

    // --- Task priority ordering tests ---

    #[test]
    fn test_task_priority_survey_before_mine_when_reordered() {
        let mut content = autopilot_content();
        // Reorder: Survey before Mine (normally Mine is higher priority)
        content.autopilot.task_priority = vec![
            "Deposit".to_string(),
            "Survey".to_string(),
            "Mine".to_string(),
            "DeepScan".to_string(),
        ];
        let mut state = autopilot_state(&content);

        // Add both a known asteroid (mine target) and a scan site (survey target)
        let asteroid_id = AsteroidId("asteroid_0001".to_string());
        state.asteroids.insert(
            asteroid_id.clone(),
            AsteroidState {
                id: asteroid_id,
                position: test_position(),
                true_composition: HashMap::from([("Fe".to_string(), 1.0)]),
                anomaly_tags: vec![],
                mass_kg: 500.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: Some(HashMap::from([("Fe".to_string(), 1.0)])),
                },
            },
        );
        // autopilot_state clears scan_sites, so add one back
        state.scan_sites.push(sim_core::ScanSite {
            id: sim_core::SiteId("site_test_001".to_string()),
            position: test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        });

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        // With reordered priority, ship should survey (not mine) since Survey is before Mine
        let assigned = commands
            .iter()
            .find(|cmd| matches!(&cmd.command, sim_core::Command::AssignShipTask { .. }));
        assert!(assigned.is_some(), "should assign a task");
        let is_survey = matches!(
            &assigned.unwrap().command,
            sim_core::Command::AssignShipTask {
                task_kind: TaskKind::Survey { .. },
                ..
            }
        );
        assert!(
            is_survey,
            "with Survey before Mine in task_priority, ship should survey first"
        );
    }

    #[test]
    fn test_empty_task_priority_assigns_nothing() {
        let mut content = autopilot_content();
        content.autopilot.task_priority = vec![];
        let state = autopilot_state(&content);

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let has_ship_task = commands
            .iter()
            .any(|cmd| matches!(&cmd.command, sim_core::Command::AssignShipTask { .. }));
        assert!(
            !has_ship_task,
            "empty task_priority should assign no ship tasks"
        );
    }

    // --- Thruster import tests ---

    /// Helper to set up state for thruster import tests.
    /// Shipyard recipe requires 4 thrusters, assembly interval is 1440 ticks.
    fn thruster_import_setup() -> (sim_core::GameContent, sim_core::GameState) {
        let mut content = base_content();
        content.techs.clear();
        content.constants.station_power_available_per_tick = 0.0;

        // Add a hull def for the shipyard test
        content.hulls.insert(
            sim_core::HullId("hull_test_ship".to_string()),
            sim_core::HullDef {
                id: sim_core::HullId("hull_test_ship".to_string()),
                name: "Test Ship Hull".to_string(),
                mass_kg: 1000.0,
                cargo_capacity_m3: 50.0,
                base_speed_ticks_per_au: 2000,
                base_propellant_capacity_kg: 100.0,
                slots: vec![],
                bonuses: vec![],
                required_tech: None,
                tags: vec![],
            },
        );

        // Add shipyard recipe to catalog
        let ship_recipe = sim_core::RecipeDef {
            id: sim_core::RecipeId("recipe_test_ship".to_string()),
            inputs: vec![
                sim_core::RecipeInput {
                    filter: sim_core::InputFilter::Element("Fe".to_string()),
                    amount: sim_core::InputAmount::Kg(5000.0),
                },
                sim_core::RecipeInput {
                    filter: sim_core::InputFilter::Component(ComponentId("thruster".to_string())),
                    amount: sim_core::InputAmount::Count(4),
                },
            ],
            outputs: vec![sim_core::OutputSpec::Ship {
                hull_id: sim_core::HullId("hull_test_ship".to_string()),
            }],
            efficiency: 1.0,
            thermal_req: None,
            required_tech: None,
            tags: vec![],
        };
        content.recipes.insert(ship_recipe.id.clone(), ship_recipe);

        // Add shipyard module def with a recipe requiring 4 thrusters
        content.module_defs.insert(
            "module_shipyard".to_string(),
            ModuleDefBuilder::new("module_shipyard")
                .name("Shipyard")
                .mass(5000.0)
                .volume(20.0)
                .power(25.0)
                .wear(0.02)
                .behavior(sim_core::ModuleBehaviorDef::Assembler(
                    sim_core::AssemblerDef {
                        assembly_interval_minutes: 1440,
                        assembly_interval_ticks: 1440,
                        recipes: vec![sim_core::RecipeId("recipe_test_ship".to_string())],
                        max_stock: HashMap::new(),
                    },
                ))
                .roles(vec!["shipyard"])
                .build(),
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
            items: [(
                "thruster".to_string(),
                sim_core::PricingEntry {
                    base_price_per_unit: 50_000.0,
                    importable: true,
                    exportable: true,
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
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
                selected_recipe: None,
            }),
            wear: sim_core::WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            thermal: None,
        });

        // Add 5000 kg Fe to station inventory
        station.inventory.push(sim_core::InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 5000.0,
            quality: 1.0,
            thermal: None,
        });

        // Unlock tech_ship_construction
        state
            .research
            .unlocked
            .insert(TechId("tech_ship_construction".to_string()));

        // Set high balance and advance past trade unlock
        state.balance = 10_000_000.0;
        state.meta.tick = sim_core::trade_unlock_tick(&content.constants);

        rebuild_station_indexes(&mut state, &content);
        (content, state)
    }

    #[test]
    fn test_autopilot_imports_thrusters_when_shipyard_ready() {
        let (content, state) = thruster_import_setup();

        let mut autopilot = AutopilotController::new();
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

        let mut autopilot = AutopilotController::new();
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

        let mut autopilot = AutopilotController::new();
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

        let mut autopilot = AutopilotController::new();
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

    // -----------------------------------------------------------------------
    // Export tests
    // -----------------------------------------------------------------------

    /// Set up content and state for export tests: pricing entries, component defs,
    /// tick past trade unlock.
    fn export_setup() -> (sim_core::GameContent, sim_core::GameState) {
        let mut content = autopilot_content();
        // Add pricing entries
        content.pricing.items.insert(
            "repair_kit".to_string(),
            PricingEntry {
                base_price_per_unit: 8000.0,
                importable: true,
                exportable: true,
                ..Default::default()
            },
        );
        content.pricing.items.insert(
            "He".to_string(),
            PricingEntry {
                base_price_per_unit: 200.0,
                importable: true,
                exportable: true,
                ..Default::default()
            },
        );
        content.pricing.items.insert(
            "Si".to_string(),
            PricingEntry {
                base_price_per_unit: 80.0,
                importable: true,
                exportable: true,
                ..Default::default()
            },
        );
        content.pricing.items.insert(
            "Fe".to_string(),
            PricingEntry {
                base_price_per_unit: 50.0,
                importable: true,
                exportable: true,
                ..Default::default()
            },
        );
        // Add He element (not in base_content) for density lookup
        content.elements.push(sim_core::ElementDef {
            id: "He".to_string(),
            density_kg_per_m3: 125.0,
            display_name: "Helium-3".to_string(),
            refined_name: None,
            category: "material".to_string(),
            melting_point_mk: None,
            latent_heat_j_per_kg: None,
            specific_heat_j_per_kg_k: None,
            boiloff_rate_per_day_at_293k: None,
            boiling_point_mk: None,
            boiloff_curve: None,
        });
        content.init_caches(); // Rebuild density_map with He
                               // Add component def for repair_kit (needed for mass calculation)
        content.component_defs.push(ComponentDef {
            id: "repair_kit".to_string(),
            name: "Repair Kit".to_string(),
            mass_kg: 50.0,
            volume_m3: 0.05,
        });

        let mut state = autopilot_state(&content);
        // Set tick past trade unlock (minutes_per_tick=1 → unlock at 525,600)
        state.meta.tick = 525_601;
        (content, state)
    }

    #[test]
    fn test_export_surplus_kits_above_reserve() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add 15 repair kits (reserve = 10, so 5 surplus)
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Component {
                component_id: ComponentId("repair_kit".to_string()),
                count: 15,
                quality: 1.0,
            });

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let export_cmds: Vec<_> = commands
            .iter()
            .filter(|cmd| {
                matches!(
                    &cmd.command,
                    Command::Export {
                        item_spec: TradeItemSpec::Component {
                            component_id,
                            count: 5,
                            ..
                        },
                        ..
                    } if component_id.0 == "repair_kit"
                )
            })
            .collect();
        assert_eq!(
            export_cmds.len(),
            1,
            "should export exactly 5 surplus kits (15 - 10 reserve)"
        );
    }

    #[test]
    fn test_export_kits_at_reserve_not_exported() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add exactly 10 repair kits (= reserve, no surplus)
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Component {
                component_id: ComponentId("repair_kit".to_string()),
                count: 10,
                quality: 1.0,
            });

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Export {
                    item_spec: TradeItemSpec::Component { .. },
                    ..
                }
            )),
            "should NOT export kits when at reserve threshold"
        );
    }

    #[test]
    fn test_export_fe_zero_margin_not_exported() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add 20,000 kg Fe (reserve is 12,000, surplus is 8,000)
        // But Fe has $0 margin: base $50/kg - surcharge $50/kg = $0
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 20_000.0,
                quality: 1.0,
                thermal: None,
            });

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands.iter().any(|cmd| matches!(
                &cmd.command,
                Command::Export {
                    item_spec: TradeItemSpec::Material { ref element, .. },
                    ..
                } if element == "Fe"
            )),
            "should NOT export Fe when revenue is $0"
        );
    }

    #[test]
    fn test_export_si_when_present() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add 1000 kg Si (no reserve, batch_size=500, so exports 500 kg)
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Material {
                element: "Si".to_string(),
                kg: 1000.0,
                quality: 1.0,
                thermal: None,
            });

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let export_cmds: Vec<_> = commands
            .iter()
            .filter(|cmd| {
                matches!(
                    &cmd.command,
                    Command::Export {
                        item_spec: TradeItemSpec::Material { ref element, .. },
                        ..
                    } if element == "Si"
                )
            })
            .collect();
        assert_eq!(export_cmds.len(), 1, "should export Si when present");
        // Verify batch size capping
        if let Command::Export { item_spec, .. } = &export_cmds[0].command {
            if let TradeItemSpec::Material { kg, .. } = item_spec {
                assert!(
                    (*kg - 500.0).abs() < f32::EPSILON,
                    "should cap at batch_size_kg (500)"
                );
            }
        }
    }

    #[test]
    fn test_export_he_when_present() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Add 200 kg He (no reserve, under batch_size so exports all 200 kg)
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Material {
                element: "He".to_string(),
                kg: 200.0,
                quality: 1.0,
                thermal: None,
            });

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let export_cmds: Vec<_> = commands
            .iter()
            .filter(|cmd| {
                matches!(
                    &cmd.command,
                    Command::Export {
                        item_spec: TradeItemSpec::Material { ref element, .. },
                        ..
                    } if element == "He"
                )
            })
            .collect();
        assert_eq!(export_cmds.len(), 1, "should export He when present");
        if let Command::Export { item_spec, .. } = &export_cmds[0].command {
            if let TradeItemSpec::Material { kg, .. } = item_spec {
                assert!(
                    (*kg - 200.0).abs() < f32::EPSILON,
                    "should export all 200 kg (under batch_size)"
                );
            }
        }
    }

    #[test]
    fn test_no_exports_before_trade_unlock() {
        let (content, mut state) = export_setup();
        let station_id = StationId("station_earth_orbit".to_string());

        // Set tick before trade unlock
        state.meta.tick = 100;

        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Component {
                component_id: ComponentId("repair_kit".to_string()),
                count: 50,
                quality: 1.0,
            });

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        assert!(
            !commands
                .iter()
                .any(|cmd| matches!(&cmd.command, Command::Export { .. })),
            "should NOT export anything before trade unlock"
        );
    }

    #[test]
    fn test_autopilot_prefers_volatile_when_water_low() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        // Install a heating module on the station
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("mod_heat_001".to_string()),
            def_id: "module_heating_unit".to_string(),
            enabled: true,
            wear: sim_core::WearState::default(),
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 0.0,
                ticks_since_last_run: 0,
                stalled: false,
                selected_recipe: None,
            }),
            thermal: None,
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
        });
        rebuild_station_indexes(&mut state, &content);
        // No H2O in inventory → needs_water = true

        // Add both Fe-rich and H2O-rich asteroids
        let fe_asteroid = AsteroidId("asteroid_fe".to_string());
        state.asteroids.insert(
            fe_asteroid.clone(),
            AsteroidState {
                id: fe_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("Fe".to_string(), 0.8)]),
                anomaly_tags: vec![AnomalyTag::new("IronRich")],
                mass_kg: 5000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("IronRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("Fe".to_string(), 0.8),
                        ("Si".to_string(), 0.2),
                    ])),
                },
            },
        );

        let h2o_asteroid = AsteroidId("asteroid_h2o".to_string());
        state.asteroids.insert(
            h2o_asteroid.clone(),
            AsteroidState {
                id: h2o_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("H2O".to_string(), 0.5)]),
                anomaly_tags: vec![AnomalyTag::new("VolatileRich")],
                mass_kg: 3000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("VolatileRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("H2O".to_string(), 0.5),
                        ("Fe".to_string(), 0.1),
                    ])),
                },
            },
        );

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        // The mine command should target the H2O-rich asteroid
        let mine_cmd = commands.iter().find(|cmd| {
            matches!(
                &cmd.command,
                Command::AssignShipTask {
                    task_kind: TaskKind::Mine { .. },
                    ..
                }
            )
        });
        assert!(mine_cmd.is_some(), "should assign a mine task");
        if let Some(cmd) = mine_cmd {
            if let Command::AssignShipTask {
                task_kind: TaskKind::Mine { asteroid, .. },
                ..
            } = &cmd.command
            {
                assert_eq!(
                    *asteroid, h2o_asteroid,
                    "should prefer H2O-rich asteroid when water is needed"
                );
            }
        }
    }

    #[test]
    fn test_autopilot_prefers_fe_when_no_heating_module() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);
        // No heating module → normal Fe-targeting behavior

        let fe_asteroid = AsteroidId("asteroid_fe".to_string());
        state.asteroids.insert(
            fe_asteroid.clone(),
            AsteroidState {
                id: fe_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("Fe".to_string(), 0.8)]),
                anomaly_tags: vec![AnomalyTag::new("IronRich")],
                mass_kg: 5000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("IronRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("Fe".to_string(), 0.8),
                        ("Si".to_string(), 0.2),
                    ])),
                },
            },
        );

        let h2o_asteroid = AsteroidId("asteroid_h2o".to_string());
        state.asteroids.insert(
            h2o_asteroid.clone(),
            AsteroidState {
                id: h2o_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("H2O".to_string(), 0.5)]),
                anomaly_tags: vec![AnomalyTag::new("VolatileRich")],
                mass_kg: 3000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("VolatileRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("H2O".to_string(), 0.5),
                        ("Fe".to_string(), 0.1),
                    ])),
                },
            },
        );

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let mine_cmd = commands.iter().find(|cmd| {
            matches!(
                &cmd.command,
                Command::AssignShipTask {
                    task_kind: TaskKind::Mine { .. },
                    ..
                }
            )
        });
        assert!(mine_cmd.is_some(), "should assign a mine task");
        if let Some(cmd) = mine_cmd {
            if let Command::AssignShipTask {
                task_kind: TaskKind::Mine { asteroid, .. },
                ..
            } = &cmd.command
            {
                assert_eq!(
                    *asteroid, fe_asteroid,
                    "should prefer Fe-rich asteroid when no heating module"
                );
            }
        }
    }

    #[test]
    fn test_deep_scan_includes_volatile_rich() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let asteroid_id = AsteroidId("asteroid_vol".to_string());
        state.asteroids.insert(
            asteroid_id.clone(),
            AsteroidState {
                id: asteroid_id.clone(),
                position: test_position(),
                true_composition: HashMap::from([("H2O".to_string(), 0.5)]),
                anomaly_tags: vec![AnomalyTag::new("VolatileRich")],
                mass_kg: 2000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("VolatileRich"), 0.9)],
                    composition: None, // Not deep-scanned yet
                },
            },
        );

        let candidates =
            crate::behaviors::test_collect_deep_scan_candidates(&state, &content, &test_position());
        assert!(
            candidates.contains(&asteroid_id),
            "VolatileRich asteroids should be deep scan candidates"
        );
    }

    #[test]
    fn test_autopilot_prefers_fe_when_h2o_above_threshold() {
        let mut content = autopilot_content();
        // Add H2O element to content so inventory volume calc works
        content.elements.push(sim_core::ElementDef {
            id: "H2O".to_string(),
            density_kg_per_m3: 1000.0,
            display_name: "Water Ice".to_string(),
            refined_name: Some("Water".to_string()),
            category: "material".to_string(),
            melting_point_mk: None,
            latent_heat_j_per_kg: None,
            specific_heat_j_per_kg_k: None,
            boiloff_rate_per_day_at_293k: None,
            boiling_point_mk: None,
            boiloff_curve: None,
        });
        let mut state = autopilot_state(&content);

        // Install heating module
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("mod_heat_001".to_string()),
            def_id: "module_heating_unit".to_string(),
            enabled: true,
            wear: sim_core::WearState::default(),
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 0.0,
                ticks_since_last_run: 0,
                stalled: false,
                selected_recipe: None,
            }),
            thermal: None,
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
        });
        rebuild_station_indexes(&mut state, &content);
        // Add H2O above threshold (500 kg) → should NOT trigger volatile targeting
        let station = state
            .stations
            .get_mut(&StationId("station_earth_orbit".to_string()))
            .unwrap();
        station.inventory.push(InventoryItem::Material {
            element: "H2O".to_string(),
            kg: 600.0,
            quality: 1.0,
            thermal: None,
        });

        let fe_asteroid = AsteroidId("asteroid_fe".to_string());
        state.asteroids.insert(
            fe_asteroid.clone(),
            AsteroidState {
                id: fe_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("Fe".to_string(), 0.8)]),
                anomaly_tags: vec![AnomalyTag::new("IronRich")],
                mass_kg: 5000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("IronRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("Fe".to_string(), 0.8),
                        ("Si".to_string(), 0.2),
                    ])),
                },
            },
        );

        let h2o_asteroid = AsteroidId("asteroid_h2o".to_string());
        state.asteroids.insert(
            h2o_asteroid.clone(),
            AsteroidState {
                id: h2o_asteroid.clone(),
                position: test_position(),
                true_composition: HashMap::from([("H2O".to_string(), 0.5)]),
                anomaly_tags: vec![AnomalyTag::new("VolatileRich")],
                mass_kg: 3000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(AnomalyTag::new("VolatileRich"), 0.9)],
                    composition: Some(HashMap::from([
                        ("H2O".to_string(), 0.5),
                        ("Fe".to_string(), 0.1),
                    ])),
                },
            },
        );

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let mine_cmd = commands.iter().find(|cmd| {
            matches!(
                &cmd.command,
                Command::AssignShipTask {
                    task_kind: TaskKind::Mine { .. },
                    ..
                }
            )
        });
        assert!(mine_cmd.is_some(), "should assign a mine task");
        if let Some(cmd) = mine_cmd {
            if let Command::AssignShipTask {
                task_kind: TaskKind::Mine { asteroid, .. },
                ..
            } = &cmd.command
            {
                assert_eq!(
                    *asteroid, fe_asteroid,
                    "should prefer Fe when H2O is above threshold despite heating module"
                );
            }
        }
    }

    // ── Propellant pipeline tests ───────────────────────────────────────

    fn add_electrolysis_module(state: &mut sim_core::GameState, enabled: bool) {
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("electrolysis_inst_001".to_string()),
            def_id: "module_electrolysis_unit".to_string(),
            enabled,
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 200.0,
                ticks_since_last_run: 0,
                stalled: false,
                selected_recipe: None,
            }),
            wear: sim_core::WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            thermal: None,
        });
    }

    fn add_heating_module(state: &mut sim_core::GameState, enabled: bool) {
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("heating_inst_001".to_string()),
            def_id: "module_heating_unit".to_string(),
            enabled,
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 100.0,
                ticks_since_last_run: 0,
                stalled: false,
                selected_recipe: None,
            }),
            wear: sim_core::WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            thermal: None,
        });
    }

    fn add_lh2_inventory(state: &mut sim_core::GameState, kg: f32) {
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.inventory.push(InventoryItem::Material {
            element: "LH2".to_string(),
            kg,
            quality: 1.0,
            thermal: None,
        });
    }

    #[test]
    fn test_propellant_noop_without_electrolysis() {
        let content = autopilot_content();
        let state = autopilot_state(&content);
        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;

        let commands = crate::behaviors::test_propellant_pipeline_commands(
            &state,
            &content,
            &owner,
            &mut next_id,
        );
        assert!(
            commands.is_empty(),
            "should emit no commands when station has no electrolysis module"
        );
    }

    #[test]
    fn test_propellant_enables_when_lh2_low() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);
        add_electrolysis_module(&mut state, false);
        add_heating_module(&mut state, false);
        rebuild_station_indexes(&mut state, &content);
        // LH2 = 0 (below threshold of 5000)

        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;
        let commands = crate::behaviors::test_propellant_pipeline_commands(
            &state,
            &content,
            &owner,
            &mut next_id,
        );

        let enables_electrolysis = commands.iter().any(|cmd| {
            matches!(
                &cmd.command,
                Command::SetModuleEnabled { module_id, enabled: true, .. }
                if module_id.0 == "electrolysis_inst_001"
            )
        });
        let enables_heating = commands.iter().any(|cmd| {
            matches!(
                &cmd.command,
                Command::SetModuleEnabled { module_id, enabled: true, .. }
                if module_id.0 == "heating_inst_001"
            )
        });

        assert!(
            enables_electrolysis,
            "should enable disabled electrolysis when LH2 is low"
        );
        assert!(
            enables_heating,
            "should enable disabled heating when LH2 is low"
        );
    }

    #[test]
    fn test_propellant_disables_when_lh2_abundant() {
        let mut content = autopilot_content();
        content.constants.autopilot_lh2_threshold_kg = 1000.0;
        let mut state = autopilot_state(&content);
        add_electrolysis_module(&mut state, true);
        rebuild_station_indexes(&mut state, &content);
        add_lh2_inventory(&mut state, 3000.0); // > 2x threshold (2000)

        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;
        let commands = crate::behaviors::test_propellant_pipeline_commands(
            &state,
            &content,
            &owner,
            &mut next_id,
        );

        let disables = commands.iter().any(|cmd| {
            matches!(
                &cmd.command,
                Command::SetModuleEnabled { module_id, enabled: false, .. }
                if module_id.0 == "electrolysis_inst_001"
            )
        });

        assert!(
            disables,
            "should disable electrolysis when LH2 > 2x threshold"
        );
    }

    #[test]
    fn test_propellant_dead_band_no_commands() {
        let mut content = autopilot_content();
        content.constants.autopilot_lh2_threshold_kg = 1000.0;
        let mut state = autopilot_state(&content);
        add_electrolysis_module(&mut state, true);
        rebuild_station_indexes(&mut state, &content);
        add_lh2_inventory(&mut state, 1500.0); // Between threshold (1000) and 2x (2000)

        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;
        let commands = crate::behaviors::test_propellant_pipeline_commands(
            &state,
            &content,
            &owner,
            &mut next_id,
        );

        assert!(
            commands.is_empty(),
            "should emit no commands in dead band (threshold < LH2 < 2x threshold)"
        );
    }

    #[test]
    fn test_propellant_skips_max_worn_module() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);
        let station_id = StationId("station_earth_orbit".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("electrolysis_inst_001".to_string()),
            def_id: "module_electrolysis_unit".to_string(),
            enabled: false,
            kind_state: sim_core::ModuleKindState::Processor(sim_core::ProcessorState {
                threshold_kg: 200.0,
                ticks_since_last_run: 0,
                stalled: false,
                selected_recipe: None,
            }),
            wear: sim_core::WearState { wear: 1.0 },
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            thermal: None,
        });
        rebuild_station_indexes(&mut state, &content);
        // LH2 = 0 (below threshold)

        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        let mut next_id = 0u64;
        let commands = crate::behaviors::test_propellant_pipeline_commands(
            &state,
            &content,
            &owner,
            &mut next_id,
        );

        assert!(
            commands.is_empty(),
            "should not enable max-worn electrolysis module"
        );
    }

    #[test]
    fn test_autopilot_surveys_nearest_site_first() {
        use sim_core::{AngleMilliDeg, BodyId, Position, RadiusAuMicro, ScanSite};
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        // Create two survey sites: one close, one far.
        let near_site = ScanSite {
            id: SiteId("site_near".to_string()),
            position: Position {
                parent_body: BodyId("test_body".to_string()),
                radius_au_um: RadiusAuMicro(100),
                angle_mdeg: AngleMilliDeg(0),
            },
            template_id: "tmpl_iron_rich".to_string(),
        };
        let far_site = ScanSite {
            id: SiteId("site_far".to_string()),
            position: Position {
                parent_body: BodyId("test_body".to_string()),
                radius_au_um: RadiusAuMicro(5_000_000),
                angle_mdeg: AngleMilliDeg(180_000),
            },
            template_id: "tmpl_iron_rich".to_string(),
        };
        // Insert far site first to ensure sort overrides insertion order.
        state.scan_sites = vec![far_site, near_site];

        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);

        let survey_cmd = commands
            .iter()
            .find(|cmd| {
                matches!(
                    &cmd.command,
                    Command::AssignShipTask {
                        task_kind: TaskKind::Survey { .. } | TaskKind::Transit { .. },
                        ..
                    }
                )
            })
            .expect("should assign a survey task");

        // Extract the survey site ID from the task (may be wrapped in Transit).
        let site_id = match &survey_cmd.command {
            Command::AssignShipTask {
                task_kind: TaskKind::Survey { site },
                ..
            } => site.clone(),
            Command::AssignShipTask {
                task_kind: TaskKind::Transit { then, .. },
                ..
            } => match then.as_ref() {
                TaskKind::Survey { site } => site.clone(),
                other => panic!("expected Survey inside Transit, got {other:?}"),
            },
            other => panic!("expected AssignShipTask, got {other:?}"),
        };

        assert_eq!(
            site_id,
            SiteId("site_near".to_string()),
            "autopilot should survey the nearest site, not the farthest"
        );
    }

    #[test]
    fn test_ship_fitting_fits_idle_ship_at_station() {
        use sim_core::{
            FittedModule, HullDef, HullId, ModuleDefId, ModuleItemId, SlotDef, SlotType,
        };

        let mut content = autopilot_content();
        // Add hull with one utility slot
        content.hulls.insert(
            HullId("hull_general_purpose".to_string()),
            HullDef {
                id: HullId("hull_general_purpose".to_string()),
                name: "General Purpose".to_string(),
                mass_kg: 5000.0,
                cargo_capacity_m3: 50.0,
                base_speed_ticks_per_au: 120,
                base_propellant_capacity_kg: 10000.0,
                slots: vec![SlotDef {
                    slot_type: SlotType("utility".to_string()),
                    label: "Utility 1".to_string(),
                }],
                bonuses: vec![],
                required_tech: None,
                tags: vec![],
            },
        );
        // Add equipment module def
        content.module_defs.insert(
            "module_cargo_expander".to_string(),
            ModuleDefBuilder::new("module_cargo_expander")
                .name("Cargo Expander")
                .mass(400.0)
                .volume(2.0)
                .behavior(sim_core::ModuleBehaviorDef::Equipment)
                .compatible_slots(vec![SlotType("utility".to_string())])
                .build(),
        );
        // Add fitting template
        content.fitting_templates.insert(
            HullId("hull_general_purpose".to_string()),
            vec![FittedModule {
                slot_index: 0,
                module_def_id: ModuleDefId("module_cargo_expander".to_string()),
            }],
        );

        let mut state = autopilot_state(&content);
        // Add module to station inventory
        let station_id = StationId("station_earth_orbit".to_string());
        if let Some(station) = state.stations.get_mut(&station_id) {
            station.inventory.push(InventoryItem::Module {
                item_id: ModuleItemId("mod_item_fit_test".to_string()),
                module_def_id: "module_cargo_expander".to_string(),
            });
        }

        let mut controller = crate::AutopilotController::default();
        let mut cmd_id = 0u64;
        let commands = controller.generate_commands(&state, &content, &mut cmd_id);

        let fit_commands: Vec<_> = commands
            .iter()
            .filter(|c| matches!(&c.command, Command::FitShipModule { .. }))
            .collect();
        assert_eq!(
            fit_commands.len(),
            1,
            "should generate exactly one fit command"
        );
        if let Command::FitShipModule {
            ship_id,
            slot_index,
            module_def_id,
            ..
        } = &fit_commands[0].command
        {
            assert_eq!(*ship_id, ShipId("ship_0001".to_string()));
            assert_eq!(*slot_index, 0);
            assert_eq!(module_def_id.0, "module_cargo_expander");
        } else {
            panic!("expected FitShipModule command");
        }
    }

    #[test]
    fn test_ship_fitting_skips_already_fitted_slot() {
        use sim_core::{
            FittedModule, HullDef, HullId, ModuleDefId, ModuleItemId, SlotDef, SlotType,
        };

        let mut content = autopilot_content();
        content.hulls.insert(
            HullId("hull_general_purpose".to_string()),
            HullDef {
                id: HullId("hull_general_purpose".to_string()),
                name: "General Purpose".to_string(),
                mass_kg: 5000.0,
                cargo_capacity_m3: 50.0,
                base_speed_ticks_per_au: 120,
                base_propellant_capacity_kg: 10000.0,
                slots: vec![SlotDef {
                    slot_type: SlotType("utility".to_string()),
                    label: "Utility 1".to_string(),
                }],
                bonuses: vec![],
                required_tech: None,
                tags: vec![],
            },
        );
        content.module_defs.insert(
            "module_cargo_expander".to_string(),
            ModuleDefBuilder::new("module_cargo_expander")
                .name("Cargo Expander")
                .mass(400.0)
                .volume(2.0)
                .behavior(sim_core::ModuleBehaviorDef::Equipment)
                .compatible_slots(vec![SlotType("utility".to_string())])
                .build(),
        );
        content.fitting_templates.insert(
            HullId("hull_general_purpose".to_string()),
            vec![FittedModule {
                slot_index: 0,
                module_def_id: ModuleDefId("module_cargo_expander".to_string()),
            }],
        );

        let mut state = autopilot_state(&content);
        // Pre-fit the slot
        let ship_id = ShipId("ship_0001".to_string());
        if let Some(ship) = state.ships.get_mut(&ship_id) {
            ship.fitted_modules.push(FittedModule {
                slot_index: 0,
                module_def_id: ModuleDefId("module_cargo_expander".to_string()),
            });
        }
        // Module available but slot already occupied
        let station_id = StationId("station_earth_orbit".to_string());
        if let Some(station) = state.stations.get_mut(&station_id) {
            station.inventory.push(InventoryItem::Module {
                item_id: ModuleItemId("mod_item_skip".to_string()),
                module_def_id: "module_cargo_expander".to_string(),
            });
        }

        let mut controller = crate::AutopilotController::default();
        let mut cmd_id = 0u64;
        let commands = controller.generate_commands(&state, &content, &mut cmd_id);

        let fit_commands: Vec<_> = commands
            .iter()
            .filter(|c| matches!(&c.command, Command::FitShipModule { .. }))
            .collect();
        assert!(
            fit_commands.is_empty(),
            "should not fit into already-occupied slot"
        );
    }
}
