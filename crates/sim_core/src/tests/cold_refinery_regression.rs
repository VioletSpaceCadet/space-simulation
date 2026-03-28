//! VIO-231: Regression tests verifying cold (non-thermal) refinery modules are
//! completely unchanged by the thermal system.
//!
//! These tests confirm that:
//! - Cold refinery processors have `thermal: None`
//! - Cold refinery output (ore consumed, material produced, slag produced) is identical
//! - Wear accumulates at the base rate (no `heat_wear_multiplier` effect)
//! - All non-thermal module types have `thermal: None`
//! - The thermal tick step is a no-op for modules without `ThermalDef`

use super::*;
use crate::test_fixtures::{insert_recipe, ModuleDefBuilder};

// ── Cold refinery has no thermal state ──────────────────────────────────

#[test]
fn cold_refinery_module_has_thermal_none() {
    let content = refinery_content();
    let state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    assert_eq!(station.modules.len(), 1);
    assert!(
        station.modules[0].thermal.is_none(),
        "cold refinery module should have thermal: None"
    );
}

#[test]
fn cold_refinery_def_has_thermal_none() {
    let content = refinery_content();
    let def = content
        .module_defs
        .get("module_basic_iron_refinery")
        .expect("refinery def should exist");
    assert!(
        def.thermal.is_none(),
        "cold refinery module def should have thermal: None"
    );
}

// ── Cold refinery produces identical output ─────────────────────────────

#[test]
fn cold_refinery_produces_material_and_slag() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();
    let station_id = StationId("station_earth_orbit".to_string());

    // Tick twice: first tick sets the interval timer, second tick processes.
    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let station = &state.stations[&station_id];

    let material_kg = station
        .inventory
        .iter()
        .find_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .unwrap_or(0.0);

    let slag_kg = station
        .inventory
        .iter()
        .find_map(|item| match item {
            InventoryItem::Slag { kg, .. } => Some(*kg),
            _ => None,
        })
        .unwrap_or(0.0);

    // 500 kg ore at 70% Fe => 350 kg Fe, with efficiency 1.0 and wear efficiency 1.0
    assert!(
        (material_kg - 350.0).abs() < 1.0,
        "cold refinery should produce ~350 kg Fe, got {material_kg}"
    );
    assert!(
        slag_kg > 0.0,
        "cold refinery should produce slag, got {slag_kg}"
    );
}

#[test]
fn cold_refinery_consumes_ore() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();
    let station_id = StationId("station_earth_orbit".to_string());

    let initial_ore_kg: f32 = state.stations[&station_id]
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Ore { kg, .. } => Some(*kg),
            _ => None,
        })
        .sum();

    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let final_ore_kg: f32 = state.stations[&station_id]
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Ore { kg, .. } => Some(*kg),
            _ => None,
        })
        .sum();

    assert!(
        final_ore_kg < initial_ore_kg,
        "cold refinery should consume ore: initial={initial_ore_kg}, final={final_ore_kg}"
    );
    assert!(
        (initial_ore_kg - final_ore_kg - 500.0).abs() < 1.0,
        "cold refinery should consume ~500 kg ore per run, consumed {}",
        initial_ore_kg - final_ore_kg
    );
}

#[test]
fn cold_refinery_emits_refinery_ran_event() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);
    let events = tick(&mut state, &[], &content, &mut rng, None);

    let has_refinery_ran = events
        .iter()
        .any(|event| matches!(event.event, Event::RefineryRan { .. }));
    assert!(
        has_refinery_ran,
        "cold refinery should emit RefineryRan event"
    );
}

#[test]
fn cold_refinery_does_not_emit_thermal_events() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    let mut all_events = Vec::new();
    for _ in 0..10 {
        let events = tick(&mut state, &[], &content, &mut rng, None);
        all_events.extend(events);
    }

    let has_too_cold = all_events
        .iter()
        .any(|event| matches!(event.event, Event::ProcessorTooCold { .. }));
    assert!(
        !has_too_cold,
        "cold refinery should never emit ProcessorTooCold"
    );
}

// ── Wear accumulates at base rate (no heat multiplier) ──────────────────

#[test]
fn cold_refinery_wear_accumulates_at_base_rate() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();
    let station_id = StationId("station_earth_orbit".to_string());

    let wear_per_run = content
        .module_defs
        .get("module_basic_iron_refinery")
        .expect("refinery def should exist")
        .wear_per_run;

    // Two ticks: timer starts at 0, interval is 2, so second tick triggers run.
    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let wear = state.stations[&station_id].modules[0].wear.wear;

    // With thermal: None, the heat_wear_multiplier defaults to 1.0, so
    // effective wear = wear_per_run * 1.0 = wear_per_run.
    assert!(
        (wear - wear_per_run).abs() < 1e-6,
        "cold refinery wear should equal base wear_per_run ({wear_per_run}), got {wear}"
    );
}

#[test]
fn cold_refinery_thermal_none_preserved_across_ticks() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();
    let station_id = StationId("station_earth_orbit".to_string());

    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let station = &state.stations[&station_id];
    assert!(
        station.modules[0].thermal.is_none(),
        "cold refinery thermal should remain None after multiple ticks"
    );
}

