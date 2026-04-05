//! VIO-595: Inter-station item transfer tests.
//!
//! Exercises `Command::TransferItems` + `TaskKind::Pickup` + the chained
//! `Transit(src) → Pickup → Transit(dst) → Deposit` task graph.

use super::*;
use crate::{
    test_fixtures, ComponentDef, ComponentId, FacilityCore, ModuleBehaviorDef, ModuleItemId,
    ModuleTypeIndex, PowerBudgetCache, PowerState, ProcessorDef, StationState, TradeItemSpec,
};

// --- Test fixtures ------------------------------------------------------

/// Content with component + module defs needed for Transfer tests.
fn transfer_content() -> GameContent {
    let mut content = test_fixtures::base_content();
    content.component_defs = vec![
        ComponentDef {
            id: "repair_kit".to_string(),
            name: "Repair Kit".to_string(),
            mass_kg: 50.0,
            volume_m3: 0.1,
            deploys_frame: None,
            deploys_seed_materials: vec![],
            deploys_seed_components: vec![],
        },
        ComponentDef {
            id: "thruster".to_string(),
            name: "Thruster".to_string(),
            mass_kg: 200.0,
            volume_m3: 0.5,
            deploys_frame: None,
            deploys_seed_materials: vec![],
            deploys_seed_components: vec![],
        },
    ];
    content.module_defs = [(
        "module_basic_iron_refinery".to_string(),
        test_fixtures::ModuleDefBuilder::new("module_basic_iron_refinery")
            .name("Basic Iron Refinery")
            .mass(1000.0)
            .volume(5.0)
            .power(10.0)
            .wear(0.01)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 10,
                processing_interval_ticks: 10,
                recipes: vec![],
            }))
            .build(),
    )]
    .into_iter()
    .collect();
    content
}

/// Base state with an additional empty "station_mars_orbit" at the same
/// position (so transit time between them is zero — keeps tests fast).
fn two_station_state(content: &GameContent) -> GameState {
    let mut state = test_fixtures::base_state(content);
    let mars_id = StationId("station_mars_orbit".to_string());
    state.stations.insert(
        mars_id.clone(),
        StationState {
            id: mars_id,
            position: test_fixtures::test_position(),
            core: FacilityCore {
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                modules: vec![],
                modifiers: crate::modifiers::ModifierSet::default(),
                crew: Default::default(),
                thermal_links: Vec::new(),
                power: PowerState::default(),
                cached_inventory_volume_m3: None,
                module_type_index: ModuleTypeIndex::default(),
                module_id_index: std::collections::HashMap::new(),
                power_budget_cache: PowerBudgetCache::default(),
            },
            leaders: Vec::new(),
            frame_id: None,
        },
    );
    state
}

fn ship_is_unassigned(state: &GameState, ship_id: &ShipId) -> bool {
    match &state.ships[ship_id].task {
        None => true,
        Some(task) => matches!(task.kind, TaskKind::Idle),
    }
}

fn transfer_command(
    state: &GameState,
    from: &str,
    to: &str,
    items: Vec<TradeItemSpec>,
) -> CommandEnvelope {
    let ship_id = test_ship_id();
    CommandEnvelope {
        id: CommandId(0),
        issued_by: state.ships[&ship_id].owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::TransferItems {
            ship_id,
            from_station: StationId(from.to_string()),
            to_station: StationId(to.to_string()),
            items,
        },
    }
}

// --- Unit: resolve_pickup mechanics -------------------------------------

#[test]
fn pickup_moves_material_from_station_to_ship() {
    let content = transfer_content();
    let mut state = two_station_state(&content);
    let mut rng = make_rng();

    let earth_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&earth_id)
        .unwrap()
        .core
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 100.0,
            quality: 0.9,
            thermal: None,
        });

    let cmd = transfer_command(
        &state,
        "station_earth_orbit",
        "station_mars_orbit",
        vec![TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        }],
    );
    // Transit is 0 ticks (both stations share position) — one apply tick
    // starts Pickup, second completes it.
    tick(&mut state, &[cmd], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    // Earth station should no longer have the Fe material.
    let earth_has_fe = state.stations[&earth_id]
        .core
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Material { element, .. } if element == "Fe"));
    assert!(!earth_has_fe, "Fe should have been picked up from Earth");
}

