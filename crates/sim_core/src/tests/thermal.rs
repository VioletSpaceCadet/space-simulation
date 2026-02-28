use super::*;

#[test]
fn thermal_state_none_round_trip() {
    let module = ModuleState {
        id: ModuleInstanceId("mod_test".to_string()),
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
    };

    let json = serde_json::to_string(&module).unwrap();
    let deserialized: ModuleState = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.thermal, None);
}

#[test]
fn thermal_state_some_round_trip() {
    let thermal = ThermalState {
        temp_mk: 1_800_000,
        thermal_group: Some("smelting".to_string()),
    };
    let module = ModuleState {
        id: ModuleInstanceId("mod_test".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: Some(thermal.clone()),
    };

    let json = serde_json::to_string(&module).unwrap();
    let deserialized: ModuleState = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.thermal.as_ref().unwrap().temp_mk, 1_800_000);
    assert_eq!(
        deserialized.thermal.as_ref().unwrap().thermal_group,
        Some("smelting".to_string())
    );
}

#[test]
fn thermal_state_backward_compat_missing_field() {
    // Simulate an old save file that has no "thermal" field
    let json = r#"{
        "id": "mod_test",
        "def_id": "module_basic_iron_refinery",
        "enabled": true,
        "kind_state": {"Processor": {"threshold_kg": 0.0, "ticks_since_last_run": 0, "stalled": false}},
        "wear": {"wear": 0.0}
    }"#;

    let deserialized: ModuleState = serde_json::from_str(json).unwrap();
    assert_eq!(deserialized.thermal, None);
}

#[test]
fn thermal_state_default_values() {
    let thermal = ThermalState::default();
    assert_eq!(thermal.temp_mk, 293_000);
    assert_eq!(thermal.thermal_group, None);
}

// --- MaterialThermalProps tests ---

#[test]
fn material_thermal_none_round_trip() {
    let item = InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 100.0,
        quality: 0.9,
        thermal: None,
    };

    let json = serde_json::to_string(&item).unwrap();
    let deserialized: InventoryItem = serde_json::from_str(&json).unwrap();
    if let InventoryItem::Material { thermal, .. } = deserialized {
        assert_eq!(thermal, None);
    } else {
        panic!("expected Material variant");
    }
}

#[test]
fn material_thermal_some_round_trip() {
    let props = MaterialThermalProps {
        temp_mk: 1_800_000,
        phase: Phase::Liquid,
        latent_heat_buffer_j: 5_000,
    };
    let item = InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 100.0,
        quality: 0.9,
        thermal: Some(props),
    };

    let json = serde_json::to_string(&item).unwrap();
    let deserialized: InventoryItem = serde_json::from_str(&json).unwrap();
    if let InventoryItem::Material { thermal, .. } = deserialized {
        let t = thermal.unwrap();
        assert_eq!(t.temp_mk, 1_800_000);
        assert_eq!(t.phase, Phase::Liquid);
        assert_eq!(t.latent_heat_buffer_j, 5_000);
    } else {
        panic!("expected Material variant");
    }
}

#[test]
fn thermal_constants_in_test_fixtures() {
    let content = crate::test_fixtures::base_content();
    assert_eq!(content.constants.thermal_sink_temp_mk, 293_000);
    assert_eq!(
        content.constants.thermal_overheat_warning_offset_mk,
        200_000
    );
    assert_eq!(
        content.constants.thermal_overheat_critical_offset_mk,
        500_000
    );
    assert!((content.constants.thermal_wear_multiplier_warning - 2.0).abs() < f32::EPSILON);
    assert!((content.constants.thermal_wear_multiplier_critical - 4.0).abs() < f32::EPSILON);
}

#[test]
fn thermal_constants_deserialize_from_json() {
    let json = r#"{
        "survey_scan_minutes": 1,
        "deep_scan_minutes": 1,
        "travel_minutes_per_hop": 1,
        "survey_tag_detection_probability": 1.0,
        "asteroid_count_per_template": 1,
        "asteroid_mass_min_kg": 500.0,
        "asteroid_mass_max_kg": 500.0,
        "ship_cargo_capacity_m3": 20.0,
        "station_cargo_capacity_m3": 10000.0,
        "station_power_available_per_minute": 100.0,
        "mining_rate_kg_per_minute": 50.0,
        "deposit_minutes": 1,
        "autopilot_iron_rich_confidence_threshold": 0.7,
        "autopilot_refinery_threshold_kg": 500.0,
        "research_roll_interval_minutes": 60,
        "data_generation_peak": 100.0,
        "data_generation_floor": 5.0,
        "data_generation_decay_rate": 0.7,
        "autopilot_slag_jettison_pct": 0.75,
        "wear_band_degraded_threshold": 0.5,
        "wear_band_critical_threshold": 0.8,
        "wear_band_degraded_efficiency": 0.75,
        "wear_band_critical_efficiency": 0.5,
        "minutes_per_tick": 1,
        "thermal_sink_temp_mk": 293000,
        "thermal_overheat_warning_offset_mk": 200000,
        "thermal_overheat_critical_offset_mk": 500000,
        "thermal_wear_multiplier_warning": 2.0,
        "thermal_wear_multiplier_critical": 4.0
    }"#;

    let constants: Constants = serde_json::from_str(json).unwrap();
    assert_eq!(constants.thermal_sink_temp_mk, 293_000);
    assert_eq!(constants.thermal_overheat_warning_offset_mk, 200_000);
    assert_eq!(constants.thermal_overheat_critical_offset_mk, 500_000);
    assert!((constants.thermal_wear_multiplier_warning - 2.0).abs() < f32::EPSILON);
    assert!((constants.thermal_wear_multiplier_critical - 4.0).abs() < f32::EPSILON);
}