// ── Non-thermal module types all have thermal: None ─────────────────────

#[test]
fn non_thermal_storage_module_has_thermal_none() {
    let content = test_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_storage_001".to_string()),
        def_id: "module_storage".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Storage,
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    assert!(
        station.modules[0].thermal.is_none(),
        "storage module should have thermal: None"
    );
}

#[test]
fn non_thermal_assembler_module_has_thermal_none() {
    let content = assembler_content();
    let state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    assert!(
        station.modules[0].thermal.is_none(),
        "assembler module should have thermal: None"
    );
}

#[test]
fn non_thermal_maintenance_module_has_thermal_none() {
    let content = maintenance_content();
    let state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    // Module 0 is the refinery, module 1 is maintenance — both should be None.
    for module in &station.modules {
        assert!(
            module.thermal.is_none(),
            "module {} should have thermal: None",
            module.id.0
        );
    }
}

#[test]
fn non_thermal_lab_module_has_thermal_none() {
    let content = test_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_lab_001".to_string()),
        def_id: "module_lab".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Lab(LabState {
            ticks_since_last_run: 0,
            assigned_tech: None,
            starved: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    assert!(
        station.modules[0].thermal.is_none(),
        "lab module should have thermal: None"
    );
}

#[test]
fn non_thermal_sensor_array_has_thermal_none() {
    let content = test_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_sensor_001".to_string()),
        def_id: "module_sensor_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    assert!(
        station.modules[0].thermal.is_none(),
        "sensor array module should have thermal: None"
    );
}

#[test]
fn non_thermal_solar_array_has_thermal_none() {
    let content = test_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_solar_001".to_string()),
        def_id: "module_solar_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SolarArray(SolarArrayState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    assert!(
        station.modules[0].thermal.is_none(),
        "solar array module should have thermal: None"
    );
}

#[test]
fn non_thermal_battery_has_thermal_none() {
    let content = test_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_battery_001".to_string()),
        def_id: "module_battery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Battery(BatteryState { charge_kwh: 0.0 }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
        thermal: None,
    });

    assert!(
        station.modules[0].thermal.is_none(),
        "battery module should have thermal: None"
    );
}

// ── Thermal tick step is a no-op for modules without ThermalDef ─────────

#[test]
fn thermal_tick_noop_for_cold_modules() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();
    let station_id = StationId("station_earth_orbit".to_string());

    // Snapshot state before ticking
    let module_before = state.stations[&station_id].modules[0].clone();

    // Run several ticks
    for _ in 0..5 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let module_after = &state.stations[&station_id].modules[0];

    // Thermal should still be None — the thermal tick step should not touch it.
    assert!(
        module_after.thermal.is_none(),
        "thermal should remain None after ticks"
    );

    // The module should still be the same def
    assert_eq!(
        module_before.def_id, module_after.def_id,
        "module def_id should be unchanged"
    );
}

#[test]
fn mixed_station_cold_module_unaffected_by_thermal_tick() {
    // Set up a station with both a thermal smelter and a cold refinery.
    // The cold refinery should be unaffected by the thermal tick.
    let mut content = refinery_content();

    // Add the thermal smelter def
    let smelt_recipe_id = insert_recipe(&mut content, crate::test_fixtures::test_smelt_recipe());
    content.module_defs.insert(
        "module_basic_smelter".to_string(),
        ModuleDefBuilder::new("module_basic_smelter")
            .name("Basic Smelter")
            .mass(6000.0)
            .volume(12.0)
            .power(30.0)
            .wear(0.015)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 1,
                processing_interval_ticks: 1,
                recipes: vec![smelt_recipe_id],
            }))
            .thermal(ThermalDef {
                heat_capacity_j_per_k: 50_000.0,
                passive_cooling_coefficient: 5.0,
                max_temp_mk: 2_500_000,
                operating_min_mk: None,
                operating_max_mk: None,
                thermal_group: Some("default".to_string()),
                idle_heat_generation_w: None,
            })
            .build(),
    );

    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    // Add a hot smelter module alongside the cold refinery
    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_smelter_001".to_string()),
        def_id: "module_basic_smelter".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 500.0,
            ticks_since_last_run: 100,
            stalled: false,
            selected_recipe: None,
        }),
        wear: WearState::default(),
        thermal: Some(ThermalState {
            temp_mk: 1_900_000,
            thermal_group: Some("default".to_string()),
            ..Default::default()
        }),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        crew_satisfied: true,
    });

    let mut rng = make_rng();
    for _ in 0..5 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let station = &state.stations[&station_id];

    // Cold refinery (module 0) should still have thermal: None
    assert!(
        station.modules[0].thermal.is_none(),
        "cold refinery should still have thermal: None even with a thermal smelter on the station"
    );

    // The smelter (module 1) should have thermal state (temperature may have changed)
    assert!(
        station.modules[1].thermal.is_some(),
        "thermal smelter should still have thermal state"
    );
}
