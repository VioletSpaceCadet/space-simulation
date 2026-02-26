use super::*;
use crate::test_fixtures::{base_content, base_state, make_rng};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;

mod assembler;
mod commands;
mod deep_scan;
mod deposit;
mod integration;
mod mining;
mod performance;
mod refinery;
mod research;
mod survey;
mod transit;
mod wear;

// --- Shared test helpers ------------------------------------------------

fn test_content() -> GameContent {
    base_content()
}

fn test_state(content: &GameContent) -> GameState {
    base_state(content)
}

fn survey_command(state: &GameState) -> CommandEnvelope {
    let ship_id = ShipId("ship_0001".to_string());
    let owner = state.ships[&ship_id].owner.clone();
    CommandEnvelope {
        id: CommandId("cmd_000001".to_string()),
        issued_by: owner,
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
            task_kind: TaskKind::Survey {
                site: SiteId("site_0001".to_string()),
            },
        },
    }
}

/// Build a state with an already-surveyed asteroid (mass 500, 70% Fe / 30% Si).
fn state_with_asteroid(content: &GameContent) -> (GameState, AsteroidId) {
    let mut state = test_state(content);
    let mut rng = make_rng();
    let cmd = survey_command(&state);
    tick(&mut state, &[cmd], content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], content, &mut rng, EventLevel::Normal);
    let asteroid_id = state.asteroids.keys().next().unwrap().clone();
    (state, asteroid_id)
}

fn deposit_command(state: &GameState) -> CommandEnvelope {
    let ship_id = ShipId("ship_0001".to_string());
    let station_id = StationId("station_earth_orbit".to_string());
    let ship = &state.ships[&ship_id];
    CommandEnvelope {
        id: CommandId("cmd_deposit_001".to_string()),
        issued_by: ship.owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
            task_kind: TaskKind::Deposit {
                station: station_id,
                blocked: false,
            },
        },
    }
}

fn mine_command(
    state: &GameState,
    asteroid_id: &AsteroidId,
    _content: &GameContent,
) -> CommandEnvelope {
    let ship_id = ShipId("ship_0001".to_string());
    let ship = &state.ships[&ship_id];
    let duration_ticks = 10;
    CommandEnvelope {
        id: CommandId("cmd_mine_001".to_string()),
        issued_by: ship.owner.clone(),
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id,
            task_kind: TaskKind::Mine {
                asteroid: asteroid_id.clone(),
                duration_ticks,
            },
        },
    }
}

fn refinery_content() -> GameContent {
    let mut content = test_content();
    content.module_defs = HashMap::from([(
        "module_basic_iron_refinery".to_string(),
        ModuleDef {
            id: "module_basic_iron_refinery".to_string(),
            name: "Basic Iron Refinery".to_string(),
            mass_kg: 5000.0,
            volume_m3: 10.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.01,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_ticks: 2,
                recipes: vec![RecipeDef {
                    id: "recipe_basic_iron".to_string(),
                    inputs: vec![RecipeInput {
                        filter: InputFilter::ItemKind(ItemKind::Ore),
                        amount: InputAmount::Kg(500.0),
                    }],
                    outputs: vec![
                        OutputSpec::Material {
                            element: "Fe".to_string(),
                            yield_formula: YieldFormula::ElementFraction {
                                element: "Fe".to_string(),
                            },
                            quality_formula: QualityFormula::ElementFractionTimesMultiplier {
                                element: "Fe".to_string(),
                                multiplier: 1.0,
                            },
                        },
                        OutputSpec::Slag {
                            yield_formula: YieldFormula::FixedFraction(1.0),
                        },
                    ],
                    efficiency: 1.0,
                }],
            }),
        },
    )]);
    content
}

