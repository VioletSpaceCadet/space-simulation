use super::*;

#[test]
fn test_constants_have_wear_fields() {
    let content = test_content();
    assert!((content.constants.wear_band_degraded_threshold - 0.5).abs() < 1e-5);
    assert!((content.constants.wear_band_critical_threshold - 0.8).abs() < 1e-5);
    assert!((content.constants.wear_band_degraded_efficiency - 0.75).abs() < 1e-5);
    assert!((content.constants.wear_band_critical_efficiency - 0.5).abs() < 1e-5);
}

#[test]
fn test_refinery_output_reduced_by_wear() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state.stations.get_mut(&station_id).unwrap().modules[0]
        .wear
        .wear = 0.6;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    let material_kg = station
        .inventory
        .iter()
        .find_map(|i| {
            if let InventoryItem::Material { element, kg, .. } = i {
                if element == "Fe" {
                    Some(*kg)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or(0.0);

    assert!(
        (material_kg - 262.5).abs() < 1.0,
        "degraded module should produce ~262.5 kg Fe, got {material_kg}"
    );
}

#[test]
fn test_refinery_accumulates_wear() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let wear = state.stations[&station_id].modules[0].wear.wear;
    let expected_wear = content
        .module_defs
        .iter()
        .find(|d| d.id == "module_basic_iron_refinery")
        .unwrap()
        .wear_per_run;
    assert!(
        (wear - expected_wear).abs() < 1e-5,
        "wear should be {expected_wear} after one run, got {wear}"
    );
}

#[test]
fn test_refinery_auto_disables_at_max_wear() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state.stations.get_mut(&station_id).unwrap().modules[0]
        .wear
        .wear = 0.995;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    assert!(
        !station.modules[0].enabled,
        "module should be auto-disabled at wear >= 1.0"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::ModuleAutoDisabled { .. })),
        "ModuleAutoDisabled event should be emitted"
    );
}

#[test]
fn test_wear_accumulated_event_emitted() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::WearAccumulated { .. })),
        "WearAccumulated event should be emitted when refinery runs"
    );
}

#[test]
fn test_maintenance_repairs_most_worn_module() {
    let content = maintenance_content();
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state.stations.get_mut(&station_id).unwrap().modules[0]
        .wear
        .wear = 0.6;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    assert!(
        (station.modules[0].wear.wear - 0.4).abs() < 0.1,
        "wear should be reduced by ~0.2, got {}",
        station.modules[0].wear.wear
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::MaintenanceRan { .. })),
        "MaintenanceRan event should be emitted"
    );
}

#[test]
fn test_maintenance_consumes_repair_kit() {
    let content = maintenance_content();
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state.stations.get_mut(&station_id).unwrap().modules[0]
        .wear
        .wear = 0.6;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    let kits = station
        .inventory
        .iter()
        .find_map(|i| {
            if let InventoryItem::Component {
                component_id,
                count,
                ..
            } = i
            {
                if component_id.0 == "repair_kit" {
                    Some(*count)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or(0);
    assert_eq!(
        kits, 4,
        "one repair kit should be consumed, got {kits} remaining"
    );
}

#[test]
fn test_maintenance_skips_when_no_repair_kits() {
    let content = maintenance_content();
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state.stations.get_mut(&station_id).unwrap().modules[0]
        .wear
        .wear = 0.6;
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .retain(|i| {
            !matches!(i, InventoryItem::Component { component_id, .. } if component_id.0 == "repair_kit")
        });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    assert!(
        station.modules[0].wear.wear > 0.6,
        "wear should not decrease without repair kits"
    );
}

#[test]
fn test_maintenance_skips_when_no_worn_modules() {
    let content = maintenance_content();
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .retain(|i| !matches!(i, InventoryItem::Ore { .. }));

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    let kits = station
        .inventory
        .iter()
        .find_map(|i| {
            if let InventoryItem::Component {
                component_id,
                count,
                ..
            } = i
            {
                if component_id.0 == "repair_kit" {
                    Some(*count)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or(0);
    assert_eq!(kits, 5, "no kits should be consumed when nothing is worn");
}

#[test]
fn test_wear_maintenance_full_cycle() {
    let mut content = maintenance_content();
    for def in &mut content.module_defs {
        if def.id == "module_basic_iron_refinery" {
            def.wear_per_run = 0.3;
        }
    }
    let mut state = state_with_maintenance(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let mut rng = make_rng();

    let mut all_events = Vec::new();
    for _ in 0..20 {
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        all_events.extend(events);
    }

    assert!(
        all_events
            .iter()
            .any(|e| matches!(e.event, Event::WearAccumulated { .. })),
        "WearAccumulated event should be emitted during cycle"
    );

    assert!(
        all_events
            .iter()
            .any(|e| matches!(e.event, Event::MaintenanceRan { .. })),
        "MaintenanceRan event should be emitted during cycle"
    );

    let station = &state.stations[&station_id];
    let kits = station
        .inventory
        .iter()
        .find_map(|i| {
            if let InventoryItem::Component {
                component_id,
                count,
                ..
            } = i
            {
                if component_id.0 == "repair_kit" {
                    Some(*count)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or(0);
    assert!(kits < 5, "some repair kits should have been consumed");
}
