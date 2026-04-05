use rand::SeedableRng;
use sim_core::{
    test_fixtures::{base_content, ModuleDefBuilder},
    Command, DataKind, Event, FacilityCore, GroundFacilityId, GroundFacilityState,
    ModuleBehaviorDef, ModuleKindState, ModuleState, ModuleTypeIndex, SensorArrayDef,
    SensorArrayState, TradeItemSpec, WearState,
};

use crate::CommandSource;

fn ground_content() -> sim_core::GameContent {
    let mut content = base_content();
    content.autopilot.ground_sensor_modules = vec![
        "module_optical_telescope".to_string(),
        "module_radio_telescope".to_string(),
    ];
    content.autopilot.ground_opex_max_fraction = 0.001;
    content.autopilot.budget_cap_fraction = 0.05;

    // Add telescope module defs
    content.module_defs.insert(
        "module_optical_telescope".to_string(),
        ModuleDefBuilder::new("module_optical_telescope")
            .name("Optical Telescope")
            .behavior(ModuleBehaviorDef::SensorArray(SensorArrayDef {
                data_kind: DataKind::new(DataKind::OPTICAL),
                action_key: "optical_scan".to_string(),
                scan_interval_minutes: 180,
                scan_interval_ticks: 180,
                sensor_type: "optical".to_string(),
                discovery_zones: vec!["earth_orbit_zone".to_string()],
                discovery_probability: 0.15,
            }))
            .operating_cost(100.0)
            .build(),
    );
    content.module_defs.insert(
        "module_radio_telescope".to_string(),
        ModuleDefBuilder::new("module_radio_telescope")
            .name("Radio Telescope")
            .behavior(ModuleBehaviorDef::SensorArray(SensorArrayDef {
                data_kind: DataKind::new(DataKind::RADIO),
                action_key: "radio_scan".to_string(),
                scan_interval_minutes: 240,
                scan_interval_ticks: 240,
                sensor_type: "radio".to_string(),
                discovery_zones: vec!["inner_belt".to_string()],
                discovery_probability: 0.08,
            }))
            .operating_cost(250.0)
            .build(),
    );

    // Add pricing
    content.pricing.items.insert(
        "module_optical_telescope".to_string(),
        sim_core::PricingEntry {
            base_price_per_unit: 5_000_000.0,
            importable: true,
            exportable: false,
            category: "module".to_string(),
        },
    );
    content.pricing.items.insert(
        "module_radio_telescope".to_string(),
        sim_core::PricingEntry {
            base_price_per_unit: 25_000_000.0,
            importable: true,
            exportable: false,
            category: "module".to_string(),
        },
    );

    // Add rocket component defs.
    content.component_defs.push(sim_core::ComponentDef {
        id: "solid_fuel_grain".to_string(),
        name: "Solid Fuel Grain".to_string(),
        mass_kg: 150.0,
        volume_m3: 0.3,
    });
    content.component_defs.push(sim_core::ComponentDef {
        id: "guidance_unit".to_string(),
        name: "Guidance Unit".to_string(),
        mass_kg: 20.0,
        volume_m3: 0.05,
    });

    // Add launch pad module def for launch tests.
    content.module_defs.insert(
        "module_launch_pad_small".to_string(),
        ModuleDefBuilder::new("module_launch_pad_small")
            .name("Small Launch Pad")
            .behavior(ModuleBehaviorDef::LaunchPad(sim_core::LaunchPadDef {
                max_payload_kg: 20000.0,
                recovery_minutes: 5,
                recovery_ticks: 5,
            }))
            .build(),
    );

    content
}

