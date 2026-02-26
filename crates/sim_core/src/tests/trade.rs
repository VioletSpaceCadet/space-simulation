use super::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;

/// Build content with pricing entries for Fe, thruster, and a module.
fn trade_content() -> GameContent {
    let mut content = test_fixtures::base_content();
    content.pricing = PricingTable {
        import_surcharge_per_kg: 100.0,
        export_surcharge_per_kg: 50.0,
        items: HashMap::from([
            (
                "Fe".to_string(),
                PricingEntry {
                    base_price_per_unit: 50.0,
                    importable: true,
                    exportable: true,
                },
            ),
            (
                "thruster".to_string(),
                PricingEntry {
                    base_price_per_unit: 500_000.0,
                    importable: true,
                    exportable: true,
                },
            ),
            (
                "repair_kit".to_string(),
                PricingEntry {
                    base_price_per_unit: 8_000.0,
                    importable: true,
                    exportable: true,
                },
            ),
            (
                "ore".to_string(),
                PricingEntry {
                    base_price_per_unit: 5.0,
                    importable: false,
                    exportable: false,
                },
            ),
            (
                "slag".to_string(),
                PricingEntry {
                    base_price_per_unit: 1.0,
                    importable: false,
                    exportable: false,
                },
            ),
            (
                "module_basic_iron_refinery".to_string(),
                PricingEntry {
                    base_price_per_unit: 2_000_000.0,
                    importable: true,
                    exportable: true,
                },
            ),
        ]),
    };
    content.component_defs = vec![
        ComponentDef {
            id: "repair_kit".to_string(),
            name: "Repair Kit".to_string(),
            mass_kg: 50.0,
            volume_m3: 0.1,
        },
        ComponentDef {
            id: "thruster".to_string(),
            name: "Thruster".to_string(),
            mass_kg: 200.0,
            volume_m3: 0.5,
        },
    ];
    content.module_defs = HashMap::from([(
        "module_basic_iron_refinery".to_string(),
        ModuleDef {
            id: "module_basic_iron_refinery".to_string(),
            name: "Basic Iron Refinery".to_string(),
            mass_kg: 1000.0,
            volume_m3: 5.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.01,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_ticks: 10,
                recipes: vec![],
            }),
        },
    )]);
    content
}

fn trade_state(content: &GameContent) -> GameState {
    let mut state = test_fixtures::base_state(content);
    state.balance = 10_000_000.0;
    state.meta.tick = TRADE_UNLOCK_TICK;
    // Pre-fill 5 scan sites to avoid replenish noise
    for index in 0..5 {
        state.scan_sites.push(ScanSite {
            id: SiteId(format!("site_pad_{index}")),
            node: NodeId("node_test".to_string()),
            template_id: "tmpl_iron_rich".to_string(),
        });
    }
    state
}

fn make_command(command: Command) -> CommandEnvelope {
    CommandEnvelope {
        id: CommandId("cmd_test".to_string()),
        issued_by: PrincipalId("principal_autopilot".to_string()),
        issued_tick: TRADE_UNLOCK_TICK,
        execute_at_tick: TRADE_UNLOCK_TICK,
        command,
    }
}

// ---- Import tests ----

#[test]
fn import_material_deducts_balance_and_adds_inventory() {
    let content = trade_content();
    let mut state = trade_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    let cmd = make_command(Command::Import {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        },
    });

    // Expected cost: 50.0 * 100 + 100.0 * 100.0 = 5000 + 10000 = 15000
    let expected_cost = 50.0 * 100.0 + 100.0 * 100.0;

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        (state.balance - (10_000_000.0 - expected_cost)).abs() < 0.01,
        "balance should be deducted by cost: got {} expected {}",
        state.balance,
        10_000_000.0 - expected_cost
    );

    let station = state.stations.get(&station_id).unwrap();
    let fe_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        (fe_kg - 100.0).abs() < 0.01,
        "should have 100kg Fe, got {fe_kg}"
    );

    let imported = events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemImported { .. }));
    assert!(imported, "should emit ItemImported event");
}

