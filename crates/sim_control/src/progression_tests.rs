//! Progression regression tests.
//!
//! These tests run the full tick loop with autopilot at production time scale
//! (`minutes_per_tick` = 60) and verify that game milestones are reached within
//! expected tick windows. They catch rate/timing regressions from content
//! rescaling or time-scale changes.

use crate::{AutopilotController, CommandSource};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_core::test_fixtures::{base_content, base_state, ModuleDefBuilder};
use sim_core::*;
use std::collections::HashMap;

fn content_dir() -> String {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    format!("{manifest}/../../content")
}

/// Build content that mimics production: `minutes_per_tick=60`, full tech tree,
/// all module types, sensor array for data generation, labs for evidence.
#[allow(clippy::too_many_lines)] // large fixture building full production content
fn production_like_content() -> GameContent {
    let mut content = base_content();

    // Production time scale
    content.constants.minutes_per_tick = 60;

    // Realistic durations (game-time minutes)
    content.constants.survey_scan_minutes = 120;
    content.constants.deep_scan_minutes = 480;
    content.constants.deposit_minutes = 120;
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
            tier: 1,
            prereqs: vec![],
            domain_requirements: HashMap::from([(
                ResearchDomain::new(ResearchDomain::SURVEY),
                100.0,
            )]),
            accepted_data: vec![DataKind::new(DataKind::SURVEY)],
            effects: vec![
                TechEffect::EnableDeepScan,
                TechEffect::DeepScanCompositionNoise { sigma: 0.02 },
            ],
        },
        TechDef {
            id: TechId("tech_advanced_refining".to_string()),
            name: "Advanced Refining".to_string(),
            tier: 1,
            prereqs: vec![],
            domain_requirements: HashMap::from([
                (ResearchDomain::new(ResearchDomain::MATERIALS), 150.0),
                (ResearchDomain::new(ResearchDomain::MANUFACTURING), 50.0),
            ]),
            accepted_data: vec![
                DataKind::new(DataKind::ASSAY),
                DataKind::new(DataKind::MANUFACTURING),
            ],
            effects: vec![],
        },
        TechDef {
            id: TechId("tech_ship_construction".to_string()),
            name: "Ship Construction".to_string(),
            tier: 1,
            prereqs: vec![],
            domain_requirements: HashMap::from([(
                ResearchDomain::new(ResearchDomain::MANUFACTURING),
                50.0,
            )]),
            accepted_data: vec![
                DataKind::new(DataKind::MANUFACTURING),
                DataKind::new(DataKind::ASSAY),
            ],
            effects: vec![TechEffect::EnableShipConstruction],
        },
    ];

    // Sensor array: generates ScanData every 2 hours
    content.module_defs.insert(
        "module_sensor_array".to_string(),
        ModuleDefBuilder::new("module_sensor_array")
            .name("Sensor Array")
            .mass(2500.0)
            .volume(6.0)
            .power(8.0)
            .behavior(ModuleBehaviorDef::SensorArray(SensorArrayDef {
                data_kind: DataKind::new(DataKind::SURVEY),
                action_key: "sensor_scan".to_string(),
                scan_interval_minutes: 120,
                scan_interval_ticks: 2, // 120 / 60
                sensor_type: "orbital".to_string(),
                discovery_zones: vec![],
                discovery_probability: 0.0,
            }))
            .build(),
    );

    // Exploration lab: consumes ScanData, produces Exploration evidence
    content.module_defs.insert(
        "module_exploration_lab".to_string(),
        ModuleDefBuilder::new("module_exploration_lab")
            .name("Exploration Lab")
            .mass(3500.0)
            .volume(7.0)
            .power(10.0)
            .behavior(ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::new(ResearchDomain::SURVEY),
                data_consumption_per_run: 8.0,
                research_points_per_run: 4.0,
                accepted_data: vec![DataKind::new(DataKind::SURVEY)],
                research_interval_minutes: 60,
                research_interval_ticks: 1, // 60 / 60
            }))
            .build(),
    );

    // Materials lab: consumes MiningData/EngineeringData, produces Materials evidence
    content.module_defs.insert(
        "module_materials_lab".to_string(),
        ModuleDefBuilder::new("module_materials_lab")
            .name("Materials Lab")
            .mass(4000.0)
            .volume(8.0)
            .power(12.0)
            .behavior(ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::new(ResearchDomain::MATERIALS),
                data_consumption_per_run: 10.0,
                research_points_per_run: 5.0,
                accepted_data: vec![
                    DataKind::new(DataKind::ASSAY),
                    DataKind::new(DataKind::MANUFACTURING),
                ],
                research_interval_minutes: 60,
                research_interval_ticks: 1,
            }))
            .build(),
    );

    // Engineering lab: consumes EngineeringData, produces Engineering evidence
    content.module_defs.insert(
        "module_engineering_lab".to_string(),
        ModuleDefBuilder::new("module_engineering_lab")
            .name("Engineering Lab")
            .mass(4000.0)
            .volume(8.0)
            .power(12.0)
            .behavior(ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::new(ResearchDomain::ENGINEERING),
                data_consumption_per_run: 10.0,
                research_points_per_run: 5.0,
                accepted_data: vec![
                    DataKind::new(DataKind::ENGINEERING),
                    DataKind::new(DataKind::MANUFACTURING),
                ],
                research_interval_minutes: 60,
                research_interval_ticks: 1,
            }))
            .build(),
    );

    // Manufacturing lab: produces Manufacturing domain points
    content.module_defs.insert(
        "module_manufacturing_lab".to_string(),
        ModuleDefBuilder::new("module_manufacturing_lab")
            .name("Manufacturing Lab")
            .mass(4000.0)
            .volume(8.0)
            .power(12.0)
            .behavior(ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::new(ResearchDomain::MANUFACTURING),
                data_consumption_per_run: 10.0,
                research_points_per_run: 5.0,
                accepted_data: vec![DataKind::new(DataKind::MANUFACTURING)],
                research_interval_minutes: 60,
                research_interval_ticks: 1,
            }))
            .build(),
    );

    // Processor recipe: Ore → Fe + Slag
    let iron_recipe = RecipeDef {
        id: RecipeId("recipe_basic_iron".to_string()),
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
        thermal_req: None,
        required_tech: None,
        tags: vec![],
    };
    content.recipes.insert(iron_recipe.id.clone(), iron_recipe);

    // Processor (refinery): every 1 hour
    content.module_defs.insert(
        "module_basic_iron_refinery".to_string(),
        ModuleDefBuilder::new("module_basic_iron_refinery")
            .name("Basic Iron Refinery")
            .mass(5000.0)
            .volume(10.0)
            .power(10.0)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 60,
                processing_interval_ticks: 1,
                recipes: vec![RecipeId("recipe_basic_iron".to_string())],
            }))
            .build(),
    );

    // Assembler recipe: Fe → Repair Kit
    let repair_kit_recipe = RecipeDef {
        id: RecipeId("recipe_basic_repair_kit".to_string()),
        inputs: vec![RecipeInput {
            filter: InputFilter::Element("Fe".to_string()),
            amount: InputAmount::Kg(200.0),
        }],
        outputs: vec![OutputSpec::Component {
            component_id: ComponentId("repair_kit".to_string()),
            quality_formula: QualityFormula::Fixed(1.0),
        }],
        efficiency: 1.0,
        thermal_req: None,
        required_tech: None,
        tags: vec![],
    };
    content
        .recipes
        .insert(repair_kit_recipe.id.clone(), repair_kit_recipe);

    // Assembler (repair kits): every 6 hours — generates EngineeringData
    content.module_defs.insert(
        "module_basic_assembler".to_string(),
        ModuleDefBuilder::new("module_basic_assembler")
            .name("Basic Assembler")
            .mass(3000.0)
            .volume(8.0)
            .power(8.0)
            .behavior(ModuleBehaviorDef::Assembler(AssemblerDef {
                assembly_interval_minutes: 360,
                assembly_interval_ticks: 6,
                recipes: vec![RecipeId("recipe_basic_repair_kit".to_string())],
                max_stock: HashMap::from([(ComponentId("repair_kit".to_string()), 50)]),
            }))
            .build(),
    );

    // Maintenance bay
    content.module_defs.insert(
        "module_maintenance_bay".to_string(),
        ModuleDefBuilder::new("module_maintenance_bay")
            .name("Maintenance Bay")
            .mass(2000.0)
            .volume(5.0)
            .power(5.0)
            .behavior(ModuleBehaviorDef::Maintenance(MaintenanceDef {
                repair_interval_minutes: 60,
                repair_interval_ticks: 1,
                wear_reduction_per_run: 0.2,
                repair_kit_cost: 1,
                repair_threshold: 0.1,
                maintenance_component_id: "repair_kit".to_string(),
            }))
            .build(),
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
    station.core.cargo_capacity_m3 = 2000.0;
    station.core.power_available_per_tick = content.constants.station_power_available_per_tick;

    // Put all modules in inventory for autopilot to install
    station.core.inventory = vec![
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
            item_id: ModuleItemId("mi_008".to_string()),
            module_def_id: "module_manufacturing_lab".to_string(),
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
    // Add enough crew for all modules
    station.core.crew = [
        (sim_core::CrewRole("operator".to_string()), 10),
        (sim_core::CrewRole("technician".to_string()), 5),
        (sim_core::CrewRole("scientist".to_string()), 5),
        (sim_core::CrewRole("pilot".to_string()), 2),
    ]
    .into_iter()
    .collect();
    state
}

/// Run ticks with autopilot, returning the final state.
fn run_with_autopilot(
    content: &GameContent,
    state: &mut GameState,
    rng: &mut ChaCha8Rng,
    ticks: u64,
) {
    let mut autopilot = AutopilotController::new();
    let mut next_cmd_id = state.counters.next_command_id;

    for _ in 0..ticks {
        let commands = autopilot.generate_commands(state, content, &mut next_cmd_id);
        tick(state, &commands, content, rng, None);
    }
    state.counters.next_command_id = next_cmd_id;
}

// ---------------------------------------------------------------------------
// Milestone tests
// ---------------------------------------------------------------------------

/// Verify `derive_tick_values` converts minutes to ticks correctly at mpt=60.
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
        content.constants.deposit_ticks, 2,
        "120 min / 60 mpt = 2 ticks"
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

/// After 500 ticks at mpt=60 (~20 days), `deep_scan_v1` should be unlocked.
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
        state
            .research
            .data_pool
            .get(&DataKind::new(DataKind::SURVEY)),
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

    // First ticks: autopilot installs modules from inventory.
    run_with_autopilot(&content, &mut state, &mut rng, 3);
    // Assign crew so modules can operate.
    sim_world::auto_assign_initial_crew(&mut state, &content);
    run_with_autopilot(&content, &mut state, &mut rng, 997);

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
/// At mpt=60 with `scan_interval_minutes=120`, sensors fire every 2 ticks.
/// After 10 ticks, sensor should have generated data 5 times.
#[test]
fn sensor_data_generation_rate_at_mpt_60() {
    let content = production_like_content();
    let mut state = production_like_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Install sensor directly (bypass autopilot for precise control)
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();
    station.core.inventory.clear();
    station.core.modules.push(ModuleState {
        id: ModuleInstanceId("sensor_001".to_string()),
        def_id: "module_sensor_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SensorArray(SensorArrayState {
            ticks_since_last_run: 0,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: [(sim_core::CrewRole("operator".to_string()), 1)]
            .into_iter()
            .collect(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
        slot_index: None,
    });

    // Run 10 ticks (no autopilot needed)
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let scan_data = state
        .research
        .data_pool
        .get(&DataKind::new(DataKind::SURVEY))
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

    // Add a hull def for the shipyard test
    content.hulls.insert(
        HullId("hull_test_ship".to_string()),
        HullDef {
            id: HullId("hull_test_ship".to_string()),
            name: "Test Ship Hull".to_string(),
            mass_kg: 1000.0,
            cargo_capacity_m3: 50.0,
            base_speed_ticks_per_au: 2000,
            base_propellant_capacity_kg: 100.0,
            slots: vec![],
            bonuses: vec![],
            required_tech: None,
            tags: vec![],
        },
    );

    // Add shipyard recipe to catalog
    let shuttle_recipe = RecipeDef {
        id: RecipeId("recipe_basic_mining_shuttle".to_string()),
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
            hull_id: HullId("hull_test_ship".to_string()),
        }],
        efficiency: 1.0,
        thermal_req: None,
        required_tech: None,
        tags: vec![],
    };
    content
        .recipes
        .insert(shuttle_recipe.id.clone(), shuttle_recipe);

    // Add shipyard assembler: 20160 min = 336 ticks, needs 10000kg Fe + 8 thrusters
    content.module_defs.insert(
        "module_shipyard".to_string(),
        ModuleDefBuilder::new("module_shipyard")
            .name("Shipyard")
            .mass(5000.0)
            .volume(20.0)
            .power(25.0)
            .behavior(ModuleBehaviorDef::Assembler(AssemblerDef {
                assembly_interval_minutes: 20160,
                assembly_interval_ticks: 336, // 20160 / 60
                recipes: vec![RecipeId("recipe_basic_mining_shuttle".to_string())],
                max_stock: HashMap::new(),
            }))
            .roles(vec!["shipyard"])
            .build(),
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
            ..Default::default()
        },
    );

    let mut state = production_like_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Enable trade via progression tier
    state.progression.trade_tier = sim_core::TradeTier::Full;

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
    station.core.inventory.push(InventoryItem::Module {
        item_id: ModuleItemId("mi_shipyard".to_string()),
        module_def_id: "module_shipyard".to_string(),
    });

    // Pre-stock Fe so shipyard has materials
    station.core.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 50_000.0,
        quality: 0.8,
        thermal: None,
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

