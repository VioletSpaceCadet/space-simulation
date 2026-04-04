use super::*;
use crate::test_fixtures::{make_rng, rebuild_indices, test_station_id, ModuleDefBuilder};

// ---------------------------------------------------------------------------
// 1. Processor output scales with crew efficiency
// ---------------------------------------------------------------------------

#[test]
fn processor_output_scales_with_crew_efficiency() {
    // Refinery with crew requirement: 2 operators needed.
    // Give 1/2 → crew_factor = 0.5 → yield should be ~50%.
    let mut content = refinery_content();
    // Add crew requirement to the refinery def
    let refinery_def = content
        .module_defs
        .get_mut("module_basic_iron_refinery")
        .unwrap();
    refinery_def
        .crew_requirement
        .insert(CrewRole("operator".to_string()), 2);

    let mut state = state_with_refinery(&content);
    let station_id = test_station_id();

    // Assign 1 of 2 required operators → crew_factor = 0.5
    let station = state.stations.get_mut(&station_id).unwrap();
    station
        .core
        .crew
        .insert(CrewRole("operator".to_string()), 1);
    station.core.modules[0]
        .assigned_crew
        .insert(CrewRole("operator".to_string()), 1);
    rebuild_indices(&mut state, &content);

    let mut rng = make_rng();
    // Tick twice: interval=2, so first run happens on tick 2
    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let station = &state.stations[&station_id];
    let fe_kg: f32 = station
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();

    // Full efficiency: 500 kg ore (recipe input) × 0.7 Fe fraction = 350 kg Fe
    // Half crew efficiency: 350 × 0.5 = 175 kg Fe
    assert!(
        (fe_kg - 175.0).abs() < 1.0,
        "half crew should produce ~175 kg Fe, got {fe_kg}"
    );
}

// ---------------------------------------------------------------------------
// 2. Processor output zero when power stalled
// ---------------------------------------------------------------------------

#[test]
fn processor_output_zero_when_power_stalled() {
    let content = refinery_content();
    let mut state = state_with_refinery(&content);
    let station_id = test_station_id();

    // Starve it of power: set available power to 0
    let station = state.stations.get_mut(&station_id).unwrap();
    station.core.power_available_per_tick = 0.0;
    rebuild_indices(&mut state, &content);

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let station = &state.stations[&station_id];
    let fe_kg: f32 = station
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();

    assert!(
        fe_kg.abs() < 0.01,
        "power-stalled processor should produce 0 Fe, got {fe_kg}"
    );
}

// ---------------------------------------------------------------------------
// 3. Lab research speed scales with efficiency
// ---------------------------------------------------------------------------

#[test]
fn lab_research_speed_scales_with_efficiency() {
    // Lab with wear=0.6 → degraded band → wear_factor=0.75
    // No crew requirement → crew_factor=1.0
    // efficiency = 0.75
    let mut content = test_content();
    content.module_defs.insert(
        "module_exploration_lab".to_string(),
        ModuleDefBuilder::new("module_exploration_lab")
            .name("Exploration Lab")
            .mass(3500.0)
            .volume(7.0)
            .power(10.0)
            .wear(0.005)
            .behavior(ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::Survey,
                data_consumption_per_run: 8.0,
                research_points_per_run: 4.0,
                accepted_data: vec![DataKind::SurveyData],
                research_interval_minutes: 1,
                research_interval_ticks: 1,
            }))
            .build(),
    );

    let mut state = test_state(&content);
    let station_id = test_station_id();
    let station = state.stations.get_mut(&station_id).unwrap();

    station.core.modules.push(crate::test_fixtures::test_module(
        "module_exploration_lab",
        ModuleKindState::Lab(LabState {
            ticks_since_last_run: 0,
            assigned_tech: Some(TechId("tech_deep_scan_v1".to_string())),
            starved: false,
        }),
    ));

    // Set wear to degraded band (0.6 → efficiency_factor = 0.75)
    let lab_idx = station.core.modules.len() - 1;
    station.core.modules[lab_idx].wear.wear = 0.6;

    // Seed data pool
    state.research.data_pool.insert(DataKind::SurveyData, 100.0);

    rebuild_indices(&mut state, &content);

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);

    // Full efficiency: 4.0 points per run
    // Degraded wear (0.75): 4.0 × 0.75 = 3.0 points
    let tech_id = TechId("tech_deep_scan_v1".to_string());
    let points = state
        .research
        .evidence
        .get(&tech_id)
        .and_then(|p| p.points.get(&ResearchDomain::Survey))
        .copied()
        .unwrap_or(0.0);

    assert!(
        (points - 3.0).abs() < 0.1,
        "degraded lab should produce ~3.0 points, got {points}"
    );
}

