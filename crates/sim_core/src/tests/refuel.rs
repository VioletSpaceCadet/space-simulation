use crate::test_fixtures::{base_content, base_state};
use crate::{Event, GameContent, InventoryItem, ShipId, StationId, TaskKind, TaskState};

fn content_with_refuel() -> GameContent {
    let mut content = base_content();
    content.constants.refuel_kg_per_minute = 100.0; // 100 kg/min
    content.constants.minutes_per_tick = 1; // 1 min/tick → 100 kg/tick
    content.constants.derive_tick_values();
    content
}

fn setup_refueling_ship(
    content: &GameContent,
    propellant_kg: f32,
    target_kg: f32,
    station_lh2_kg: f32,
) -> crate::GameState {
    let mut state = base_state(content);
    let ship_id = ShipId("ship_0001".to_string());
    let station_id = StationId("station_earth_orbit".to_string());

    // Set ship propellant and assign refuel task
    let ship = state.ships.get_mut(&ship_id).unwrap();
    ship.propellant_kg = propellant_kg;
    ship.propellant_capacity_kg = target_kg;
    ship.task = Some(TaskState {
        kind: TaskKind::Refuel {
            station_id: station_id.clone(),
            target_kg,
        },
        started_tick: 0,
        eta_tick: 0, // ongoing — eta not used
    });

    // Add LH2 to station inventory
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Material {
        element: "LH2".to_string(),
        kg: station_lh2_kg,
        quality: 1.0,
        thermal: None,
    });

    state
}

#[test]
fn refuel_transfers_lh2_per_tick() {
    let content = content_with_refuel();
    let mut state = setup_refueling_ship(&content, 0.0, 1000.0, 5000.0);
    let mut events = Vec::new();

    crate::tasks::resolve_refuels(&mut state, &content, &mut events);

    let ship = state.ships.get(&ShipId("ship_0001".to_string())).unwrap();
    // Should have received refuel_kg_per_tick = 100 kg
    assert!((ship.propellant_kg - 100.0).abs() < 1.0);
    // Still refueling (not at target 1000)
    assert!(ship.task.is_some());
}

#[test]
fn refuel_completes_when_target_reached() {
    let content = content_with_refuel();
    // Ship at 950 kg, target 1000 kg — will complete in 1 tick (needs 50, rate is 100)
    let mut state = setup_refueling_ship(&content, 950.0, 1000.0, 5000.0);
    let mut events = Vec::new();

    crate::tasks::resolve_refuels(&mut state, &content, &mut events);

    let ship = state.ships.get(&ShipId("ship_0001".to_string())).unwrap();
    // Ship should be at target and task cleared
    assert!(ship.propellant_kg >= 1000.0);
    assert!(ship.task.is_none());
    // RefuelComplete event emitted
    assert!(events
        .iter()
        .any(|e| matches!(&e.event, Event::RefuelComplete { .. })));
}

#[test]
fn refuel_aborts_when_station_empty() {
    let content = content_with_refuel();
    // Station has no LH2
    let mut state = setup_refueling_ship(&content, 100.0, 1000.0, 0.0);
    let mut events = Vec::new();

    crate::tasks::resolve_refuels(&mut state, &content, &mut events);

    let ship = state.ships.get(&ShipId("ship_0001".to_string())).unwrap();
    // Task should be cleared
    assert!(ship.task.is_none());
    // RefuelAborted event emitted
    assert!(events.iter().any(
        |e| matches!(&e.event, Event::RefuelAborted { reason, .. } if reason == "station_empty")
    ));
}

#[test]
fn refuel_rate_limited() {
    let content = content_with_refuel();
    // Ship needs 500 kg but rate is 100 kg/tick
    let mut state = setup_refueling_ship(&content, 500.0, 1000.0, 5000.0);
    let mut events = Vec::new();

    crate::tasks::resolve_refuels(&mut state, &content, &mut events);

    let ship = state.ships.get(&ShipId("ship_0001".to_string())).unwrap();
    // Should get exactly rate (100 kg), not full 500 needed
    assert!((ship.propellant_kg - 600.0).abs() < 1.0);
    // Still refueling
    assert!(ship.task.is_some());
}

#[test]
fn refuel_station_lh2_depleted() {
    let content = content_with_refuel();
    // Station has only 30 kg LH2, ship needs 100/tick
    let mut state = setup_refueling_ship(&content, 0.0, 1000.0, 30.0);
    let mut events = Vec::new();

    crate::tasks::resolve_refuels(&mut state, &content, &mut events);

    let ship = state.ships.get(&ShipId("ship_0001".to_string())).unwrap();
    // Should get only 30 kg (all available)
    assert!((ship.propellant_kg - 30.0).abs() < 1.0);
    // Station LH2 should be depleted
    let station = state
        .stations
        .get(&StationId("station_earth_orbit".to_string()))
        .unwrap();
    let station_lh2: f32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(station_lh2 < 1.0);
}
