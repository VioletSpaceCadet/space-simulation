//! Shared test fixtures for `sim_core` and downstream crates.
//!
//! `base_content()` provides a full-featured `GameContent` suitable for
//! integration-level tests (techs, templates, all elements, compressed durations).
//! `minimal_content()` provides the bare minimum for content-validation tests.

use crate::{
    AnomalyTag, AsteroidId, AsteroidTemplateDef, Constants, Counters, DataKind, ElementDef,
    GameContent, GameState, InputAmount, InputFilter, ItemKind, LotId, MetaState, ModuleDef,
    ModuleInstanceId, ModuleKindState, ModuleState, NodeDef, NodeId, OutputSpec, PricingTable,
    PrincipalId, ProcessorDef, ProcessorState, QualityFormula, RadiatorDef, RadiatorState,
    RecipeDef, RecipeThermalReq, ResearchState, ScanSite, ShipId, ShipState, SiteId,
    SolarSystemDef, StationId, StationState, TechDef, TechEffect, TechId, ThermalDef, ThermalState,
    WearState, YieldFormula,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;

/// Full-featured content: `deep_scan_v1` tech, `iron_rich` template, ore/Fe/Si/slag elements,
/// single-node solar system, compressed durations for fast tests.
#[allow(clippy::too_many_lines)] // struct-literal fixture, splitting reduces readability
pub fn base_content() -> GameContent {
    let mut content = GameContent {
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
                solar_intensity: 1.0,
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
                melting_point_mk: None,
                latent_heat_j_per_kg: None,
                specific_heat_j_per_kg_k: None,
            },
            ElementDef {
                id: "Fe".to_string(),
                density_kg_per_m3: 7874.0,
                display_name: "Iron".to_string(),
                refined_name: Some("Iron Ingot".to_string()),
                melting_point_mk: Some(1_811_000),
                latent_heat_j_per_kg: Some(247_000),
                specific_heat_j_per_kg_k: Some(449),
            },
            ElementDef {
                id: "Si".to_string(),
                density_kg_per_m3: 2329.0,
                display_name: "Silicon".to_string(),
                refined_name: None,
                melting_point_mk: Some(1_687_000),
                latent_heat_j_per_kg: Some(1_787_000),
                specific_heat_j_per_kg_k: Some(710),
            },
            ElementDef {
                id: "slag".to_string(),
                density_kg_per_m3: 2500.0,
                display_name: "Slag".to_string(),
                refined_name: None,
                melting_point_mk: None,
                latent_heat_j_per_kg: None,
                specific_heat_j_per_kg_k: None,
            },
        ],
        module_defs: HashMap::new(),
        component_defs: vec![],
        pricing: PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: HashMap::new(),
        },
        constants: Constants {
            survey_scan_minutes: 1,
            deep_scan_minutes: 1,
            travel_minutes_per_hop: 1,
            // Always detect tags so tests are predictable.
            survey_tag_detection_probability: 1.0,
            asteroid_count_per_template: 1,
            asteroid_mass_min_kg: 500.0, // fixed range so tests are deterministic
            asteroid_mass_max_kg: 500.0,
            ship_cargo_capacity_m3: 20.0,
            station_cargo_capacity_m3: 10_000.0,
            station_power_available_per_minute: 100.0,
            mining_rate_kg_per_minute: 50.0,
            deposit_minutes: 1, // fast for tests
            autopilot_iron_rich_confidence_threshold: 0.7,
            autopilot_refinery_threshold_kg: 500.0,
            research_roll_interval_minutes: 60,
            data_generation_peak: 100.0,
            data_generation_floor: 5.0,
            data_generation_decay_rate: 0.7,
            autopilot_slag_jettison_pct: 0.75,
            wear_band_degraded_threshold: 0.5,
            wear_band_critical_threshold: 0.8,
            wear_band_degraded_efficiency: 0.75,
            wear_band_critical_efficiency: 0.5,
            minutes_per_tick: 1,
            // Thermal system
            thermal_sink_temp_mk: 293_000,
            thermal_overheat_warning_offset_mk: 200_000,
            thermal_overheat_critical_offset_mk: 500_000,
            thermal_wear_multiplier_warning: 2.0,
            thermal_wear_multiplier_critical: 4.0,
            // Derived fields — filled by derive_tick_values()
            survey_scan_ticks: 0,
            deep_scan_ticks: 0,
            travel_ticks_per_hop: 0,
            mining_rate_kg_per_tick: 0.0,
            deposit_ticks: 0,
            station_power_available_per_tick: 0.0,
            research_roll_interval_ticks: 0,
        },
        density_map: HashMap::new(),
    };
    content.constants.derive_tick_values();
    content.init_caches();
    content
}

