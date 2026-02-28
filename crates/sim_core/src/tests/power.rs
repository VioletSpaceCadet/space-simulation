use super::*;

fn solar_array_content() -> GameContent {
    let mut content = test_content();
    content.module_defs.insert(
        "module_basic_solar_array".to_string(),
        ModuleDef {
            id: "module_basic_solar_array".to_string(),
            name: "Basic Solar Array".to_string(),
            mass_kg: 1500.0,
            volume_m3: 12.0,
            power_consumption_per_run: 0.0,
            wear_per_run: 0.002,
            behavior: ModuleBehaviorDef::SolarArray(SolarArrayDef {
                base_output_kw: 50.0,
            }),
        },
    );
    content.module_defs.insert(
        "module_basic_iron_refinery".to_string(),
        ModuleDef {
            id: "module_basic_iron_refinery".to_string(),
            name: "Basic Iron Refinery".to_string(),
            mass_kg: 5000.0,
            volume_m3: 10.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.01,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 60,
                recipes: vec![],
            }),
        },
    );
    content
}

fn state_with_solar_array(content: &GameContent) -> GameState {
    let mut state = test_state(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("solar_inst_0001".to_string()),
        def_id: "module_basic_solar_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SolarArray(SolarArrayState::default()),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });
    state
}

#[test]
fn power_budget_solar_only() {
    let content = solar_array_content();
    let mut state = state_with_solar_array(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state
        .stations
        .get(&StationId("station_earth_orbit".to_string()))
        .unwrap();
    // Earth orbit has solar_intensity = 1.0, base_output = 50 kW, no wear
    assert!(
        (station.power.generated_kw - 50.0).abs() < f32::EPSILON,
        "generated_kw should be 50.0, got {}",
        station.power.generated_kw
    );
    assert!(
        station.power.consumed_kw.abs() < f32::EPSILON,
        "consumed_kw should be 0.0, got {}",
        station.power.consumed_kw
    );
    assert!(
        station.power.deficit_kw.abs() < f32::EPSILON,
        "deficit_kw should be 0.0, got {}",
        station.power.deficit_kw
    );
}

#[test]
fn power_budget_with_consumer() {
    let content = solar_array_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    // Add a refinery consuming 10 kW
    station.modules.push(ModuleState {
        id: ModuleInstanceId("refinery_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        (station.power.generated_kw - 50.0).abs() < f32::EPSILON,
        "generated = {}",
        station.power.generated_kw
    );
    assert!(
        (station.power.consumed_kw - 10.0).abs() < f32::EPSILON,
        "consumed = {}",
        station.power.consumed_kw
    );
    assert!(
        station.power.deficit_kw.abs() < f32::EPSILON,
        "deficit should be 0 (50 > 10), got {}",
        station.power.deficit_kw
    );
}

#[test]
fn power_budget_deficit_when_insufficient() {
    let mut content = solar_array_content();
    // Make a high-power consumer
    content.module_defs.insert(
        "module_power_hungry".to_string(),
        ModuleDef {
            id: "module_power_hungry".to_string(),
            name: "Power Hungry".to_string(),
            mass_kg: 1000.0,
            volume_m3: 5.0,
            power_consumption_per_run: 80.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 60,
                recipes: vec![],
            }),
        },
    );

    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("hungry_inst_0001".to_string()),
        def_id: "module_power_hungry".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        (station.power.generated_kw - 50.0).abs() < f32::EPSILON,
        "generated = {}",
        station.power.generated_kw
    );
    assert!(
        (station.power.consumed_kw - 80.0).abs() < f32::EPSILON,
        "consumed = {}",
        station.power.consumed_kw
    );
    assert!(
        (station.power.deficit_kw - 30.0).abs() < f32::EPSILON,
        "deficit should be 30 (80 - 50), got {}",
        station.power.deficit_kw
    );
}

#[test]
fn power_budget_solar_intensity_affects_output() {
    let mut content = solar_array_content();
    // Set the test node to low solar intensity (like inner belt)
    for node in &mut content.solar_system.nodes {
        if node.id == NodeId("node_test".to_string()) {
            node.solar_intensity = 0.4;
        }
    }

    let mut state = state_with_solar_array(&content);
    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state
        .stations
        .get(&StationId("station_earth_orbit".to_string()))
        .unwrap();
    // 50 kW * 0.4 = 20 kW
    assert!(
        (station.power.generated_kw - 20.0).abs() < f32::EPSILON,
        "generated_kw should be 20.0 at solar_intensity 0.4, got {}",
        station.power.generated_kw
    );
}

#[test]
fn power_budget_wear_reduces_output() {
    let content = solar_array_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Set solar array to degraded wear level
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules[0].wear.wear = content.constants.wear_band_degraded_threshold;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    let expected = 50.0 * content.constants.wear_band_degraded_efficiency;
    assert!(
        (station.power.generated_kw - expected).abs() < 0.01,
        "generated_kw should be {expected} with degraded wear, got {}",
        station.power.generated_kw
    );
}

#[test]
fn power_budget_disabled_modules_excluded() {
    let content = solar_array_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Disable the solar array
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules[0].enabled = false;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        station.power.generated_kw.abs() < f32::EPSILON,
        "disabled solar array should generate 0 kW, got {}",
        station.power.generated_kw
    );
}

