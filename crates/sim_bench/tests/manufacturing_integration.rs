//! Manufacturing DAG integration tests with real content (VIO-374).
//!
//! These tests use `load_content` and `AutopilotController` to validate:
//! - 4-tier manufacturing chain (ore → Fe → fe_plate → structural_beam → hull_panel)
//! - Competing demand with priority using real content
//! - Determinism regression (same seed = identical state)

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_control::{AutopilotController, CommandSource};
use sim_core::{
    AsteroidId, ComponentId, GameState, InventoryItem, LotId, ModuleKindState, RecipeId, StationId,
    TechId,
};
use std::collections::HashMap;

/// Helper: resolve the content directory relative to the workspace root.
fn content_dir() -> String {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    format!("{manifest}/../../content")
}

/// Helper: count components of a given ID in a station's inventory.
fn component_count(state: &GameState, station_id: &StationId, component_id_str: &str) -> u32 {
    state.stations[station_id]
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == component_id_str => Some(*count),
            _ => None,
        })
        .sum()
}

/// Helper: total kg of a material element in a station's inventory.
fn material_kg(state: &GameState, station_id: &StationId, element_id: &str) -> f32 {
    state.stations[station_id]
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Material { element, kg, .. } if element == element_id => Some(*kg),
            _ => None,
        })
        .sum()
}

/// Run the simulation for N ticks with autopilot, returning the final state.
fn run_with_autopilot(
    state: &mut GameState,
    content: &sim_core::GameContent,
    rng: &mut ChaCha8Rng,
    ticks: u64,
    next_cmd_id: &mut u64,
    autopilot: &mut AutopilotController,
) {
    for _ in 0..ticks {
        let commands = autopilot.generate_commands(state, content, next_cmd_id);
        sim_core::tick(state, &commands, content, rng, None);
    }
}

// =========================================================================
// 4-Tier Manufacturing Chain
// =========================================================================

/// Validates the full manufacturing chain with real content:
/// ore → Fe (refinery) → fe_plate (plate press) → structural_beam → hull_panel
#[test]
fn four_tier_manufacturing_chain_with_real_content() {
    let content = sim_world::load_content(&content_dir()).unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut state = sim_world::build_initial_state(&content, 42, &mut rng);
    let station_id = StationId("station_earth_orbit".to_string());

    let mut autopilot = AutopilotController::new();
    let mut next_cmd_id = 0u64;

    // Seed the station with ore for the refinery to process
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Ore {
            lot_id: LotId("lot_chain_test_001".to_string()),
            asteroid_id: AsteroidId("asteroid_chain_test".to_string()),
            kg: 50_000.0,
            composition: HashMap::from([("Fe".to_string(), 0.7), ("Si".to_string(), 0.3)]),
        });

    // Phase 1: Let autopilot install modules and start Tier 1+2 processing.
    // With real content: minutes_per_tick=60.
    // Refinery: 1000kg ore → Fe per tick (60min interval = 1 tick).
    // Plate press: 500kg Fe → 1 fe_plate per tick (60min interval = 1 tick).
    run_with_autopilot(
        &mut state,
        &content,
        &mut rng,
        30,
        &mut next_cmd_id,
        &mut autopilot,
    );

    // Tier 1+2: Verify Fe was refined and fe_plates were pressed
    let fe_kg = material_kg(&state, &station_id, "Fe");
    let fe_plates = component_count(&state, &station_id, "fe_plate");
    assert!(
        fe_kg > 0.0 || fe_plates > 0,
        "Tier 1+2: Fe should be refined from ore and/or pressed into fe_plates. \
         Fe: {fe_kg} kg, fe_plates: {fe_plates}"
    );

    // Phase 2: Run longer for Tier 3 (structural_beam).
    // Structural assembler: needs 3 fe_plates, runs every 8 ticks (480min/60min).
    run_with_autopilot(
        &mut state,
        &content,
        &mut rng,
        50,
        &mut next_cmd_id,
        &mut autopilot,
    );

    // Tier 3: Verify structural_beams were produced
    let structural_beams = component_count(&state, &station_id, "structural_beam");
    assert!(
        structural_beams > 0,
        "Tier 3: structural_beams should have been produced by the structural assembler. \
         fe_plates: {}, Fe: {} kg",
        component_count(&state, &station_id, "fe_plate"),
        material_kg(&state, &station_id, "Fe")
    );

    // Tier 4: hull_panel requires tech_advanced_manufacturing
    let hull_panels_before = component_count(&state, &station_id, "hull_panel");
    assert_eq!(
        hull_panels_before, 0,
        "hull_panels should NOT be produced before tech_advanced_manufacturing unlock"
    );

    // Manually unlock the tech (too expensive to reach naturally in a test)
    state
        .research
        .unlocked
        .insert(TechId("tech_advanced_manufacturing".to_string()));

    // Select hull_panel recipe on the structural assembler
    let station = state.stations.get_mut(&station_id).unwrap();
    for module in &mut station.modules {
        if module.def_id == "module_structural_assembler" {
            if let ModuleKindState::Assembler(ref mut asmb) = module.kind_state {
                asmb.selected_recipe = Some(RecipeId("recipe_hull_panel".to_string()));
            }
        }
    }

    // Ensure enough materials: add more Fe for continuous plate production
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 20_000.0,
            quality: 0.7,
            thermal: None,
        });

    // Run more ticks for hull_panel production.
    // hull_panel needs 2x structural_beam + 2x fe_plate, 480min interval.
    // Need structural_beams to accumulate first (the structural_assembler
    // will switch to hull_panel recipe since we selected it).
    run_with_autopilot(
        &mut state,
        &content,
        &mut rng,
        100,
        &mut next_cmd_id,
        &mut autopilot,
    );

    let hull_panels = component_count(&state, &station_id, "hull_panel");
    assert!(
        hull_panels > 0,
        "Tier 4: hull_panels should have been produced after tech unlock. \
         structural_beams: {}, fe_plates: {}",
        component_count(&state, &station_id, "structural_beam"),
        component_count(&state, &station_id, "fe_plate")
    );
}