/// Bare-minimum content for validation tests: no techs, no templates, just Fe element.
pub fn minimal_content() -> GameContent {
    let mut content = GameContent {
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
                melting_point_mk: None,
                latent_heat_j_per_kg: None,
                specific_heat_j_per_kg_k: None,
            },
            ElementDef {
                id: "Fe".to_string(),
                density_kg_per_m3: 7874.0,
                display_name: "Iron".to_string(),
                refined_name: None,
                melting_point_mk: Some(1_811_000),
                latent_heat_j_per_kg: Some(247_000),
                specific_heat_j_per_kg_k: Some(449),
            },
            ElementDef {
                id: "slag".to_string(),
                density_kg_per_m3: 2500.0,
                display_name: "Slag".to_string(),
                refined_name: None,
                melting_point_mk: None,
                latent_heat_j_per_kg: None,
                specific_heat_j_per_kg_k: None,
            },
        ],
        module_defs: HashMap::new(),
        component_defs: vec![],
        pricing: PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: HashMap::new(),
        },
        constants: Constants {
            survey_scan_minutes: 1,
            deep_scan_minutes: 1,
            travel_minutes_per_hop: 1,
            survey_tag_detection_probability: 1.0,
            asteroid_count_per_template: 0,
            station_power_available_per_minute: 0.0,
            asteroid_mass_min_kg: 100.0,
            asteroid_mass_max_kg: 100.0,
            ship_cargo_capacity_m3: 20.0,
            station_cargo_capacity_m3: 1000.0,
            mining_rate_kg_per_minute: 50.0,
            deposit_minutes: 1,
            autopilot_iron_rich_confidence_threshold: 0.7,
            autopilot_refinery_threshold_kg: 500.0,
            research_roll_interval_minutes: 60,
            data_generation_peak: 100.0,
            data_generation_floor: 5.0,
            data_generation_decay_rate: 0.7,
            autopilot_slag_jettison_pct: 0.75,
            wear_band_degraded_threshold: 0.5,
            wear_band_critical_threshold: 0.8,
            wear_band_degraded_efficiency: 0.75,
            wear_band_critical_efficiency: 0.5,
            minutes_per_tick: 1,
            // Thermal system
            thermal_sink_temp_mk: 293_000,
            thermal_overheat_warning_offset_mk: 200_000,
            thermal_overheat_critical_offset_mk: 500_000,
            thermal_wear_multiplier_warning: 2.0,
            thermal_wear_multiplier_critical: 4.0,
            // Derived fields — filled by derive_tick_values()
            survey_scan_ticks: 0,
            deep_scan_ticks: 0,
            travel_ticks_per_hop: 0,
            mining_rate_kg_per_tick: 0.0,
            deposit_ticks: 0,
            station_power_available_per_tick: 0.0,
            research_roll_interval_ticks: 0,
        },
        density_map: HashMap::new(),
    };
    content.constants.derive_tick_values();
    content.init_caches();
    content
}

/// Standard game state: 1 ship, 1 station, 1 scan site at `node_test`.
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
                power: crate::PowerState::default(),
                cached_inventory_volume_m3: None,
            },
        )]),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: std::collections::HashMap::new(),
            evidence: std::collections::HashMap::new(),
            action_counts: std::collections::HashMap::new(),
        },
        balance: 0.0,
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