fn ground_state(content: &sim_core::GameContent) -> sim_core::GameState {
    let mut state = sim_core::test_fixtures::base_state(content);
    state.scan_sites.clear();
    state.balance = 1_000_000_000.0; // $1B

    let facility_id = GroundFacilityId("ground_earth".to_string());
    state.ground_facilities.insert(
        facility_id.clone(),
        GroundFacilityState {
            id: facility_id,
            name: "Earth Operations".to_string(),
            position: sim_core::test_fixtures::test_position(),
            core: FacilityCore {
                modules: vec![],
                inventory: vec![],
                cargo_capacity_m3: 10000.0,
                power_available_per_tick: 0.0,
                ..Default::default()
            },
            launch_transits: vec![],
        },
    );
    state
}

#[test]
fn autopilot_purchases_optical_telescope_first() {
    let content = ground_content();
    let state = ground_state(&content);
    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;

    let commands = controller.generate_commands(&state, &content, &mut next_id);

    // Should purchase optical telescope (first in priority order)
    let import_cmd = commands.iter().find(|c| {
        matches!(&c.command, Command::Import { item_spec, .. } if matches!(
            item_spec, TradeItemSpec::Module { module_def_id } if module_def_id == "module_optical_telescope"
        ))
    });
    assert!(
        import_cmd.is_some(),
        "should purchase optical telescope first"
    );

    // Should NOT purchase radio telescope yet (one per tick)
    let radio_cmd = commands.iter().find(|c| {
        matches!(&c.command, Command::Import { item_spec, .. } if matches!(
            item_spec, TradeItemSpec::Module { module_def_id } if module_def_id == "module_radio_telescope"
        ))
    });
    assert!(radio_cmd.is_none(), "should not buy radio on same tick");
}

#[test]
fn autopilot_skips_purchase_when_already_installed() {
    let content = ground_content();
    let mut state = ground_state(&content);
    let facility_id = GroundFacilityId("ground_earth".to_string());

    // Pre-install optical telescope
    let facility = state.ground_facilities.get_mut(&facility_id).unwrap();
    facility.core.modules.push(ModuleState {
        id: sim_core::ModuleInstanceId("optical_001".to_string()),
        def_id: "module_optical_telescope".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
    });
    facility.core.module_type_index = ModuleTypeIndex::default();
    sim_core::test_fixtures::rebuild_indices(&mut state, &content);

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    // Should skip optical (installed) and buy radio
    let radio_cmd = commands.iter().find(|c| {
        matches!(&c.command, Command::Import { item_spec, .. } if matches!(
            item_spec, TradeItemSpec::Module { module_def_id } if module_def_id == "module_radio_telescope"
        ))
    });
    assert!(
        radio_cmd.is_some(),
        "should purchase radio when optical already installed"
    );
}

#[test]
fn autopilot_skips_purchase_when_insufficient_budget() {
    let content = ground_content();
    let mut state = ground_state(&content);
    // balance * budget_cap_fraction = 1000 * 0.05 = $50 — not enough for $5M telescope
    state.balance = 1000.0;

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    let sensor_import = commands.iter().any(|c| {
        matches!(&c.command, Command::Import { item_spec, .. } if matches!(
            item_spec, TradeItemSpec::Module { module_def_id }
            if module_def_id == "module_optical_telescope" || module_def_id == "module_radio_telescope"
        ))
    });
    assert!(
        !sensor_import,
        "should not purchase with insufficient budget"
    );
}

#[test]
fn sensor_budget_disables_when_over_opex_limit() {
    let content = ground_content();
    let mut state = ground_state(&content);
    let facility_id = GroundFacilityId("ground_earth".to_string());

    // Set balance low so opex exceeds max_fraction
    // opex = 100 + 250 = 350 per tick
    // max_opex = balance * 0.001 = 100_000 * 0.001 = 100
    state.balance = 100_000.0;

    let facility = state.ground_facilities.get_mut(&facility_id).unwrap();
    facility.core.modules.push(ModuleState {
        id: sim_core::ModuleInstanceId("optical_001".to_string()),
        def_id: "module_optical_telescope".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
    });
    facility.core.modules.push(ModuleState {
        id: sim_core::ModuleInstanceId("radio_001".to_string()),
        def_id: "module_radio_telescope".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
    });
    facility.core.module_type_index = ModuleTypeIndex::default();
    sim_core::test_fixtures::rebuild_indices(&mut state, &content);

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    // Should disable the more expensive sensor (radio at $250/tick) first
    let disable_cmds: Vec<_> = commands
        .iter()
        .filter(|c| matches!(&c.command, Command::SetModuleEnabled { enabled: false, .. }))
        .collect();
    assert!(
        !disable_cmds.is_empty(),
        "should disable sensors when over opex budget"
    );
}