#[test]
fn full_transfer_moves_material_to_destination() {
    let content = transfer_content();
    let mut state = two_station_state(&content);
    let mut rng = make_rng();

    let earth_id = StationId("station_earth_orbit".to_string());
    let mars_id = StationId("station_mars_orbit".to_string());
    state
        .stations
        .get_mut(&earth_id)
        .unwrap()
        .core
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 200.0,
            quality: 0.9,
            thermal: None,
        });

    let cmd = transfer_command(
        &state,
        "station_earth_orbit",
        "station_mars_orbit",
        vec![TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 200.0,
        }],
    );
    // Drive the chain: Pickup → Transit(0) → Deposit. Each step is a tick.
    for _ in 0..10 {
        tick(&mut state, &[cmd.clone()], &content, &mut rng, None);
        // Only apply the command on tick 0 — subsequent ticks are empty.
        break;
    }
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let mars_fe_kg: f32 = state.stations[&mars_id]
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        (mars_fe_kg - 200.0).abs() < 0.01,
        "expected 200 kg Fe on Mars, got {}",
        mars_fe_kg
    );

    let earth_fe_kg: f32 = state.stations[&earth_id]
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        earth_fe_kg < 0.01,
        "Earth should have 0 kg Fe left, got {}",
        earth_fe_kg
    );
}

#[test]
fn transfer_moves_components_between_stations() {
    let content = transfer_content();
    let mut state = two_station_state(&content);
    let mut rng = make_rng();

    let earth_id = StationId("station_earth_orbit".to_string());
    let mars_id = StationId("station_mars_orbit".to_string());
    state
        .stations
        .get_mut(&earth_id)
        .unwrap()
        .core
        .inventory
        .push(InventoryItem::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: 5,
            quality: 0.9,
        });

    let cmd = transfer_command(
        &state,
        "station_earth_orbit",
        "station_mars_orbit",
        vec![TradeItemSpec::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: 3,
        }],
    );
    tick(&mut state, &[cmd], &content, &mut rng, None);
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let mars_repair_kits: u32 = state.stations[&mars_id]
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == "repair_kit" => Some(*count),
            _ => None,
        })
        .sum();
    assert_eq!(mars_repair_kits, 3, "3 repair kits should arrive at Mars");

    // Earth should keep the remaining 2 repair kits (partial transfer).
    let earth_repair_kits: u32 = state.stations[&earth_id]
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == "repair_kit" => Some(*count),
            _ => None,
        })
        .sum();
    assert_eq!(earth_repair_kits, 2, "2 repair kits should remain on Earth");
}

#[test]
fn transfer_moves_module_between_stations() {
    let content = transfer_content();
    let mut state = two_station_state(&content);
    let mut rng = make_rng();

    let earth_id = StationId("station_earth_orbit".to_string());
    let mars_id = StationId("station_mars_orbit".to_string());
    state
        .stations
        .get_mut(&earth_id)
        .unwrap()
        .core
        .inventory
        .push(InventoryItem::Module {
            item_id: ModuleItemId("module_item_0001".to_string()),
            module_def_id: "module_basic_iron_refinery".to_string(),
        });

    let cmd = transfer_command(
        &state,
        "station_earth_orbit",
        "station_mars_orbit",
        vec![TradeItemSpec::Module {
            module_def_id: "module_basic_iron_refinery".to_string(),
        }],
    );
    tick(&mut state, &[cmd], &content, &mut rng, None);
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let mars_has_refinery = state.stations[&mars_id]
        .core
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Module { module_def_id, .. } if module_def_id == "module_basic_iron_refinery"));
    assert!(mars_has_refinery, "refinery should arrive at Mars");

    let earth_has_refinery = state.stations[&earth_id]
        .core
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Module { .. }));
    assert!(!earth_has_refinery, "refinery should leave Earth");
}

