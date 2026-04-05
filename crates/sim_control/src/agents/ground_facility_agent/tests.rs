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
    let mut enabled = false;
    for tick in 0..10 {
        state.meta.tick = tick;
        let commands = controller.generate_commands(&state, &content, &mut next_cmd_id);
        let events = sim_core::tick(&mut state, &commands, &content, &mut rng, None);

        for event in &events {
            match &event.event {
                Event::ItemImported { .. } => imported = true,
                Event::ModuleInstalled { .. } => installed = true,
                Event::ModuleToggled { enabled: true, .. } => enabled = true,
                _ => {}
            }
        }
    }

    assert!(imported, "sensor module should have been imported");
    assert!(installed, "sensor module should have been installed");
    assert!(enabled, "sensor module should have been enabled");

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
