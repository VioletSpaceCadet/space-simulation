use super::*;

#[test]
fn test_deposit_moves_inventory_to_station() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_test_0001".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 100.0,
            composition: std::collections::HashMap::from([
                ("Fe".to_string(), 0.7_f32),
                ("Si".to_string(), 0.3_f32),
            ]),
        });

    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station_has_ore = state.stations[&station_id]
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Ore { kg, .. } if *kg == 100.0));
    assert!(station_has_ore, "ore should transfer to station");
}

#[test]
fn test_deposit_clears_ship_inventory() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_test_0001".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 100.0,
            composition: std::collections::HashMap::from([
                ("Fe".to_string(), 0.7_f32),
                ("Si".to_string(), 0.3_f32),
            ]),
        });

    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        state.ships[&ship_id].inventory.is_empty(),
        "ship inventory should be empty after deposit"
    );
}

#[test]
fn test_deposit_emits_ore_deposited_event() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_test_0001".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 50.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::OreDeposited { .. })),
        "OreDeposited event should be emitted"
    );
}

#[test]
fn test_ship_starts_with_empty_inventory() {
    let content = test_content();
    let state = test_state(&content);
    let ship = state.ships.values().next().unwrap();
    assert!(
        ship.inventory.is_empty(),
        "ship inventory should be empty at start"
    );
    assert!(
        (ship.cargo_capacity_m3 - 20.0).abs() < 1e-5,
        "ship capacity should be 20 m³"
    );
}

#[test]
fn test_station_starts_with_empty_inventory() {
    let content = test_content();
    let state = test_state(&content);
    let station = state.stations.values().next().unwrap();
    assert!(
        station.inventory.is_empty(),
        "station inventory should be empty at start"
    );
    assert!(
        (station.cargo_capacity_m3 - 10_000.0).abs() < 1e-5,
        "station capacity should be 10,000 m³"
    );
}

#[test]
fn test_deposit_respects_station_capacity() {
    let mut content = test_content();
    content.constants.station_cargo_capacity_m3 = 0.001;
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.001;

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_cap_test".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 500.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        state.stations[&station_id].inventory.is_empty(),
        "station should not accept ore beyond its capacity"
    );
    assert!(
        !state.ships[&ship_id].inventory.is_empty(),
        "ship should retain ore that did not fit in the station"
    );
    let ship = &state.ships[&ship_id];
    assert!(
        matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Deposit { blocked: true, .. })),
        "ship should stay in blocked Deposit task when station is full"
    );
}

#[test]
fn test_deposit_partial_when_station_partially_full() {
    let mut content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.04;
    content.constants.station_cargo_capacity_m3 = 0.04;

    let ship_id = ShipId("ship_0001".to_string());
    let ship = state.ships.get_mut(&ship_id).unwrap();
    ship.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_a".to_string()),
        asteroid_id: AsteroidId("asteroid_test".to_string()),
        kg: 100.0,
        composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
    });
    ship.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_b".to_string()),
        asteroid_id: AsteroidId("asteroid_test".to_string()),
        kg: 100.0,
        composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
    });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_ore_kg: f32 = state.stations[&station_id]
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Ore { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    assert!(
        (station_ore_kg - 100.0).abs() < 1.0,
        "station should have accepted only the first lot (100 kg), got {station_ore_kg} kg"
    );

    let ship_ore_kg: f32 = state.ships[&ship_id]
        .inventory
        .iter()
        .filter_map(|i| {
            if let InventoryItem::Ore { kg, .. } = i {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();
    assert!(
        (ship_ore_kg - 100.0).abs() < 1.0,
        "ship should retain the second lot (100 kg) that didn't fit, got {ship_ore_kg} kg"
    );
}

#[test]
fn test_deposit_ship_waits_when_station_full() {
    let content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.001;

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_block_test".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 500.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let ship = &state.ships[&ship_id];
    assert!(
        matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Deposit { .. })),
        "ship should stay in Deposit task when station is full"
    );
    assert!(
        !ship.inventory.is_empty(),
        "ship should retain ore when station is full"
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::DepositBlocked { .. })),
        "DepositBlocked event should be emitted"
    );
}

#[test]
fn test_deposit_unblocks_when_space_opens() {
    let content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.001;

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_unblock_test".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 100.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 10_000.0;
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        state.ships[&ship_id].inventory.is_empty(),
        "ship should have deposited ore after space opened"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::DepositUnblocked { .. })),
        "DepositUnblocked event should be emitted"
    );
}

#[test]
fn test_deposit_blocked_event_only_emitted_once() {
    let content = test_content();
    let mut state = test_state(&content);

    let station_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .cargo_capacity_m3 = 0.001;

    let ship_id = ShipId("ship_0001".to_string());
    state
        .ships
        .get_mut(&ship_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_dedup_test".to_string()),
            asteroid_id: AsteroidId("asteroid_test".to_string()),
            kg: 500.0,
            composition: std::collections::HashMap::from([("Fe".to_string(), 1.0_f32)]),
        });

    let mut rng = make_rng();
    let cmd = deposit_command(&state);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let events1 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    let events2 = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let count1 = events1
        .iter()
        .filter(|e| matches!(e.event, Event::DepositBlocked { .. }))
        .count();
    let count2 = events2
        .iter()
        .filter(|e| matches!(e.event, Event::DepositBlocked { .. }))
        .count();
    assert_eq!(count1, 1, "first tick should emit DepositBlocked");
    assert_eq!(count2, 0, "second tick should NOT re-emit DepositBlocked");
}