// ── Thermal test fixtures (VIO-209) ─────────────────────────────

/// Content with smelter and radiator module defs.
/// Uses `base_content()` and adds thermal module definitions.
/// Power is inherited from `base_state()` (`power_available_per_tick: 100.0`).
#[allow(clippy::too_many_lines)]
pub fn thermal_content() -> GameContent {
    let mut content = base_content();

    content.module_defs.insert(
        "module_basic_smelter".to_string(),
        ModuleDef {
            id: "module_basic_smelter".to_string(),
            name: "Basic Smelter".to_string(),
            mass_kg: 6000.0,
            volume_m3: 12.0,
            power_consumption_per_run: 30.0,
            wear_per_run: 0.015,
            behavior: crate::ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 1,
                processing_interval_ticks: 1,
                recipes: vec![RecipeDef {
                    id: "recipe_smelt_iron".to_string(),
                    inputs: vec![crate::RecipeInput {
                        filter: InputFilter::ItemKind(ItemKind::Ore),
                        amount: InputAmount::Kg(500.0),
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
                    thermal_req: Some(RecipeThermalReq {
                        min_temp_mk: 1_800_000,
                        optimal_min_mk: 1_850_000,
                        optimal_max_mk: 1_950_000,
                        max_temp_mk: 2_100_000,
                        heat_per_run_j: 50_000_000,
                    }),
                }],
            }),
            thermal: Some(ThermalDef {
                heat_capacity_j_per_k: 50_000.0,
                passive_cooling_coefficient: 5.0,
                max_temp_mk: 2_500_000,
                operating_min_mk: None,
                operating_max_mk: None,
                thermal_group: Some("default".to_string()),
            }),
        },
    );

    content.module_defs.insert(
        "module_basic_radiator".to_string(),
        ModuleDef {
            id: "module_basic_radiator".to_string(),
            name: "Basic Radiator".to_string(),
            mass_kg: 800.0,
            volume_m3: 15.0,
            power_consumption_per_run: 0.0,
            wear_per_run: 0.001,
            behavior: crate::ModuleBehaviorDef::Radiator(RadiatorDef {
                cooling_capacity_w: 500.0,
            }),
            thermal: Some(ThermalDef {
                heat_capacity_j_per_k: 10_000.0,
                passive_cooling_coefficient: 10.0,
                max_temp_mk: 3_000_000,
                operating_min_mk: None,
                operating_max_mk: None,
                thermal_group: Some("default".to_string()),
            }),
        },
    );

    content
}

fn smelter_module(temp_mk: u32) -> ModuleState {
    ModuleState {
        id: ModuleInstanceId("mod_smelter_001".to_string()),
        def_id: "module_basic_smelter".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Processor(ProcessorState {
            threshold_kg: 500.0,
            ticks_since_last_run: 100,
            stalled: false,
        }),
        wear: WearState::default(),
        thermal: Some(ThermalState {
            temp_mk,
            thermal_group: Some("default".to_string()),
        }),
        power_stalled: false,
    }
}

fn radiator_module() -> ModuleState {
    ModuleState {
        id: ModuleInstanceId("mod_radiator_001".to_string()),
        def_id: "module_basic_radiator".to_string(),
        enabled: true,
        kind_state: ModuleKindState::Radiator(RadiatorState::default()),
        wear: WearState::default(),
        thermal: Some(ThermalState {
            temp_mk: 293_000,
            thermal_group: Some("default".to_string()),
        }),
        power_stalled: false,
    }
}

fn second_radiator_module() -> ModuleState {
    let mut module = radiator_module();
    module.id = ModuleInstanceId("mod_radiator_002".to_string());
    module
}

fn ore_inventory() -> Vec<crate::InventoryItem> {
    vec![crate::InventoryItem::Ore {
        lot_id: LotId("lot_thermal_001".to_string()),
        asteroid_id: AsteroidId("ast_thermal_001".to_string()),
        kg: 5000.0,
        composition: HashMap::from([("Fe".to_string(), 0.7), ("Si".to_string(), 0.3)]),
    }]
}

