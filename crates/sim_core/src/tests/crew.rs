//! Crew system tests (VIO-387).
//!
//! Tests crew satisfaction, assignment commands, transition events,
//! trade import, and determinism.

use super::*;
use crate::test_fixtures::{base_content, base_state, test_station_id, ModuleDefBuilder};
use std::collections::BTreeMap;

/// Helper: create content with a processor module that requires 1 operator.
fn crew_content() -> GameContent {
    let mut content = base_content();
    let recipe_id = insert_recipe(
        &mut content,
        RecipeDef {
            id: RecipeId("recipe_crew_test".to_string()),
            inputs: vec![RecipeInput {
                filter: InputFilter::Element("Fe".to_string()),
                amount: InputAmount::Kg(100.0),
            }],
            outputs: vec![OutputSpec::Material {
                element: "Fe".to_string(),
                yield_formula: YieldFormula::FixedFraction(0.9),
                quality_formula: QualityFormula::Fixed(1.0),
            }],
            efficiency: 1.0,
            thermal_req: None,
            required_tech: None,
            tags: vec![],
        },
    );
    content.module_defs.insert(
        "module_crew_processor".to_string(),
        ModuleDefBuilder::new("module_crew_processor")
            .name("Crew Processor")
            .mass(1000.0)
            .volume(5.0)
            .power(10.0)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 1,
                processing_interval_ticks: 1,
                recipes: vec![recipe_id],
            }))
            .crew("operator", 1)
            .build(),
    );
    content.crew_roles.insert(
        CrewRole("operator".to_string()),
        CrewRoleDef {
            id: CrewRole("operator".to_string()),
            name: "Operator".to_string(),
            recruitment_cost: 50000.0,
        },
    );
    content
}

/// Helper: create state with the crew processor installed and Fe in inventory.
fn crew_state(content: &GameContent, crew_count: u32) -> GameState {
    let mut state = base_state(content);
    let station_id = test_station_id();
    let station = state.stations.get_mut(&station_id).unwrap();
    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_crew_proc".to_string()),
        def_id: "module_crew_processor".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
            selected_recipe: None,
        }),
        wear: WearState::default(),
        thermal: None,
        power_stalled: false,
        module_priority: 0,
        assigned_crew: if crew_count > 0 {
            BTreeMap::from([(CrewRole("operator".to_string()), 1)])
        } else {
            BTreeMap::new()
        },
        crew_satisfied: crew_count > 0,
    });
    station
        .crew
        .insert(CrewRole("operator".to_string()), crew_count);
    station.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 10000.0,
        quality: 1.0,
        thermal: None,
    });
    station.rebuild_module_index(content);
    state
}

#[test]
fn staffed_module_runs() {
    let content = crew_content();
    let mut state = crew_state(&content, 1);
    let mut rng = make_rng();

    // Run 5 ticks — module should process since it has crew
    for _ in 0..5 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let station = &state.stations[&test_station_id()];
    // Fe was consumed (started at 10000, processor runs each tick)
    let fe_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        fe_kg < 10000.0,
        "staffed processor should consume Fe. remaining: {fe_kg}"
    );
}

#[test]
fn understaffed_module_skips() {
    let content = crew_content();
    let mut state = crew_state(&content, 0); // no crew assigned
    let mut rng = make_rng();

    for _ in 0..5 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let station = &state.stations[&test_station_id()];
    let fe_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        (fe_kg - 10000.0).abs() < 1.0,
        "understaffed processor should NOT consume Fe. remaining: {fe_kg}"
    );
}

#[test]
fn empty_crew_requirement_always_satisfied() {
    let content = base_content(); // modules have no crew_requirement
    let mut state = base_state(&content);
    let station = state.stations.get_mut(&test_station_id()).unwrap();

    // Add a no-crew processor
    station.modules.push(ModuleState {
        id: ModuleInstanceId("mod_no_crew".to_string()),
        def_id: "module_basic_iron_refinery".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 0.0,
            ticks_since_last_run: 0,
            stalled: false,
            selected_recipe: None,
        }),
        wear: WearState::default(),
        thermal: None,
        power_stalled: false,
        module_priority: 0,
        assigned_crew: BTreeMap::new(),
        crew_satisfied: true,
    });
    // base_content modules have empty crew_requirement — always satisfied
    assert!(is_crew_satisfied(&BTreeMap::new(), &BTreeMap::new()));
}

