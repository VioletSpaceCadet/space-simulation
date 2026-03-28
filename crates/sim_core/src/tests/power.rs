use super::*;
use crate::test_fixtures::ModuleDefBuilder;

fn solar_array_content() -> GameContent {
    let mut content = test_content();
    content.module_defs.insert(
        "module_basic_solar_array".to_string(),
        ModuleDefBuilder::new("module_basic_solar_array")
            .name("Basic Solar Array")
            .mass(1500.0)
            .volume(12.0)
            .wear(0.002)
            .behavior(ModuleBehaviorDef::SolarArray(SolarArrayDef {
                base_output_kw: 50.0,
            }))
            .build(),
    );
    content.module_defs.insert(
        "module_basic_iron_refinery".to_string(),
        ModuleDefBuilder::new("module_basic_iron_refinery")
            .name("Basic Iron Refinery")
            .mass(5000.0)
            .volume(10.0)
            .power(10.0)
            .wear(0.01)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 60,
                recipes: vec![],
            }))
            .build(),
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
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });
    state
}

#[test]
fn power_budget_solar_only() {
    let content = solar_array_content();
    let mut state = state_with_solar_array(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);

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
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

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
        ModuleDefBuilder::new("module_power_hungry")
            .name("Power Hungry")
            .mass(1000.0)
            .volume(5.0)
            .power(80.0)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 60,
                recipes: vec![],
            }))
            .build(),
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
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

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
    // Set "test_body" (the station's parent body) to low solar intensity (like inner belt).
    // base_content() already provides a "test_body" body; we update its solar_intensity.
    if let Some(body) = content
        .solar_system
        .bodies
        .iter_mut()
        .find(|b| b.id.0 == "test_body")
    {
        body.solar_intensity = 0.4;
    }

    let mut state = state_with_solar_array(&content);
    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

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
    tick(&mut state, &[], &content, &mut rng, None);

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
    tick(&mut state, &[], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        station.power.generated_kw.abs() < f32::EPSILON,
        "disabled solar array should generate 0 kW, got {}",
        station.power.generated_kw
    );
}