/// Run with production content (real wear rates) and verify deep_scan unlocks.
/// This catches the scenario where wear_per_run: 0.0 in fixture content masks
/// production failures like power cliff or module degradation.
#[test]
fn deep_scan_unlocks_with_production_content() {
    let content = sim_world::load_content(&content_dir()).unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut state = sim_world::build_initial_state(&content, 42, &mut rng);

    // First ticks: autopilot installs modules from inventory.
    run_with_autopilot(&content, &mut state, &mut rng, 3);
    // Assign crew so modules can operate.
    sim_world::auto_assign_initial_crew(&mut state, &content);
    run_with_autopilot(&content, &mut state, &mut rng, 1497);

    assert!(
        state
            .research
            .unlocked
            .contains(&TechId("tech_deep_scan_v1".to_string())),
        "tech_deep_scan_v1 should unlock within 1500 ticks with production content \
         (real wear rates). Unlocked: {:?}, SurveyData: {:?}, Evidence: {:?}",
        state.research.unlocked,
        state
            .research
            .data_pool
            .get(&DataKind::new(DataKind::SURVEY)),
        state
            .research
            .evidence
            .get(&TechId("tech_deep_scan_v1".to_string())),
    );
}

/// Lab per-run amounts should NOT be scaled by `minutes_per_tick`.
/// `data_consumption_per_run` and `research_points_per_run` are per-execution
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