#[test]
fn sensor_budget_reenables_when_budget_allows() {
    let content = ground_content();
    let mut state = ground_state(&content);
    let facility_id = GroundFacilityId("ground_earth".to_string());

    // High balance: opex (100) << max_opex (1B * 0.001 = 1M)
    state.balance = 1_000_000_000.0;

    let facility = state.ground_facilities.get_mut(&facility_id).unwrap();
    // Disabled optical telescope
    facility.core.modules.push(ModuleState {
        id: sim_core::ModuleInstanceId("optical_001".to_string()),
        def_id: "module_optical_telescope".to_string(),
        enabled: false,
        kind_state: ModuleKindState::SensorArray(SensorArrayState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
    });
    facility.core.module_type_index = ModuleTypeIndex::default();
    sim_core::test_fixtures::rebuild_indices(&mut state, &content);

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    let enable_cmds: Vec<_> = commands
        .iter()
        .filter(|c| matches!(&c.command, Command::SetModuleEnabled { enabled: true, .. }))
        .collect();
    assert!(
        !enable_cmds.is_empty(),
        "should re-enable sensor when budget allows"
    );
}

/// Integration test: autopilot buys a sensor for a ground facility, the sensor
/// runs via the proxy-station pattern, and discovers scan sites within 500 ticks.
#[test]
fn autopilot_managed_ground_facility_discovers_within_500_ticks() {
    // Content with 100% discovery probability for reliable testing.
    let mut content = ground_content();
    content.module_defs.insert(
        "module_optical_telescope".to_string(),
        ModuleDefBuilder::new("module_optical_telescope")
            .name("Optical Telescope")
            .behavior(ModuleBehaviorDef::SensorArray(SensorArrayDef {
                data_kind: DataKind::new(DataKind::OPTICAL),
                action_key: "optical_scan".to_string(),
                scan_interval_minutes: 1, // Fire every tick (test uses minutes_per_tick=1)
                scan_interval_ticks: 1,
                sensor_type: "optical".to_string(),
                discovery_zones: vec!["earth_orbit_zone".to_string()],
                discovery_probability: 1.0, // 100% for test reliability
            }))
            .operating_cost(100.0)
            .build(),
    );

    // Add zone body for discovery target.
    content.solar_system.bodies.push(sim_core::OrbitalBodyDef {
        id: sim_core::BodyId("earth_orbit_zone".to_string()),
        name: "Earth Orbit".to_string(),
        parent: None,
        body_type: sim_core::BodyType::Zone,
        radius_au_um: 1_000_000,
        angle_mdeg: 0,
        solar_intensity: 1.0,
        zone: Some(sim_core::ZoneDef {
            radius_min_au_um: 900_000,
            radius_max_au_um: 1_100_000,
            angle_start_mdeg: 0,
            angle_span_mdeg: 360_000,
            resource_class: sim_core::spatial::ResourceClass::Mixed,
            scan_site_weight: 1,
            implicit_comm_tier: None,
        }),
    });

    content.constants.replenish_target_count = 100;

    let mut state = ground_state(&content);
    state.scan_sites.clear();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
    let mut controller = crate::AutopilotController::new();

    // Run 10 ticks — enough for: import (tick 0), install (tick 1),
    // enable (tick 2), sensor fire + discover (tick 3+).
    let mut next_cmd_id = state.counters.next_command_id;
    let mut imported = false;
    let mut installed = false;
    for tick in 0..10 {
        state.meta.tick = tick;
        let commands = controller.generate_commands(&state, &content, &mut next_cmd_id);
        let events = sim_core::tick(&mut state, &commands, &content, &mut rng, None);

        for event in &events {
            match &event.event {
                Event::ItemImported { .. } => imported = true,
                Event::ModuleInstalled { .. } => installed = true,
                _ => {}
            }
        }
    }

    assert!(imported, "sensor module should have been imported");
    assert!(installed, "sensor module should have been installed");

    // Verify the ground facility has an installed, enabled sensor.
    let facility = state
        .ground_facilities
        .get(&GroundFacilityId("ground_earth".to_string()))
        .expect("ground facility exists");
    let has_sensor = facility
        .core
        .modules
        .iter()
        .any(|m| m.def_id == "module_optical_telescope" && m.enabled);
    assert!(
        has_sensor,
        "ground facility should have an enabled optical telescope"
    );

    // Scan sites should exist (from sensor discovery and/or replenish).
    assert!(
        !state.scan_sites.is_empty(),
        "scan_sites should contain discovered sites"
    );
}

#[test]
fn component_purchase_buys_when_budget_allows() {
    let mut content = ground_content();
    content.pricing.items.insert(
        "solid_fuel_grain".to_string(),
        sim_core::PricingEntry {
            base_price_per_unit: 50_000.0,
            importable: true,
            exportable: false,
            category: "component".to_string(),
        },
    );
    content.pricing.items.insert(
        "guidance_unit".to_string(),
        sim_core::PricingEntry {
            base_price_per_unit: 200_000.0,
            importable: true,
            exportable: false,
            category: "component".to_string(),
        },
    );
    let state = ground_state(&content);
    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;

    let commands = controller.generate_commands(&state, &content, &mut next_id);

    let component_imports: Vec<_> = commands
        .iter()
        .filter(|c| {
            matches!(
                &c.command,
                Command::Import { item_spec, .. }
                    if matches!(item_spec, sim_core::TradeItemSpec::Component { .. })
            )
        })
        .collect();
    assert!(
        !component_imports.is_empty(),
        "should purchase rocket components"
    );
}

#[test]
fn component_purchase_skips_when_insufficient_budget() {
    let mut content = ground_content();
    content.pricing.items.insert(
        "solid_fuel_grain".to_string(),
        sim_core::PricingEntry {
            base_price_per_unit: 50_000.0,
            importable: true,
            exportable: false,
            category: "component".to_string(),
        },
    );
    let mut state = ground_state(&content);
    state.balance = 100.0; // too low for any purchase

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    let component_imports = commands.iter().any(|c| {
        matches!(
            &c.command,
            Command::Import { item_spec, .. }
                if matches!(item_spec, sim_core::TradeItemSpec::Component { .. })
        )
    });
    assert!(
        !component_imports,
        "should not purchase with insufficient budget"
    );
}

#[test]
fn launch_execution_skips_without_rocket_component() {
    let content = ground_content();
    let mut state = ground_state(&content);

    // Add a pad but no rocket component.
    let facility = state
        .ground_facilities
        .get_mut(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    facility.core.modules.push(ModuleState {
        id: sim_core::ModuleInstanceId("pad_001".to_string()),
        def_id: "module_launch_pad_small".to_string(),
        enabled: true,
        kind_state: ModuleKindState::LaunchPad(sim_core::LaunchPadState::default()),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
    });
    sim_core::test_fixtures::rebuild_indices(&mut state, &content);

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    let launch_cmds = commands
        .iter()
        .any(|c| matches!(&c.command, Command::Launch { .. }));
    assert!(!launch_cmds, "should not launch without rocket component");
}

fn satellite_content() -> sim_core::GameContent {
    let mut content = ground_content();

    // Add satellite defs.
    content.satellite_defs.insert(
        "sat_comm_relay".to_string(),
        sim_core::SatelliteDef {
            id: "sat_comm_relay".to_string(),
            name: "Comm Relay".to_string(),
            satellite_type: "communication".to_string(),
            mass_kg: 800.0,
            wear_rate: 0.00008,
            required_tech: Some(sim_core::TechId("tech_satellite_basics".to_string())),
            behavior_config: serde_json::json!({ "comm_tier": "Basic" }),
        },
    );
    content.satellite_defs.insert(
        "sat_survey".to_string(),
        sim_core::SatelliteDef {
            id: "sat_survey".to_string(),
            name: "Survey Satellite".to_string(),
            satellite_type: "survey".to_string(),
            mass_kg: 500.0,
            wear_rate: 0.00015,
            required_tech: Some(sim_core::TechId("tech_satellite_basics".to_string())),
            behavior_config: serde_json::json!({ "discovery_multiplier": 2.0 }),
        },
    );

    // Add component defs for satellite products (needed for compute_mass).
    content.component_defs.push(sim_core::ComponentDef {
        id: "sat_comm_relay".to_string(),
        name: "Comm Relay".to_string(),
        mass_kg: 800.0,
        volume_m3: 2.0,
    });
    content.component_defs.push(sim_core::ComponentDef {
        id: "sat_survey".to_string(),
        name: "Survey Satellite".to_string(),
        mass_kg: 500.0,
        volume_m3: 1.5,
    });

    // Satellite config.
    content.autopilot.satellite_priority =
        vec!["sat_comm_relay".to_string(), "sat_survey".to_string()];
    content.autopilot.satellite_launch_rocket = "rocket_sounding".to_string();
    content.autopilot.satellite_tech = "tech_satellite_basics".to_string();
    content.autopilot.satellite_replacement_wear = 0.7;

    // Add rocket def for satellite launches.
    content.rocket_defs.insert(
        "rocket_sounding".to_string(),
        sim_core::RocketDef {
            id: "rocket_sounding".to_string(),
            name: "Sounding Rocket".to_string(),
            payload_capacity_kg: 1000.0,
            base_launch_cost: 2_000_000.0,
            fuel_kg: 500.0,
            transit_minutes: 3,
            required_tech: None,
        },
    );

    // Pricing for satellite components.
    content.pricing.items.insert(
        "sat_comm_relay".to_string(),
        sim_core::PricingEntry {
            base_price_per_unit: 500_000.0,
            importable: true,
            exportable: false,
            category: "component".to_string(),
        },
    );
    content.pricing.items.insert(
        "sat_survey".to_string(),
        sim_core::PricingEntry {
            base_price_per_unit: 300_000.0,
            importable: true,
            exportable: false,
            category: "component".to_string(),
        },
    );

    content
}

#[test]
fn satellite_management_skips_without_tech() {
    let content = satellite_content();
    let state = ground_state(&content);
    // tech_satellite_basics not unlocked.
    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    let sat_import = commands.iter().any(|c| {
        matches!(&c.command, Command::Import { item_spec, .. }
            if matches!(item_spec, TradeItemSpec::Component { component_id, .. }
                if component_id.0.starts_with("sat_")))
    });
    assert!(!sat_import, "should not import satellite without tech");
}

#[test]
fn satellite_management_imports_first_priority_component() {
    let content = satellite_content();
    let mut state = ground_state(&content);
    // Unlock satellite tech.
    state
        .research
        .unlocked
        .insert(sim_core::TechId("tech_satellite_basics".to_string()));

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    // Should import sat_comm_relay (first in priority).
    let import_comm = commands.iter().any(|c| {
        matches!(&c.command, Command::Import { item_spec, .. }
            if matches!(item_spec, TradeItemSpec::Component { component_id, .. }
                if component_id.0 == "sat_comm_relay"))
    });
    assert!(
        import_comm,
        "should import first priority satellite component (sat_comm_relay)"
    );
}

#[test]
fn satellite_management_launches_when_component_available() {
    let content = satellite_content();
    let mut state = ground_state(&content);
    state
        .research
        .unlocked
        .insert(sim_core::TechId("tech_satellite_basics".to_string()));

    // Add satellite component + fuel + launch pad to facility.
    let facility = state
        .ground_facilities
        .get_mut(&GroundFacilityId("ground_earth".to_string()))
        .unwrap();
    facility
        .core
        .inventory
        .push(sim_core::InventoryItem::Component {
            component_id: sim_core::ComponentId("sat_comm_relay".to_string()),
            count: 1,
            quality: 1.0,
        });
    facility
        .core
        .inventory
        .push(sim_core::InventoryItem::Material {
            element: "LH2".to_string(),
            kg: 10000.0,
            quality: 1.0,
            thermal: None,
        });
    facility.core.modules.push(ModuleState {
        id: sim_core::ModuleInstanceId("pad_001".to_string()),
        def_id: "module_launch_pad_small".to_string(),
        enabled: true,
        kind_state: ModuleKindState::LaunchPad(sim_core::LaunchPadState::default()),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
    });
    sim_core::test_fixtures::rebuild_indices(&mut state, &content);

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    let launch_sat = commands.iter().any(|c| {
        matches!(&c.command, Command::Launch { payload, .. }
            if matches!(payload, sim_core::LaunchPayload::Satellite { satellite_def_id }
                if satellite_def_id == "sat_comm_relay"))
    });
    assert!(
        launch_sat,
        "should launch satellite when component is available"
    );
}

#[test]
fn satellite_management_skips_when_satellite_already_active() {
    let content = satellite_content();
    let mut state = ground_state(&content);
    state
        .research
        .unlocked
        .insert(sim_core::TechId("tech_satellite_basics".to_string()));

    // Add an active comm relay satellite.
    state.satellites.insert(
        sim_core::SatelliteId("sat_existing".to_string()),
        sim_core::SatelliteState {
            id: sim_core::SatelliteId("sat_existing".to_string()),
            def_id: "sat_comm_relay".to_string(),
            name: "Existing Relay".to_string(),
            position: sim_core::test_fixtures::test_position(),
            deployed_tick: 0,
            wear: 0.1,
            enabled: true,
            satellite_type: "communication".to_string(),
            payload_config: None,
        },
    );

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    // Should skip comm relay (already active) and import survey instead.
    let import_survey = commands.iter().any(|c| {
        matches!(&c.command, Command::Import { item_spec, .. }
            if matches!(item_spec, TradeItemSpec::Component { component_id, .. }
                if component_id.0 == "sat_survey"))
    });
    assert!(
        import_survey,
        "should import next priority (sat_survey) when comm relay already active"
    );
}

#[test]
fn satellite_management_replaces_aging_satellite() {
    let content = satellite_content();
    let mut state = ground_state(&content);
    state
        .research
        .unlocked
        .insert(sim_core::TechId("tech_satellite_basics".to_string()));

    // Add an aging comm relay (wear > replacement_wear threshold of 0.7).
    state.satellites.insert(
        sim_core::SatelliteId("sat_old".to_string()),
        sim_core::SatelliteState {
            id: sim_core::SatelliteId("sat_old".to_string()),
            def_id: "sat_comm_relay".to_string(),
            name: "Old Relay".to_string(),
            position: sim_core::test_fixtures::test_position(),
            deployed_tick: 0,
            wear: 0.8, // above 0.7 threshold
            enabled: true,
            satellite_type: "communication".to_string(),
            payload_config: None,
        },
    );

    let mut controller = crate::AutopilotController::new();
    let mut next_id = 100;
    let commands = controller.generate_commands(&state, &content, &mut next_id);

    // Should import a replacement comm relay.
    let import_comm = commands.iter().any(|c| {
        matches!(&c.command, Command::Import { item_spec, .. }
            if matches!(item_spec, TradeItemSpec::Component { component_id, .. }
                if component_id.0 == "sat_comm_relay"))
    });
    assert!(import_comm, "should import replacement for aging satellite");
}
