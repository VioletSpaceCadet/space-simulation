use super::*;

fn electrolysis_content() -> GameContent {
    let mut content = test_content();
    // Add H2O, LH2, LOX elements
    content.elements.push(ElementDef {
        id: "H2O".to_string(),
        density_kg_per_m3: 1000.0,
        display_name: "Water".to_string(),
        refined_name: Some("Water".to_string()),
        category: "material".to_string(),
        melting_point_mk: None,
        latent_heat_j_per_kg: None,
        specific_heat_j_per_kg_k: None,
        boiloff_rate_per_day_at_293k: None,
        boiling_point_mk: None,
    });
    content.elements.push(ElementDef {
        id: "LH2".to_string(),
        density_kg_per_m3: 71.0,
        display_name: "Liquid Hydrogen".to_string(),
        refined_name: Some("LH2".to_string()),
        category: "material".to_string(),
        melting_point_mk: None,
        latent_heat_j_per_kg: None,
        specific_heat_j_per_kg_k: None,
        boiloff_rate_per_day_at_293k: None,
        boiling_point_mk: None,
    });
    content.elements.push(ElementDef {
        id: "LOX".to_string(),
        density_kg_per_m3: 1141.0,
        display_name: "Liquid Oxygen".to_string(),
        refined_name: Some("LOX".to_string()),
        category: "material".to_string(),
        melting_point_mk: None,
        latent_heat_j_per_kg: None,
        specific_heat_j_per_kg_k: None,
        boiloff_rate_per_day_at_293k: None,
        boiling_point_mk: None,
    });

    content.module_defs.insert(
        "module_electrolysis_unit".to_string(),
        ModuleDef {
            id: "module_electrolysis_unit".to_string(),
            name: "Electrolysis Unit".to_string(),
            mass_kg: 800.0,
            volume_m3: 3.0,
            power_consumption_per_run: 25.0,
            wear_per_run: 0.012,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 1,
                processing_interval_ticks: 1,
                recipes: vec![RecipeDef {
                    id: "recipe_electrolysis".to_string(),
                    inputs: vec![RecipeInput {
                        filter: InputFilter::Element("H2O".to_string()),
                        amount: InputAmount::Kg(1000.0),
                    }],
                    outputs: vec![
                        OutputSpec::Material {
                            element: "LH2".to_string(),
                            yield_formula: YieldFormula::FixedFraction(0.111),
                            quality_formula: QualityFormula::Fixed(1.0),
                        },
                        OutputSpec::Material {
                            element: "LOX".to_string(),
                            yield_formula: YieldFormula::FixedFraction(0.889),
                            quality_formula: QualityFormula::Fixed(1.0),
                        },
                    ],
                    efficiency: 1.0,
                    thermal_req: None,
                }],
            }),
            thermal: None,
        },
    );

    // Solar array for power
    content.module_defs.insert(
        "module_basic_solar_array".to_string(),
        ModuleDef {
            id: "module_basic_solar_array".to_string(),
            name: "Basic Solar Array".to_string(),
            mass_kg: 1500.0,
            volume_m3: 12.0,
            power_consumption_per_run: 0.0,
            wear_per_run: 0.002,
            behavior: ModuleBehaviorDef::SolarArray(SolarArrayDef {
                base_output_kw: 50.0,
            }),
            thermal: None,
        },
    );

    content
}