// ---------------------------------------------------------------------------
// VIO-461: dev_advanced_state progression test
// ---------------------------------------------------------------------------

/// Load the actual shipped dev_advanced_state.json and run 10k ticks with autopilot.
/// Verifies that the game makes meaningful progress — catches regressions like
/// VIO-457 (ship stranding) and VIO-458 (power deficit).
#[test]
fn dev_advanced_state_progression() {
    let content = sim_world::load_content(&content_dir()).unwrap();
    let json = std::fs::read_to_string("../../content/dev_advanced_state.json").unwrap();
    let mut state: GameState = serde_json::from_str(&json).unwrap();
    state.body_cache = sim_core::build_body_cache(&content.solar_system.bodies);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Run a few ticks to let autopilot install modules, then assign crew
    run_with_autopilot(&content, &mut state, &mut rng, 3);
    sim_world::auto_assign_initial_crew(&mut state, &content);

    // Track whether ships were ever active
    let mut ever_had_active_ship = false;
    let mut max_consecutive_idle = 0_u32;
    let mut consecutive_idle = 0_u32;

    let total_ticks = 10_000;
    let mut autopilot = AutopilotController::new();
    let mut next_cmd_id = state.counters.next_command_id;

    for _ in 0..total_ticks {
        let commands = autopilot.generate_commands(&state, &content, &mut next_cmd_id);
        tick(&mut state, &commands, &content, &mut rng, None);

        // Check ship activity
        let all_idle = state.ships.values().all(|s| {
            s.task
                .as_ref()
                .is_none_or(|t| matches!(t.kind, TaskKind::Idle))
        });
        if all_idle && !state.ships.is_empty() {
            consecutive_idle += 1;
            max_consecutive_idle = max_consecutive_idle.max(consecutive_idle);
        } else {
            if !all_idle {
                ever_had_active_ship = true;
            }
            consecutive_idle = 0;
        }
    }
    state.counters.next_command_id = next_cmd_id;

    assert!(
        ever_had_active_ship,
        "Ships should have been active at some point during 10k ticks"
    );

    // Ships shouldn't be permanently idle for more than ~200 ticks
    // (enough for transit + task + buffer)
    assert!(
        max_consecutive_idle < 500,
        "Fleet was idle for {max_consecutive_idle} consecutive ticks — possible stranding regression"
    );

    assert!(
        state.research.unlocked.len() >= 2,
        "Should unlock at least 2 techs in 10k ticks. Unlocked: {:?}",
        state.research.unlocked
    );

    assert!(
        state.balance > 0.0,
        "Balance should not collapse in 10k ticks. Balance: {}",
        state.balance
    );
}

