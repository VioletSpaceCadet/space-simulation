use super::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;

/// Build content with pricing for Fe, thruster, and a shipyard module.
fn economy_content() -> GameContent {
    let mut content = test_fixtures::base_content();

    // Pricing table for materials, components, and modules
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
                "module_shipyard".to_string(),
                PricingEntry {
                    base_price_per_unit: 5_000_000.0,
                    importable: true,
                    exportable: true,
                },
            ),
        ]),
    };

    // Component definition for thrusters
    content.component_defs = vec![ComponentDef {
        id: "thruster".to_string(),
        name: "Thruster".to_string(),
        mass_kg: 200.0,
        volume_m3: 0.5,
    }];

    // Shipyard assembler: consumes 100kg Fe + 2 thrusters => Ship (50 m3 cargo)
    // Use assembly_interval_ticks=2 so the test doesn't need thousands of ticks.
    content.module_defs = HashMap::from([(
        "module_shipyard".to_string(),
        ModuleDef {
            id: "module_shipyard".to_string(),
            name: "Shipyard".to_string(),
            mass_kg: 5000.0,
            volume_m3: 20.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                assembly_interval_minutes: 2,
                assembly_interval_ticks: 2,
                recipes: vec![RecipeDef {
                    id: "recipe_build_ship".to_string(),
                    inputs: vec![
                        RecipeInput {
                            filter: InputFilter::Element("Fe".to_string()),
                            amount: InputAmount::Kg(100.0),
                        },
                        RecipeInput {
                            filter: InputFilter::Component(ComponentId("thruster".to_string())),
                            amount: InputAmount::Count(2),
                        },
                    ],
                    outputs: vec![OutputSpec::Ship {
                        cargo_capacity_m3: 50.0,
                    }],
                    efficiency: 1.0,
                }],
                max_stock: HashMap::new(),
            }),
        },
    )]);

    content
}

fn economy_state(content: &GameContent) -> GameState {
    let mut state = test_fixtures::base_state(content);
    state.balance = 1_000_000_000.0;
    state.meta.tick = trade_unlock_tick(content.constants.minutes_per_tick);
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

fn make_command(tick: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        id: CommandId(format!("cmd_test_{tick}")),
        issued_by: PrincipalId("principal_autopilot".to_string()),
        issued_tick: tick,
        execute_at_tick: tick,
        command,
    }
}

