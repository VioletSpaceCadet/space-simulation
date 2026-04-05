use std::collections::{BTreeMap, HashMap};

use sim_core::test_fixtures::{base_content, base_state, test_position};
use sim_core::{
    AsteroidId, AsteroidKnowledge, AsteroidState, Command, HullId, InventoryItem, LotId,
    PrincipalId, ShipId, StationId, TaskKind, TaskState, TechId,
};

use crate::agents::ship_agent::ShipAgent;
use crate::agents::Agent;
use crate::objectives::ShipObjective;

use super::concerns::{CrewRecruitment, ModuleManagement, SlagJettison};
use super::{StationAgent, StationConcern, StationContext};

#[test]
fn new_agent_has_default_concerns() {
    let agent = StationAgent::new(StationId("test_station".to_string()));
    assert_eq!(agent.station_id, StationId("test_station".to_string()));
    assert_eq!(agent.concerns.len(), 9);
    // Verify concern ordering matches expected sequence
    let names: Vec<&str> = agent.concerns.iter().map(|c| c.name()).collect();
    assert_eq!(
        names,
        vec![
            "module_management",
            "lab_assignment",
            "crew_assignment",
            "crew_recruitment",
            "component_import",
            "slag_jettison",
            "material_export",
            "propellant_management",
            "ship_fitting",
        ]
    );
}

#[test]
fn base_state_produces_no_commands() {
    let content = base_content();
    let state = base_state(&content);
    let owner = PrincipalId("principal_autopilot".to_string());
    let station_id = state.stations.keys().next().unwrap().clone();
    let mut agent = StationAgent::new(station_id);
    let mut next_id = 1;

    let commands = agent.generate(&state, &content, &owner, &mut next_id, None);
    // base_state has no modules in inventory, no disabled modules, no labs, etc.
    assert!(commands.is_empty());
}

#[test]
fn missing_station_produces_no_commands() {
    let content = base_content();
    let state = base_state(&content);
    let owner = PrincipalId("principal_autopilot".to_string());
    let mut agent = StationAgent::new(StationId("nonexistent".to_string()));
    let mut next_id = 1;

    let commands = agent.generate(&state, &content, &owner, &mut next_id, None);
    assert!(commands.is_empty());
}

#[test]
fn manage_modules_installs_from_inventory() {
    let content = base_content();
    let mut state = base_state(&content);
    let owner = PrincipalId("principal_autopilot".to_string());
    let station_id = state.stations.keys().next().unwrap().clone();

    // Add a module to station inventory
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .core
        .inventory
        .push(InventoryItem::Module {
            item_id: sim_core::ModuleItemId("item_1".to_string()),
            module_def_id: "mod_def_test".to_string(),
        });

    let mut concern = ModuleManagement;
    let mut next_id = 1;
    let mut ctx = StationContext {
        station_id: &station_id,
        state: &state,
        content: &content,
        owner: &owner,
        next_id: &mut next_id,
        trade_import_unlocked: false,
        trade_export_unlocked: false,
        decisions: None,
    };

    let commands = concern.generate(&mut ctx);

    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0].command,
        Command::InstallModule {
            facility_id: sim_core::FacilityId::Station(sid),
            ..
        } if *sid == station_id
    ));
}

#[test]
fn jettison_slag_fires_above_threshold() {
    let mut content = base_content();
    content.autopilot.slag_jettison_pct = 0.5;
    let mut state = base_state(&content);
    let owner = PrincipalId("principal_autopilot".to_string());
    let station_id = state.stations.keys().next().unwrap().clone();

    // Fill station above threshold with slag — use tiny capacity so volume ratio is high
    let station = state.stations.get_mut(&station_id).unwrap();
    station.core.cargo_capacity_m3 = 0.001;
    station.core.inventory.push(InventoryItem::Slag {
        kg: 100.0,
        composition: std::collections::HashMap::new(),
    });
    station.core.cached_inventory_volume_m3 = None;

    let mut concern = SlagJettison;
    let mut next_id = 1;
    let mut ctx = StationContext {
        station_id: &station_id,
        state: &state,
        content: &content,
        owner: &owner,
        next_id: &mut next_id,
        trade_import_unlocked: false,
        trade_export_unlocked: false,
        decisions: None,
    };

    let commands = concern.generate(&mut ctx);

    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0].command,
        Command::JettisonSlag { station_id: sid } if *sid == station_id
    ));
}