#[test]
fn assign_crew_command_works() {
    let content = crew_content();
    let mut state = crew_state(&content, 0);
    // Station has 0 operators but the state says crew roster has 0.
    // Add crew to roster without assigning
    state
        .stations
        .get_mut(&test_station_id())
        .unwrap()
        .crew
        .insert(CrewRole("operator".to_string()), 2);

    let mut rng = make_rng();
    let cmd = Command::AssignCrew {
        station_id: test_station_id(),
        module_id: ModuleInstanceId("mod_crew_proc".to_string()),
        role: CrewRole("operator".to_string()),
        count: 1,
    };
    let events = tick(
        &mut state,
        &[CommandEnvelope {
            id: CommandId(0),
            issued_by: PrincipalId("test".to_string()),
            issued_tick: 0,
            execute_at_tick: 0,
            command: cmd,
        }],
        &content,
        &mut rng,
        None,
    );

    // Should have CrewAssigned event
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::CrewAssigned { .. })),
        "expected CrewAssigned event"
    );
    // Module should have 1 operator assigned
    let module = &state.stations[&test_station_id()].modules[0];
    assert_eq!(
        module.assigned_crew.get(&CrewRole("operator".to_string())),
        Some(&1)
    );
}

#[test]
fn assign_crew_fails_when_none_available() {
    let content = crew_content();
    let mut state = crew_state(&content, 0);
    // Station has 0 operators in crew roster

    let mut rng = make_rng();
    let cmd = Command::AssignCrew {
        station_id: test_station_id(),
        module_id: ModuleInstanceId("mod_crew_proc".to_string()),
        role: CrewRole("operator".to_string()),
        count: 1,
    };
    let events = tick(
        &mut state,
        &[CommandEnvelope {
            id: CommandId(0),
            issued_by: PrincipalId("test".to_string()),
            issued_tick: 0,
            execute_at_tick: 0,
            command: cmd,
        }],
        &content,
        &mut rng,
        None,
    );

    // Should NOT have CrewAssigned event (no crew available)
    assert!(
        !events
            .iter()
            .any(|e| matches!(&e.event, Event::CrewAssigned { .. })),
        "should not assign when no crew available"
    );
}

#[test]
fn crew_import_via_trade() {
    let mut content = crew_content();
    // Add pricing for operator crew
    content.pricing.items.insert(
        "operator".to_string(),
        PricingEntry {
            base_price_per_unit: 50000.0,
            importable: true,
            exportable: false,
            category: "crew".to_string(),
        },
    );

    content.constants.trade_unlock_delay_minutes = 0;
    let mut state = base_state(&content);
    state.balance = 1_000_000.0;
    let mut rng = make_rng();

    let cmd = Command::Import {
        station_id: test_station_id(),
        item_spec: TradeItemSpec::Crew {
            role: CrewRole("operator".to_string()),
            count: 2,
        },
    };
    let events = tick(
        &mut state,
        &[CommandEnvelope {
            id: CommandId(0),
            issued_by: PrincipalId("test".to_string()),
            issued_tick: 0,
            execute_at_tick: 0,
            command: cmd,
        }],
        &content,
        &mut rng,
        None,
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::ItemImported { .. })),
        "expected ItemImported event"
    );
    let station = &state.stations[&test_station_id()];
    assert_eq!(
        station.crew.get(&CrewRole("operator".to_string())),
        Some(&2),
        "station should have 2 operators after import"
    );
    assert!(
        state.balance < 1_000_000.0,
        "balance should be reduced after crew import"
    );
}

#[test]
fn crew_determinism() {
    let content = crew_content();

    let run = |seed: u64| -> GameState {
        let mut state = crew_state(&content, 1);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..20 {
            tick(&mut state, &[], &content, &mut rng, None);
        }
        state
    };

    let state_a = run(42);
    let state_b = run(42);

    let station_a = &state_a.stations[&test_station_id()];
    let station_b = &state_b.stations[&test_station_id()];
    assert_eq!(station_a.crew, station_b.crew, "crew rosters should match");
    assert_eq!(
        station_a.modules[0].assigned_crew, station_b.modules[0].assigned_crew,
        "assigned crew should match"
    );
    assert_eq!(
        station_a.modules[0].crew_satisfied, station_b.modules[0].crew_satisfied,
        "crew_satisfied should match"
    );
}

#[test]
fn available_crew_computation() {
    let content = crew_content();
    let state = crew_state(&content, 3); // 3 operators on station, 1 assigned to module
    let station = &state.stations[&test_station_id()];
    assert_eq!(station.available_crew(&CrewRole("operator".to_string())), 2);
    assert_eq!(
        station.available_crew(&CrewRole("technician".to_string())),
        0
    );
}
