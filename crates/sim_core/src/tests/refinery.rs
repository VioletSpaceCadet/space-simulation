use super::*;

#[test]
fn test_refinery_produces_material_and_slag() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let has_material = station.inventory.iter().any(|i| {
        matches!(i, InventoryItem::Material { element, kg, .. } if element == "Fe" && *kg > 0.0)
    });
    assert!(
        has_material,
        "station should have Fe Material after refinery runs"
    );

    let has_slag = station
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Slag { kg, .. } if *kg > 0.0));
    assert!(has_slag, "station should have Slag after refinery runs");
}

#[test]
fn test_refinery_quality_equals_fe_fraction() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let quality = station.inventory.iter().find_map(|i| {
        if let InventoryItem::Material {
            element, quality, ..
        } = i
        {
            if element == "Fe" {
                Some(*quality)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(quality.is_some(), "Fe Material should exist");
    assert!(
        (quality.unwrap() - 0.7).abs() < 1e-4,
        "quality should equal Fe fraction (0.7) with multiplier 1.0"
    );
}

#[test]
fn test_refinery_skips_when_below_threshold() {
    let content = refinery_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 9999.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
    });
    station.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_0001".to_string()),
        asteroid_id: AsteroidId("asteroid_0001".to_string()),
        kg: 1000.0,
        composition: std::collections::HashMap::from([
            ("Fe".to_string(), 0.7f32),
            ("Si".to_string(), 0.3f32),
        ]),
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    assert!(
        !station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Material { .. })),
        "refinery should not run when ore is below threshold"
    );
}

#[test]
fn test_refinery_emits_refinery_ran_event() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::RefineryRan { .. })),
        "RefineryRan event should be emitted when refinery processes ore"
    );
}

#[test]
fn test_refinery_stalls_when_station_full() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.34;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(ps.stalled, "module should be stalled when output won't fit");
        assert_eq!(ps.ticks_since_last_run, 0, "timer should reset on stall");
    } else {
        panic!("expected processor state");
    }

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::ModuleStalled { .. })),
        "ModuleStalled event should be emitted"
    );

    assert!(
        !station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Material { .. })),
        "no material should be produced when stalled"
    );
    assert!(
        !station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Slag { .. })),
        "no slag should be produced when stalled"
    );
}

#[test]
fn test_refinery_resumes_after_stall_cleared() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.34;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    if let ModuleKindState::Processor(ps) = &state.stations[&station_id].modules[0].kind_state {
        assert!(ps.stalled);
    }

    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 10_000.0;

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(!ps.stalled, "module should no longer be stalled");
    }

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::ModuleResumed { .. })),
        "ModuleResumed event should be emitted"
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::RefineryRan { .. })),
        "RefineryRan should fire after resuming"
    );
}

#[test]
fn test_stall_event_only_emitted_once() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.34;

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events1 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let stall_count_1 = events1
        .iter()
        .filter(|e| matches!(e.event, Event::ModuleStalled { .. }))
        .count();
    assert_eq!(stall_count_1, 1);

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events2 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let stall_count_2 = events2
        .iter()
        .filter(|e| matches!(e.event, Event::ModuleStalled { .. }))
        .count();
    assert_eq!(
        stall_count_2, 0,
        "ModuleStalled should not be re-emitted while already stalled"
    );
}

#[test]
fn test_storage_pressure_cascade() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.50;

    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events_run1 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events_run1
            .iter()
            .any(|e| matches!(e.event, Event::RefineryRan { .. })),
        "first refinery run should succeed"
    );

    let station = &state.stations[&station_id];
    assert!(
        station
            .inventory
            .iter()
            .any(|i| matches!(i, InventoryItem::Material { .. })),
        "material should exist after first run"
    );

    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 1200.0,
            quality: 0.5,
        });

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events_run2 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        !events_run2
            .iter()
            .any(|e| matches!(e.event, Event::RefineryRan { .. })),
        "second refinery run should NOT happen (stalled)"
    );
    assert!(
        events_run2
            .iter()
            .any(|e| matches!(e.event, Event::ModuleStalled { .. })),
        "ModuleStalled should be emitted"
    );

    let station = &state.stations[&station_id];
    if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
        assert!(ps.stalled, "module should be stalled");
    }
}