// =========================================================================
// Competing Demand with Real Content
// =========================================================================

/// With real content modules and priority set, the higher-priority assembler
/// gets first access to scarce intermediate goods.
#[test]
fn competing_demand_with_real_content() {
    let content = sim_world::load_content(&content_dir()).unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut state = sim_world::build_initial_state(&content, 42, &mut rng);
    let station_id = StationId("station_earth_orbit".to_string());

    let mut autopilot = AutopilotController::new();
    let mut next_cmd_id = 0u64;

    // Install modules first
    run_with_autopilot(
        &mut state,
        &content,
        &mut rng,
        5,
        &mut next_cmd_id,
        &mut autopilot,
    );

    // Set priorities: structural_assembler (5) > basic_assembler (3)
    let station = state.stations.get_mut(&station_id).unwrap();
    for module in &mut station.modules {
        if module.def_id == "module_structural_assembler" {
            module.manufacturing_priority = 5;
        } else if module.def_id == "module_basic_assembler" {
            module.manufacturing_priority = 3;
            // Select advanced_repair_kit recipe (needs fe_plate + repair_kit)
            if let ModuleKindState::Assembler(ref mut asmb) = module.kind_state {
                asmb.selected_recipe = Some(RecipeId("recipe_advanced_repair_kit".to_string()));
            }
        }
    }

    // Give Fe for plate production and pre-seed some fe_plates
    let station = state.stations.get_mut(&station_id).unwrap();
    station.inventory.push(InventoryItem::Material {
        element: "Fe".to_string(),
        kg: 10_000.0,
        quality: 0.7,
        thermal: None,
    });
    station.inventory.push(InventoryItem::Component {
        component_id: ComponentId("fe_plate".to_string()),
        count: 5,
        quality: 1.0,
    });

    // Collect events to count assembler runs by recipe
    let mut structural_beam_runs = 0_usize;
    for _ in 0..60 {
        let commands = autopilot.generate_commands(&state, &content, &mut next_cmd_id);
        let events = sim_core::tick(&mut state, &commands, &content, &mut rng, None);
        for envelope in &events {
            if let sim_core::Event::AssemblerRan { recipe_id, .. } = &envelope.event {
                if recipe_id.0 == "recipe_structural_beam" {
                    structural_beam_runs += 1;
                }
            }
        }
    }

    // The structural assembler (priority 5) should have run at least once,
    // getting first access to the pre-seeded fe_plates.
    assert!(
        structural_beam_runs > 0,
        "structural assembler (priority 5) should have run with real content. \
         structural_beams: {}, fe_plates: {}",
        component_count(&state, &station_id, "structural_beam"),
        component_count(&state, &station_id, "fe_plate")
    );
}