// ---------------------------------------------------------------------------
// 4. Compound efficiency stacking: crew × wear
// ---------------------------------------------------------------------------

#[test]
fn compound_efficiency_stacking() {
    // Refinery with crew requirement: 2 operators.
    // Assign 1 → crew_factor = 0.5
    // Wear = 0.6 → degraded → wear_factor = 0.75
    // Combined efficiency = 0.5 × 0.75 = 0.375
    let mut content = refinery_content();
    let refinery_def = content
        .module_defs
        .get_mut("module_basic_iron_refinery")
        .unwrap();
    refinery_def
        .crew_requirement
        .insert(CrewRole("operator".to_string()), 2);

    let mut state = state_with_refinery(&content);
    let station_id = test_station_id();

    let station = state.stations.get_mut(&station_id).unwrap();
    station
        .core
        .crew
        .insert(CrewRole("operator".to_string()), 1);
    station.core.modules[0]
        .assigned_crew
        .insert(CrewRole("operator".to_string()), 1);
    station.core.modules[0].wear.wear = 0.6; // degraded band → 0.75

    rebuild_indices(&mut state, &content);

    let mut rng = make_rng();
    tick(&mut state, &[], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let station = &state.stations[&station_id];
    let fe_kg: f32 = station
        .core
        .inventory
        .iter()
        .filter_map(|i| match i {
            InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
            _ => None,
        })
        .sum();

    // Full efficiency: 500 kg ore (recipe input) × 0.7 Fe fraction = 350 kg Fe
    // Compound: 350 × 0.5 (crew) × 0.75 (wear) = 131.25 kg Fe
    assert!(
        (fe_kg - 131.25).abs() < 1.0,
        "compound efficiency (crew=0.5 × wear=0.75) should produce ~131.25 kg Fe, got {fe_kg}"
    );
}

// ---------------------------------------------------------------------------
// 5. Assembler produces nothing when efficiency < 0.5
// ---------------------------------------------------------------------------

#[test]
fn assembler_zero_output_below_half_efficiency() {
    // Assembler uses round(1.0 * efficiency). At efficiency < 0.5, round → 0.
    // Wear = 0.85 → critical band → wear_factor = 0.5
    // Add crew requirement: 2 operators, assign 0 → crew_factor = 0.0
    // Combined: 0.0 * 0.5 = 0.0 → round(0) = 0 → no output
    let mut content = assembler_content();
    let assembler_def = content
        .module_defs
        .get_mut("module_basic_assembler")
        .unwrap();
    assembler_def
        .crew_requirement
        .insert(CrewRole("operator".to_string()), 2);

    let mut state = state_with_assembler(&content);
    let station_id = test_station_id();

    // No crew assigned, crew_factor = 0.0
    rebuild_indices(&mut state, &content);

    let mut rng = make_rng();
    for _ in 0..5 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    let station = &state.stations[&station_id];
    let repair_kit_count: u32 = station
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

    assert_eq!(
        repair_kit_count, 0,
        "assembler with zero efficiency should produce no items"
    );
}