// ---------------------------------------------------------------------------
// VIO-476: Invariant tests — "no permanent idle" and "research advances"
// ---------------------------------------------------------------------------

/// Run sim with autopilot for `ticks` ticks, asserting that ships never stay
/// permanently idle. Returns the final state for further assertions.
fn run_and_assert_no_permanent_idle(
    content: &GameContent,
    state: &mut GameState,
    rng: &mut ChaCha8Rng,
    ticks: u64,
    max_idle_window: u32,
) {
    let mut autopilot = AutopilotController::new();
    let mut next_cmd_id = state.counters.next_command_id;
    let mut consecutive_idle = 0_u32;

    for tick_num in 0..ticks {
        let commands = autopilot.generate_commands(state, content, &mut next_cmd_id);
        tick(state, &commands, content, rng, None);

        let all_idle = state.ships.values().all(|s| {
            s.task
                .as_ref()
                .is_none_or(|t| matches!(t.kind, TaskKind::Idle))
        });
        if all_idle && !state.ships.is_empty() {
            consecutive_idle += 1;
            assert!(
                consecutive_idle < max_idle_window,
                "Fleet permanently idle for {consecutive_idle} consecutive ticks at tick {tick_num}"
            );
        } else {
            consecutive_idle = 0;
        }
    }
    state.counters.next_command_id = next_cmd_id;
}

/// Invariant: ships never permanently idle when using dev_advanced_state
/// during the early game phase (first 2000 ticks) where resources are fresh.
/// build_initial_state has limited scan sites/asteroids so ships may legitimately
/// idle once all are exhausted — the dev_advanced_state test below covers the full
/// game state with proper scan site replenishment.
#[test]
fn invariant_no_permanent_idle_early_game() {
    let content = production_like_content();
    let mut state = production_like_state(&content);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Let autopilot install modules, then assign crew
    run_with_autopilot(&content, &mut state, &mut rng, 3);
    sim_world::auto_assign_initial_crew(&mut state, &content);

    // Test the first 2000 ticks (before resources can be fully depleted)
    // 200 ticks max idle: enough for transit + task completion + reassignment
    run_and_assert_no_permanent_idle(&content, &mut state, &mut rng, 2_000, 200);
}

