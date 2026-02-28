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
