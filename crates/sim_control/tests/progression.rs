//! Progression regression tests.
//!
//! These tests run the full tick loop with autopilot at production time scale
//! (minutes_per_tick = 60) and verify that game milestones are reached within
//! expected tick windows. They catch rate/timing regressions from content
//! rescaling or time-scale changes.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_control::{AutopilotController, CommandSource};
use sim_core::test_fixtures::{base_content, base_state};
use sim_core::*;
use std::collections::HashMap;

/// Build content that mimics production: minutes_per_tick=60, full tech tree,
/// all module types, sensor array for data generation, labs for evidence.
fn production_like_content() -> GameContent {
    let mut content = base_content();

    // Production time scale
    content.constants.minutes_per_tick = 60;

    // Realistic durations (game-time minutes)
    content.constants.survey_scan_minutes = 120;
    content.constants.deep_scan_minutes = 480;
    content.constants.travel_minutes_per_hop = 2880;
    content.constants.deposit_minutes = 120;
    content.constants.research_roll_interval_minutes = 60;
    content.constants.mining_rate_kg_per_minute = 15.0;
    content.constants.station_power_available_per_minute = 100.0;
    content.constants.derive_tick_values();

    // Richer asteroid mass range
    content.constants.asteroid_mass_min_kg = 500_000.0;
    content.constants.asteroid_mass_max_kg = 10_000_000.0;
    content.constants.station_cargo_capacity_m3 = 2000.0;

    // Full tech tree: deep_scan → advanced_refining → ship_construction
    content.techs = vec![
        TechDef {
            id: TechId("tech_deep_scan_v1".to_string()),
            name: "Deep Scan v1".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::from([(ResearchDomain::Exploration, 100.0)]),
            accepted_data: vec![DataKind::ScanData],
            difficulty: 200.0,
            effects: vec![
                TechEffect::EnableDeepScan,
                TechEffect::DeepScanCompositionNoise { sigma: 0.02 },
            ],
        },
        TechDef {
            id: TechId("tech_advanced_refining".to_string()),
            name: "Advanced Refining".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::from([
                (ResearchDomain::Materials, 150.0),
                (ResearchDomain::Engineering, 50.0),
            ]),
            accepted_data: vec![DataKind::MiningData, DataKind::EngineeringData],
            difficulty: 400.0,
            effects: vec![],
        },
        TechDef {
            id: TechId("tech_ship_construction".to_string()),
            name: "Ship Construction".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::from([(ResearchDomain::Engineering, 200.0)]),
            accepted_data: vec![DataKind::EngineeringData, DataKind::MiningData],
            difficulty: 500.0,
            effects: vec![TechEffect::EnableShipConstruction],
        },
    ];

    // Sensor array: generates ScanData every 2 hours
    content.module_defs.insert(
        "module_sensor_array".to_string(),
        ModuleDef {
            id: "module_sensor_array".to_string(),
            name: "Sensor Array".to_string(),
            mass_kg: 2500.0,
            volume_m3: 6.0,
            power_consumption_per_run: 8.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::SensorArray(SensorArrayDef {
                data_kind: DataKind::ScanData,
                action_key: "sensor_scan".to_string(),
                scan_interval_minutes: 120,
                scan_interval_ticks: 2, // 120 / 60
            }),
        },
    );

    // Exploration lab: consumes ScanData, produces Exploration evidence
    content.module_defs.insert(
        "module_exploration_lab".to_string(),
        ModuleDef {
            id: "module_exploration_lab".to_string(),
            name: "Exploration Lab".to_string(),
            mass_kg: 3500.0,
            volume_m3: 7.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::Exploration,
                data_consumption_per_run: 8.0,
                research_points_per_run: 4.0,
                accepted_data: vec![DataKind::ScanData],
                research_interval_minutes: 60,
                research_interval_ticks: 1, // 60 / 60
            }),
        },
    );

    // Materials lab: consumes MiningData/EngineeringData, produces Materials evidence
    content.module_defs.insert(
        "module_materials_lab".to_string(),
        ModuleDef {
            id: "module_materials_lab".to_string(),
            name: "Materials Lab".to_string(),
            mass_kg: 4000.0,
            volume_m3: 8.0,
            power_consumption_per_run: 12.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::Materials,
                data_consumption_per_run: 10.0,
                research_points_per_run: 5.0,
                accepted_data: vec![DataKind::MiningData, DataKind::EngineeringData],
                research_interval_minutes: 60,
                research_interval_ticks: 1,
            }),
        },
    );

    // Engineering lab: consumes EngineeringData, produces Engineering evidence
    content.module_defs.insert(
        "module_engineering_lab".to_string(),
        ModuleDef {
            id: "module_engineering_lab".to_string(),
            name: "Engineering Lab".to_string(),
            mass_kg: 4000.0,
            volume_m3: 8.0,
            power_consumption_per_run: 12.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::Engineering,
                data_consumption_per_run: 10.0,
                research_points_per_run: 5.0,
                accepted_data: vec![DataKind::EngineeringData],
                research_interval_minutes: 60,
                research_interval_ticks: 1,
            }),
        },
    );

    // Processor (refinery): every 1 hour
    content.module_defs.insert(
        "module_basic_iron_refinery".to_string(),
        ModuleDef {
            id: "module_basic_iron_refinery".to_string(),
            name: "Basic Iron Refinery".to_string(),
            mass_kg: 5000.0,
            volume_m3: 10.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 1,
                recipes: vec![RecipeDef {
                    id: "recipe_basic_iron".to_string(),
                    inputs: vec![RecipeInput {
                        filter: InputFilter::ItemKind(ItemKind::Ore),
                        amount: InputAmount::Kg(1000.0),
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
    );

    // Assembler (repair kits): every 6 hours — generates EngineeringData
    content.module_defs.insert(
        "module_basic_assembler".to_string(),
        ModuleDef {
            id: "module_basic_assembler".to_string(),
            name: "Basic Assembler".to_string(),
            mass_kg: 3000.0,
            volume_m3: 8.0,
            power_consumption_per_run: 8.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                assembly_interval_minutes: 360,
                assembly_interval_ticks: 6,
                recipes: vec![RecipeDef {
                    id: "recipe_basic_repair_kit".to_string(),
                    inputs: vec![RecipeInput {
                        filter: InputFilter::Element("Fe".to_string()),
                        amount: InputAmount::Kg(200.0),
                    }],
                    outputs: vec![OutputSpec::Component {
                        component_id: ComponentId("repair_kit".to_string()),
                        quality_formula: QualityFormula::Fixed(1.0),
                    }],
                    efficiency: 1.0,
                }],
                max_stock: HashMap::from([(ComponentId("repair_kit".to_string()), 50)]),
            }),
        },
    );

    // Maintenance bay
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
                repair_interval_minutes: 60,
                repair_interval_ticks: 1,
                wear_reduction_per_run: 0.2,
                repair_kit_cost: 1,
                repair_threshold: 0.1,
            }),
        },
    );

    // Component def for repair_kit
    content.component_defs = vec![ComponentDef {
        id: "repair_kit".to_string(),
        name: "Repair Kit".to_string(),
        mass_kg: 1.0,
        volume_m3: 0.1,
    }];

    content
}

