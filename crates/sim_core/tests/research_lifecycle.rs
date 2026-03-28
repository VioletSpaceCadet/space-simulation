//! Integration test: seed data → lab consumes → domain points accumulate → tech unlocks.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_core::test_fixtures::{base_content, base_state, ModuleDefBuilder};
use sim_core::*;
use std::collections::HashMap;

#[test]
fn full_research_lifecycle() {
    let mut content = base_content();

    // Add exploration lab module def
    content.module_defs.insert(
        "module_exploration_lab".to_string(),
        ModuleDefBuilder::new("module_exploration_lab")
            .name("Exploration Lab")
            .mass(3500.0)
            .volume(7.0)
            .power(10.0)
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

    // Make tech require Survey domain — low threshold for test
    content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Survey, 10.0)]);

    let mut state = base_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Seed data pool with plenty of scan data
    *state
        .research
        .data_pool
        .entry(DataKind::SurveyData)
        .or_insert(0.0) = 1000.0;

    // Install lab with assigned tech
    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .modules
        .push(ModuleState {
            id: ModuleInstanceId("module_inst_lab_001".to_string()),
            def_id: "module_exploration_lab".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Lab(LabState {
                ticks_since_last_run: 0,
                assigned_tech: Some(TechId("tech_deep_scan_v1".to_string())),
                starved: false,
            }),
            wear: WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            thermal: None,
        });

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let tech_id = TechId("tech_deep_scan_v1".to_string());

    // Run enough ticks for labs to accumulate points and research to roll
    for _ in 0..120 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    // Tech should be unlocked (low difficulty, lots of data)
    assert!(
        state.research.unlocked.contains(&tech_id),
        "tech should unlock after sufficient lab work. Evidence: {:?}",
        state.research.evidence.get(&tech_id),
    );
}

#[test]
fn research_lifecycle_no_data_means_no_unlock() {
    let mut content = base_content();

    content.module_defs.insert(
        "module_exploration_lab".to_string(),
        ModuleDefBuilder::new("module_exploration_lab")
            .name("Exploration Lab")
            .mass(3500.0)
            .volume(7.0)
            .power(10.0)
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

    content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Survey, 100.0)]);

    let mut state = base_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // NO data in pool — lab will starve

    state
        .stations
        .get_mut(&station_id)
        .unwrap()
        .modules
        .push(ModuleState {
            id: ModuleInstanceId("module_inst_lab_001".to_string()),
            def_id: "module_exploration_lab".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Lab(LabState {
                ticks_since_last_run: 0,
                assigned_tech: Some(TechId("tech_deep_scan_v1".to_string())),
                starved: false,
            }),
            wear: WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            thermal: None,
        });

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let tech_id = TechId("tech_deep_scan_v1".to_string());

    for _ in 0..120 {
        tick(&mut state, &[], &content, &mut rng, None);
    }

    // Tech should NOT be unlocked (no data, lab starved)
    assert!(
        !state.research.unlocked.contains(&tech_id),
        "tech should NOT unlock without data",
    );
}
