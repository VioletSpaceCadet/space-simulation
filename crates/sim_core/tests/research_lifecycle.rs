//! Integration test: seed data → lab consumes → domain points accumulate → tech unlocks.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_core::test_fixtures::base_content;
use sim_core::test_fixtures::base_state;
use sim_core::*;
use std::collections::HashMap;

#[test]
fn full_research_lifecycle() {
    let mut content = base_content();

    // Add exploration lab module def
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
                research_interval_ticks: 1,
            }),
        },
    );

    // Make tech require Exploration domain, low difficulty for test
    content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Exploration, 10.0)]);
    content.techs[0].difficulty = 5.0;

    let mut state = base_state(&content);
    let station_id = StationId("station_earth_orbit".to_string());

    // Seed data pool with plenty of scan data
    *state
        .research
        .data_pool
        .entry(DataKind::ScanData)
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
        });

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let tech_id = TechId("tech_deep_scan_v1".to_string());

    // Run enough ticks for labs to accumulate points and research to roll
    for _ in 0..120 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
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
                research_interval_ticks: 1,
            }),
        },
    );

    content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Exploration, 100.0)]);
    content.techs[0].difficulty = 1_000_000.0; // very high

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
        });

    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let tech_id = TechId("tech_deep_scan_v1".to_string());

    for _ in 0..120 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }

    // Tech should NOT be unlocked (no data, lab starved)
    assert!(
        !state.research.unlocked.contains(&tech_id),
        "tech should NOT unlock without data",
    );
}
