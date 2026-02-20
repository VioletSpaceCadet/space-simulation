use sim_core::{
    mine_duration, shortest_hop_count, AnomalyTag, AsteroidId, AsteroidState, Command,
    CommandEnvelope, CommandId, GameContent, GameState, PrincipalId, SiteId, TaskKind, TechId,
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
const IRON_RICH_CONFIDENCE_THRESHOLD: f32 = 0.7;

impl CommandSource for AutopilotController {
    #[allow(clippy::too_many_lines)]
    fn generate_commands(
        &mut self,
        state: &GameState,
        content: &GameContent,
        next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        use std::cmp::Ordering;

        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());

        // Collect idle autopilot ships, sorted for determinism.
        let mut idle_ships: Vec<_> = state
            .ships
            .values()
            .filter(|ship| {
                ship.owner == owner
                    && ship
                        .task
                        .as_ref()
                        .is_none_or(|t| matches!(t.kind, TaskKind::Idle))
            })
            .map(|ship| ship.id.clone())
            .collect();
        idle_ships.sort_by(|a, b| a.0.cmp(&b.0));

        let deep_scan_unlocked = state
            .research
            .unlocked
            .contains(&TechId("tech_deep_scan_v1".to_string()));

        // Survey candidates.
        let mut next_site = state.scan_sites.iter();

        // Deep scan candidates: IronRich confidence above threshold, composition unknown.
        let mut deep_scan_candidates: Vec<AsteroidId> = state
            .asteroids
            .values()
            .filter(|asteroid| {
                asteroid.knowledge.composition.is_none()
                    && asteroid.knowledge.tag_beliefs.iter().any(|(tag, conf)| {
                        *tag == AnomalyTag::IronRich && *conf > IRON_RICH_CONFIDENCE_THRESHOLD
                    })
            })
            .map(|a| a.id.clone())
            .collect();
        deep_scan_candidates.sort_by(|a, b| a.0.cmp(&b.0));
        let mut next_deep_scan = deep_scan_candidates.iter();

        // Mine candidates: deep-scanned, has remaining mass, sorted by value desc.
        let mut mine_candidates: Vec<&AsteroidState> = state
            .asteroids
            .values()
            .filter(|a| a.mass_kg > 0.0 && a.knowledge.composition.is_some())
            .collect();
        mine_candidates.sort_by(|a, b| {
            let value = |ast: &&AsteroidState| {
                ast.mass_kg
                    * ast
                        .knowledge
                        .composition
                        .as_ref()
                        .and_then(|c| c.get("Fe"))
                        .copied()
                        .unwrap_or(0.0)
            };
            value(b).partial_cmp(&value(a)).unwrap_or(Ordering::Equal)
        });
        let mut next_mine = mine_candidates.iter();

        let mut commands = Vec::new();

        for ship_id in idle_ships {
            let ship = &state.ships[&ship_id];

            // Priority 1: ship has cargo → deposit at nearest station.
            if !ship.cargo.is_empty() {
                let Some(station) = state.stations.values().min_by_key(|s| {
                    shortest_hop_count(&ship.location_node, &s.location_node, &content.solar_system)
                        .unwrap_or(u64::MAX)
                }) else {
                    continue;
                };

                let deposit_task = TaskKind::Deposit {
                    station: station.id.clone(),
                };
                let final_task = match shortest_hop_count(
                    &ship.location_node,
                    &station.location_node,
                    &content.solar_system,
                ) {
                    Some(0) | None => deposit_task,
                    Some(hops) => TaskKind::Transit {
                        destination: station.location_node.clone(),
                        total_ticks: hops * content.constants.travel_ticks_per_hop,
                        then: Box::new(deposit_task),
                    },
                };

                let cmd_id = CommandId(format!("cmd_{:06}", *next_command_id));
                *next_command_id += 1;
                commands.push(CommandEnvelope {
                    id: cmd_id,
                    issued_by: ship.owner.clone(),
                    issued_tick: state.meta.tick,
                    execute_at_tick: state.meta.tick,
                    command: Command::AssignShipTask {
                        ship_id,
                        task_kind: final_task,
                    },
                });
                continue;
            }

            // Priority 2: mine best available asteroid.
            if let Some(asteroid) = next_mine.next() {
                let target_node = asteroid.location_node.clone();
                let duration_ticks = mine_duration(asteroid, ship, content);
                let task_kind = TaskKind::Mine {
                    asteroid: asteroid.id.clone(),
                    duration_ticks,
                };
                let final_task = match shortest_hop_count(
                    &ship.location_node,
                    &target_node,
                    &content.solar_system,
                ) {
                    Some(0) | None => task_kind,
                    Some(hops) => TaskKind::Transit {
                        destination: target_node,
                        total_ticks: hops * content.constants.travel_ticks_per_hop,
                        then: Box::new(task_kind),
                    },
                };
                let cmd_id = CommandId(format!("cmd_{:06}", *next_command_id));
                *next_command_id += 1;
                commands.push(CommandEnvelope {
                    id: cmd_id,
                    issued_by: ship.owner.clone(),
                    issued_tick: state.meta.tick,
                    execute_at_tick: state.meta.tick,
                    command: Command::AssignShipTask {
                        ship_id,
                        task_kind: final_task,
                    },
                });
                continue;
            }

            // Priority 3: deep scan (enables future mining).
            if deep_scan_unlocked {
                if let Some(asteroid_id) = next_deep_scan.next() {
                    let node = state.asteroids[asteroid_id].location_node.clone();
                    let task_kind = TaskKind::DeepScan {
                        asteroid: asteroid_id.clone(),
                    };
                    let final_task =
                        match shortest_hop_count(&ship.location_node, &node, &content.solar_system)
                        {
                            Some(0) | None => task_kind,
                            Some(hops) => TaskKind::Transit {
                                destination: node,
                                total_ticks: hops * content.constants.travel_ticks_per_hop,
                                then: Box::new(task_kind),
                            },
                        };
                    let cmd_id = CommandId(format!("cmd_{:06}", *next_command_id));
                    *next_command_id += 1;
                    commands.push(CommandEnvelope {
                        id: cmd_id,
                        issued_by: ship.owner.clone(),
                        issued_tick: state.meta.tick,
                        execute_at_tick: state.meta.tick,
                        command: Command::AssignShipTask {
                            ship_id,
                            task_kind: final_task,
                        },
                    });
                    continue;
                }
            }

            // Priority 4: survey unscanned sites.
            if let Some(site) = next_site.next() {
                let target_node = site.node.clone();
                let task_kind = TaskKind::Survey {
                    site: SiteId(site.id.0.clone()),
                };
                let final_task = match shortest_hop_count(
                    &ship.location_node,
                    &target_node,
                    &content.solar_system,
                ) {
                    Some(0) | None => task_kind,
                    Some(hops) => TaskKind::Transit {
                        destination: target_node,
                        total_ticks: hops * content.constants.travel_ticks_per_hop,
                        then: Box::new(task_kind),
                    },
                };
                let cmd_id = CommandId(format!("cmd_{:06}", *next_command_id));
                *next_command_id += 1;
                commands.push(CommandEnvelope {
                    id: cmd_id,
                    issued_by: ship.owner.clone(),
                    issued_tick: state.meta.tick,
                    execute_at_tick: state.meta.tick,
                    command: Command::AssignShipTask {
                        ship_id,
                        task_kind: final_task,
                    },
                });
            }

            // Nothing to do for this ship.
        }

        commands
    }
}

