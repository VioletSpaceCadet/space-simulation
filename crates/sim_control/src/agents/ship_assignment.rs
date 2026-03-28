use std::collections::BTreeMap;

use sim_core::{
    compute_entity_absolute, AsteroidId, GameContent, GameState, PrincipalId, ShipId, SiteId,
    TechId,
};

use crate::behaviors::{
    collect_deep_scan_candidates, collect_idle_ships, deposit_priority, element_mining_value,
    should_opportunistic_refuel, station_has_module_with_role, total_element_inventory,
};
use crate::objectives::ShipObjective;

use super::ship_agent::ShipAgent;

/// Temporary bridge that assigns `ShipObjective`s to idle ship agents.
///
/// Preserves the shared-iterator deduplication pattern from `ShipTaskScheduler`:
/// candidates are pre-sorted, and a shared iterator ensures no two ships
/// target the same asteroid or scan site.
///
/// Runs BEFORE ship agents in execution order. Replaced by
/// `StationAgent::assign_ship_objectives()` in Phase B (VIO-451).
#[allow(dead_code)] // Wired into AutopilotController in VIO-448
pub(crate) struct ShipAssignmentBridge;

#[allow(dead_code)] // Wired into AutopilotController in VIO-448
impl ShipAssignmentBridge {
    /// Assign objectives to idle ship agents that have no current objective.
    ///
    /// Uses the shared-iterator pattern: candidate lists are pre-sorted once,
    /// and a single pass through idle ships ensures each candidate is assigned
    /// to at most one ship (AD1 from plan).
    pub(crate) fn assign_objectives(
        ship_agents: &mut BTreeMap<ShipId, ShipAgent>,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
    ) {
        let idle_ships = collect_idle_ships(state, owner);
        let assignable: Vec<ShipId> = idle_ships
            .into_iter()
            .filter(|id| ship_agents.get(id).is_some_and(|a| a.objective.is_none()))
            .collect();

        if assignable.is_empty() {
            return;
        }

        // Use first assignable ship's position as reference for distance sorting.
        let reference_pos = &state.ships[&assignable[0]].position;

        let deep_scan_unlocked = state
            .research
            .unlocked
            .contains(&TechId(content.autopilot.deep_scan_tech.clone()));

        // --- Pre-compute sorted candidate lists (Schwartzian transforms) ---

        // Deep scan: sorted by distance from reference (nearest first).
        let deep_scan_candidates = collect_deep_scan_candidates(state, content, reference_pos);
        let mut next_deep_scan = deep_scan_candidates.iter();

        // Survey sites: sorted by distance from reference (nearest first).
        let survey_candidates: Vec<SiteId> = if state.scan_sites.is_empty() {
            Vec::new()
        } else {
            let ref_abs = compute_entity_absolute(reference_pos, &state.body_cache);
            let mut decorated: Vec<(u128, SiteId)> = state
                .scan_sites
                .iter()
                .map(|site| {
                    let dist = ref_abs.distance_squared(compute_entity_absolute(
                        &site.position,
                        &state.body_cache,
                    ));
                    (dist, site.id.clone())
                })
                .collect();
            decorated.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1 .0.cmp(&b.1 .0)));
            decorated.into_iter().map(|(_, id)| id).collect()
        };
        let mut next_site = survey_candidates.iter();

        // Mine candidates: sorted by mining value (mass × element fraction), descending.
        // Volatile detection determines which element to prioritize.
        let propellant_role = &content.autopilot.propellant_role;
        let support_role = &content.autopilot.propellant_support_role;
        let has_propellant_module = station_has_module_with_role(state, propellant_role);
        let volatile_element = &content.autopilot.volatile_element;
        let propellant_element = &content.autopilot.propellant_element;
        let primary_element = &content.autopilot.primary_mining_element;
        let needs_volatiles = station_has_module_with_role(state, support_role)
            && (total_element_inventory(state, volatile_element)
                < content.constants.autopilot_volatile_threshold_kg
                || (has_propellant_module
                    && total_element_inventory(state, propellant_element)
                        < content.constants.autopilot_lh2_threshold_kg));

        let sort_element = if needs_volatiles {
            volatile_element
        } else {
            primary_element
        };
        let mut mine_decorated: Vec<(f32, AsteroidId)> = state
            .asteroids
            .values()
            .filter(|a| a.mass_kg > 0.0 && a.knowledge.composition.is_some())
            .map(|a| (element_mining_value(a, sort_element), a.id.clone()))
            .collect();
        mine_decorated.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1 .0.cmp(&b.1 .0)));
        let mine_candidates: Vec<AsteroidId> =
            mine_decorated.into_iter().map(|(_, id)| id).collect();
        let mut next_mine = mine_candidates.iter();

        // --- Assign objectives using shared iterators ---
        for ship_id in assignable {
            let ship = &state.ships[&ship_id];

            // Skip ships that would refuel — don't consume iterator slots.
            if should_opportunistic_refuel(ship, state, content) {
                continue;
            }

            // Skip ships that would deposit — don't consume iterator slots.
            if deposit_priority(ship, state, content).is_some() {
                continue;
            }

            // Iterate configurable priority order from content.autopilot.task_priority.
            for priority in &content.autopilot.task_priority {
                let objective = match priority.as_str() {
                    "Mine" => next_mine.next().map(|id| ShipObjective::Mine {
                        asteroid_id: id.clone(),
                    }),
                    "DeepScan" if deep_scan_unlocked => {
                        next_deep_scan.next().map(|id| ShipObjective::DeepScan {
                            asteroid_id: id.clone(),
                        })
                    }
                    "Survey" => next_site.next().map(|id| ShipObjective::Survey {
                        site_id: id.clone(),
                    }),
                    // "Deposit" handled by ShipAgent; unknown priorities ignored.
                    _ => None,
                };
                if let Some(obj) = objective {
                    if let Some(agent) = ship_agents.get_mut(&ship_id) {
                        agent.objective = Some(obj);
                    }
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::test_fixtures::{base_content, base_state, test_position};
    use sim_core::{
        AsteroidKnowledge, AsteroidState, HullId, InventoryItem, LotId, ScanSite, TaskKind,
        TaskState,
    };

    fn test_owner() -> PrincipalId {
        PrincipalId("principal_autopilot".to_string())
    }

    fn make_ship_id(name: &str) -> ShipId {
        ShipId(name.to_string())
    }

    fn make_asteroid_id(name: &str) -> AsteroidId {
        AsteroidId(name.to_string())
    }

    fn setup() -> (GameState, GameContent, BTreeMap<ShipId, ShipAgent>) {
        let content = base_content();
        let state = base_state(&content);
        let agents = BTreeMap::new();
        (state, content, agents)
    }

    fn add_idle_ship(
        state: &mut GameState,
        agents: &mut BTreeMap<ShipId, ShipAgent>,
        ship_id: ShipId,
    ) {
        use sim_core::ShipState;
        let ship = ShipState {
            id: ship_id.clone(),
            owner: test_owner(),
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
            crew: std::collections::BTreeMap::new(),
            leaders: vec![],
        };
        state.ships.insert(ship_id.clone(), ship);
        agents.insert(ship_id.clone(), ShipAgent::new(ship_id));
    }

    fn add_mineable_asteroid(state: &mut GameState, asteroid_id: AsteroidId, fe_fraction: f32) {
        state.asteroids.insert(
            asteroid_id.clone(),
            AsteroidState {
                id: asteroid_id,
                position: test_position(),
                true_composition: std::collections::HashMap::new(),
                anomaly_tags: vec![],
                mass_kg: 1000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: Some({
                        let mut c = std::collections::HashMap::new();
                        c.insert("Fe".to_string(), fe_fraction);
                        c
                    }),
                },
            },
        );
    }

    #[test]
    fn no_idle_ships_no_assignments() {
        let (state, content, mut agents) = setup();
        let owner = test_owner();

        ShipAssignmentBridge::assign_objectives(&mut agents, &state, &content, &owner);

        assert!(agents.is_empty());
    }

    #[test]
    fn two_ships_two_asteroids_no_double_assignment() {
        let (mut state, content, mut agents) = setup();
        let owner = test_owner();

        let ship_a = make_ship_id("ship_a");
        let ship_b = make_ship_id("ship_b");
        add_idle_ship(&mut state, &mut agents, ship_a.clone());
        add_idle_ship(&mut state, &mut agents, ship_b.clone());

        let asteroid_1 = make_asteroid_id("asteroid_1");
        let asteroid_2 = make_asteroid_id("asteroid_2");
        add_mineable_asteroid(&mut state, asteroid_1.clone(), 0.8);
        add_mineable_asteroid(&mut state, asteroid_2.clone(), 0.5);

        ShipAssignmentBridge::assign_objectives(&mut agents, &state, &content, &owner);

        let obj_a = agents[&ship_a]
            .objective
            .as_ref()
            .expect("ship_a should have objective");
        let obj_b = agents[&ship_b]
            .objective
            .as_ref()
            .expect("ship_b should have objective");

        // Both should be Mine objectives
        let id_a = match obj_a {
            ShipObjective::Mine { asteroid_id } => asteroid_id.clone(),
            other => panic!("expected Mine, got {other:?}"),
        };
        let id_b = match obj_b {
            ShipObjective::Mine { asteroid_id } => asteroid_id.clone(),
            other => panic!("expected Mine, got {other:?}"),
        };

        // No double-assignment: different asteroids
        assert_ne!(id_a, id_b);
        // Higher value asteroid (0.8 Fe) assigned first
        assert_eq!(id_a, asteroid_1);
        assert_eq!(id_b, asteroid_2);
    }

    #[test]
    fn three_ships_one_asteroid_one_site() {
        let (mut state, content, mut agents) = setup();
        let owner = test_owner();

        let ship_a = make_ship_id("ship_a");
        let ship_b = make_ship_id("ship_b");
        let ship_c = make_ship_id("ship_c");
        add_idle_ship(&mut state, &mut agents, ship_a.clone());
        add_idle_ship(&mut state, &mut agents, ship_b.clone());
        add_idle_ship(&mut state, &mut agents, ship_c.clone());

        // Clear pre-existing scan sites from base_state
        state.scan_sites.clear();

        add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

        let site_id = SiteId("site_1".to_string());
        state.scan_sites.push(ScanSite {
            id: site_id.clone(),
            position: test_position(),
            template_id: "template_default".to_string(),
        });

        ShipAssignmentBridge::assign_objectives(&mut agents, &state, &content, &owner);

        // ship_a: Mine (highest priority with available candidate)
        assert!(matches!(
            agents[&ship_a].objective,
            Some(ShipObjective::Mine { .. })
        ));
        // ship_b: Survey (mine exhausted, survey available)
        assert!(matches!(
            agents[&ship_b].objective,
            Some(ShipObjective::Survey { .. })
        ));
        // ship_c: no objective (all candidates consumed)
        assert!(agents[&ship_c].objective.is_none());
    }

    #[test]
    fn existing_objective_not_overwritten() {
        let (mut state, content, mut agents) = setup();
        let owner = test_owner();

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut agents, ship_id.clone());

        add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

        // Pre-set an objective
        let existing = ShipObjective::DeepScan {
            asteroid_id: make_asteroid_id("other"),
        };
        agents.get_mut(&ship_id).unwrap().objective = Some(existing);

        ShipAssignmentBridge::assign_objectives(&mut agents, &state, &content, &owner);

        // Should still have the original DeepScan objective
        assert!(matches!(
            agents[&ship_id].objective,
            Some(ShipObjective::DeepScan { .. })
        ));
    }

    #[test]
    fn busy_ship_not_assigned() {
        let (mut state, content, mut agents) = setup();
        let owner = test_owner();

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut agents, ship_id.clone());

        // Make ship busy (active task)
        state.ships.get_mut(&ship_id).unwrap().task = Some(TaskState {
            kind: TaskKind::Mine {
                asteroid: make_asteroid_id("asteroid_x"),
                duration_ticks: 10,
            },
            started_tick: 0,
            eta_tick: 10,
        });

        add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

        ShipAssignmentBridge::assign_objectives(&mut agents, &state, &content, &owner);

        // Busy ship should not get an objective
        assert!(agents[&ship_id].objective.is_none());
    }

    #[test]
    fn ship_with_cargo_skipped_no_iterator_consumption() {
        let (mut state, content, mut agents) = setup();
        let owner = test_owner();

        let ship_a = make_ship_id("ship_a");
        let ship_b = make_ship_id("ship_b");
        add_idle_ship(&mut state, &mut agents, ship_a.clone());
        add_idle_ship(&mut state, &mut agents, ship_b.clone());

        let asteroid_1 = make_asteroid_id("asteroid_1");
        add_mineable_asteroid(&mut state, asteroid_1.clone(), 0.8);

        // Give ship_a cargo so deposit_priority fires → bridge skips it
        state
            .ships
            .get_mut(&ship_a)
            .unwrap()
            .inventory
            .push(InventoryItem::Ore {
                lot_id: LotId("lot_1".to_string()),
                asteroid_id: make_asteroid_id("some_asteroid"),
                kg: 50.0,
                composition: {
                    let mut c = std::collections::HashMap::new();
                    c.insert("Fe".to_string(), 0.8_f32);
                    c
                },
            });

        ShipAssignmentBridge::assign_objectives(&mut agents, &state, &content, &owner);

        // ship_a skipped (has cargo), no objective assigned
        assert!(agents[&ship_a].objective.is_none());
        // ship_b gets the asteroid (iterator not consumed by ship_a)
        assert!(matches!(
            agents[&ship_b].objective,
            Some(ShipObjective::Mine { ref asteroid_id }) if *asteroid_id == asteroid_1
        ));
    }

    #[test]
    fn survey_assigned_when_no_mine_candidates() {
        let (mut state, content, mut agents) = setup();
        let owner = test_owner();

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut agents, ship_id.clone());

        // Clear pre-existing scan sites, then add one
        state.scan_sites.clear();
        let site_id = SiteId("site_1".to_string());
        state.scan_sites.push(ScanSite {
            id: site_id.clone(),
            position: test_position(),
            template_id: "template_default".to_string(),
        });

        ShipAssignmentBridge::assign_objectives(&mut agents, &state, &content, &owner);

        assert!(matches!(
            agents[&ship_id].objective,
            Some(ShipObjective::Survey { ref site_id }) if site_id.0 == "site_1"
        ));
    }

    #[test]
    fn no_candidates_no_objective() {
        let (mut state, content, mut agents) = setup();
        let owner = test_owner();

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut agents, ship_id.clone());

        // Clear pre-existing scan sites; no asteroids either
        state.scan_sites.clear();
        ShipAssignmentBridge::assign_objectives(&mut agents, &state, &content, &owner);

        assert!(agents[&ship_id].objective.is_none());
    }
}