#[test]
fn recruit_crew_skips_when_salary_would_bankrupt() {
    use sim_core::test_fixtures::ModuleDefBuilder;

    let mut content = base_content();
    let role = sim_core::CrewRole("engineer".to_string());
    content.crew_roles.insert(
        role.clone(),
        sim_core::CrewRoleDef {
            id: role.clone(),
            name: "Engineer".to_string(),
            recruitment_cost: 100.0,
            salary_per_hour: 1_000_000.0, // Absurdly high → guarantees bankruptcy
        },
    );
    content.constants.trade_unlock_delay_minutes = 0;
    content.pricing.items.insert(
        "engineer".to_string(),
        sim_core::PricingEntry {
            base_price_per_unit: 10.0,
            importable: true,
            exportable: false,
            category: String::new(),
        },
    );
    // Module def requiring an engineer
    let mut mod_def = ModuleDefBuilder::new("mod_crew_test")
        .behavior(sim_core::ModuleBehaviorDef::Equipment)
        .build();
    mod_def.crew_requirement.insert(role.clone(), 1);
    content
        .module_defs
        .insert("mod_crew_test".to_string(), mod_def);

    let mut state = base_state(&content);
    state.balance = 1000.0;
    let owner = PrincipalId("principal_autopilot".to_string());
    let station_id = state.stations.keys().next().unwrap().clone();

    let station = state.stations.get_mut(&station_id).unwrap();
    station.core.modules.push(sim_core::ModuleState {
        id: sim_core::ModuleInstanceId("mod_1".to_string()),
        def_id: "mod_crew_test".to_string(),
        enabled: true,
        kind_state: sim_core::ModuleKindState::Equipment,
        wear: sim_core::WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
        slot_index: None,
    });
    station.rebuild_module_index(&content);

    let mut concern = CrewRecruitment;
    let mut next_id = 1;
    let mut ctx = StationContext {
        station_id: &station_id,
        state: &state,
        content: &content,
        owner: &owner,
        next_id: &mut next_id,
        trade_import_unlocked: true,
        trade_export_unlocked: true,
        decisions: None,
    };

    let commands = concern.generate(&mut ctx);

    // Should produce NO import command because salary projection shows bankruptcy
    assert!(
        commands.is_empty(),
        "should skip recruitment when salary would bankrupt: got {commands:?}"
    );
}

// --- SF-06: Framed-station slot-aware install tests ------------------------

/// Build a test environment where the starting station has a 2-slot frame
/// (1 industrial + 1 research) and two test module defs that fit those
/// slots: `sf06_industrial` and `sf06_research`.
fn sf06_framed_setup() -> (sim_core::GameContent, sim_core::GameState, StationId) {
    use sim_core::test_fixtures::ModuleDefBuilder;
    use sim_core::{FrameDef, FrameId, ModuleBehaviorDef, SlotDef, SlotType};

    let mut content = base_content();
    content.module_defs.insert(
        "sf06_industrial".to_string(),
        ModuleDefBuilder::new("sf06_industrial")
            .behavior(ModuleBehaviorDef::Equipment)
            .compatible_slots(vec![SlotType("industrial".to_string())])
            .build(),
    );
    content.module_defs.insert(
        "sf06_research".to_string(),
        ModuleDefBuilder::new("sf06_research")
            .behavior(ModuleBehaviorDef::Equipment)
            .compatible_slots(vec![SlotType("research".to_string())])
            .build(),
    );
    let frame_id = FrameId("frame_sf06".to_string());
    content.frames.insert(
        frame_id.clone(),
        FrameDef {
            id: frame_id.clone(),
            name: "SF06 Test Frame".to_string(),
            base_cargo_capacity_m3: 500.0,
            base_power_capacity_kw: 30.0,
            slots: vec![
                SlotDef {
                    slot_type: SlotType("industrial".to_string()),
                    label: "I1".to_string(),
                },
                SlotDef {
                    slot_type: SlotType("research".to_string()),
                    label: "R1".to_string(),
                },
            ],
            bonuses: vec![],
            required_tech: None,
            tags: vec![],
        },
    );

    let mut state = base_state(&content);
    let station_id = state.stations.keys().next().unwrap().clone();
    let station = state.stations.get_mut(&station_id).unwrap();
    station.frame_id = Some(frame_id);
    (content, state, station_id)
}