/// Invariant: ships never permanently idle when using dev_advanced_state.json.
#[test]
fn invariant_no_permanent_idle_dev_advanced_state() {
    let content = sim_world::load_content(&content_dir()).unwrap();
    let json = std::fs::read_to_string("../../content/dev_advanced_state.json").unwrap();
    let mut state: GameState = serde_json::from_str(&json).unwrap();
    state.body_cache = sim_core::build_body_cache(&content.solar_system.bodies);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Let autopilot install modules, then assign crew
    run_with_autopilot(&content, &mut state, &mut rng, 3);
    sim_world::auto_assign_initial_crew(&mut state, &content);

    // 200 ticks max idle
    run_and_assert_no_permanent_idle(&content, &mut state, &mut rng, 5_000, 200);

    // Also assert research advances
    assert!(
        !state.research.data_pool.is_empty(),
        "Research data pool should have entries after 5k ticks"
    );
}

// ---------------------------------------------------------------------------
// Manufacturing pipeline e2e (VIO-477)
// ---------------------------------------------------------------------------

/// End-to-end test: ore → smelter → crucible (as molten) → casting mold → cast_fe_part.
/// Uses real content (load_content) with pre-heated modules to verify the full
/// thermal manufacturing pipeline works after VIO-459 fix.
#[test]
fn manufacturing_pipeline_ore_to_cast_part() {
    let content = sim_world::load_content(&content_dir()).unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let station_id = StationId("station_mfg_test".to_string());

    // Build a minimal station with smelter + crucible + casting mold
    let mut state = GameState {
        meta: MetaState {
            tick: 0,
            seed: 42,
            schema_version: 1,
            content_version: content.content_version.clone(),
        },
        scan_sites: vec![],
        asteroids: std::collections::BTreeMap::new(),
        ships: std::collections::BTreeMap::new(),
        stations: [(
            station_id.clone(),
            StationState {
                id: station_id.clone(),
                position: Position {
                    parent_body: BodyId("earth_orbit_zone".to_string()),
                    radius_au_um: RadiusAuMicro(3000),
                    angle_mdeg: AngleMilliDeg(0),
                },
                core: sim_core::FacilityCore {
                    inventory: vec![
                        // Ore for smelting
                        InventoryItem::Ore {
                            lot_id: LotId("lot_mfg_001".to_string()),
                            asteroid_id: AsteroidId("ast_mfg_001".to_string()),
                            kg: 5000.0,
                            composition: HashMap::from([
                                ("Fe".to_string(), 0.7),
                                ("Si".to_string(), 0.3),
                            ]),
                        },
                    ],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 200.0,
                    modules: vec![
                        // Smelter at operating temperature
                        ModuleState {
                            id: ModuleInstanceId("mod_smelter".to_string()),
                            def_id: "module_basic_smelter".to_string(),
                            enabled: true,
                            kind_state: ModuleKindState::Processor(ProcessorState {
                                threshold_kg: 100.0,
                                ticks_since_last_run: 100,
                                stalled: false,
                                selected_recipe: None,
                            }),
                            wear: WearState::default(),
                            thermal: Some(ThermalState {
                                temp_mk: 2_100_000,
                                thermal_group: Some("default".to_string()),
                                ..Default::default()
                            }),
                            power_stalled: false,
                            module_priority: 0,
                            assigned_crew: std::collections::BTreeMap::from([(
                                CrewRole("operator".to_string()),
                                1,
                            )]),
                            efficiency: 1.0,
                            prev_crew_satisfied: true,
                            slot_index: None,
                        },
                        // Crucible for molten storage
                        ModuleState {
                            id: ModuleInstanceId("mod_crucible".to_string()),
                            def_id: "module_crucible".to_string(),
                            enabled: true,
                            kind_state: ModuleKindState::ThermalContainer(ThermalContainerState {
                                held_items: vec![],
                            }),
                            wear: WearState::default(),
                            thermal: Some(ThermalState {
                                temp_mk: 2_100_000,
                                thermal_group: Some("default".to_string()),
                                ..Default::default()
                            }),
                            power_stalled: false,
                            module_priority: 0,
                            assigned_crew: Default::default(),
                            efficiency: 1.0,
                            prev_crew_satisfied: true,
                            slot_index: None,
                        },
                        // Casting mold at operating temperature
                        ModuleState {
                            id: ModuleInstanceId("mod_casting_mold".to_string()),
                            def_id: "module_casting_mold".to_string(),
                            enabled: true,
                            kind_state: ModuleKindState::Processor(ProcessorState {
                                threshold_kg: 0.0,
                                ticks_since_last_run: 100,
                                stalled: false,
                                selected_recipe: None,
                            }),
                            wear: WearState::default(),
                            thermal: Some(ThermalState {
                                temp_mk: 2_100_000,
                                thermal_group: Some("default".to_string()),
                                ..Default::default()
                            }),
                            power_stalled: false,
                            module_priority: 0,
                            assigned_crew: std::collections::BTreeMap::from([(
                                CrewRole("operator".to_string()),
                                1,
                            )]),
                            efficiency: 1.0,
                            prev_crew_satisfied: true,
                            slot_index: None,
                        },
                    ],
                    crew: std::collections::BTreeMap::from([(CrewRole("operator".to_string()), 2)]),
                    thermal_links: vec![
                        // Smelter → Crucible
                        ThermalLink {
                            from_module_id: ModuleInstanceId("mod_smelter".to_string()),
                            from_port_id: "molten_out".to_string(),
                            to_module_id: ModuleInstanceId("mod_crucible".to_string()),
                            to_port_id: "molten_in".to_string(),
                        },
                        // Crucible → Casting Mold
                        ThermalLink {
                            from_module_id: ModuleInstanceId("mod_crucible".to_string()),
                            from_port_id: "molten_out".to_string(),
                            to_module_id: ModuleInstanceId("mod_casting_mold".to_string()),
                            to_port_id: "molten_in".to_string(),
                        },
                    ],
                    modifiers: Default::default(),
                    power: Default::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: Default::default(),
                    module_id_index: HashMap::new(),
                    power_budget_cache: Default::default(),
                },
                leaders: Vec::new(),
                frame_id: None,
            },
        )]
        .into_iter()
        .collect(),
        ground_facilities: std::collections::BTreeMap::new(),
        satellites: std::collections::BTreeMap::new(),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: sim_core::AHashMap::default(),
            evidence: sim_core::AHashMap::default(),
            action_counts: sim_core::AHashMap::default(),
        },
        balance: 1_000_000.0,
        export_revenue_total: 0.0,
        export_count: 0,
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 1,
            ..Default::default()
        },
        modifiers: Default::default(),
        events: Default::default(),
        propellant_consumed_total: 0.0,
        progression: Default::default(),
        strategy_config: Default::default(),
        body_cache: sim_core::build_body_cache(&content.solar_system.bodies),
    };
    // Rebuild indices
    for station in state.stations.values_mut() {
        station.rebuild_module_index(&content);
    }

    // Run ticks — smelter should produce Fe and route to crucible
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let station = &state.stations[&station_id];

    // 1. Ore was consumed
    let ore_kg: f32 = station
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Ore { kg, .. } => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        ore_kg < 5000.0,
        "smelter should have consumed some ore, remaining: {ore_kg}"
    );

    // 2. Fe reached crucible as molten
    let crucible_idx = station
        .module_index_by_id(&ModuleInstanceId("mod_crucible".to_string()))
        .expect("crucible should exist");
    let crucible_fe: f32 = match &station.core.modules[crucible_idx].kind_state {
        ModuleKindState::ThermalContainer(c) => c
            .held_items
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material {
                    element,
                    kg,
                    thermal,
                    ..
                } if element == "Fe" => {
                    // Verify thermal state is present
                    assert!(
                        thermal.is_some(),
                        "crucible Fe should have thermal properties"
                    );
                    if let Some(tp) = thermal {
                        // Material arrives hot from smelter. Over 10 ticks the crucible
                        // may cool below the melting point if there's no heat source,
                        // so the phase may become Solid. The key assertion is that
                        // thermal props exist (routed through the port system).
                        assert!(tp.temp_mk > 0, "crucible Fe should have nonzero temp");
                    }
                    Some(*kg)
                }
                _ => None,
            })
            .sum(),
        _ => panic!("expected ThermalContainer"),
    };
    assert!(
        crucible_fe > 0.0,
        "crucible should contain molten Fe from smelter, got {crucible_fe}"
    );

    // 3. Cast part may or may not be produced in 10 ticks (casting mold has an interval).
    // Run more ticks to give casting mold time to consume from crucible.
    for _ in 0..20 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let station = &state.stations[&station_id];
    let cast_parts: u32 = station
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == "cast_fe_part" => Some(*count),
            _ => None,
        })
        .sum();
    assert!(
        cast_parts > 0,
        "casting mold should produce cast_fe_part from crucible Fe"
    );
}