/// Replays a scripted sequence of commands from a JSON file.
pub struct ScenarioSource {
    // TODO: load tick -> Vec<Command> map from file
}

impl CommandSource for ScenarioSource {
    fn generate_commands(
        &mut self,
        _state: &GameState,
        _content: &GameContent,
        _next_command_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        // TODO: emit commands scheduled for the current tick
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{
        AsteroidId, AsteroidKnowledge, AsteroidState, Constants, Counters, ElementDef,
        FacilitiesState, GameContent, GameState, MetaState, NodeDef, NodeId, PrincipalId,
        ResearchState, ShipId, ShipState, SolarSystemDef, StationId, StationState,
    };
    use std::collections::{HashMap, HashSet};

    fn autopilot_content() -> GameContent {
        GameContent {
            content_version: "test".to_string(),
            techs: vec![],
            solar_system: SolarSystemDef {
                nodes: vec![NodeDef {
                    id: NodeId("node_a".to_string()),
                    name: "A".to_string(),
                }],
                edges: vec![],
            },
            asteroid_templates: vec![],
            elements: vec![ElementDef {
                id: "Fe".to_string(),
                density_kg_per_m3: 7874.0,
                display_name: "Iron".to_string(),
                refined_name: None,
            }],
            module_defs: vec![],
            constants: Constants {
                survey_scan_ticks: 1,
                deep_scan_ticks: 1,
                travel_ticks_per_hop: 1,
                survey_scan_data_amount: 1.0,
                survey_scan_data_quality: 1.0,
                deep_scan_data_amount: 1.0,
                deep_scan_data_quality: 1.0,
                survey_tag_detection_probability: 1.0,
                asteroid_count_per_template: 0,
                station_compute_units_total: 0,
                station_power_per_compute_unit_per_tick: 0.0,
                station_efficiency: 1.0,
                station_power_available_per_tick: 0.0,
                asteroid_mass_min_kg: 500.0,
                asteroid_mass_max_kg: 500.0,
                ship_cargo_capacity_m3: 20.0,
                station_cargo_capacity_m3: 10_000.0,
                mining_rate_kg_per_tick: 50.0,
                deposit_ticks: 1,
            },
        }
    }

    fn autopilot_state(content: &GameContent) -> GameState {
        let node = NodeId("node_a".to_string());
        let ship_id = ShipId("ship_0001".to_string());
        let station_id = StationId("station_earth_orbit".to_string());
        let owner = PrincipalId(AUTOPILOT_OWNER.to_string());
        GameState {
            meta: MetaState {
                tick: 0,
                seed: 0,
                schema_version: 1,
                content_version: "test".to_string(),
            },
            scan_sites: vec![],
            asteroids: HashMap::new(),
            ships: HashMap::from([(
                ship_id.clone(),
                ShipState {
                    id: ship_id,
                    location_node: node.clone(),
                    owner,
                    cargo: HashMap::new(),
                    cargo_capacity_m3: content.constants.ship_cargo_capacity_m3,
                    task: None,
                },
            )]),
            stations: HashMap::from([(
                station_id.clone(),
                StationState {
                    id: station_id,
                    location_node: node,
                    power_available_per_tick: 0.0,
                    cargo: HashMap::new(),
                    cargo_capacity_m3: content.constants.station_cargo_capacity_m3,
                    facilities: FacilitiesState {
                        compute_units_total: 0,
                        power_per_compute_unit_per_tick: 0.0,
                        efficiency: 1.0,
                    },
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
    fn test_autopilot_assigns_mine_when_asteroid_known() {
        let content = autopilot_content();
        let mut state = autopilot_state(&content);

        let asteroid_id = AsteroidId("asteroid_0001".to_string());
        state.asteroids.insert(
            asteroid_id.clone(),
            AsteroidState {
                id: asteroid_id.clone(),
                location_node: NodeId("node_a".to_string()),
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
            .cargo
            .insert("Fe".to_string(), 100.0);

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
}
