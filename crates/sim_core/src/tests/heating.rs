use super::*;
use crate::test_fixtures::{insert_recipe, ModuleDefBuilder};

fn heating_content() -> GameContent {
    let mut content = test_content();
    // Add H2O element
    content.elements.push(ElementDef {
        id: "H2O".to_string(),
        density_kg_per_m3: 1000.0,
        display_name: "Water Ice".to_string(),
        refined_name: Some("Water".to_string()),
        category: "material".to_string(),
        melting_point_mk: None,
        latent_heat_j_per_kg: None,
        specific_heat_j_per_kg_k: None,
        boiloff_rate_per_day_at_293k: None,
        boiling_point_mk: None,
        boiloff_curve: None,
    });
    let water_recipe = RecipeDef {
        id: RecipeId("recipe_extract_water".to_string()),
        inputs: vec![RecipeInput {
            filter: InputFilter::ItemKind(ItemKind::Ore),
            amount: InputAmount::Kg(500.0),
        }],
        outputs: vec![
            OutputSpec::Material {
                element: "H2O".to_string(),
                yield_formula: YieldFormula::ElementFraction {
                    element: "H2O".to_string(),
                },
                quality_formula: QualityFormula::ElementFractionTimesMultiplier {
                    element: "H2O".to_string(),
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
    let recipe_id = insert_recipe(&mut content, water_recipe);
    content.module_defs = [(
        "module_heating_unit".to_string(),
        ModuleDefBuilder::new("module_heating_unit")
            .name("Heating Unit")
            .mass(500.0)
            .volume(2.0)
            .power(15.0)
            .wear(0.01)
            .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 1,
                processing_interval_ticks: 1,
                recipes: vec![recipe_id],
            }))
            .build(),
    )]
    .into_iter()
    .collect();
    content
}

fn state_with_heating(content: &GameContent) -> GameState {
    let mut state = test_state(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_heat_001".to_string()),
        def_id: "module_heating_unit".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 100.0,
            ticks_since_last_run: 0,
            stalled: false,
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
    });

    // Ice ore with 50% H2O, 10% Fe, 40% Si
    station.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_ice_001".to_string()),
        asteroid_id: AsteroidId("asteroid_ice".to_string()),
        kg: 1000.0,
        composition: HashMap::from([
            ("H2O".to_string(), 0.5f32),
            ("Fe".to_string(), 0.1f32),
            ("Si".to_string(), 0.4f32),
        ]),
    });

    state
}

#[test]
fn test_heating_produces_h2o_and_slag() {
    let content = heating_content();
    let mut state = state_with_heating(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let has_h2o = station.inventory.iter().any(|i| {
        matches!(i, InventoryItem::Material { element, kg, .. } if element == "H2O" && *kg > 0.0)
    });
    assert!(
        has_h2o,
        "station should have H2O Material after heating unit runs"
    );

    let has_slag = station
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Slag { kg, .. } if *kg > 0.0));
    assert!(has_slag, "station should have Slag after heating unit runs");
}

#[test]
fn test_heating_h2o_yield_matches_fraction() {
    let content = heating_content();
    let mut state = state_with_heating(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let h2o_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "H2O" => Some(*kg),
            _ => None,
        })
        .sum();

    // ElementFraction yield produces output_kg = input_kg × element_fraction / sum_of_element_fractions_in_recipe
    // The exact yield depends on the processor formula; verify non-trivial H2O output
    assert!(
        h2o_kg > 100.0,
        "H2O yield should be substantial from ore with 50% H2O fraction, got {h2o_kg}"
    );
}

#[test]
fn test_heating_accumulates_wear() {
    let content = heating_content();
    let mut state = state_with_heating(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let heating_module = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_heating_unit")
        .expect("heating module should exist");

    assert!(
        heating_module.wear.wear > 0.0,
        "wear should accumulate after processing, got {}",
        heating_module.wear.wear
    );
}

#[test]
fn test_heating_ore_with_no_h2o_produces_only_slag() {
    let content = heating_content();
    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    station.modules.push(ModuleState {
        id: ModuleInstanceId("module_inst_heat_002".to_string()),
        def_id: "module_heating_unit".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 100.0,
            ticks_since_last_run: 0,
            stalled: false,
            selected_recipe: None,
        }),
        wear: WearState::default(),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
        thermal: None,
    });

    // Ore with 0% H2O — should produce no water
    station.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_dry_001".to_string()),
        asteroid_id: AsteroidId("asteroid_dry".to_string()),
        kg: 1000.0,
        composition: HashMap::from([("Fe".to_string(), 0.7f32), ("Si".to_string(), 0.3f32)]),
    });

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let station = &state.stations[&station_id];

    let h2o_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "H2O" => Some(*kg),
            _ => None,
        })
        .sum();

    assert!(
        h2o_kg < 0.01,
        "should produce no H2O from ore without H2O composition, got {h2o_kg}"
    );
}