#[test]
fn import_component_deducts_balance_and_adds_inventory() {
    let content = trade_content();
    let mut state = trade_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    let cmd = make_command(Command::Import {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Component {
            component_id: ComponentId("thruster".to_string()),
            count: 2,
        },
    });

    // cost: 500_000 * 2 + (200 * 2) * 100 = 1_000_000 + 40_000 = 1_040_000
    let expected_cost = 500_000.0 * 2.0 + (200.0 * 2.0) * 100.0;

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        (state.balance - (10_000_000.0 - expected_cost)).abs() < 0.01,
        "balance: got {} expected {}",
        state.balance,
        10_000_000.0 - expected_cost
    );

    let station = state.stations.get(&station_id).unwrap();
    let thruster_count: u32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == "thruster" => Some(*count),
            _ => None,
        })
        .sum();
    assert_eq!(thruster_count, 2, "should have 2 thrusters");

    assert!(events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemImported { .. })));
}

#[test]
fn import_module_deducts_balance_and_adds_with_unique_id() {
    let content = trade_content();
    let mut state = trade_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    let cmd = make_command(Command::Import {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Module {
            module_def_id: "module_basic_iron_refinery".to_string(),
        },
    });

    // cost: 2_000_000 * 1 + 1000 * 100 = 2_000_000 + 100_000 = 2_100_000
    let expected_cost = 2_000_000.0 + 1000.0 * 100.0;

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        (state.balance - (10_000_000.0 - expected_cost)).abs() < 0.01,
        "balance: got {} expected {}",
        state.balance,
        10_000_000.0 - expected_cost
    );

    let station = state.stations.get(&station_id).unwrap();
    let module_items: Vec<_> = station
        .inventory
        .iter()
        .filter(|item| {
            matches!(item, InventoryItem::Module { module_def_id, .. }
                if module_def_id == "module_basic_iron_refinery")
        })
        .collect();
    assert_eq!(module_items.len(), 1, "should have 1 module item");

    // Check the item_id starts with "module_item_"
    if let InventoryItem::Module { item_id, .. } = module_items[0] {
        assert!(
            item_id.0.starts_with("module_item_"),
            "module item_id should start with module_item_: {}",
            item_id.0
        );
    }

    assert!(events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemImported { .. })));
}

#[test]
fn import_insufficient_funds_emits_event_no_change() {
    let content = trade_content();
    let mut state = trade_state(&content);
    state.balance = 100.0; // not enough for Fe import
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    let cmd = make_command(Command::Import {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        },
    });

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        (state.balance - 100.0).abs() < 0.01,
        "balance should not change"
    );

    let station = state.stations.get(&station_id).unwrap();
    let has_fe = station
        .inventory
        .iter()
        .any(|item| matches!(item, InventoryItem::Material { element, .. } if element == "Fe"));
    assert!(!has_fe, "should not have Fe in inventory");

    let insufficient = events
        .iter()
        .any(|e| matches!(&e.event, Event::InsufficientFunds { .. }));
    assert!(insufficient, "should emit InsufficientFunds event");
}

#[test]
fn import_non_importable_is_rejected() {
    let content = trade_content();
    let mut state = trade_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    let cmd = make_command(Command::Import {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Material {
            element: "ore".to_string(),
            kg: 100.0,
        },
    });

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        (state.balance - 10_000_000.0).abs() < 0.01,
        "balance should not change for non-importable"
    );

    let imported = events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemImported { .. }));
    assert!(!imported, "should not emit ItemImported");
}

// ---- Export tests ----

#[test]
fn export_material_removes_from_inventory_and_adds_revenue() {
    let content = trade_content();
    let mut state = trade_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    // Pre-add 100 kg Fe to station
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 100.0,
        quality: 1.0,
    });

    let cmd = make_command(Command::Export {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 50.0,
        },
    });

    // revenue: 50.0 * 50 - 50.0 * 50.0 = 2500 - 2500 = 0.0 (floored)
    // Actually: base_price * quantity - mass * surcharge = 50 * 50 - 50 * 50 = 0
    let expected_revenue = (50.0_f64 * 50.0 - 50.0 * 50.0).max(0.0);

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        (state.balance - (10_000_000.0 + expected_revenue)).abs() < 0.01,
        "balance: got {} expected {}",
        state.balance,
        10_000_000.0 + expected_revenue
    );

    let station = state.stations.get(&station_id).unwrap();
    let fe_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        (fe_kg - 50.0).abs() < 0.01,
        "should have 50kg Fe remaining, got {fe_kg}"
    );

    assert!(events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemExported { .. })));
}