fn state_with_electrolysis(content: &GameContent) -> GameState {
    let mut state = test_state(content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    // Solar array for power (50 kW > 25 kW electrolysis)
    station.modules.push(ModuleState {
        id: ModuleInstanceId("solar_inst_0001".to_string()),
        def_id: "module_basic_solar_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SolarArray(SolarArrayState::default()),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    // Electrolysis unit
    station.modules.push(ModuleState {
        id: ModuleInstanceId("electrolysis_inst_0001".to_string()),
        def_id: "module_electrolysis_unit".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 200.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    // H2O Material in station inventory
    station.inventory.push(InventoryItem::Material {
        element: "H2O".to_string(),
        kg: 5000.0,
        quality: 1.0,
        thermal: None,
    });

    state
}

#[test]
fn test_electrolysis_produces_lh2_and_lox() {
    let content = electrolysis_content();
    let mut state = state_with_electrolysis(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let lh2_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum();

    let lox_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LOX" => Some(*kg),
            _ => None,
        })
        .sum();

    assert!(
        lh2_kg > 0.0,
        "should produce LH2 from electrolysis, got {lh2_kg}"
    );
    assert!(
        lox_kg > 0.0,
        "should produce LOX from electrolysis, got {lox_kg}"
    );
}

#[test]
fn test_electrolysis_stoichiometric_ratio() {
    let content = electrolysis_content();
    let mut state = state_with_electrolysis(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let lh2_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum();

    let lox_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LOX" => Some(*kg),
            _ => None,
        })
        .sum();

    // Stoichiometric ratio: LH2 ~11.1%, LOX ~88.9%
    let total = lh2_kg + lox_kg;
    assert!(total > 0.0, "should have produced propellant");
    let lh2_fraction = lh2_kg / total;
    assert!(
        (lh2_fraction - 0.111).abs() < 0.02,
        "LH2 fraction should be ~11.1%, got {:.1}%",
        lh2_fraction * 100.0
    );
}

#[test]
fn test_electrolysis_consumes_h2o() {
    let content = electrolysis_content();
    let mut state = state_with_electrolysis(&content);
    let mut rng = make_rng();

    let station_id = StationId("station_earth_orbit".to_string());
    let initial_h2o: f32 = state.stations[&station_id]
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "H2O" => Some(*kg),
            _ => None,
        })
        .sum();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let remaining_h2o: f32 = state.stations[&station_id]
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "H2O" => Some(*kg),
            _ => None,
        })
        .sum();

    assert!(
        remaining_h2o < initial_h2o,
        "H2O should be consumed: started {initial_h2o}, remaining {remaining_h2o}"
    );
}

#[test]
fn test_electrolysis_accumulates_wear() {
    let content = electrolysis_content();
    let mut state = state_with_electrolysis(&content);
    let mut rng = make_rng();

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let electrolysis = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_electrolysis_unit")
        .expect("electrolysis module should exist");

    assert!(
        electrolysis.wear.wear > 0.0,
        "wear should accumulate after processing, got {}",
        electrolysis.wear.wear
    );
}

#[test]
fn test_electrolysis_skips_without_h2o() {
    let content = electrolysis_content();
    let mut state = state_with_electrolysis(&content);
    let mut rng = make_rng();

    let station_id = StationId("station_earth_orbit".to_string());

    // Remove all H2O from inventory
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .retain(|i| !matches!(i, InventoryItem::Material { element, .. } if element == "H2O"));

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];

    // No LH2 or LOX produced
    let lh2_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        lh2_kg < 0.01,
        "no LH2 should be produced without H2O, got {lh2_kg}"
    );

    // No wear accumulated (processor didn't run)
    let electrolysis = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_electrolysis_unit")
        .expect("electrolysis module should exist");
    assert!(
        electrolysis.wear.wear < 0.001,
        "no wear should accumulate when processor has no input, got {}",
        electrolysis.wear.wear
    );
}

#[test]
fn test_electrolysis_skips_without_power() {
    let content = electrolysis_content();
    let mut state = state_with_electrolysis(&content);
    let mut rng = make_rng();

    let station_id = StationId("station_earth_orbit".to_string());

    // Set power budget to 0 — insufficient for 25 kW electrolysis
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .power_available_per_tick = 0.0;

    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

    let station = &state.stations[&station_id];

    // No products
    let lh2_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        lh2_kg < 0.01,
        "no LH2 should be produced without power, got {lh2_kg}"
    );

    // No wear accumulated (processor didn't run)
    let electrolysis = station
        .modules
        .iter()
        .find(|m| m.def_id == "module_electrolysis_unit")
        .expect("electrolysis module should exist");
    assert!(
        electrolysis.wear.wear < 0.001,
        "no wear should accumulate without power, got {}",
        electrolysis.wear.wear
    );
}

