use sim_core::{
    Command, CommandEnvelope, GameContent, GameState, PrincipalId, ShipId, ShipState, TaskKind,
};

use crate::behaviors::{deposit_priority, make_cmd, maybe_transit, should_opportunistic_refuel};
use crate::objectives::ShipObjective;

use super::Agent;

/// A ship-level agent that converts a `ShipObjective` into tactical commands.
///
/// The ship agent handles the "how" of executing an objective: transit routing,
/// opportunistic refueling, deposit priority, and objective invalidation.
/// It does NOT pick its own target — the station layer (or assignment bridge)
/// assigns objectives.
#[allow(dead_code)] // Wired into AutopilotController in VIO-448
pub(crate) struct ShipAgent {
    pub(crate) ship_id: ShipId,
    pub(crate) objective: Option<ShipObjective>,
}

#[allow(dead_code)] // Wired into AutopilotController in VIO-448
impl ShipAgent {
    pub(crate) fn new(ship_id: ShipId) -> Self {
        Self {
            ship_id,
            objective: None,
        }
    }

    /// Validate and potentially clear the current objective.
    /// Returns false if the objective was invalidated (ship should go idle).
    fn validate_objective(&mut self, state: &GameState) -> bool {
        let Some(objective) = &self.objective else {
            return true; // No objective = nothing to invalidate
        };

        let valid = match objective {
            ShipObjective::Mine { asteroid_id } => state
                .asteroids
                .get(asteroid_id)
                .is_some_and(|a| a.mass_kg > 0.0),
            ShipObjective::DeepScan { asteroid_id } => state
                .asteroids
                .get(asteroid_id)
                .is_some_and(|a| a.knowledge.composition.is_none()),
            ShipObjective::Survey { site_id } => state.scan_sites.iter().any(|s| s.id == *site_id),
            ShipObjective::Deposit { station_id } => state.stations.contains_key(station_id),
            ShipObjective::Idle => true,
        };

        if !valid {
            self.objective = None;
        }
        valid
    }

    /// Convert the current objective to a `TaskKind`, wrapping with transit if needed.
    fn objective_to_task(
        &self,
        ship: &ShipState,
        state: &GameState,
        content: &GameContent,
    ) -> Option<TaskKind> {
        let objective = self.objective.as_ref()?;
        let ship_speed = ship.ticks_per_au(content.constants.ticks_per_au);

        match objective {
            ShipObjective::Mine { asteroid_id } => {
                let asteroid = state.asteroids.get(asteroid_id)?;
                Some(maybe_transit(
                    TaskKind::Mine {
                        asteroid: asteroid_id.clone(),
                        duration_ticks: sim_core::mine_duration(asteroid, ship, content),
                    },
                    &ship.position,
                    &asteroid.position,
                    ship_speed,
                    state,
                    content,
                ))
            }
            ShipObjective::DeepScan { asteroid_id } => {
                let asteroid = state.asteroids.get(asteroid_id)?;
                Some(maybe_transit(
                    TaskKind::DeepScan {
                        asteroid: asteroid_id.clone(),
                    },
                    &ship.position,
                    &asteroid.position,
                    ship_speed,
                    state,
                    content,
                ))
            }
            ShipObjective::Survey { site_id } => {
                let site = state.scan_sites.iter().find(|s| s.id == *site_id)?;
                Some(maybe_transit(
                    TaskKind::Survey {
                        site: site_id.clone(),
                    },
                    &ship.position,
                    &site.position,
                    ship_speed,
                    state,
                    content,
                ))
            }
            ShipObjective::Deposit { station_id } => {
                let station = state.stations.get(station_id)?;
                Some(maybe_transit(
                    TaskKind::Deposit {
                        station: station_id.clone(),
                        blocked: false,
                    },
                    &ship.position,
                    &station.position,
                    ship_speed,
                    state,
                    content,
                ))
            }
            ShipObjective::Idle => None,
        }
    }
}

