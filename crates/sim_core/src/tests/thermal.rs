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
