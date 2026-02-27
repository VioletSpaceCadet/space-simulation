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