#[test]
fn thermal_constants_default_when_missing_from_json() {
    // Simulate old constants.json without thermal fields
    let json = r#"{
        "survey_scan_minutes": 1,
        "deep_scan_minutes": 1,
        "travel_minutes_per_hop": 1,
        "survey_tag_detection_probability": 1.0,
        "asteroid_count_per_template": 1,
        "asteroid_mass_min_kg": 500.0,
        "asteroid_mass_max_kg": 500.0,
        "ship_cargo_capacity_m3": 20.0,
        "station_cargo_capacity_m3": 10000.0,
        "station_power_available_per_minute": 100.0,
        "mining_rate_kg_per_minute": 50.0,
        "deposit_minutes": 1,
        "autopilot_iron_rich_confidence_threshold": 0.7,
        "autopilot_refinery_threshold_kg": 500.0,
        "research_roll_interval_minutes": 60,
        "data_generation_peak": 100.0,
        "data_generation_floor": 5.0,
        "data_generation_decay_rate": 0.7,
        "wear_band_degraded_threshold": 0.5,
        "wear_band_critical_threshold": 0.8,
        "wear_band_degraded_efficiency": 0.75,
        "wear_band_critical_efficiency": 0.5,
        "minutes_per_tick": 1
    }"#;

    let constants: Constants = serde_json::from_str(json).unwrap();
    assert_eq!(constants.thermal_sink_temp_mk, 293_000);
    assert_eq!(constants.thermal_overheat_warning_offset_mk, 200_000);
    assert_eq!(constants.thermal_overheat_critical_offset_mk, 500_000);
    assert!((constants.thermal_wear_multiplier_warning - 2.0).abs() < f32::EPSILON);
    assert!((constants.thermal_wear_multiplier_critical - 4.0).abs() < f32::EPSILON);
}

#[test]
fn material_thermal_backward_compat_missing_field() {
    // Old save file: Material without "thermal" field (internally tagged with "kind")
    let json = r#"{"kind":"Material","element":"Fe","kg":100.0,"quality":0.9}"#;

    let deserialized: InventoryItem = serde_json::from_str(json).unwrap();
    if let InventoryItem::Material { thermal, .. } = deserialized {
        assert_eq!(thermal, None);
    } else {
        panic!("expected Material variant");
    }
}

// --- ThermalDef on ModuleDef tests ---

#[test]
fn thermal_def_round_trip() {
    let thermal_def = ThermalDef {
        heat_capacity_j_per_k: 500.0,
        passive_cooling_coefficient: 0.05,
        max_temp_mk: 2_500_000,
        operating_min_mk: Some(1_000_000),
        operating_max_mk: Some(2_000_000),
        thermal_group: Some("smelting".to_string()),
    };

    let json = serde_json::to_string(&thermal_def).unwrap();
    let deserialized: ThermalDef = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, thermal_def);
}

#[test]
fn module_def_thermal_none_backward_compat() {
    // Old module_defs.json entry without "thermal" field
    let json = r#"{
        "id": "mod_test",
        "name": "Test Module",
        "mass_kg": 1000.0,
        "volume_m3": 5.0,
        "power_consumption_per_run": 10.0,
        "wear_per_run": 0.01,
        "behavior": {"Storage": {"capacity_m3": 100.0}}
    }"#;

    let module_def: ModuleDef = serde_json::from_str(json).unwrap();
    assert_eq!(module_def.thermal, None);
}

#[test]
fn module_def_with_thermal_parses() {
    let json = r#"{
        "id": "mod_smelter",
        "name": "Iron Smelter",
        "mass_kg": 5000.0,
        "volume_m3": 10.0,
        "power_consumption_per_run": 100.0,
        "wear_per_run": 0.02,
        "behavior": {"Storage": {"capacity_m3": 50.0}},
        "thermal": {
            "heat_capacity_j_per_k": 500.0,
            "passive_cooling_coefficient": 0.05,
            "max_temp_mk": 2500000,
            "operating_min_mk": 1000000,
            "operating_max_mk": 2000000,
            "thermal_group": "smelting"
        }
    }"#;

    let module_def: ModuleDef = serde_json::from_str(json).unwrap();
    let thermal = module_def.thermal.unwrap();
    assert!((thermal.heat_capacity_j_per_k - 500.0).abs() < f32::EPSILON);
    assert!((thermal.passive_cooling_coefficient - 0.05).abs() < f32::EPSILON);
    assert_eq!(thermal.max_temp_mk, 2_500_000);
    assert_eq!(thermal.operating_min_mk, Some(1_000_000));
    assert_eq!(thermal.operating_max_mk, Some(2_000_000));
    assert_eq!(thermal.thermal_group, Some("smelting".to_string()));
}

// --- Element thermal properties tests ---

#[test]
fn element_fe_has_thermal_props() {
    let content = crate::test_fixtures::base_content();
    let fe = content.elements.iter().find(|e| e.id == "Fe").unwrap();
    assert_eq!(fe.melting_point_mk, Some(1_811_000));
    assert_eq!(fe.latent_heat_j_per_kg, Some(247_000));
    assert_eq!(fe.specific_heat_j_per_kg_k, Some(449));
}

#[test]
fn element_ore_has_no_thermal_props() {
    let content = crate::test_fixtures::base_content();
    let ore = content.elements.iter().find(|e| e.id == "ore").unwrap();
    assert_eq!(ore.melting_point_mk, None);
    assert_eq!(ore.latent_heat_j_per_kg, None);
    assert_eq!(ore.specific_heat_j_per_kg_k, None);
}