#[test]
fn manage_modules_framed_picks_compatible_slot() {
    let (content, mut state, station_id) = sf06_framed_setup();
    let station = state.stations.get_mut(&station_id).unwrap();
    station.core.inventory.push(InventoryItem::Module {
        item_id: sim_core::ModuleItemId("item_ind".to_string()),
        module_def_id: "sf06_industrial".to_string(),
    });

    let owner = PrincipalId("principal_autopilot".to_string());
    let mut concern = ModuleManagement;
    let mut next_id = 1;
    let mut ctx = StationContext {
        station_id: &station_id,
        state: &state,
        content: &content,
        owner: &owner,
        next_id: &mut next_id,
        trade_import_unlocked: false,
        trade_export_unlocked: false,
        decisions: None,
    };
    let commands = concern.generate(&mut ctx);

    let install = commands
        .iter()
        .find(|c| matches!(&c.command, Command::InstallModule { .. }))
        .expect("should issue an install command");
    match &install.command {
        Command::InstallModule { slot_index, .. } => {
            assert_eq!(
                *slot_index,
                Some(0),
                "industrial module should go to slot 0"
            );
        }
        other => panic!("expected InstallModule, got {other:?}"),
    }
}

#[test]
fn manage_modules_framed_skips_when_no_compatible_slot() {
    // Station with a research-only inventory item but no research slot
    // free: the autopilot should not issue an install command at all.
    let (mut content, mut state, station_id) = sf06_framed_setup();
    // Replace the research slot with another industrial slot so there is
    // no research slot available at all.
    let frame = content
        .frames
        .get_mut(&sim_core::FrameId("frame_sf06".to_string()))
        .unwrap();
    frame.slots[1].slot_type = sim_core::SlotType("industrial".to_string());

    let station = state.stations.get_mut(&station_id).unwrap();
    station.core.inventory.push(InventoryItem::Module {
        item_id: sim_core::ModuleItemId("item_research".to_string()),
        module_def_id: "sf06_research".to_string(),
    });

    let owner = PrincipalId("principal_autopilot".to_string());
    let mut concern = ModuleManagement;
    let mut next_id = 1;
    let mut ctx = StationContext {
        station_id: &station_id,
        state: &state,
        content: &content,
        owner: &owner,
        next_id: &mut next_id,
        trade_import_unlocked: false,
        trade_export_unlocked: false,
        decisions: None,
    };
    let commands = concern.generate(&mut ctx);

    assert!(
        !commands
            .iter()
            .any(|c| matches!(&c.command, Command::InstallModule { .. })),
        "autopilot should skip installs when no compatible slot exists"
    );
}