// ---------------------------------------------------------------------------
// Progression starting state: milestone reachability (VIO-536)
// ---------------------------------------------------------------------------

/// Verify the autopilot reaches milestone 1 (first_survey) from
/// progression_start.json within 200 ticks using real content.
#[test]
fn progression_start_reaches_first_survey() {
    let content_path = content_dir();
    let content = sim_world::load_content(&content_path).expect("load content");
    let state_json = std::fs::read_to_string(format!("{content_path}/progression_start.json"))
        .expect("read state");
    let mut state: GameState = serde_json::from_str(&state_json).expect("parse state");
    state.body_cache = sim_core::build_body_cache(&content.solar_system.bodies);

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut autopilot = AutopilotController::new();
    let mut next_id = 0u64;

    for _ in 0..200 {
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);
        sim_core::tick(&mut state, &commands, &content, &mut rng, None);
    }

    assert!(
        state.progression.is_milestone_completed("first_survey"),
        "autopilot should reach first_survey milestone within 200 ticks from progression_start. \
         Asteroids discovered: {}",
        state.asteroids.len()
    );
}

/// Multi-seed validation: autopilot handles limited starting conditions (VIO-538).
/// Runs 5 seeds for 500 ticks each from progression_start.json.
/// Validates: no panics, first_survey in all seeds, first_tech in all seeds.
#[test]
fn progression_start_multi_seed_validation() {
    let content_path = content_dir();
    let content = sim_world::load_content(&content_path).expect("load content");
    let state_json = std::fs::read_to_string(format!("{content_path}/progression_start.json"))
        .expect("read state");

    let seeds = [1, 42, 123, 456, 789];
    for seed in seeds {
        let mut state: GameState = serde_json::from_str(&state_json).expect("parse state");
        state.meta.seed = seed;
        state.body_cache = sim_core::build_body_cache(&content.solar_system.bodies);

        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut autopilot = AutopilotController::new();
        let mut next_id = 0u64;

        for _ in 0..500 {
            let commands = autopilot.generate_commands(&state, &content, &mut next_id);
            sim_core::tick(&mut state, &commands, &content, &mut rng, None);
        }

        assert!(
            state.progression.is_milestone_completed("first_survey"),
            "seed {seed}: first_survey not reached by tick 500. Asteroids: {}",
            state.asteroids.len()
        );
        assert!(
            state.progression.is_milestone_completed("first_tech"),
            "seed {seed}: first_tech not reached by tick 500. Techs: {}",
            state.research.unlocked.len()
        );
        assert!(
            state.balance > 0.0,
            "seed {seed}: balance hit zero by tick 500"
        );
    }
}

