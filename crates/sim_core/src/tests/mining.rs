use super::*;

#[test]
fn test_mine_emits_ore_mined_event() {
    let content = test_content();
    let (mut state, asteroid_id) = state_with_asteroid(&content);
    let mut rng = make_rng();

    let cmd = mine_command(&state, &asteroid_id, &content);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let completion_tick = state.meta.tick + 9;
    while state.meta.tick < completion_tick {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }
    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::OreMined { .. })),
        "OreMined event should be emitted when mining completes"
    );
}

#[test]
fn test_mine_adds_ore_to_ship_inventory() {
    let content = test_content();
    let (mut state, asteroid_id) = state_with_asteroid(&content);
    let mut rng = make_rng();

    let ship_id = ShipId("ship_0001".to_string());
    assert!(state.ships[&ship_id].inventory.is_empty());

    let cmd = mine_command(&state, &asteroid_id, &content);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let completion_tick = state.meta.tick + 10;
    while state.meta.tick <= completion_tick {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let inv = &state.ships[&ship_id].inventory;
    assert!(
        !inv.is_empty(),
        "ship inventory should not be empty after mining"
    );
    assert!(
        inv.iter()
            .any(|i| matches!(i, InventoryItem::Ore { kg, .. } if *kg > 0.0)),
        "extracted mass must be positive"
    );
}

#[test]
fn test_mine_reduces_asteroid_mass() {
    let content = test_content();
    let (mut state, asteroid_id) = state_with_asteroid(&content);
    let mut rng = make_rng();

    let original_mass = state.asteroids[&asteroid_id].mass_kg;
    let cmd = mine_command(&state, &asteroid_id, &content);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    let completion_tick = state.meta.tick + 10;
    while state.meta.tick <= completion_tick {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let remaining = state.asteroids.get(&asteroid_id).map_or(0.0, |a| a.mass_kg);
    assert!(
        remaining < original_mass,
        "asteroid mass must decrease after mining"
    );
}

#[test]
fn test_mine_removes_depleted_asteroid() {
    let mut content = test_content();
    content.constants.mining_rate_kg_per_tick = 1_000_000.0;
    let (mut state, asteroid_id) = state_with_asteroid(&content);
    let mut rng = make_rng();

    let cmd = mine_command(&state, &asteroid_id, &content);
    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    for _ in 0..11 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    assert!(
        !state.asteroids.contains_key(&asteroid_id),
        "fully mined asteroid should be removed from state"
    );
}