#[test]
fn test_electrolysis_continuous_production() {
    let content = electrolysis_content();
    let mut state = state_with_electrolysis(&content);
    let mut rng = make_rng();

    // Run 10 ticks
    for _ in 0..10 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let station_id = StationId("station_earth_orbit".to_string());
    let station = &state.stations[&station_id];

    let lh2_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum();

    let lox_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LOX" => Some(*kg),
            _ => None,
        })
        .sum();

    // 5000 kg H2O, 1000 kg/tick consumed → 5 ticks of production
    // Each tick: 111 kg LH2 + 889 kg LOX
    assert!(
        lh2_kg > 400.0,
        "should have accumulated significant LH2 over 10 ticks, got {lh2_kg}"
    );
    assert!(
        lox_kg > 3000.0,
        "should have accumulated significant LOX over 10 ticks, got {lox_kg}"
    );
}

#[test]
fn test_full_chain_ore_to_propellant() {
    let mut content = electrolysis_content();

    // Add heating unit for ore → H2O
    content.module_defs.insert(
        "module_heating_unit".to_string(),
        ModuleDef {
            id: "module_heating_unit".to_string(),
            name: "Heating Unit".to_string(),
            mass_kg: 500.0,
            volume_m3: 2.0,
            power_consumption_per_run: 15.0,
            wear_per_run: 0.01,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 1,
                processing_interval_ticks: 1,
                recipes: vec![RecipeDef {
                    id: "recipe_extract_water".to_string(),
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
                }],
            }),
            thermal: None,
        },
    );

    let mut state = test_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());
    let station = state.stations.get_mut(&station_id).unwrap();

    // Solar array
    station.modules.push(ModuleState {
        id: ModuleInstanceId("solar_inst_0001".to_string()),
        def_id: "module_basic_solar_array".to_string(),
        enabled: true,
        kind_state: ModuleKindState::SolarArray(SolarArrayState::default()),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    // Heating unit (ore → H2O)
    station.modules.push(ModuleState {
        id: ModuleInstanceId("heating_inst_0001".to_string()),
        def_id: "module_heating_unit".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 100.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    // Electrolysis unit (H2O → LH2 + LOX)
    station.modules.push(ModuleState {
        id: ModuleInstanceId("electrolysis_inst_0001".to_string()),
        def_id: "module_electrolysis_unit".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 200.0,
            ticks_since_last_run: 0,
            stalled: false,
        }),
        wear: WearState::default(),
        power_stalled: false,
        thermal: None,
    });

    // Ice-rich ore with 50% H2O
    station.inventory.push(InventoryItem::Ore {
        lot_id: LotId("lot_ice_001".to_string()),
        asteroid_id: AsteroidId("asteroid_ice".to_string()),
        kg: 10000.0,
        composition: HashMap::from([
            ("H2O".to_string(), 0.5f32),
            ("Fe".to_string(), 0.2f32),
            ("Si".to_string(), 0.3f32),
        ]),
    });

    let mut rng = make_rng();

    // Run enough ticks for the full pipeline: ore → H2O → LH2 + LOX
    for _ in 0..20 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    let station = &state.stations[&station_id];

    let lh2_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LH2" => Some(*kg),
            _ => None,
        })
        .sum();

    let lox_kg: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "LOX" => Some(*kg),
            _ => None,
        })
        .sum();

    assert!(
        lh2_kg > 0.0,
        "full chain should produce LH2 from ore, got {lh2_kg}"
    );
    assert!(
        lox_kg > 0.0,
        "full chain should produce LOX from ore, got {lox_kg}"
    );

    // Verify some ore was consumed
    let remaining_ore: f32 = station
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Ore { kg, .. } => Some(*kg),
            _ => None,
        })
        .sum();
    assert!(
        remaining_ore < 10000.0,
        "ore should be consumed by heating unit, remaining {remaining_ore}"
    );
}