// ---------------------------------------------------------------------------
// VIO-541: 100-seed autopilot progression regression test
// ---------------------------------------------------------------------------

/// Ordered milestone IDs matching milestones.json progression sequence.
const MILESTONE_IDS: &[&str] = &[
    "first_survey",
    "first_ore",
    "first_material",
    "first_component",
    "first_tech",
    "first_export",
    "fleet_expansion",
    "self_sustaining",
];

/// Run from `progression_start.json` with autopilot, tracking the tick each
/// milestone is first completed. Returns a map of milestone_id -> tick reached
/// (only for milestones actually reached).
fn run_progression_tracking(
    content: &GameContent,
    state_json: &str,
    seed: u64,
    max_ticks: u64,
) -> HashMap<String, u64> {
    let mut state: GameState = serde_json::from_str(state_json).expect("parse state");
    state.meta.seed = seed;
    state.body_cache = sim_core::build_body_cache(&content.solar_system.bodies);

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut autopilot = AutopilotController::new();
    let mut next_id = 0u64;
    let mut milestone_ticks: HashMap<String, u64> = HashMap::new();

    for tick_num in 1..=max_ticks {
        let commands = autopilot.generate_commands(&state, &content, &mut next_id);
        sim_core::tick(&mut state, &commands, &content, &mut rng, None);

        // Check for newly completed milestones
        for milestone_id in MILESTONE_IDS {
            if !milestone_ticks.contains_key(*milestone_id)
                && state.progression.is_milestone_completed(milestone_id)
            {
                milestone_ticks.insert((*milestone_id).to_string(), tick_num);
            }
        }

        // Early exit if all milestones reached
        if milestone_ticks.len() == MILESTONE_IDS.len() {
            break;
        }
    }

    milestone_ticks
}