#[test]
fn manage_modules_framed_avoids_double_booking_same_tick() {
    // Two industrial modules in inventory + only one industrial slot:
    // the autopilot should issue exactly one install command and pick a
    // distinct slot. The second item is skipped this tick.
    let (content, mut state, station_id) = sf06_framed_setup();
    let station = state.stations.get_mut(&station_id).unwrap();
    station.core.inventory.push(InventoryItem::Module {
        item_id: sim_core::ModuleItemId("item_ind_a".to_string()),
        module_def_id: "sf06_industrial".to_string(),
    });
    station.core.inventory.push(InventoryItem::Module {
        item_id: sim_core::ModuleItemId("item_ind_b".to_string()),
        module_def_id: "sf06_industrial".to_string(),
    });

    let owner = PrincipalId("principal_autopilot".to_string());
    let mut concern = ModuleManagement;
    let mut next_id = 1;
    let mut ctx = StationContext {
        station_id: &station_id,
        state: &state,
        content: &content,
        owner: &owner,
        next_id: &mut next_id,
        trade_import_unlocked: false,
        trade_export_unlocked: false,
        decisions: None,
    };
    let commands = concern.generate(&mut ctx);

    let installs: Vec<_> = commands
        .iter()
        .filter(|c| matches!(&c.command, Command::InstallModule { .. }))
        .collect();
    assert_eq!(
        installs.len(),
        1,
        "only one install should fire (one industrial slot, two candidates)"
    );
}

// --- Ship assignment tests (ported from ShipAssignmentBridge) ---

fn test_owner() -> PrincipalId {
    PrincipalId("principal_autopilot".to_string())
}

fn make_ship_id(name: &str) -> ShipId {
    ShipId(name.to_string())
}

fn make_asteroid_id(name: &str) -> AsteroidId {
    AsteroidId(name.to_string())
}

fn assignment_setup() -> (
    sim_core::GameState,
    sim_core::GameContent,
    BTreeMap<ShipId, ShipAgent>,
) {
    let content = base_content();
    let state = base_state(&content);
    let agents = BTreeMap::new();
    (state, content, agents)
}

fn add_idle_ship(
    state: &mut sim_core::GameState,
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

fn add_mineable_asteroid(
    state: &mut sim_core::GameState,
    asteroid_id: AsteroidId,
    fe_fraction: f32,
) {
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
                    let mut composition = std::collections::HashMap::new();
                    composition.insert("Fe".to_string(), fe_fraction);
                    composition
                }),
            },
        },
    );
}

fn station_id_from_state(state: &sim_core::GameState) -> StationId {
    state.stations.keys().next().unwrap().clone()
}

#[test]
fn assign_no_idle_ships_no_assignments() {
    let (state, content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);
    let agent = StationAgent::new(station_id);

    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    assert!(ship_agents.is_empty());
}

#[test]
fn assign_two_ships_two_asteroids_no_double_assignment() {
    let (mut state, content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_a = make_ship_id("ship_a");
    let ship_b = make_ship_id("ship_b");
    add_idle_ship(&mut state, &mut ship_agents, ship_a.clone());
    add_idle_ship(&mut state, &mut ship_agents, ship_b.clone());

    let asteroid_1 = make_asteroid_id("asteroid_1");
    let asteroid_2 = make_asteroid_id("asteroid_2");
    add_mineable_asteroid(&mut state, asteroid_1.clone(), 0.8);
    add_mineable_asteroid(&mut state, asteroid_2.clone(), 0.5);

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    let obj_a = ship_agents[&ship_a]
        .objective
        .as_ref()
        .expect("ship_a should have objective");
    let obj_b = ship_agents[&ship_b]
        .objective
        .as_ref()
        .expect("ship_b should have objective");

    let id_a = match obj_a {
        ShipObjective::Mine { asteroid_id } => asteroid_id.clone(),
        other => panic!("expected Mine, got {other:?}"),
    };
    let id_b = match obj_b {
        ShipObjective::Mine { asteroid_id } => asteroid_id.clone(),
        other => panic!("expected Mine, got {other:?}"),
    };

    assert_ne!(id_a, id_b);
    assert_eq!(id_a, asteroid_1);
    assert_eq!(id_b, asteroid_2);
}

#[test]
fn assign_ship_with_cargo_skipped_no_iterator_consumption() {
    let (mut state, content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_a = make_ship_id("ship_a");
    let ship_b = make_ship_id("ship_b");
    add_idle_ship(&mut state, &mut ship_agents, ship_a.clone());
    add_idle_ship(&mut state, &mut ship_agents, ship_b.clone());

    let asteroid_1 = make_asteroid_id("asteroid_1");
    add_mineable_asteroid(&mut state, asteroid_1.clone(), 0.8);

    // Give ship_a cargo so deposit_priority fires → skipped
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
                let mut composition = std::collections::HashMap::new();
                composition.insert("Fe".to_string(), 0.8_f32);
                composition
            },
        });

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    assert!(ship_agents[&ship_a].objective.is_none());
    assert!(matches!(
        ship_agents[&ship_b].objective,
        Some(ShipObjective::Mine { ref asteroid_id }) if *asteroid_id == asteroid_1
    ));
}

