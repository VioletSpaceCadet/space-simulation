use super::*;

#[test]
fn test_assembler_produces_component() {
    let content = assembler_content();
    let mut state = state_with_assembler(&content);
    let mut rng = make_rng();

    // Tick past the interval (2 ticks)
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let has_repair_kit = station.inventory.iter().any(|i| {
        matches!(i, InventoryItem::Component { component_id, count, .. }
            if component_id.0 == "repair_kit" && *count >= 1)
    });
    assert!(
        has_repair_kit,
        "station should have RepairKit after assembler runs"
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::AssemblerRan { .. })),
        "AssemblerRan event should be emitted"
    );

    // Verify Fe was consumed
    let fe_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| {
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
        .sum();
    assert!(
        (fe_kg - 400.0).abs() < 1.0,
        "100 kg Fe should be consumed, got {fe_kg} remaining (expected ~400)"
    );
}

#[test]
fn test_assembler_skips_insufficient_material() {
    let content = assembler_content();
    let mut state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Replace 500kg Fe with only 50kg
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .retain(|i| !matches!(i, InventoryItem::Material { .. }));
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 50.0,
            quality: 0.7,
        });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        !events
            .iter()
            .any(|e| matches!(e.event, Event::AssemblerRan { .. })),
        "assembler should not run with insufficient material"
    );

    let station = &state.stations[&station_id];
    assert!(
        !station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Component { component_id, .. } if component_id.0 == "repair_kit")),
        "no RepairKit should be produced"
    );
}

#[test]
fn test_assembler_stalls_on_capacity() {
    let content = assembler_content();
    let mut state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Set very small cargo capacity so output won't fit
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.001;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    if let ModuleKindState::Assembler(asmb) = &station.modules[0].kind_state {
        assert!(
            asmb.stalled,
            "module should be stalled when output won't fit"
        );
    } else {
        panic!("expected assembler state");
    }

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::ModuleStalled { .. })),
        "ModuleStalled event should be emitted"
    );
}

#[test]
fn test_assembler_accumulates_wear() {
    let content = assembler_content();
    let mut state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let wear = state.stations[&station_id].modules[0].wear.wear;
    assert!(
        (wear - 0.008).abs() < 1e-5,
        "wear should be 0.008 after one run, got {wear}"
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::WearAccumulated { .. })),
        "WearAccumulated event should be emitted"
    );
}

#[test]
fn test_assembler_auto_disables_at_max_wear() {
    let content = assembler_content();
    let mut state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Set wear close to max
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
fn test_assembler_skips_when_disabled() {
    let content = assembler_content();
    let mut state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state.stations.get_mut(&station_id).unwrap().modules[0].enabled = false;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        !events
            .iter()
            .any(|e| matches!(e.event, Event::AssemblerRan { .. })),
        "disabled assembler should not run"
    );
}

#[test]
fn test_assembler_merges_component_stacks() {
    let content = assembler_content();
    let mut state = state_with_assembler(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Pre-seed with existing repair kits
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: 3,
            quality: 1.0,
        });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    let kit_count: u32 = station
        .inventory
        .iter()
        .filter_map(|i| {
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
        .sum();
    assert_eq!(
        kit_count, 4,
        "should have 3 original + 1 produced = 4, got {kit_count}"
    );

    // Should be a single stack, not two
    let kit_stacks: usize = station
        .inventory
        .iter()
        .filter(|i| {
            matches!(i, InventoryItem::Component { component_id, .. } if component_id.0 == "repair_kit")
        })
        .count();
    assert_eq!(
        kit_stacks, 1,
        "repair kits should merge into a single stack"
    );
}