impl Agent for ShipAgent {
    fn name(&self) -> &'static str {
        "ship_agent"
    }

    fn generate(
        &mut self,
        state: &GameState,
        content: &GameContent,
        _owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let Some(ship) = state.ships.get(&self.ship_id) else {
            return Vec::new();
        };

        // Only act on idle ships
        let is_idle = ship
            .task
            .as_ref()
            .is_none_or(|t| matches!(t.kind, TaskKind::Idle));
        if !is_idle {
            return Vec::new();
        }

        // Validate current objective — clear if target is gone
        self.validate_objective(state);

        // Opportunistic refuel takes precedence over everything
        if should_opportunistic_refuel(ship, state, content) {
            if let Some(task_kind) = crate::behaviors::try_refuel(ship, state, content) {
                return vec![make_cmd(
                    &ship.owner,
                    state.meta.tick,
                    next_id,
                    Command::AssignShipTask {
                        ship_id: self.ship_id.clone(),
                        task_kind,
                    },
                )];
            }
        }

        // Deposit priority: if ship has cargo, deposit first regardless of objective
        if let Some(deposit_task) = deposit_priority(ship, state, content) {
            return vec![make_cmd(
                &ship.owner,
                state.meta.tick,
                next_id,
                Command::AssignShipTask {
                    ship_id: self.ship_id.clone(),
                    task_kind: deposit_task,
                },
            )];
        }

        // Convert objective to task
        if let Some(task_kind) = self.objective_to_task(ship, state, content) {
            return vec![make_cmd(
                &ship.owner,
                state.meta.tick,
                next_id,
                Command::AssignShipTask {
                    ship_id: self.ship_id.clone(),
                    task_kind,
                },
            )];
        }

        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::test_fixtures::{base_content, base_state, test_position};
    use sim_core::{
        AsteroidId, AsteroidKnowledge, AsteroidState, Command, HullId, InventoryItem, LotId,
        SiteId, StationId,
    };
    use std::collections::BTreeMap;

    fn test_ship_id() -> ShipId {
        ShipId("ship_test".to_string())
    }

    fn test_station_id() -> StationId {
        StationId("station_earth_orbit".to_string())
    }

    fn test_asteroid_id() -> AsteroidId {
        AsteroidId("asteroid_1".to_string())
    }

    fn setup_state_with_ship() -> (GameState, GameContent) {
        let content = base_content();
        let mut state = base_state(&content);
        let ship = ShipState {
            id: test_ship_id(),
            owner: PrincipalId("principal_autopilot".to_string()),
            position: test_position(),
            inventory: vec![],
            task: None,
            hull_id: HullId("hull_general_purpose".to_string()),
            fitted_modules: vec![],
            modifiers: Default::default(),
            propellant_kg: 0.0,
            propellant_capacity_kg: 0.0,
            cargo_capacity_m3: 100.0,
            speed_ticks_per_au: None,
            crew: BTreeMap::new(),
            leaders: vec![],
        };
        state.ships.insert(test_ship_id(), ship);
        (state, content)
    }

    fn add_asteroid(state: &mut GameState, id: AsteroidId, mass_kg: f32, has_composition: bool) {
        let composition = if has_composition {
            let mut comp = std::collections::HashMap::new();
            comp.insert("Fe".to_string(), 0.8);
            Some(comp)
        } else {
            None
        };
        state.asteroids.insert(
            id.clone(),
            AsteroidState {
                id,
                position: test_position(),
                true_composition: std::collections::HashMap::new(),
                anomaly_tags: vec![],
                mass_kg,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition,
                },
            },
        );
    }

    #[test]
    fn test_no_objective_produces_no_commands() {
        let (state, content) = setup_state_with_ship();
        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_idle_objective_produces_no_commands() {
        let (state, content) = setup_state_with_ship();
        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::Idle);
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_mine_objective_generates_mine_task() {
        let (mut state, content) = setup_state_with_ship();
        let asteroid_id = test_asteroid_id();
        add_asteroid(&mut state, asteroid_id.clone(), 1000.0, true);

        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::Mine {
            asteroid_id: asteroid_id.clone(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert_eq!(commands.len(), 1);
        match &commands[0].command {
            Command::AssignShipTask { ship_id, task_kind } => {
                assert_eq!(*ship_id, test_ship_id());
                assert!(
                    matches!(task_kind, TaskKind::Mine { asteroid, .. } if *asteroid == asteroid_id)
                );
            }
            other => panic!("expected AssignShipTask, got {other:?}"),
        }
    }

    #[test]
    fn test_deep_scan_objective_generates_deep_scan_task() {
        let (mut state, content) = setup_state_with_ship();
        let asteroid_id = test_asteroid_id();
        add_asteroid(&mut state, asteroid_id.clone(), 1000.0, false);

        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::DeepScan {
            asteroid_id: asteroid_id.clone(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert_eq!(commands.len(), 1);
        match &commands[0].command {
            Command::AssignShipTask { task_kind, .. } => {
                assert!(
                    matches!(task_kind, TaskKind::DeepScan { asteroid, .. } if *asteroid == asteroid_id)
                );
            }
            other => panic!("expected AssignShipTask, got {other:?}"),
        }
    }

    #[test]
    fn test_survey_objective_generates_survey_task() {
        let (mut state, content) = setup_state_with_ship();
        let site_id = SiteId("site_1".to_string());
        state.scan_sites.push(sim_core::ScanSite {
            id: site_id.clone(),
            position: test_position(),
            template_id: "template_default".to_string(),
        });

        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::Survey {
            site_id: site_id.clone(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert_eq!(commands.len(), 1);
        match &commands[0].command {
            Command::AssignShipTask { task_kind, .. } => {
                assert!(matches!(task_kind, TaskKind::Survey { site, .. } if *site == site_id));
            }
            other => panic!("expected AssignShipTask, got {other:?}"),
        }
    }

    #[test]
    fn test_deposit_objective_generates_deposit_task() {
        let (state, content) = setup_state_with_ship();
        let station_id = test_station_id();

        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::Deposit {
            station_id: station_id.clone(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert_eq!(commands.len(), 1);
        match &commands[0].command {
            Command::AssignShipTask { task_kind, .. } => {
                assert!(
                    matches!(task_kind, TaskKind::Deposit { station, .. } if *station == station_id)
                );
            }
            other => panic!("expected AssignShipTask, got {other:?}"),
        }
    }

    #[test]
    fn test_mine_objective_invalidated_when_asteroid_depleted() {
        let (mut state, content) = setup_state_with_ship();
        let asteroid_id = test_asteroid_id();
        add_asteroid(&mut state, asteroid_id.clone(), 0.0, true); // mass = 0

        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::Mine {
            asteroid_id: asteroid_id.clone(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
        assert!(agent.objective.is_none()); // Objective was cleared
    }

    #[test]
    fn test_deep_scan_invalidated_when_composition_known() {
        let (mut state, content) = setup_state_with_ship();
        let asteroid_id = test_asteroid_id();
        add_asteroid(&mut state, asteroid_id.clone(), 1000.0, true); // composition IS known

        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::DeepScan {
            asteroid_id: asteroid_id.clone(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
        assert!(agent.objective.is_none());
    }

    #[test]
    fn test_mine_objective_invalidated_when_asteroid_missing() {
        let (state, content) = setup_state_with_ship();
        // No asteroid in state

        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::Mine {
            asteroid_id: test_asteroid_id(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
        assert!(agent.objective.is_none());
    }

    #[test]
    fn test_deposit_priority_overrides_mine_objective() {
        let (mut state, content) = setup_state_with_ship();
        let asteroid_id = test_asteroid_id();
        add_asteroid(&mut state, asteroid_id.clone(), 1000.0, true);

        // Give ship some ore so deposit_priority fires
        state
            .ships
            .get_mut(&test_ship_id())
            .unwrap()
            .inventory
            .push(InventoryItem::Ore {
                lot_id: LotId("lot_1".to_string()),
                asteroid_id: asteroid_id.clone(),
                kg: 50.0,
                composition: {
                    let mut c = std::collections::HashMap::new();
                    c.insert("Fe".to_string(), 0.8_f32);
                    c
                },
            });

        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::Mine {
            asteroid_id: asteroid_id.clone(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert_eq!(commands.len(), 1);
        // Should be a Deposit task, not Mine
        match &commands[0].command {
            Command::AssignShipTask { task_kind, .. } => {
                assert!(matches!(task_kind, TaskKind::Deposit { .. }));
            }
            other => panic!("expected AssignShipTask with Deposit, got {other:?}"),
        }
        // Mine objective should still be set (not invalidated, just overridden for this tick)
        assert!(agent.objective.is_some());
    }

    #[test]
    fn test_non_idle_ship_produces_no_commands() {
        let (mut state, content) = setup_state_with_ship();
        let asteroid_id = test_asteroid_id();
        add_asteroid(&mut state, asteroid_id.clone(), 1000.0, true);

        // Ship has an active task
        state.ships.get_mut(&test_ship_id()).unwrap().task = Some(sim_core::TaskState {
            kind: TaskKind::Mine {
                asteroid: asteroid_id.clone(),
                duration_ticks: 10,
            },
            started_tick: 0,
            eta_tick: 10,
        });

        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = ShipAgent::new(test_ship_id());
        agent.objective = Some(ShipObjective::Mine {
            asteroid_id: asteroid_id.clone(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_missing_ship_produces_no_commands() {
        let (state, content) = setup_state_with_ship();
        let owner = PrincipalId("principal_autopilot".to_string());
        // Agent for a ship that doesn't exist
        let mut agent = ShipAgent::new(ShipId("nonexistent".to_string()));
        agent.objective = Some(ShipObjective::Mine {
            asteroid_id: test_asteroid_id(),
        });
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
    }
}