#[test]
fn assign_busy_ship_not_assigned() {
    let (mut state, content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_id = make_ship_id("ship_a");
    add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());

    state.ships.get_mut(&ship_id).unwrap().task = Some(TaskState {
        kind: TaskKind::Mine {
            asteroid: make_asteroid_id("asteroid_x"),
            duration_ticks: 10,
        },
        started_tick: 0,
        eta_tick: 10,
    });

    add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    assert!(ship_agents[&ship_id].objective.is_none());
}

#[test]
fn assign_deep_scan_when_tech_unlocked() {
    let (mut state, mut content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_id = make_ship_id("ship_a");
    add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());
    state.scan_sites.clear();

    let tech_id = TechId("tech_deep_scan".to_string());
    content.autopilot.deep_scan_tech = "tech_deep_scan".to_string();
    content.autopilot.deep_scan_targets = vec![sim_core::DeepScanTargetConfig {
        tag: "IronRich".to_string(),
        min_confidence: 0.5,
    }];
    state.research.unlocked.insert(tech_id);

    let asteroid_id = make_asteroid_id("asteroid_scan");
    state.asteroids.insert(
        asteroid_id.clone(),
        AsteroidState {
            id: asteroid_id.clone(),
            position: test_position(),
            true_composition: std::collections::HashMap::new(),
            anomaly_tags: vec![],
            mass_kg: 500.0,
            knowledge: AsteroidKnowledge {
                tag_beliefs: vec![(sim_core::AnomalyTag("IronRich".to_string()), 0.9)],
                composition: None,
            },
        },
    );

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    assert!(matches!(
        ship_agents[&ship_id].objective,
        Some(ShipObjective::DeepScan { ref asteroid_id }) if asteroid_id.0 == "asteroid_scan"
    ));
}

#[test]
fn assign_deep_scan_skipped_without_tech() {
    let (mut state, mut content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_id = make_ship_id("ship_a");
    add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());
    state.scan_sites.clear();

    content.autopilot.deep_scan_tech = "tech_deep_scan".to_string();
    content.autopilot.deep_scan_targets = vec![sim_core::DeepScanTargetConfig {
        tag: "IronRich".to_string(),
        min_confidence: 0.5,
    }];

    state.asteroids.insert(
        make_asteroid_id("asteroid_scan"),
        AsteroidState {
            id: make_asteroid_id("asteroid_scan"),
            position: test_position(),
            true_composition: std::collections::HashMap::new(),
            anomaly_tags: vec![],
            mass_kg: 500.0,
            knowledge: AsteroidKnowledge {
                tag_beliefs: vec![(sim_core::AnomalyTag("IronRich".to_string()), 0.9)],
                composition: None,
            },
        },
    );

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    assert!(ship_agents[&ship_id].objective.is_none());
}

#[test]
fn assign_ship_not_at_station_still_assigned() {
    let (mut state, content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_id = make_ship_id("ship_a");
    add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());

    // Move ship to a different position than the station (simulates
    // completing a task at a remote location — VIO-457 regression test)
    let mut different_pos = test_position();
    different_pos.radius_au_um = sim_core::RadiusAuMicro(999_999);
    state.ships.get_mut(&ship_id).unwrap().position = different_pos;

    add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    // Ship at remote position still gets assigned (ship agent handles transit)
    assert!(ship_agents[&ship_id].objective.is_some());
}

