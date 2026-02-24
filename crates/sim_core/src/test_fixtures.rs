//! Shared test fixtures for sim_core and downstream crates.
//!
//! `base_content()` provides a full-featured `GameContent` suitable for
//! integration-level tests (techs, templates, all elements, compressed durations).
//! `minimal_content()` provides the bare minimum for content-validation tests.

use crate::{
    AnomalyTag, AsteroidTemplateDef, Constants, Counters, DataKind, ElementDef, GameContent,
    GameState, MetaState, NodeDef, NodeId, PrincipalId, ResearchState, ScanSite, ShipId, ShipState,
    SiteId, SolarSystemDef, StationId, StationState, TechDef, TechEffect, TechId,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;

/// Full-featured content: deep_scan_v1 tech, iron_rich template, ore/Fe/Si/slag elements,
/// single-node solar system, compressed durations for fast tests.
pub fn base_content() -> GameContent {
    GameContent {
        content_version: "test".to_string(),
        techs: vec![TechDef {
            id: TechId("tech_deep_scan_v1".to_string()),
            name: "Deep Scan v1".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::new(),
            accepted_data: vec![DataKind::ScanData],
            difficulty: 10.0,
            effects: vec![
                TechEffect::EnableDeepScan,
                // sigma=0: mapped composition matches true composition exactly
                TechEffect::DeepScanCompositionNoise { sigma: 0.0 },
            ],
        }],
        solar_system: SolarSystemDef {
            nodes: vec![NodeDef {
                id: NodeId("node_test".to_string()),
                name: "Test Node".to_string(),
            }],
            edges: vec![],
        },
        asteroid_templates: vec![AsteroidTemplateDef {
            id: "tmpl_iron_rich".to_string(),
            anomaly_tags: vec![AnomalyTag::IronRich],
            composition_ranges: HashMap::from([
                // Fixed ranges so true_composition is deterministic.
                ("Fe".to_string(), (0.7, 0.7)),
                ("Si".to_string(), (0.3, 0.3)),
            ]),
        }],
        elements: vec![
            ElementDef {
                id: "ore".to_string(),
                density_kg_per_m3: 3000.0,
                display_name: "Raw Ore".to_string(),
                refined_name: None,
            },
            ElementDef {
                id: "Fe".to_string(),
                density_kg_per_m3: 7874.0,
                display_name: "Iron".to_string(),
                refined_name: Some("Iron Ingot".to_string()),
            },
            ElementDef {
                id: "Si".to_string(),
                density_kg_per_m3: 2329.0,
                display_name: "Silicon".to_string(),
                refined_name: None,
            },
            ElementDef {
                id: "slag".to_string(),
                density_kg_per_m3: 2500.0,
                display_name: "Slag".to_string(),
                refined_name: None,
            },
        ],
        module_defs: vec![],
        component_defs: vec![],
        constants: Constants {
            survey_scan_ticks: 1,
            deep_scan_ticks: 1,
            travel_ticks_per_hop: 1,
            // Always detect tags so tests are predictable.
            survey_tag_detection_probability: 1.0,
            asteroid_count_per_template: 1,
            asteroid_mass_min_kg: 500.0, // fixed range so tests are deterministic
            asteroid_mass_max_kg: 500.0,
            ship_cargo_capacity_m3: 20.0,
            station_cargo_capacity_m3: 10_000.0,
            station_power_available_per_tick: 100.0,
            mining_rate_kg_per_tick: 50.0,
            deposit_ticks: 1, // fast for tests
            autopilot_iron_rich_confidence_threshold: 0.7,
            autopilot_refinery_threshold_kg: 500.0,
            research_roll_interval_ticks: 60,
            data_generation_peak: 100.0,
            data_generation_floor: 5.0,
            data_generation_decay_rate: 0.7,
            wear_band_degraded_threshold: 0.5,
            wear_band_critical_threshold: 0.8,
            wear_band_degraded_efficiency: 0.75,
            wear_band_critical_efficiency: 0.5,
        },
    }
}

/// Bare-minimum content for validation tests: no techs, no templates, just Fe element.
pub fn minimal_content() -> GameContent {
    GameContent {
        content_version: "test".to_string(),
        techs: vec![],
        solar_system: SolarSystemDef {
            nodes: vec![],
            edges: vec![],
        },
        asteroid_templates: vec![],
        elements: vec![
            ElementDef {
                id: "ore".to_string(),
                density_kg_per_m3: 3000.0,
                display_name: "Raw Ore".to_string(),
                refined_name: None,
            },
            ElementDef {
                id: "Fe".to_string(),
                density_kg_per_m3: 7874.0,
                display_name: "Iron".to_string(),
                refined_name: None,
            },
            ElementDef {
                id: "slag".to_string(),
                density_kg_per_m3: 2500.0,
                display_name: "Slag".to_string(),
                refined_name: None,
            },
        ],
        module_defs: vec![],
        component_defs: vec![],
        constants: Constants {
            survey_scan_ticks: 1,
            deep_scan_ticks: 1,
            travel_ticks_per_hop: 1,
            survey_tag_detection_probability: 1.0,
            asteroid_count_per_template: 0,
            station_power_available_per_tick: 0.0,
            asteroid_mass_min_kg: 100.0,
            asteroid_mass_max_kg: 100.0,
            ship_cargo_capacity_m3: 20.0,
            station_cargo_capacity_m3: 1000.0,
            mining_rate_kg_per_tick: 50.0,
            deposit_ticks: 1,
            autopilot_iron_rich_confidence_threshold: 0.7,
            autopilot_refinery_threshold_kg: 500.0,
            research_roll_interval_ticks: 60,
            data_generation_peak: 100.0,
            data_generation_floor: 5.0,
            data_generation_decay_rate: 0.7,
            wear_band_degraded_threshold: 0.5,
            wear_band_critical_threshold: 0.8,
            wear_band_degraded_efficiency: 0.75,
            wear_band_critical_efficiency: 0.5,
        },
    }
}

/// Standard game state: 1 ship, 1 station, 1 scan site at node_test.
pub fn base_state(content: &GameContent) -> GameState {
    let node_id = NodeId("node_test".to_string());
    let ship_id = ShipId("ship_0001".to_string());
    let station_id = StationId("station_earth_orbit".to_string());
    let owner = PrincipalId("principal_autopilot".to_string());

    GameState {
        meta: MetaState {
            tick: 0,
            seed: 42,
            schema_version: 1,
            content_version: content.content_version.clone(),
        },
        scan_sites: vec![ScanSite {
            id: SiteId("site_0001".to_string()),
            node: node_id.clone(),
            template_id: "tmpl_iron_rich".to_string(),
        }],
        asteroids: std::collections::HashMap::new(),
        ships: std::collections::HashMap::from([(
            ship_id.clone(),
            ShipState {
                id: ship_id,
                location_node: node_id.clone(),
                owner,
                inventory: vec![],
                cargo_capacity_m3: 20.0,
                task: None,
            },
        )]),
        stations: std::collections::HashMap::from([(
            station_id.clone(),
            StationState {
                id: station_id,
                location_node: node_id,
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                modules: vec![],
            },
        )]),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: std::collections::HashMap::new(),
            evidence: std::collections::HashMap::new(),
            action_counts: std::collections::HashMap::new(),
        },
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
    }
}

/// Deterministic RNG seeded with 42.
pub fn make_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(42)
}