#[test]
fn transfer_respects_ship_cargo_capacity() {
    let content = transfer_content();
    let mut state = two_station_state(&content);
    let mut rng = make_rng();

    // Shrink the ship's cargo hold so only a fraction of the request fits.
    // Fe density is 7874 kg/m^3, cargo is 1.0 m^3 → max ~7874 kg.
    let ship_id = test_ship_id();
    state.ships.get_mut(&ship_id).unwrap().cargo_capacity_m3 = 1.0;

    let earth_id = StationId("station_earth_orbit".to_string());
    let mars_id = StationId("station_mars_orbit".to_string());
    state
        .stations
        .get_mut(&earth_id)
        .unwrap()
        .core
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 20_000.0,
            quality: 0.9,
            thermal: None,
        });

    let cmd = transfer_command(
        &state,
        "station_earth_orbit",
        "station_mars_orbit",
        vec![TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 20_000.0,
        }],
    );
    tick(&mut state, &[cmd], &content, &mut rng, None);
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let mars_fe_kg: f32 = state.stations[&mars_id]
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    // Ship capacity 1.0 m^3 * 7874 kg/m^3 = 7874 kg fits.
    assert!(
        (mars_fe_kg - 7874.0).abs() < 1.0,
        "expected ~7874 kg Fe on Mars (cargo capacity), got {}",
        mars_fe_kg
    );

    let earth_fe_kg: f32 = state.stations[&earth_id]
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        (earth_fe_kg - (20_000.0 - 7874.0)).abs() < 1.0,
        "expected ~12126 kg Fe remaining on Earth, got {}",
        earth_fe_kg
    );
}

#[test]
fn transfer_same_station_is_rejected() {
    let content = transfer_content();
    let mut state = two_station_state(&content);
    let mut rng = make_rng();

    let earth_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&earth_id)
        .unwrap()
        .core
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 100.0,
            quality: 0.9,
            thermal: None,
        });

    let cmd = transfer_command(
        &state,
        "station_earth_orbit",
        "station_earth_orbit",
        vec![TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        }],
    );
    tick(&mut state, &[cmd], &content, &mut rng, None);

    let ship_id = test_ship_id();
    assert!(
        ship_is_unassigned(&state, &ship_id),
        "ship should not be assigned a task for same-station transfer"
    );
}

#[test]
fn transfer_missing_from_station_is_rejected() {
    let content = transfer_content();
    let mut state = two_station_state(&content);
    let mut rng = make_rng();

    let cmd = transfer_command(
        &state,
        "station_nonexistent",
        "station_mars_orbit",
        vec![TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        }],
    );
    tick(&mut state, &[cmd], &content, &mut rng, None);

    let ship_id = test_ship_id();
    assert!(
        ship_is_unassigned(&state, &ship_id),
        "ship should not be assigned a task when source station is missing"
    );
}

#[test]
fn transfer_with_only_crew_items_is_rejected() {
    let content = transfer_content();
    let mut state = two_station_state(&content);
    let mut rng = make_rng();

    let cmd = transfer_command(
        &state,
        "station_earth_orbit",
        "station_mars_orbit",
        vec![TradeItemSpec::Crew {
            role: crate::CrewRole("engineer".to_string()),
            count: 1,
        }],
    );
    tick(&mut state, &[cmd], &content, &mut rng, None);

    let ship_id = test_ship_id();
    assert!(
        ship_is_unassigned(&state, &ship_id),
        "transfer with only Crew items should be rejected (unsupported)"
    );
}

#[test]
fn pickup_emits_items_picked_up_event() {
    let content = transfer_content();
    let mut state = two_station_state(&content);
    let mut rng = make_rng();

    let earth_id = StationId("station_earth_orbit".to_string());
    state
        .stations
        .get_mut(&earth_id)
        .unwrap()
        .core
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 100.0,
            quality: 0.9,
            thermal: None,
        });

    let cmd = transfer_command(
        &state,
        "station_earth_orbit",
        "station_mars_orbit",
        vec![TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        }],
    );
    let mut events: Vec<EventEnvelope> = Vec::new();

    // Run ticks and collect events until we see ItemsPickedUp.
    for _ in 0..10 {
        let evts = tick(&mut state, &[cmd.clone()], &content, &mut rng, None);
        events.extend(evts);
        break;
    }
    for _ in 0..10 {
        let evts = tick(&mut state, &[], &content, &mut rng, None);
        events.extend(evts);
    }

    let has_pickup_event = events.iter().any(|e| {
        matches!(&e.event, Event::ItemsPickedUp { items, .. } if items.iter().any(|i| matches!(i, InventoryItem::Material { element, .. } if element == "Fe")))
    });
    assert!(
        has_pickup_event,
        "ItemsPickedUp event should be emitted during pickup phase"
    );
}