/// Station with a smelter module at ambient temperature (293K).
/// Includes ore inventory for processing. Power from `base_state()` defaults.
pub fn state_with_smelter(content: &GameContent) -> GameState {
    let mut state = base_state(content);
    let station = state
        .stations
        .get_mut(&StationId("station_earth_orbit".to_string()))
        .unwrap();
    station.modules.push(smelter_module(293_000));
    station.inventory = ore_inventory();
    state
}

/// Station with a smelter at the given temperature (milli-Kelvin).
/// Includes ore inventory for processing.
pub fn state_with_smelter_at_temp(content: &GameContent, temp_mk: u32) -> GameState {
    let mut state = base_state(content);
    let station = state
        .stations
        .get_mut(&StationId("station_earth_orbit".to_string()))
        .unwrap();
    station.modules.push(smelter_module(temp_mk));
    station.inventory = ore_inventory();
    state
}

/// Station with a single radiator module.
pub fn state_with_radiator(content: &GameContent) -> GameState {
    let mut state = base_state(content);
    let station = state
        .stations
        .get_mut(&StationId("station_earth_orbit".to_string()))
        .unwrap();
    station.modules.push(radiator_module());
    state
}

/// Station with a smelter (at ambient) + 2 radiators, plus ore inventory.
/// The common thermal test setup.
pub fn state_with_smelter_and_radiators(content: &GameContent) -> GameState {
    let mut state = base_state(content);
    let station = state
        .stations
        .get_mut(&StationId("station_earth_orbit".to_string()))
        .unwrap();
    station.modules.push(smelter_module(293_000));
    station.modules.push(radiator_module());
    station.modules.push(second_radiator_module());
    station.inventory = ore_inventory();
    state
}

#[cfg(test)]
mod thermal_fixture_tests {
    use super::*;

    #[test]
    fn smelter_fixture_ticks_without_panic() {
        let content = thermal_content();
        let mut state = state_with_smelter(&content);
        let mut rng = make_rng();
        for _ in 0..10 {
            crate::tick(
                &mut state,
                &[],
                &content,
                &mut rng,
                crate::EventLevel::Normal,
            );
        }
    }

    #[test]
    fn radiator_fixture_ticks_without_panic() {
        let content = thermal_content();
        let mut state = state_with_radiator(&content);
        let mut rng = make_rng();
        for _ in 0..10 {
            crate::tick(
                &mut state,
                &[],
                &content,
                &mut rng,
                crate::EventLevel::Normal,
            );
        }
    }

    #[test]
    fn smelter_and_radiators_fixture_ticks_without_panic() {
        let content = thermal_content();
        let mut state = state_with_smelter_and_radiators(&content);
        let mut rng = make_rng();
        for _ in 0..10 {
            crate::tick(
                &mut state,
                &[],
                &content,
                &mut rng,
                crate::EventLevel::Normal,
            );
        }
    }

    #[test]
    fn cold_smelter_stalls() {
        let content = thermal_content();
        let mut state = state_with_smelter(&content);
        let mut rng = make_rng();
        let events = crate::tick(
            &mut state,
            &[],
            &content,
            &mut rng,
            crate::EventLevel::Normal,
        );

        // Smelter at 293K should stall (requires 1800K min)
        let has_too_cold = events
            .iter()
            .any(|e| matches!(&e.event, crate::Event::ProcessorTooCold { .. }));
        assert!(has_too_cold, "cold smelter should emit ProcessorTooCold");
    }

    #[test]
    fn hot_smelter_runs() {
        let content = thermal_content();
        let mut state = state_with_smelter_at_temp(&content, 1_900_000);
        let mut rng = make_rng();
        let events = crate::tick(
            &mut state,
            &[],
            &content,
            &mut rng,
            crate::EventLevel::Normal,
        );

        let has_produced = events
            .iter()
            .any(|e| matches!(&e.event, crate::Event::RefineryRan { .. }));
        assert!(has_produced, "hot smelter should run and produce output");
    }
}