#[test]
fn export_component_removes_and_adds_revenue() {
    let content = trade_content();
    let mut state = trade_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Component {
        component_id: ComponentId("repair_kit".to_string()),
        count: 5,
        quality: 1.0,
    });

    let cmd = make_command(Command::Export {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: 2,
        },
    });

    // revenue: 8000 * 2 - (50 * 2) * 50 = 16000 - 5000 = 11000
    let expected_revenue = 8000.0 * 2.0 - (50.0 * 2.0) * 50.0;

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        (state.balance - (10_000_000.0 + expected_revenue)).abs() < 0.01,
        "balance: got {} expected {}",
        state.balance,
        10_000_000.0 + expected_revenue
    );

    let station = state.stations.get(&station_id).unwrap();
    let kit_count: u32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == "repair_kit" => Some(*count),
            _ => None,
        })
        .sum();
    assert_eq!(kit_count, 3, "should have 3 repair kits remaining");

    assert!(events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemExported { .. })));
}

#[test]
fn export_non_exportable_is_rejected() {
    let content = trade_content();
    let mut state = trade_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Slag {
        kg: 100.0,
        composition: HashMap::new(),
    });

    let cmd = make_command(Command::Export {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Material {
            element: "slag".to_string(),
            kg: 50.0,
        },
    });

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        (state.balance - 10_000_000.0).abs() < 0.01,
        "balance should not change"
    );

    let exported = events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemExported { .. }));
    assert!(!exported, "should not emit ItemExported for non-exportable");
}

#[test]
fn export_more_than_available_is_rejected() {
    let content = trade_content();
    let mut state = trade_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 100.0,
        quality: 1.0,
    });

    let cmd = make_command(Command::Export {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 1000.0,
        },
    });

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    assert!(
        (state.balance - 10_000_000.0).abs() < 0.01,
        "balance should not change"
    );

    let station = state.stations.get(&station_id).unwrap();
    let fe_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        (fe_kg - 100.0).abs() < 0.01,
        "Fe should still be 100kg, got {fe_kg}"
    );

    let exported = events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemExported { .. }));
    assert!(!exported, "should not emit ItemExported");
}

#[test]
fn import_merges_material_with_existing() {
    let content = trade_content();
    let mut state = trade_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    // Pre-add 50 kg Fe with quality 1.0
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 50.0,
        quality: 1.0,
    });

    let cmd = make_command(Command::Import {
        station_id: station_id.clone(),
        item_spec: TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        },
    });

    tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

    let station = state.stations.get(&station_id).unwrap();
    // Should merge into single entry
    let fe_entries: Vec<_> = station
        .inventory
        .iter()
        .filter(|item| matches!(item, InventoryItem::Material { element, .. } if element == "Fe"))
        .collect();
    assert_eq!(
        fe_entries.len(),
        1,
        "should merge into one Fe entry, got {}",
        fe_entries.len()
    );
    if let InventoryItem::Material { kg, .. } = fe_entries[0] {
        assert!(
            (*kg - 150.0).abs() < 0.01,
            "merged Fe should be 150kg, got {kg}"
        );
    }
}

#[test]
fn import_rejected_before_trade_unlock_tick() {
    let content = trade_content();
    let mut state = trade_state(&content);
    state.meta.tick = TRADE_UNLOCK_TICK - 1;
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());
    let balance_before = state.balance;

    let cmd = CommandEnvelope {
        id: CommandId("cmd_early".to_string()),
        issued_by: PrincipalId("principal_autopilot".to_string()),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "Fe".to_string(),
                kg: 100.0,
            },
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    assert!(
        (state.balance - balance_before).abs() < 0.01,
        "balance should be unchanged before trade unlock"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemImported { .. })),
        "should not emit ItemImported before trade unlock"
    );
}

#[test]
fn export_rejected_before_trade_unlock_tick() {
    let content = trade_content();
    let mut state = trade_state(&content);
    state.meta.tick = TRADE_UNLOCK_TICK - 1;
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    // Add exportable Fe
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 500.0,
        quality: 0.7,
    });
    let balance_before = state.balance;

    let cmd = CommandEnvelope {
        id: CommandId("cmd_early_export".to_string()),
        issued_by: PrincipalId("principal_autopilot".to_string()),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::Export {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "Fe".to_string(),
                kg: 100.0,
            },
        },
    };

    let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
    assert!(
        (state.balance - balance_before).abs() < 0.01,
        "balance should be unchanged before trade unlock"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemExported { .. })),
        "should not emit ItemExported before trade unlock"
    );
}