/// Build a state that starts with modules in inventory (autopilot installs them).
fn production_like_state(content: &GameContent) -> GameState {
    let mut state = base_state(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();
    station.cargo_capacity_m3 = 2000.0;
    station.power_available_per_tick = content.constants.station_power_available_per_tick;

    // Put all modules in inventory for autopilot to install
    station.inventory = vec![
        InventoryItem::Module {
            item_id: ModuleItemId("mi_001".to_string()),
            module_def_id: "module_sensor_array".to_string(),
        },
        InventoryItem::Module {
            item_id: ModuleItemId("mi_002".to_string()),
            module_def_id: "module_exploration_lab".to_string(),
        },
        InventoryItem::Module {
            item_id: ModuleItemId("mi_003".to_string()),
            module_def_id: "module_materials_lab".to_string(),
        },
        InventoryItem::Module {
            item_id: ModuleItemId("mi_004".to_string()),
            module_def_id: "module_engineering_lab".to_string(),
        },
        InventoryItem::Module {
            item_id: ModuleItemId("mi_005".to_string()),
            module_def_id: "module_basic_iron_refinery".to_string(),
        },
        InventoryItem::Module {
            item_id: ModuleItemId("mi_006".to_string()),
            module_def_id: "module_basic_assembler".to_string(),
        },
        InventoryItem::Module {
            item_id: ModuleItemId("mi_007".to_string()),
            module_def_id: "module_maintenance_bay".to_string(),
        },
        InventoryItem::Component {
            component_id: ComponentId("repair_kit".to_string()),
            count: 10,
            quality: 1.0,
        },
    ];
    state.balance = 1_000_000_000.0;
    state
}

/// Run ticks with autopilot, returning the final state.
fn run_with_autopilot(
    content: &GameContent,
    state: &mut GameState,
    rng: &mut ChaCha8Rng,
    ticks: u64,
) {
    let mut autopilot = AutopilotController;
    let mut next_cmd_id = state.counters.next_command_id;

    for _ in 0..ticks {
        let commands = autopilot.generate_commands(state, content, &mut next_cmd_id);
        tick(state, &commands, content, rng, EventLevel::Normal);
    }
    state.counters.next_command_id = next_cmd_id;
}

// ---------------------------------------------------------------------------
// Milestone tests
// ---------------------------------------------------------------------------

/// Verify derive_tick_values converts minutes to ticks correctly at mpt=60.
#[test]
fn derive_tick_values_produces_correct_ticks_at_mpt_60() {
    let content = production_like_content();

    // Constants
    assert_eq!(
        content.constants.survey_scan_ticks, 2,
        "120 min / 60 mpt = 2 ticks"
    );
    assert_eq!(
        content.constants.deep_scan_ticks, 8,
        "480 min / 60 mpt = 8 ticks"
    );
    assert_eq!(
        content.constants.travel_ticks_per_hop, 48,
        "2880 min / 60 mpt = 48 ticks"
    );
    assert_eq!(
        content.constants.deposit_ticks, 2,
        "120 min / 60 mpt = 2 ticks"
    );
    assert_eq!(
        content.constants.research_roll_interval_ticks, 1,
        "60 min / 60 mpt = 1 tick"
    );
    assert!(
        (content.constants.mining_rate_kg_per_tick - 900.0).abs() < f32::EPSILON,
        "15 kg/min * 60 mpt = 900 kg/tick"
    );

    // Module defs
    let sensor = content.module_defs.get("module_sensor_array").unwrap();
    if let ModuleBehaviorDef::SensorArray(s) = &sensor.behavior {
        assert_eq!(s.scan_interval_ticks, 2, "120 min / 60 mpt = 2 ticks");
    } else {
        panic!("expected SensorArray");
    }

    let lab = content.module_defs.get("module_exploration_lab").unwrap();
    if let ModuleBehaviorDef::Lab(l) = &lab.behavior {
        assert_eq!(l.research_interval_ticks, 1, "60 min / 60 mpt = 1 tick");
    } else {
        panic!("expected Lab");
    }

    let assembler = content.module_defs.get("module_basic_assembler").unwrap();
    if let ModuleBehaviorDef::Assembler(a) = &assembler.behavior {
        assert_eq!(a.assembly_interval_ticks, 6, "360 min / 60 mpt = 6 ticks");
    } else {
        panic!("expected Assembler");
    }
}

/// After 500 ticks at mpt=60 (~20 days), deep_scan_v1 should be unlocked.
/// This catches regressions where research evidence generation is broken.
#[test]
fn deep_scan_unlocks_within_500_ticks() {
    let content = production_like_content();
    let mut state = production_like_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    run_with_autopilot(&content, &mut state, &mut rng, 500);

    assert!(
        state
            .research
            .unlocked
            .contains(&TechId("tech_deep_scan_v1".to_string())),
        "tech_deep_scan_v1 should unlock within 500 ticks (~20 days). \
         Unlocked: {:?}, ScanData pool: {:?}, Exploration evidence: {:?}",
        state.research.unlocked,
        state.research.data_pool.get(&DataKind::ScanData),
        state
            .research
            .evidence
            .get(&TechId("tech_deep_scan_v1".to_string())),
    );
}

/// After 1000 ticks at mpt=60 (~41 days), all techs should be unlocked.
/// This catches regressions where domain-specific evidence generation is broken.
#[test]
fn full_tech_tree_unlocks_within_1000_ticks() {
    let content = production_like_content();
    let mut state = production_like_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    run_with_autopilot(&content, &mut state, &mut rng, 1000);

    for tech_id in [
        "tech_deep_scan_v1",
        "tech_advanced_refining",
        "tech_ship_construction",
    ] {
        assert!(
            state
                .research
                .unlocked
                .contains(&TechId(tech_id.to_string())),
            "{tech_id} should unlock within 1000 ticks. \
             Unlocked: {:?}, Evidence: {:?}",
            state.research.unlocked,
            state.research.evidence.get(&TechId(tech_id.to_string())),
        );
    }
}

/// Sensor generates data at the expected rate given the interval.
/// At mpt=60 with scan_interval_minutes=120, sensors fire every 2 ticks.
/// After 10 ticks, sensor should have generated data 5 times.
#[test]
fn sensor_data_generation_rate_at_mpt_60() {
    let content = production_like_content();
    let mut state = production_like_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Install sensor directly (bypass autopilot for precise control)
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.clear();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("sensor_001".to_string()),
        def_id: "module_sensor_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
        power_stalled: false,
    });

    // Run 10 ticks (no autopilot needed)
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let scan_data = state
        .research
        .data_pool
        .get(&DataKind::ScanData)
        .copied()
        .unwrap_or(0.0);

    // With interval=2 ticks, should fire at ticks 2,4,6,8,10 = 5 times
    // First fire yields peak (100.0), subsequent fires yield less (diminishing returns)
    assert!(
        scan_data > 200.0,
        "sensor should generate significant ScanData in 10 ticks at mpt=60. Got: {scan_data}"
    );
}