#[test]
fn assign_three_ships_waterfall_mine_then_survey() {
    let (mut state, content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_a = make_ship_id("ship_a");
    let ship_b = make_ship_id("ship_b");
    let ship_c = make_ship_id("ship_c");
    add_idle_ship(&mut state, &mut ship_agents, ship_a.clone());
    add_idle_ship(&mut state, &mut ship_agents, ship_b.clone());
    add_idle_ship(&mut state, &mut ship_agents, ship_c.clone());

    state.scan_sites.clear();
    add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

    let site_id = sim_core::SiteId("site_1".to_string());
    state.scan_sites.push(sim_core::ScanSite {
        id: site_id.clone(),
        position: test_position(),
        template_id: "template_default".to_string(),
    });

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    assert!(matches!(
        ship_agents[&ship_a].objective,
        Some(ShipObjective::Mine { .. })
    ));
    assert!(matches!(
        ship_agents[&ship_b].objective,
        Some(ShipObjective::Survey { .. })
    ));
    // ship_c: all candidates consumed
    assert!(ship_agents[&ship_c].objective.is_none());
}

#[test]
fn assign_existing_objective_not_overwritten() {
    let (mut state, content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_id = make_ship_id("ship_a");
    add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());
    add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

    // Pre-set an objective
    ship_agents.get_mut(&ship_id).unwrap().objective = Some(ShipObjective::DeepScan {
        asteroid_id: make_asteroid_id("other"),
    });

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    assert!(matches!(
        ship_agents[&ship_id].objective,
        Some(ShipObjective::DeepScan { .. })
    ));
}

#[test]
fn assign_survey_when_no_mine_candidates() {
    let (mut state, content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_id = make_ship_id("ship_a");
    add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());

    state.scan_sites.clear();
    state.scan_sites.push(sim_core::ScanSite {
        id: sim_core::SiteId("site_1".to_string()),
        position: test_position(),
        template_id: "template_default".to_string(),
    });

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    assert!(matches!(
        ship_agents[&ship_id].objective,
        Some(ShipObjective::Survey { ref site_id }) if site_id.0 == "site_1"
    ));
}

#[test]
fn assign_no_candidates_no_objective() {
    let (mut state, content, mut ship_agents) = assignment_setup();
    let owner = test_owner();
    let station_id = station_id_from_state(&state);

    let ship_id = make_ship_id("ship_a");
    add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());

    state.scan_sites.clear();

    let agent = StationAgent::new(station_id);
    agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner, None);

    assert!(ship_agents[&ship_id].objective.is_none());
}