// =========================================================================
// Determinism Regression
// =========================================================================

/// Two runs with the same seed and real content must produce identical state.
#[test]
fn determinism_same_seed_identical_state_with_real_content() {
    let content = sim_world::load_content(&content_dir()).unwrap();
    let tick_count = 500;

    let run = |seed: u64| -> GameState {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut state = sim_world::build_initial_state(&content, seed, &mut rng);

        // Seed ore for the manufacturing chain
        let station_id = StationId("station_earth_orbit".to_string());
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Ore {
                lot_id: LotId("lot_det_001".to_string()),
                asteroid_id: AsteroidId("asteroid_det_001".to_string()),
                kg: 10_000.0,
                composition: HashMap::from([("Fe".to_string(), 0.7), ("Si".to_string(), 0.3)]),
            });

        let mut autopilot = AutopilotController::new();
        let mut next_cmd_id = 0u64;

        for _ in 0..tick_count {
            let commands = autopilot.generate_commands(&state, &content, &mut next_cmd_id);
            sim_core::tick(&mut state, &commands, &content, &mut rng, None);
        }
        state
    };

    let state1 = run(42);
    let state2 = run(42);

    let station_id = StationId("station_earth_orbit".to_string());

    // Tick counter
    assert_eq!(
        state1.meta.tick, state2.meta.tick,
        "tick counters must match"
    );

    // Station inventories
    let inv1 = &state1.stations[&station_id].inventory;
    let inv2 = &state2.stations[&station_id].inventory;
    assert_eq!(
        inv1.len(),
        inv2.len(),
        "inventory length must match: run1={}, run2={}",
        inv1.len(),
        inv2.len()
    );

    // Module states (count, id, wear)
    let mods1 = &state1.stations[&station_id].modules;
    let mods2 = &state2.stations[&station_id].modules;
    assert_eq!(
        mods1.len(),
        mods2.len(),
        "module count must match: run1={}, run2={}",
        mods1.len(),
        mods2.len()
    );
    for (m1, m2) in mods1.iter().zip(mods2.iter()) {
        assert_eq!(m1.id, m2.id, "module ID mismatch");
        assert_eq!(m1.def_id, m2.def_id, "module def_id mismatch");
        assert_eq!(
            m1.enabled, m2.enabled,
            "module enabled mismatch for {}",
            m1.id
        );
        assert!(
            (m1.wear.wear - m2.wear.wear).abs() < f32::EPSILON,
            "module wear mismatch for {}: {} vs {}",
            m1.id,
            m1.wear.wear,
            m2.wear.wear
        );
    }

    // Research state
    assert_eq!(
        state1.research.unlocked, state2.research.unlocked,
        "unlocked techs must match"
    );

    // Balance
    assert!(
        (state1.balance - state2.balance).abs() < f64::EPSILON,
        "balance must match: {} vs {}",
        state1.balance,
        state2.balance
    );

    // Ship state
    for (ship_id, ship1) in &state1.ships {
        let ship2 = &state2.ships[ship_id];
        assert_eq!(
            ship1.inventory.len(),
            ship2.inventory.len(),
            "ship {} inventory length mismatch",
            ship_id
        );
    }

    // Event counters
    assert_eq!(
        state1.counters.next_event_id, state2.counters.next_event_id,
        "event counter mismatch"
    );
}