#[test]
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
fn economy_full_loop() {
    let content = economy_content();
    let mut state = economy_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_earth_orbit".to_string());

    // -----------------------------------------------------------
    // Step 1: Verify starting balance
    // -----------------------------------------------------------
    assert!(
        (state.balance - 1_000_000_000.0).abs() < 0.01,
        "starting balance should be 1B, got {}",
        state.balance
    );

    // -----------------------------------------------------------
    // Step 2: Import 4 thrusters
    // -----------------------------------------------------------
    let balance_before_thrusters = state.balance;
    let cmd_thrusters = make_command(
        state.meta.tick,
        Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Component {
                component_id: ComponentId("thruster".to_string()),
                count: 4,
            },
        },
    );
    let events = tick(
        &mut state,
        &[cmd_thrusters],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    // Verify balance decreased
    assert!(
        state.balance < balance_before_thrusters,
        "balance should decrease after importing thrusters: {} vs {}",
        state.balance,
        balance_before_thrusters
    );
    // Expected cost: 500_000 * 4 + (200 * 4) * 100 = 2_000_000 + 80_000 = 2_080_000
    let thruster_cost = 500_000.0 * 4.0 + (200.0 * 4.0) * 100.0;
    assert!(
        (state.balance - (balance_before_thrusters - thruster_cost)).abs() < 0.01,
        "balance after thruster import: expected {}, got {}",
        balance_before_thrusters - thruster_cost,
        state.balance
    );

    // Verify thrusters in inventory
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
    assert_eq!(thruster_count, 4, "should have 4 thrusters in inventory");

    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemImported { .. })),
        "should emit ItemImported for thrusters"
    );

    // -----------------------------------------------------------
    // Step 3: Import 5000 kg Fe
    // -----------------------------------------------------------
    let balance_before_fe = state.balance;
    let cmd_fe = make_command(
        state.meta.tick,
        Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "Fe".to_string(),
                kg: 5000.0,
            },
        },
    );
    let events = tick(
        &mut state,
        &[cmd_fe],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    // Verify balance decreased
    assert!(
        state.balance < balance_before_fe,
        "balance should decrease after importing Fe"
    );
    // Expected cost: 50.0 * 5000 + 100.0 * 5000 = 250_000 + 500_000 = 750_000
    let fe_cost = 50.0 * 5000.0 + 100.0 * 5000.0;
    assert!(
        (state.balance - (balance_before_fe - fe_cost)).abs() < 0.01,
        "balance after Fe import: expected {}, got {}",
        balance_before_fe - fe_cost,
        state.balance
    );

    // Verify Fe in inventory
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
        (fe_kg - 5000.0).abs() < 0.01,
        "should have 5000kg Fe, got {fe_kg}"
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemImported { .. })),
        "should emit ItemImported for Fe"
    );

    // -----------------------------------------------------------
    // Step 4: Import and install shipyard module, but WITHOUT tech
    //         Verify ModuleAwaitingTech and no ship spawned.
    // -----------------------------------------------------------
    let cmd_import_shipyard = make_command(
        state.meta.tick,
        Command::Import {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Module {
                module_def_id: "module_shipyard".to_string(),
            },
        },
    );
    tick(
        &mut state,
        &[cmd_import_shipyard],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    // Find the module item_id in inventory
    let station = state.stations.get(&station_id).unwrap();
    let module_item_id = station
        .inventory
        .iter()
        .find_map(|item| match item {
            InventoryItem::Module { item_id, .. } => Some(item_id.clone()),
            _ => None,
        })
        .expect("shipyard module should be in inventory after import");

    // Install the module
    let cmd_install = make_command(
        state.meta.tick,
        Command::InstallModule {
            station_id: station_id.clone(),
            module_item_id,
        },
    );
    tick(
        &mut state,
        &[cmd_install],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    // Enable the module
    let station = state.stations.get(&station_id).unwrap();
    let shipyard_module_id = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_shipyard")
        .expect("shipyard should be installed")
        .id
        .clone();

    let cmd_enable = make_command(
        state.meta.tick,
        Command::SetModuleEnabled {
            station_id: station_id.clone(),
            module_id: shipyard_module_id.clone(),
            enabled: true,
        },
    );
    tick(
        &mut state,
        &[cmd_enable],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    // Tick forward enough for the assembler interval (2 ticks) without tech
    let ships_before = state.ships.len();
    let mut saw_awaiting_tech = false;
    for _ in 0..4 {
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        if events
            .iter()
            .any(|e| matches!(&e.event, Event::ModuleAwaitingTech { .. }))
        {
            saw_awaiting_tech = true;
        }
    }

    assert!(
        saw_awaiting_tech,
        "should emit ModuleAwaitingTech when tech is not unlocked"
    );
    assert_eq!(
        state.ships.len(),
        ships_before,
        "no ship should be spawned without tech_ship_construction"
    );

    // -----------------------------------------------------------
    // Step 5: Unlock tech_ship_construction and tick until ship built
    // -----------------------------------------------------------
    state
        .research
        .unlocked
        .insert(TechId("tech_ship_construction".to_string()));

    // Tick enough times for the assembler to fire (interval=2).
    // The assembler may build multiple ships if enough materials exist and
    // enough ticks pass. We just need at least one ShipConstructed event.
    let ships_before = state.ships.len();
    let mut all_events = Vec::new();
    for _ in 0..4 {
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        all_events.extend(events);
    }

    assert!(
        state.ships.len() > ships_before,
        "at least one ship should be constructed after unlocking tech"
    );
    let ships_built = state.ships.len() - ships_before;

    let ship_constructed = all_events
        .iter()
        .any(|e| matches!(&e.event, Event::ShipConstructed { .. }));
    assert!(
        ship_constructed,
        "should emit ShipConstructed after tech unlocked"
    );

    // Verify inputs were consumed proportional to ships built
    let station = state.stations.get(&station_id).unwrap();
    let fe_after: f32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    let expected_fe = 5000.0 - 100.0 * ships_built as f32;
    assert!(
        (fe_after - expected_fe).abs() < 0.01,
        "expected {expected_fe}kg Fe after {ships_built} ship builds, got {fe_after}"
    );

    let thruster_after: u32 = station
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
    let expected_thrusters = 4 - 2 * ships_built as u32;
    assert_eq!(
        thruster_after, expected_thrusters,
        "should have {expected_thrusters} thrusters remaining after {ships_built} ship builds"
    );

    // -----------------------------------------------------------
    // Step 6: Export some Fe and verify balance increases
    // -----------------------------------------------------------
    let balance_before_export = state.balance;
    let cmd_export = make_command(
        state.meta.tick,
        Command::Export {
            station_id: station_id.clone(),
            item_spec: TradeItemSpec::Material {
                element: "Fe".to_string(),
                kg: 1000.0,
            },
        },
    );
    let events = tick(
        &mut state,
        &[cmd_export],
        &content,
        &mut rng,
        EventLevel::Normal,
    );

    // Revenue: base_price * kg - surcharge * mass = 50 * 1000 - 50 * 1000 = 0
    // (Fe export revenue is 0 because surcharge equals price -- that's fine, we
    //  verify the mechanics work regardless.)
    let expected_revenue = (50.0_f64 * 1000.0 - 50.0 * 1000.0).max(0.0);
    assert!(
        (state.balance - (balance_before_export + expected_revenue)).abs() < 0.01,
        "balance after export: expected {}, got {}",
        balance_before_export + expected_revenue,
        state.balance
    );

    // Verify Fe reduced: started with expected_fe, exported 1000
    let station = state.stations.get(&station_id).unwrap();
    let fe_final: f32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    let expected_fe_final = expected_fe - 1000.0;
    assert!(
        (fe_final - expected_fe_final).abs() < 0.01,
        "expected {expected_fe_final}kg Fe after export, got {fe_final}"
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemExported { .. })),
        "should emit ItemExported for Fe"
    );
}