fn state_with_refinery(content: &GameContent) -> GameState {
    let mut state = test_state(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_0001".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 100.0,
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

    state
}

fn assembler_content() -> GameContent {
    let mut content = test_content();
    content.module_defs = HashMap::from([(
        "module_basic_assembler".to_string(),
        ModuleDef {
            id: "module_basic_assembler".to_string(),
            name: "Basic Assembler".to_string(),
            mass_kg: 3000.0,
            volume_m3: 8.0,
            power_consumption_per_run: 8.0,
            wear_per_run: 0.008,
            behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                assembly_interval_ticks: 2,
                max_stock: HashMap::new(),
                recipes: vec![RecipeDef {
                    id: "recipe_basic_repair_kit".to_string(),
                    inputs: vec![RecipeInput {
                        filter: InputFilter::Element("Fe".to_string()),
                        amount: InputAmount::Kg(100.0),
                    }],
                    outputs: vec![OutputSpec::Component {
                        component_id: ComponentId("repair_kit".to_string()),
                        quality_formula: QualityFormula::Fixed(1.0),
                    }],
                    efficiency: 1.0,
                }],
            }),
        },
    )]);
    content.component_defs = vec![crate::ComponentDef {
        id: "repair_kit".to_string(),
        name: "Repair Kit".to_string(),
        mass_kg: 5.0,
        volume_m3: 0.01,
    }];
    content
}

fn state_with_assembler(content: &GameContent) -> GameState {
    let mut state = test_state(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_0001".to_string()),
        def_id: "module_basic_assembler".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Assembler(AssemblerState {
            ticks_since_last_run: 0,
            stalled: false,
            capped: false,
            cap_override: HashMap::new(),
        }),
        wear: WearState::default(),
    });

    station.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 500.0,
        quality: 0.7,
    });

    state
}

fn maintenance_content() -> GameContent {
    let mut content = refinery_content();
    content.module_defs.insert(
        "module_maintenance_bay".to_string(),
        ModuleDef {
            id: "module_maintenance_bay".to_string(),
            name: "Maintenance Bay".to_string(),
            mass_kg: 2000.0,
            volume_m3: 5.0,
            power_consumption_per_run: 5.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Maintenance(MaintenanceDef {
                repair_interval_ticks: 2,
                wear_reduction_per_run: 0.2,
                repair_kit_cost: 1,
                repair_threshold: 0.0,
            }),
        },
    );
    content
}

fn state_with_maintenance(content: &GameContent) -> GameState {
    let mut state = state_with_refinery(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_0002".to_string()),
        def_id: "module_maintenance_bay".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Maintenance(MaintenanceState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
    });

    station.inventory.push(InventoryItem::Component {
        component_id: ComponentId("repair_kit".to_string()),
        count: 5,
        quality: 1.0,
    });

    state
}

#[test]
fn test_station_volume_cache_invalidation() {
    let content = test_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    let station = state.stations.get_mut(&station_id).unwrap();
    assert!(
        station.cached_inventory_volume_m3.is_none(),
        "cache starts empty"
    );

    // First call computes and caches.
    let vol1 = station.used_volume_m3(&content);
    assert!(
        station.cached_inventory_volume_m3.is_some(),
        "cache populated after first call"
    );
    assert!(
        (station.used_volume_m3(&content) - vol1).abs() < f32::EPSILON,
        "cached value is stable"
    );

    // Mutate inventory and invalidate.
    station.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_cache_test".to_string()),
        asteroid_id: AsteroidId("asteroid_test".to_string()),
        kg: 100.0,
        composition: HashMap::from([("Fe".to_string(), 1.0)]),
    });
    station.invalidate_volume_cache();
    assert!(
        station.cached_inventory_volume_m3.is_none(),
        "cache cleared after invalidation"
    );

    // Recompute — should reflect the new item.
    let vol2 = station.used_volume_m3(&content);
    assert!(
        vol2 > vol1,
        "volume increased after adding ore (was {vol1}, now {vol2})"
    );
    assert!(
        station.cached_inventory_volume_m3.is_some(),
        "cache repopulated"
    );
}