#[test]
fn manage_modules_sheds_load_during_power_deficit() {
    use sim_core::test_fixtures::ModuleDefBuilder;

    let mut content = base_content();
    // Two modules with different power priorities.
    // Lower number = less critical = shed first (matching sim_core convention).
    content.module_defs.insert(
        "module_least_critical".to_string(),
        ModuleDefBuilder::new("module_least_critical")
            .name("Least Critical")
            .mass(100.0)
            .volume(1.0)
            .power(20.0)
            .power_stall_priority(0) // lowest = shed first
            .behavior(sim_core::ModuleBehaviorDef::Assembler(
                sim_core::AssemblerDef {
                    assembly_interval_minutes: 1,
                    assembly_interval_ticks: 1,
                    max_stock: HashMap::new(),
                    recipes: vec![],
                },
            ))
            .build(),
    );
    content.module_defs.insert(
        "module_most_critical".to_string(),
        ModuleDefBuilder::new("module_most_critical")
            .name("Most Critical")
            .mass(100.0)
            .volume(1.0)
            .power(15.0)
            .power_stall_priority(4) // highest = shed last
            .behavior(sim_core::ModuleBehaviorDef::Assembler(
                sim_core::AssemblerDef {
                    assembly_interval_minutes: 1,
                    assembly_interval_ticks: 1,
                    max_stock: HashMap::new(),
                    recipes: vec![],
                },
            ))
            .build(),
    );

    let mut state = base_state(&content);
    let owner = PrincipalId("principal_autopilot".to_string());
    let station_id = state.stations.keys().next().unwrap().clone();

    // Install the two modules
    let station = state.stations.get_mut(&station_id).unwrap();
    station.core.modules.push(sim_core::ModuleState {
        id: sim_core::ModuleInstanceId("mod_least_critical".to_string()),
        def_id: "module_least_critical".to_string(),
        enabled: true,
        kind_state: sim_core::ModuleKindState::Assembler(sim_core::AssemblerState {
            ticks_since_last_run: 0,
            stalled: false,
            capped: false,
            cap_override: HashMap::new(),
            selected_recipe: None,
        }),
        wear: sim_core::WearState::default(),
        thermal: None,
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        slot_index: None,
    });
    station.core.modules.push(sim_core::ModuleState {
        id: sim_core::ModuleInstanceId("mod_most_critical".to_string()),
        def_id: "module_most_critical".to_string(),
        enabled: true,
        kind_state: sim_core::ModuleKindState::Assembler(sim_core::AssemblerState {
            ticks_since_last_run: 0,
            stalled: false,
            capped: false,
            cap_override: HashMap::new(),
            selected_recipe: None,
        }),
        wear: sim_core::WearState::default(),
        thermal: None,
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        slot_index: None,
    });

    // Set power state with deficit: 30kW gen, 50kW consumed = 20kW deficit
    station.core.power = sim_core::PowerState {
        generated_kw: 30.0,
        consumed_kw: 50.0,
        deficit_kw: 20.0,
        ..Default::default()
    };
    station.rebuild_module_index(&content);

    let mut concern = ModuleManagement;
    let mut next_id = 0u64;
    let mut ctx = StationContext {
        station_id: &station_id,
        state: &state,
        content: &content,
        owner: &owner,
        next_id: &mut next_id,
        trade_import_unlocked: false,
        trade_export_unlocked: false,
        decisions: None,
    };

    let commands = concern.generate(&mut ctx);

    // Should disable the least-critical module (stall_priority=0, shed first)
    // 20kW power consumption >= 20kW deficit, so only one module needs shedding
    let disable_cmds: Vec<_> = commands
        .iter()
        .filter(|c| matches!(&c.command, Command::SetModuleEnabled { enabled: false, .. }))
        .collect();
    assert!(
        !disable_cmds.is_empty(),
        "should disable at least one module during deficit"
    );
    // The least-critical module (stall_priority=0) should be disabled first
    let first_disabled = match &disable_cmds[0].command {
        Command::SetModuleEnabled { module_id, .. } => module_id.0.clone(),
        _ => panic!("expected SetModuleEnabled"),
    };
    assert_eq!(
        first_disabled, "mod_least_critical",
        "least-critical module (lowest priority number) should be shed first"
    );
}

#[test]
fn decision_logging_disabled_produces_no_records() {
    let content = base_content();
    let state = base_state(&content);
    let owner = PrincipalId("principal_autopilot".to_string());
    let station_id = state.stations.keys().next().unwrap().clone();
    let mut agent = StationAgent::new(station_id);
    let mut next_id = 1;

    // No decisions parameter (None) — should still produce commands without logging
    let commands = agent.generate(&state, &content, &owner, &mut next_id, None);
    assert!(commands.is_empty()); // base_state has nothing to do
}

#[test]
fn decision_logging_captures_ship_objective() {
    let (mut state, content, mut ship_agents) = assignment_setup();
    let station_id = station_id_from_state(&state);
    let agent = StationAgent::new(station_id);

    add_idle_ship(&mut state, &mut ship_agents, make_ship_id("ship_1"));
    add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 1.0);

    let mut decisions = Vec::new();
    let owner = test_owner();
    agent.assign_ship_objectives(
        &mut ship_agents,
        &state,
        &content,
        &owner,
        Some(&mut decisions),
    );

    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].concern, "ship_objectives");
    assert_eq!(decisions[0].decision_type, "assign_mine");
    assert_eq!(decisions[0].chosen_id, "asteroid_1");
}