// --- Power stalling tests ---

fn stall_content() -> GameContent {
    let mut content = solar_array_content();
    // Small solar array: only 15 kW (enough for one 10 kW module, not two)
    content
        .module_defs
        .get_mut("module_basic_solar_array")
        .unwrap()
        .behavior = ModuleBehaviorDef::SolarArray(SolarArrayDef {
        base_output_kw: 15.0,
    });
    // Add a sensor array (lowest priority, 8 kW)
    content.module_defs.insert(
        "module_sensor_array".to_string(),
        ModuleDef {
            id: "module_sensor_array".to_string(),
            name: "Sensor Array".to_string(),
            mass_kg: 2500.0,
            volume_m3: 6.0,
            power_consumption_per_run: 8.0,
            wear_per_run: 0.003,
            behavior: ModuleBehaviorDef::SensorArray(SensorArrayDef {
                data_kind: crate::DataKind::ScanData,
                action_key: "sensor_scan".to_string(),
                scan_interval_minutes: 120,
                scan_interval_ticks: 120,
            }),
        },
    );
    content
}

#[test]
fn power_stall_lowest_priority_first() {
    let content = stall_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Override solar array to 15 kW
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules[0] = ModuleState {
        id: ModuleInstanceId("solar_inst_0001".to_string()),
        def_id: "module_basic_solar_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SolarArray(SolarArrayState::default()),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    };

    // Add refinery (priority 3, 10 kW) and sensor (priority 0, 8 kW)
    // Total consumption: 18 kW, generation: 15 kW, deficit: 3 kW
    // Sensor (lowest priority) should be stalled first
    station.modules.push(ModuleState {
        id: ModuleInstanceId("refinery_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });
    station.modules.push(ModuleState {
        id: ModuleInstanceId("sensor_inst_0001".to_string()),
        def_id: "module_sensor_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState::default()),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    // Solar array (idx 0) should not be stalled
    assert!(
        !station.modules[0].power_stalled,
        "solar array should not be stalled"
    );
    // Refinery (idx 1, priority 3) should NOT be stalled
    assert!(
        !station.modules[1].power_stalled,
        "refinery (higher priority) should not be stalled"
    );
    // Sensor (idx 2, priority 0) should be stalled
    assert!(
        station.modules[2].power_stalled,
        "sensor (lowest priority) should be stalled"
    );
}

#[test]
fn power_stall_no_stalling_without_solar_arrays() {
    // Station with no solar arrays should not stall modules
    let content = solar_array_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    // Add just a refinery, no solar array
    station.modules.push(ModuleState {
        id: ModuleInstanceId("refinery_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        !station.modules[0].power_stalled,
        "modules should not be stalled when no solar arrays exist"
    );
}

#[test]
fn power_stall_clears_when_power_restored() {
    let content = stall_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    let station = state.stations.get_mut(&station_id).unwrap();
    // Add refinery (10 kW) and sensor (8 kW) — total 18 kW vs 15 kW solar = deficit
    station.modules.push(ModuleState {
        id: ModuleInstanceId("refinery_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });
    station.modules.push(ModuleState {
        id: ModuleInstanceId("sensor_inst_0001".to_string()),
        def_id: "module_sensor_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState::default()),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    // Phase 1: tick with deficit — sensor should be stalled
    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        station.modules[2].power_stalled,
        "sensor should be stalled during deficit (18 kW > 15 kW)"
    );

    // Phase 2: disable the sensor to restore surplus, then tick again
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules[2].enabled = false;

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        !station.modules[1].power_stalled,
        "refinery should not be stalled after power restored (15 kW > 10 kW)"
    );
    assert!(
        station.power.deficit_kw.abs() < f32::EPSILON,
        "no deficit expected after disabling sensor"
    );
}

// --- Battery tests ---

fn battery_content() -> GameContent {
    let mut content = solar_array_content();
    content.module_defs.insert(
        "module_basic_battery".to_string(),
        ModuleDef {
            id: "module_basic_battery".to_string(),
            name: "Basic Battery".to_string(),
            mass_kg: 2000.0,
            volume_m3: 4.0,
            power_consumption_per_run: 0.0,
            wear_per_run: 0.001,
            behavior: ModuleBehaviorDef::Battery(BatteryDef {
                capacity_kwh: 100.0,
                charge_rate_kw: 20.0,
                discharge_rate_kw: 30.0,
            }),
        },
    );
    content
}

#[test]
fn battery_charges_from_surplus() {
    let content = battery_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Solar: 50 kW, no consumers. Surplus = 50 kW, charge rate = 20 kW.
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("battery_inst_0001".to_string()),
        def_id: "module_basic_battery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Battery(BatteryState { charge_kwh: 0.0 }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    // Charge rate is 20 kW, surplus is 50 kW, so should charge at 20 kW
    assert!(
        (station.power.battery_charge_kw - 20.0).abs() < f32::EPSILON,
        "battery should charge at charge_rate_kw, got {}",
        station.power.battery_charge_kw
    );
    if let ModuleKindState::Battery(ref bs) = station.modules[1].kind_state {
        assert!(
            (bs.charge_kwh - 20.0).abs() < f32::EPSILON,
            "battery charge should be 20 kWh, got {}",
            bs.charge_kwh
        );
    } else {
        panic!("expected Battery kind_state");
    }
}

#[test]
fn battery_discharges_to_cover_deficit() {
    let mut content = battery_content();
    // Add a high-power consumer (80 kW demand vs 50 kW solar = 30 kW deficit)
    content.module_defs.insert(
        "module_power_hungry".to_string(),
        ModuleDef {
            id: "module_power_hungry".to_string(),
            name: "Power Hungry".to_string(),
            mass_kg: 1000.0,
            volume_m3: 5.0,
            power_consumption_per_run: 80.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 60,
                recipes: vec![],
            }),
        },
    );

    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    // Battery with 50 kWh charge, discharge_rate = 30 kW
    station.modules.push(ModuleState {
        id: ModuleInstanceId("battery_inst_0001".to_string()),
        def_id: "module_basic_battery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Battery(BatteryState { charge_kwh: 50.0 }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });
    station.modules.push(ModuleState {
        id: ModuleInstanceId("hungry_inst_0001".to_string()),
        def_id: "module_power_hungry".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    // Deficit = 80 - 50 = 30 kW, discharge rate = 30 kW, battery has 50 kWh
    // Battery discharges 30 kW, fully covering deficit
    assert!(
        (station.power.battery_discharge_kw - 30.0).abs() < f32::EPSILON,
        "battery should discharge 30 kW, got {}",
        station.power.battery_discharge_kw
    );
    assert!(
        station.power.deficit_kw.abs() < f32::EPSILON,
        "deficit should be 0 after battery discharge, got {}",
        station.power.deficit_kw
    );
    // No modules should be stalled since battery covers deficit
    assert!(
        !station.modules[2].power_stalled,
        "consumer should not be stalled when battery covers deficit"
    );
    // Battery charge should decrease
    if let ModuleKindState::Battery(ref bs) = station.modules[1].kind_state {
        assert!(
            (bs.charge_kwh - 20.0).abs() < f32::EPSILON,
            "battery charge should be 20 kWh after discharging 30, got {}",
            bs.charge_kwh
        );
    } else {
        panic!("expected Battery kind_state");
    }
}

#[test]
fn battery_partial_discharge_then_stall() {
    let mut content = battery_content();
    // 80 kW consumer vs 50 kW solar = 30 kW deficit
    content.module_defs.insert(
        "module_power_hungry".to_string(),
        ModuleDef {
            id: "module_power_hungry".to_string(),
            name: "Power Hungry".to_string(),
            mass_kg: 1000.0,
            volume_m3: 5.0,
            power_consumption_per_run: 80.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 60,
                recipes: vec![],
            }),
        },
    );

    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    // Battery with only 10 kWh — not enough to cover 30 kW deficit
    station.modules.push(ModuleState {
        id: ModuleInstanceId("battery_inst_0001".to_string()),
        def_id: "module_basic_battery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Battery(BatteryState { charge_kwh: 10.0 }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });
    station.modules.push(ModuleState {
        id: ModuleInstanceId("hungry_inst_0001".to_string()),
        def_id: "module_power_hungry".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    // Battery discharges 10 kW (limited by stored charge), deficit = 30 - 10 = 20 kW
    assert!(
        (station.power.battery_discharge_kw - 10.0).abs() < f32::EPSILON,
        "battery should discharge 10 kW (all it has), got {}",
        station.power.battery_discharge_kw
    );
    assert!(
        (station.power.deficit_kw - 20.0).abs() < f32::EPSILON,
        "remaining deficit should be 20 kW, got {}",
        station.power.deficit_kw
    );
    // Consumer should be stalled since battery can't cover full deficit
    assert!(
        station.modules[2].power_stalled,
        "consumer should be stalled when battery can't fully cover deficit"
    );
}

#[test]
fn battery_charge_limited_by_capacity() {
    let content = battery_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Battery nearly full — only 5 kWh headroom (capacity = 100 kWh)
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("battery_inst_0001".to_string()),
        def_id: "module_basic_battery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Battery(BatteryState { charge_kwh: 95.0 }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    // Charge limited by headroom (5 kWh), not charge_rate (20 kW)
    assert!(
        (station.power.battery_charge_kw - 5.0).abs() < f32::EPSILON,
        "battery should charge only 5 kW (headroom limited), got {}",
        station.power.battery_charge_kw
    );
}

#[test]
fn battery_wear_reduces_effective_capacity() {
    let content = battery_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Battery at degraded wear — effective capacity reduced
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("battery_inst_0001".to_string()),
        def_id: "module_basic_battery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Battery(BatteryState { charge_kwh: 0.0 }),
        wear: WearState {
            wear: content.constants.wear_band_degraded_threshold,
        },
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    // Run multiple ticks to charge up
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let station = state.stations.get(&station_id).unwrap();
    if let ModuleKindState::Battery(ref bs) = station.modules[1].kind_state {
        let effective_capacity = 100.0 * content.constants.wear_band_degraded_efficiency;
        assert!(
            bs.charge_kwh <= effective_capacity + 0.01,
            "battery charge {} should not exceed effective capacity {} (wear-limited)",
            bs.charge_kwh,
            effective_capacity
        );
    } else {
        panic!("expected Battery kind_state");
    }
}

#[test]
fn battery_not_stalled_by_power_system() {
    let content = battery_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("battery_inst_0001".to_string()),
        def_id: "module_basic_battery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Battery(BatteryState { charge_kwh: 50.0 }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        !station.modules[1].power_stalled,
        "battery should never be power_stalled"
    );
}