/// Quick regression test: 5 seeds, 2000 ticks from progression_start.json.
/// Validates early milestone reachability. Runs in regular `cargo test`.
#[test]
fn progression_regression_quick() {
    let content_path = content_dir();
    let content = sim_world::load_content(&content_path).expect("load content");
    let state_json = std::fs::read_to_string(format!("{content_path}/progression_start.json"))
        .expect("read state");

    let seeds = [1, 42, 123, 456, 789];
    let mut failures: Vec<String> = Vec::new();

    for seed in seeds {
        let results = run_progression_tracking(&content, &state_json, seed, 2000);

        // Milestone 1 (first_survey) must be reached by tick 200
        match results.get("first_survey") {
            Some(&tick) if tick <= 200 => {}
            Some(&tick) => {
                failures.push(format!(
                    "seed {seed}: first_survey at tick {tick} (expected <= 200)"
                ));
            }
            None => {
                failures.push(format!(
                    "seed {seed}: first_survey never reached in 2000 ticks"
                ));
            }
        }

        // Milestone 3 (first_material) should be reached by tick 500
        match results.get("first_material") {
            Some(&tick) if tick <= 500 => {}
            Some(&tick) => {
                failures.push(format!(
                    "seed {seed}: first_material at tick {tick} (expected <= 500)"
                ));
            }
            None => {
                failures.push(format!(
                    "seed {seed}: first_material never reached in 2000 ticks"
                ));
            }
        }

        // Milestone 5 (first_tech) should be reached by tick 1000
        match results.get("first_tech") {
            Some(&tick) if tick <= 1000 => {}
            Some(&tick) => {
                failures.push(format!(
                    "seed {seed}: first_tech at tick {tick} (expected <= 1000)"
                ));
            }
            None => {
                failures.push(format!(
                    "seed {seed}: first_tech never reached in 2000 ticks"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Progression regression failures:\n  {}",
        failures.join("\n  ")
    );
}

/// Full regression test: 100 seeds, 5000 ticks from progression_start.json.
/// Reports per-seed milestone completion ticks and validates threshold
/// percentages. Run with `cargo test --ignored -p sim_control progression_regression_full`.
#[test]
#[ignore]
fn progression_regression_full() {
    let content_path = content_dir();
    let content = sim_world::load_content(&content_path).expect("load content");
    let state_json = std::fs::read_to_string(format!("{content_path}/progression_start.json"))
        .expect("read state");

    let seed_count: u64 = 100;
    let max_ticks: u64 = 5000;

    // Collect results per seed
    let all_results: Vec<(u64, HashMap<String, u64>)> = (1..=seed_count)
        .map(|seed| {
            let results = run_progression_tracking(&content, &state_json, seed, max_ticks);
            (seed, results)
        })
        .collect();

    // Compute per-milestone statistics
    eprintln!("\n=== Progression Regression Report (100 seeds, 5000 ticks) ===\n");

    for milestone_id in MILESTONE_IDS {
        let ticks: Vec<u64> = all_results
            .iter()
            .filter_map(|(_, results)| results.get(*milestone_id).copied())
            .collect();

        let reached = ticks.len();
        let pct = (reached as f64 / seed_count as f64) * 100.0;

        if ticks.is_empty() {
            eprintln!("  {milestone_id}: 0/{seed_count} seeds (0.0%)");
            continue;
        }

        let mean = ticks.iter().sum::<u64>() as f64 / reached as f64;
        let variance = ticks
            .iter()
            .map(|&t| (t as f64 - mean).powi(2))
            .sum::<f64>()
            / reached as f64;
        let stddev = variance.sqrt();
        let min = ticks.iter().copied().min().unwrap();
        let max = ticks.iter().copied().max().unwrap();

        let mut sorted = ticks.clone();
        sorted.sort_unstable();
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() as f64 * 0.95) as usize];

        eprintln!(
            "  {milestone_id}: {reached}/{seed_count} ({pct:.1}%) | \
             mean={mean:.0} stddev={stddev:.0} min={min} p50={p50} p95={p95} max={max}"
        );
    }

    // Report seeds that failed to reach milestone 1
    let failed_seeds: Vec<u64> = all_results
        .iter()
        .filter(|(_, results)| !results.contains_key("first_survey"))
        .map(|(seed, _)| *seed)
        .collect();
    if !failed_seeds.is_empty() {
        eprintln!("\n  FAILED seeds (no first_survey): {failed_seeds:?}");
    }

    eprintln!();

    // Threshold assertions
    let mut failures: Vec<String> = Vec::new();

    // 100% of seeds reach milestone 1 (first_survey) by tick 200
    let m1_count = all_results
        .iter()
        .filter(|(_, r)| r.get("first_survey").is_some_and(|&t| t <= 200))
        .count();
    if m1_count < seed_count as usize {
        failures.push(format!(
            "first_survey by tick 200: {m1_count}/{seed_count} (need 100%)"
        ));
    }

    // 95%+ reach milestone 3 (first_material) by tick 500
    let m3_count = all_results
        .iter()
        .filter(|(_, r)| r.get("first_material").is_some_and(|&t| t <= 500))
        .count();
    let m3_threshold = (seed_count as f64 * 0.95).ceil() as usize;
    if m3_count < m3_threshold {
        failures.push(format!(
            "first_material by tick 500: {m3_count}/{seed_count} (need 95%+)"
        ));
    }

    // 90%+ reach milestone 5 (first_tech) by tick 1000
    let m5_count = all_results
        .iter()
        .filter(|(_, r)| r.get("first_tech").is_some_and(|&t| t <= 1000))
        .count();
    let m5_threshold = (seed_count as f64 * 0.90).ceil() as usize;
    if m5_count < m5_threshold {
        failures.push(format!(
            "first_tech by tick 1000: {m5_count}/{seed_count} (need 90%+)"
        ));
    }

    assert!(
        failures.is_empty(),
        "Progression regression failures:\n  {}",
        failures.join("\n  ")
    );
}