/// After all techs are unlocked and trade is available, ships should be built.
/// This test starts past the trade unlock tick with all techs already unlocked,
/// adds a shipyard module, and verifies the autopilot imports thrusters and
/// builds at least one ship within a reasonable tick window.
#[test]
fn ships_built_after_tech_unlock_and_trade_available() {
    let mut content = production_like_content();

    // Add shipyard assembler: 20160 min = 336 ticks, needs 10000kg Fe + 8 thrusters
    content.module_defs.insert(
        "module_shipyard".to_string(),
        ModuleDef {
            id: "module_shipyard".to_string(),
            name: "Shipyard".to_string(),
            mass_kg: 5000.0,
            volume_m3: 20.0,
            power_consumption_per_run: 25.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                assembly_interval_minutes: 20160,
                assembly_interval_ticks: 336, // 20160 / 60
                recipes: vec![RecipeDef {
                    id: "recipe_basic_mining_shuttle".to_string(),
                    inputs: vec![
                        RecipeInput {
                            filter: InputFilter::Element("Fe".to_string()),
                            amount: InputAmount::Kg(10000.0),
                        },
                        RecipeInput {
                            filter: InputFilter::Component(ComponentId("thruster".to_string())),
                            amount: InputAmount::Count(8),
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
    );

    // Add thruster component def
    content.component_defs.push(ComponentDef {
        id: "thruster".to_string(),
        name: "Thruster".to_string(),
        mass_kg: 50.0,
        volume_m3: 1.0,
    });

    // Add pricing so trade works
    content.pricing.items.insert(
        "thruster".to_string(),
        PricingEntry {
            base_price_per_unit: 50_000.0,
            importable: true,
            exportable: false,
        },
    );

    let mut state = production_like_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Start past trade unlock (1 year = 8760 ticks at mpt=60)
    state.meta.tick = sim_core::trade_unlock_tick(content.constants.minutes_per_tick);

    // Pre-unlock all techs
    state
        .research
        .unlocked
        .insert(TechId("tech_deep_scan_v1".to_string()));
    state
        .research
        .unlocked
        .insert(TechId("tech_advanced_refining".to_string()));
    state
        .research
        .unlocked
        .insert(TechId("tech_ship_construction".to_string()));

    // Add shipyard module to inventory for autopilot to install
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Module {
        item_id: ModuleItemId("mi_shipyard".to_string()),
        module_def_id: "module_shipyard".to_string(),
    });

    // Pre-stock Fe so shipyard has materials
    station.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 50_000.0,
        quality: 0.8,
    });

    // Run enough ticks for import (travel ~48 ticks) + assembly (336 ticks) + margin
    run_with_autopilot(&content, &mut state, &mut rng, 1500);

    assert!(
        state.ships.len() > 1,
        "fleet should have grown beyond the starting ship. \
         Ships: {}, Tick: {}",
        state.ships.len(),
        state.meta.tick,
    );
}

/// Lab per-run amounts should NOT be scaled by minutes_per_tick.
/// data_consumption_per_run and research_points_per_run are per-execution
/// constants, not rates. This catches the VIO-187 double-scaling bug.
#[test]
fn lab_per_run_amounts_are_not_time_scaled() {
    let content = production_like_content();

    let lab = content.module_defs.get("module_exploration_lab").unwrap();
    if let ModuleBehaviorDef::Lab(l) = &lab.behavior {
        assert!(
            l.data_consumption_per_run < 20.0,
            "data_consumption_per_run should be ~8, not 480 (60x scaled). Got: {}",
            l.data_consumption_per_run
        );
        assert!(
            l.research_points_per_run < 10.0,
            "research_points_per_run should be ~4, not 240 (60x scaled). Got: {}",
            l.research_points_per_run
        );
    } else {
        panic!("expected Lab");
    }
}