#[test]
fn solar_output_boosted_by_tech_modifier() {
    let content = solar_array_content();
    let mut state = state_with_solar_array(&content);

    // Add a SolarOutput modifier simulating an unlocked tech
    state.modifiers.add(crate::modifiers::Modifier {
        stat: crate::modifiers::StatId::SolarOutput,
        op: crate::modifiers::ModifierOp::PctAdditive,
        value: 0.5,
        source: crate::modifiers::ModifierSource::Tech("tech_solar_efficiency".into()),
        condition: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

    let station = state
        .stations
        .get(&StationId("station_earth_orbit".to_string()))
        .unwrap();
    // Base 50 kW * (1 + 0.5) = 75 kW
    assert!(
        (station.power.generated_kw - 75.0).abs() < 0.01,
        "solar output should be 75.0 with +50% tech modifier, got {}",
        station.power.generated_kw
    );
}

#[test]
fn solar_tech_modifier_does_not_affect_battery() {
    let content = battery_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Add battery at full capacity
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("battery_inst_0001".to_string()),
        def_id: "module_basic_battery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Battery(BatteryState { charge_kwh: 95.0 }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    // Add SolarOutput modifier — should NOT affect battery capacity
    state.modifiers.add(crate::modifiers::Modifier {
        stat: crate::modifiers::StatId::SolarOutput,
        op: crate::modifiers::ModifierOp::PctAdditive,
        value: 0.5,
        source: crate::modifiers::ModifierSource::Tech("tech_solar_efficiency".into()),
        condition: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    // Battery capacity should still be 100 kWh (no SolarOutput effect on battery)
    // With 95 kWh stored, headroom = 5 kWh, charge limited to 5 kW
    assert!(
        (station.power.battery_charge_kw - 5.0).abs() < f32::EPSILON,
        "battery charge should be 5 kW (headroom limited, unaffected by solar tech), got {}",
        station.power.battery_charge_kw
    );
}

// --- PowerConsumption modifier tests ---

#[test]
fn power_consumption_reduced_by_tech_modifier() {
    let content = solar_array_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Add a refinery consuming 10 kW
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("refinery_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    // Add -40% power consumption modifier
    state.modifiers.add(crate::modifiers::Modifier {
        stat: crate::modifiers::StatId::PowerConsumption,
        op: crate::modifiers::ModifierOp::PctAdditive,
        value: -0.4,
        source: crate::modifiers::ModifierSource::Tech("tech_electrolysis_efficiency".into()),
        condition: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    // 10 kW * (1 - 0.4) = 6 kW consumed
    assert!(
        (station.power.consumed_kw - 6.0).abs() < 0.01,
        "consumed_kw should be 6.0 with -40% modifier, got {}",
        station.power.consumed_kw
    );
}

#[test]
fn power_consumption_modifier_prevents_stall() {
    // Without modifier: 15 kW solar, 10 + 8 = 18 kW consumption → deficit → stall
    // With -40% modifier: 15 kW solar, (10 + 8) * 0.6 = 10.8 kW consumption → surplus → no stall
    let content = stall_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("refinery_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });
    station.modules.push(ModuleState {
        id: ModuleInstanceId("sensor_inst_0001".to_string()),
        def_id: "module_sensor_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState::default()),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    // Add -40% power consumption modifier
    state.modifiers.add(crate::modifiers::Modifier {
        stat: crate::modifiers::StatId::PowerConsumption,
        op: crate::modifiers::ModifierOp::PctAdditive,
        value: -0.4,
        source: crate::modifiers::ModifierSource::Tech("tech_electrolysis_efficiency".into()),
        condition: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    // (10 + 8) * 0.6 = 10.8 kW < 15 kW → no deficit, no stall
    assert!(
        station.power.deficit_kw.abs() < f32::EPSILON,
        "should have no deficit with -40% consumption, got {}",
        station.power.deficit_kw
    );
    assert!(
        !station.modules[1].power_stalled,
        "refinery should not be stalled with reduced consumption"
    );
    assert!(
        !station.modules[2].power_stalled,
        "sensor should not be stalled with reduced consumption"
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
        ModuleDefBuilder::new("module_sensor_array")
            .name("Sensor Array")
            .mass(2500.0)
            .volume(6.0)
            .power(8.0)
            .wear(0.003)
            .behavior(ModuleBehaviorDef::SensorArray(SensorArrayDef {
                data_kind: crate::DataKind::SurveyData,
                action_key: "sensor_scan".to_string(),
                scan_interval_minutes: 120,
                scan_interval_ticks: 120,
            }))
            .build(),
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
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
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
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });
    station.modules.push(ModuleState {
        id: ModuleInstanceId("sensor_inst_0001".to_string()),
        def_id: "module_sensor_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState::default()),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

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
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

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
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });
    station.modules.push(ModuleState {
        id: ModuleInstanceId("sensor_inst_0001".to_string()),
        def_id: "module_sensor_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState::default()),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    // Phase 1: tick with deficit — sensor should be stalled
    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        station.modules[2].power_stalled,
        "sensor should be stalled during deficit (18 kW > 15 kW)"
    );

    // Phase 2: disable the sensor to restore surplus, then tick again
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules[2].enabled = false;

    tick(&mut state, &[], &content, &mut rng, None);

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
        ModuleDefBuilder::new("module_basic_battery")
            .name("Basic Battery")
            .mass(2000.0)
            .volume(4.0)
            .wear(0.001)
            .behavior(ModuleBehaviorDef::Battery(BatteryDef {
                capacity_kwh: 100.0,
                charge_rate_kw: 20.0,
                discharge_rate_kw: 30.0,
            }))
            .build(),
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
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

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
        ModuleDefBuilder::new("module_power_hungry")
            .name("Power Hungry")
            .mass(1000.0)
            .volume(5.0)
            .power(80.0)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 60,
                recipes: vec![],
            }))
            .build(),
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
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
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
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

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
        ModuleDefBuilder::new("module_power_hungry")
            .name("Power Hungry")
            .mass(1000.0)
            .volume(5.0)
            .power(80.0)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 60,
                recipes: vec![],
            }))
            .build(),
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
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
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
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

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
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

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
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    // Run multiple ticks to charge up
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, None);
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
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    assert!(
        !station.modules[1].power_stalled,
        "battery should never be power_stalled"
    );
}

#[test]
fn battery_capacity_doubled_by_tech_modifier() {
    let content = battery_content();
    let mut state = state_with_solar_array(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Battery with 95 kWh stored (base capacity 100 kWh → headroom 5 kWh)
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("battery_inst_0001".to_string()),
        def_id: "module_basic_battery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Battery(BatteryState { charge_kwh: 95.0 }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    // Add +100% battery capacity modifier (2x capacity → 200 kWh)
    state.modifiers.add(crate::modifiers::Modifier {
        stat: crate::modifiers::StatId::BatteryCapacity,
        op: crate::modifiers::ModifierOp::PctAdditive,
        value: 1.0,
        source: crate::modifiers::ModifierSource::Tech("tech_battery_storage".into()),
        condition: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    // With 2x capacity (200 kWh), headroom = 200 - 95 = 105 kWh
    // Charge limited by charge_rate (20 kW), not headroom
    assert!(
        (station.power.battery_charge_kw - 20.0).abs() < f32::EPSILON,
        "with doubled capacity, charge should be rate-limited at 20 kW, got {}",
        station.power.battery_charge_kw
    );
}

#[test]
fn battery_discharge_rate_unchanged_by_capacity_modifier() {
    let mut content = battery_content();
    // High-power consumer: 80 kW demand vs 50 kW solar = 30 kW deficit
    content.module_defs.insert(
        "module_power_hungry".to_string(),
        ModuleDefBuilder::new("module_power_hungry")
            .name("Power Hungry")
            .mass(1000.0)
            .volume(5.0)
            .power(80.0)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 60,
                recipes: vec![],
            }))
            .build(),
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
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
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
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    // Add +100% battery capacity modifier
    state.modifiers.add(crate::modifiers::Modifier {
        stat: crate::modifiers::StatId::BatteryCapacity,
        op: crate::modifiers::ModifierOp::PctAdditive,
        value: 1.0,
        source: crate::modifiers::ModifierSource::Tech("tech_battery_storage".into()),
        condition: None,
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

    let station = state.stations.get(&station_id).unwrap();
    // Deficit = 80 - 50 = 30 kW, discharge rate = 30 kW (unchanged by capacity modifier)
    assert!(
        (station.power.battery_discharge_kw - 30.0).abs() < f32::EPSILON,
        "discharge rate should be 30 kW (unchanged by capacity modifier), got {}",
        station.power.battery_discharge_kw
    );
}

#[test]
fn power_priority_fallback_uses_behavior_type() {
    let def = ModuleDefBuilder::new("test_processor").name("Test").build();
    assert_eq!(def.power_priority(), Some(3), "Processor default is 3");
}

#[test]
fn power_priority_content_override_takes_precedence() {
    let def = ModuleDefBuilder::new("test_processor")
        .name("Test")
        .power_stall_priority(7)
        .build();
    assert_eq!(
        def.power_priority(),
        Some(7),
        "content override should take precedence over behavior default"
    );
}
