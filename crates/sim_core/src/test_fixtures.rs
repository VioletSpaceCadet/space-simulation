//! Shared test fixtures for `sim_core` and downstream crates.
//!
//! `base_content()` provides a full-featured `GameContent` suitable for
//! integration-level tests (techs, templates, all elements, compressed durations).
//! `minimal_content()` provides the bare minimum for content-validation tests.

use crate::AHashMap;
use crate::{AngleMilliDeg, BodyId, Position, RadiusAuMicro};
use crate::{
    AnomalyTag, AsteroidId, AsteroidTemplateDef, AutopilotConfig, BodyType, Constants, Counters,
    CrewRole, DataKind, ElementDef, GameContent, GameState, HullId, InitialStationDef, InputAmount,
    InputFilter, ItemKind, LotId, MetaState, ModuleBehaviorDef, ModuleDef, ModuleInstanceId,
    ModuleKindState, ModulePort, ModuleState, NodeDef, NodeId, OrbitalBodyDef, OutputSpec,
    PricingTable, PrincipalId, ProcessorDef, ProcessorState, QualityFormula, RadiatorDef,
    RadiatorState, RecipeDef, RecipeId, RecipeThermalReq, ResearchState, ScanSite, ShipId,
    ShipState, SiteId, SlotType, SolarSystemDef, StationId, StationState, TechDef, TechEffect,
    TechId, ThermalDef, ThermalState, WearState, YieldFormula,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::{BTreeMap, HashMap};

/// Standard station ID used across test fixtures.
pub const TEST_STATION: &str = "station_earth_orbit";

/// Standard ship ID used across test fixtures.
pub const TEST_SHIP: &str = "ship_0001";

/// Convenience: create a `StationId` from the standard test station ID.
pub fn test_station_id() -> StationId {
    StationId(TEST_STATION.to_string())
}

/// Convenience: create a `ShipId` from the standard test ship ID.
pub fn test_ship_id() -> ShipId {
    ShipId(TEST_SHIP.to_string())
}

/// Build a `ModuleState` with sensible defaults.
/// Instance ID is `{def_id}_instance` — use unique def_ids per module in a test.
pub fn test_module(def_id: &str, kind_state: ModuleKindState) -> ModuleState {
    ModuleState {
        id: ModuleInstanceId(format!("{def_id}_instance")),
        def_id: def_id.to_string(),
        enabled: true,
        kind_state,
        wear: WearState::default(),
        thermal: None,
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
    }
}

/// Build a `ModuleState` with a thermal state at the given temperature.
pub fn test_module_thermal(
    def_id: &str,
    kind_state: ModuleKindState,
    temp_mk: u32,
    thermal_group: &str,
) -> ModuleState {
    ModuleState {
        thermal: Some(ThermalState {
            temp_mk,
            thermal_group: Some(thermal_group.to_string()),
            ..Default::default()
        }),
        ..test_module(def_id, kind_state)
    }
}

/// Standard test position used across fixtures.
pub fn test_position() -> Position {
    Position {
        parent_body: BodyId("test_body".to_string()),
        radius_au_um: RadiusAuMicro(0),
        angle_mdeg: AngleMilliDeg(0),
    }
}

// ---------------------------------------------------------------------------
// ModuleDef test builder
// ---------------------------------------------------------------------------

/// Builder for `ModuleDef` with sensible test defaults.
///
/// All optional fields default to empty/none. Use chainable methods to set
/// only the fields relevant to your test:
///
/// ```ignore
/// ModuleDefBuilder::new("my_processor")
///     .behavior(ModuleBehaviorDef::Processor(ProcessorDef { ... }))
///     .power(10.0)
///     .crew("operator", 1)
///     .build()
/// ```
pub struct ModuleDefBuilder {
    def: ModuleDef,
}

impl ModuleDefBuilder {
    /// Create a builder with test defaults: zero mass/volume/power/wear,
    /// empty Processor behavior, no thermal/crew/ports.
    pub fn new(id: &str) -> Self {
        Self {
            def: ModuleDef {
                id: id.to_string(),
                name: id.to_string(),
                mass_kg: 0.0,
                volume_m3: 0.0,
                power_consumption_per_run: 0.0,
                wear_per_run: 0.0,
                behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_minutes: 1,
                    processing_interval_ticks: 1,
                    recipes: vec![],
                }),
                thermal: None,
                compatible_slots: Vec::new(),
                ship_modifiers: Vec::new(),
                power_stall_priority: None,
                roles: vec![],
                crew_requirement: BTreeMap::new(),
                required_tech: None,
                ports: Vec::new(),
            },
        }
    }

    pub fn name(mut self, name: &str) -> Self {
        self.def.name = name.to_string();
        self
    }

    pub fn mass(mut self, kg: f32) -> Self {
        self.def.mass_kg = kg;
        self
    }

    pub fn volume(mut self, m3: f32) -> Self {
        self.def.volume_m3 = m3;
        self
    }

    pub fn power(mut self, watts: f32) -> Self {
        self.def.power_consumption_per_run = watts;
        self
    }

    pub fn wear(mut self, per_run: f32) -> Self {
        self.def.wear_per_run = per_run;
        self
    }

    pub fn behavior(mut self, behavior: ModuleBehaviorDef) -> Self {
        self.def.behavior = behavior;
        self
    }

    pub fn thermal(mut self, thermal: ThermalDef) -> Self {
        self.def.thermal = Some(thermal);
        self
    }

    pub fn compatible_slots(mut self, slots: Vec<SlotType>) -> Self {
        self.def.compatible_slots = slots;
        self
    }

    pub fn power_stall_priority(mut self, priority: u8) -> Self {
        self.def.power_stall_priority = Some(priority);
        self
    }

    pub fn roles(mut self, roles: Vec<&str>) -> Self {
        self.def.roles = roles.into_iter().map(String::from).collect();
        self
    }

    pub fn crew(mut self, role: &str, count: u32) -> Self {
        self.def
            .crew_requirement
            .insert(CrewRole(role.to_string()), count);
        self
    }

    pub fn required_tech(mut self, tech_id: &str) -> Self {
        self.def.required_tech = Some(crate::TechId(tech_id.to_string()));
        self
    }

    pub fn ports(mut self, ports: Vec<ModulePort>) -> Self {
        self.def.ports = ports;
        self
    }

    pub fn ship_modifiers(mut self, modifiers: Vec<crate::modifiers::Modifier>) -> Self {
        self.def.ship_modifiers = modifiers;
        self
    }

    pub fn build(self) -> ModuleDef {
        self.def
    }
}

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
            accepted_data: vec![DataKind::SurveyData],
            effects: vec![
                TechEffect::EnableDeepScan,
                // sigma=0: mapped composition matches true composition exactly
                TechEffect::DeepScanCompositionNoise { sigma: 0.0 },
            ],
        }],
        solar_system: SolarSystemDef {
            bodies: vec![OrbitalBodyDef {
                id: BodyId("test_body".to_string()),
                name: "Test Body".to_string(),
                parent: None,
                body_type: BodyType::Belt,
                radius_au_um: 0,
                angle_mdeg: 0,
                solar_intensity: 1.0,
                zone: None,
            }],
            nodes: vec![NodeDef {
                id: NodeId("node_test".to_string()),
                name: "Test Node".to_string(),
                solar_intensity: 1.0,
            }],
            edges: vec![],
        },
        asteroid_templates: vec![AsteroidTemplateDef {
            id: "tmpl_iron_rich".to_string(),
            anomaly_tags: vec![AnomalyTag::new("IronRich")],
            composition_ranges: HashMap::from([
                // Fixed ranges so true_composition is deterministic.
                ("Fe".to_string(), (0.7, 0.7)),
                ("Si".to_string(), (0.3, 0.3)),
            ]),
            preferred_class: Some(crate::spatial::ResourceClass::MetalRich),
        }],
        elements: vec![
            ElementDef {
                id: "ore".to_string(),
                density_kg_per_m3: 3000.0,
                display_name: "Raw Ore".to_string(),
                refined_name: None,
                category: "raw_ore".to_string(),
                melting_point_mk: None,
                latent_heat_j_per_kg: None,
                specific_heat_j_per_kg_k: None,
                boiloff_rate_per_day_at_293k: None,
                boiling_point_mk: None,
                boiloff_curve: None,
            },
            ElementDef {
                id: "Fe".to_string(),
                density_kg_per_m3: 7874.0,
                display_name: "Iron".to_string(),
                refined_name: Some("Iron Ingot".to_string()),
                category: "material".to_string(),
                melting_point_mk: Some(1_811_000),
                latent_heat_j_per_kg: Some(247_000),
                specific_heat_j_per_kg_k: Some(449),
                boiloff_rate_per_day_at_293k: None,
                boiling_point_mk: None,
                boiloff_curve: None,
            },
            ElementDef {
                id: "Si".to_string(),
                density_kg_per_m3: 2329.0,
                display_name: "Silicon".to_string(),
                refined_name: None,
                category: "material".to_string(),
                melting_point_mk: Some(1_687_000),
                latent_heat_j_per_kg: Some(1_787_000),
                specific_heat_j_per_kg_k: Some(710),
                boiloff_rate_per_day_at_293k: None,
                boiling_point_mk: None,
                boiloff_curve: None,
            },
            ElementDef {
                id: "slag".to_string(),
                density_kg_per_m3: 2500.0,
                display_name: "Slag".to_string(),
                refined_name: None,
                category: "byproduct".to_string(),
                melting_point_mk: None,
                latent_heat_j_per_kg: None,
                specific_heat_j_per_kg_k: None,
                boiloff_rate_per_day_at_293k: None,
                boiling_point_mk: None,
                boiloff_curve: None,
            },
            ElementDef {
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
            },
        ],
        module_defs: AHashMap::default(),
        component_defs: vec![],
        recipes: BTreeMap::new(),
        pricing: PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: AHashMap::default(),
        },
        constants: Constants {
            survey_scan_minutes: 1,
            deep_scan_minutes: 1,
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
            autopilot_volatile_threshold_kg: 500.0,
            autopilot_refinery_threshold_kg: 500.0,
            data_generation_peak: 100.0,
            data_generation_floor: 5.0,
            data_generation_decay_rate: 0.7,
            autopilot_slag_jettison_pct: 0.75,
            autopilot_export_batch_size_kg: 500.0,
            autopilot_export_min_revenue: 1_000.0,
            autopilot_lh2_threshold_kg: 5_000.0,
            wear_band_degraded_threshold: 0.5,
            wear_band_critical_threshold: 0.8,
            wear_band_degraded_efficiency: 0.75,
            wear_band_critical_efficiency: 0.5,
            minutes_per_tick: 1,
            // Spatial system
            docking_range_au_um: 10_000,
            ticks_per_au: 2_133,
            min_transit_ticks: 1,
            replenish_check_interval_ticks: 1,
            replenish_target_count: 5,
            // Thermal system
            thermal_sink_temp_mk: 293_000,
            thermal_overheat_warning_offset_mk: 200_000,
            thermal_overheat_critical_offset_mk: 500_000,
            thermal_overheat_damage_offset_mk: 800_000,
            thermal_wear_multiplier_warning: 2.0,
            thermal_wear_multiplier_critical: 4.0,
            // Extracted constants (previously hardcoded)
            t_max_absolute_mk: 10_000_000,
            min_meaningful_kg: 1e-3,
            replenish_batch_size: 5,
            trade_unlock_delay_minutes: 525_600,
            autopilot_budget_cap_fraction: 0.05,
            autopilot_lh2_abundant_multiplier: 2.0,
            boiloff_hot_offset_mk: 100_000,
            // Derived fields — filled by derive_tick_values()
            survey_scan_ticks: 0,
            deep_scan_ticks: 0,
            mining_rate_kg_per_tick: 0.0,
            deposit_ticks: 0,
            station_power_available_per_tick: 0.0,
            refuel_kg_per_tick: 0.0,
            events_enabled: false,
            event_global_cooldown_ticks: 200,
            event_history_capacity: 100,
            // Propulsion
            fuel_cost_per_au: 500.0,
            reference_mass_kg: 15_000.0,
            refuel_kg_per_minute: 16.67,
            autopilot_refuel_threshold_pct: 0.8,
            autopilot_refuel_max_pct: 0.99,
            autopilot_shipyard_component_count: 4,
            // Bottleneck detection
            bottleneck_storage_threshold_pct: 0.95,
            bottleneck_slag_ratio_threshold: 0.5,
            bottleneck_wear_threshold: 0.8,
        },
        alert_rules: Vec::new(),
        events: Vec::new(),
        hulls: BTreeMap::new(),
        fitting_templates: BTreeMap::new(),
        initial_station: InitialStationDef::default(),
        autopilot: AutopilotConfig::default(),
        crew_roles: BTreeMap::new(),
        scoring: Default::default(),
        milestones: Vec::new(),
        density_map: AHashMap::default(),
    };
    content.constants.derive_tick_values();
    content.init_caches();
    content
}

/// Standard test recipe: Ore → Fe + Slag (basic iron refinery recipe).
pub fn test_iron_recipe() -> RecipeDef {
    RecipeDef {
        id: RecipeId("recipe_basic_iron".to_string()),
        inputs: vec![crate::RecipeInput {
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
    }
}

/// Smelter recipe: Ore → Fe + Slag with thermal requirements.
pub fn test_smelt_recipe() -> RecipeDef {
    RecipeDef {
        id: RecipeId("recipe_smelt_iron".to_string()),
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
            efficiency_floor: 0.8,
            quality_floor: 0.3,
            quality_at_max: 0.6,
        }),
        required_tech: None,
        tags: vec![],
    }
}

/// Insert a recipe into the content's recipe catalog, using its own id as the key.
pub fn insert_recipe(content: &mut GameContent, recipe: RecipeDef) -> RecipeId {
    let id = recipe.id.clone();
    content.recipes.insert(id.clone(), recipe);
    id
}

/// Bare-minimum content for validation tests: no techs, no templates, just Fe element.
pub fn minimal_content() -> GameContent {
    let mut content = GameContent {
        content_version: "test".to_string(),
        techs: vec![],
        solar_system: SolarSystemDef {
            bodies: vec![],
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
                category: "raw_ore".to_string(),
                melting_point_mk: None,
                latent_heat_j_per_kg: None,
                specific_heat_j_per_kg_k: None,
                boiloff_rate_per_day_at_293k: None,
                boiling_point_mk: None,
                boiloff_curve: None,
            },
            ElementDef {
                id: "Fe".to_string(),
                density_kg_per_m3: 7874.0,
                display_name: "Iron".to_string(),
                refined_name: None,
                category: "material".to_string(),
                melting_point_mk: Some(1_811_000),
                latent_heat_j_per_kg: Some(247_000),
                specific_heat_j_per_kg_k: Some(449),
                boiloff_rate_per_day_at_293k: None,
                boiling_point_mk: None,
                boiloff_curve: None,
            },
            ElementDef {
                id: "slag".to_string(),
                density_kg_per_m3: 2500.0,
                display_name: "Slag".to_string(),
                refined_name: None,
                category: "byproduct".to_string(),
                melting_point_mk: None,
                latent_heat_j_per_kg: None,
                specific_heat_j_per_kg_k: None,
                boiloff_rate_per_day_at_293k: None,
                boiling_point_mk: None,
                boiloff_curve: None,
            },
            ElementDef {
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
            },
        ],
        module_defs: AHashMap::default(),
        component_defs: vec![],
        recipes: BTreeMap::new(),
        pricing: PricingTable {
            import_surcharge_per_kg: 100.0,
            export_surcharge_per_kg: 50.0,
            items: AHashMap::default(),
        },
        constants: Constants {
            survey_scan_minutes: 1,
            deep_scan_minutes: 1,
            survey_tag_detection_probability: 1.0,
            asteroid_count_per_template: 0,
            station_power_available_per_minute: 0.0,
            asteroid_mass_min_kg: 100.0,
            asteroid_mass_max_kg: 100.0,
            ship_cargo_capacity_m3: 20.0,
            station_cargo_capacity_m3: 1000.0,
            mining_rate_kg_per_minute: 50.0,
            deposit_minutes: 1,
            autopilot_volatile_threshold_kg: 500.0,
            autopilot_refinery_threshold_kg: 500.0,
            data_generation_peak: 100.0,
            data_generation_floor: 5.0,
            data_generation_decay_rate: 0.7,
            autopilot_slag_jettison_pct: 0.75,
            autopilot_export_batch_size_kg: 500.0,
            autopilot_export_min_revenue: 1_000.0,
            autopilot_lh2_threshold_kg: 5_000.0,
            wear_band_degraded_threshold: 0.5,
            wear_band_critical_threshold: 0.8,
            wear_band_degraded_efficiency: 0.75,
            wear_band_critical_efficiency: 0.5,
            minutes_per_tick: 1,
            // Spatial system
            docking_range_au_um: 10_000,
            ticks_per_au: 2_133,
            min_transit_ticks: 1,
            replenish_check_interval_ticks: 1,
            replenish_target_count: 5,
            // Thermal system
            thermal_sink_temp_mk: 293_000,
            thermal_overheat_warning_offset_mk: 200_000,
            thermal_overheat_critical_offset_mk: 500_000,
            thermal_overheat_damage_offset_mk: 800_000,
            thermal_wear_multiplier_warning: 2.0,
            thermal_wear_multiplier_critical: 4.0,
            // Extracted constants (previously hardcoded)
            t_max_absolute_mk: 10_000_000,
            min_meaningful_kg: 1e-3,
            replenish_batch_size: 5,
            trade_unlock_delay_minutes: 525_600,
            autopilot_budget_cap_fraction: 0.05,
            autopilot_lh2_abundant_multiplier: 2.0,
            boiloff_hot_offset_mk: 100_000,
            // Derived fields — filled by derive_tick_values()
            survey_scan_ticks: 0,
            deep_scan_ticks: 0,
            mining_rate_kg_per_tick: 0.0,
            deposit_ticks: 0,
            station_power_available_per_tick: 0.0,
            refuel_kg_per_tick: 0.0,
            events_enabled: false,
            event_global_cooldown_ticks: 200,
            event_history_capacity: 100,
            // Propulsion
            fuel_cost_per_au: 500.0,
            reference_mass_kg: 15_000.0,
            refuel_kg_per_minute: 16.67,
            autopilot_refuel_threshold_pct: 0.8,
            autopilot_refuel_max_pct: 0.99,
            autopilot_shipyard_component_count: 4,
            // Bottleneck detection
            bottleneck_storage_threshold_pct: 0.95,
            bottleneck_slag_ratio_threshold: 0.5,
            bottleneck_wear_threshold: 0.8,
        },
        alert_rules: Vec::new(),
        events: Vec::new(),
        hulls: BTreeMap::new(),
        fitting_templates: BTreeMap::new(),
        initial_station: InitialStationDef::default(),
        autopilot: AutopilotConfig {
            propellant_role: String::new(),
            propellant_support_role: String::new(),
            shipyard_role: String::new(),
            propellant_element: String::new(),
            primary_mining_element: String::new(),
            deep_scan_tech: String::new(),
            ship_construction_tech: String::new(),
            shipyard_import_component: String::new(),
            export_component: crate::ExportComponentConfig {
                component_id: String::new(),
                reserve: 0,
            },
            export_elements: vec![],
            deep_scan_targets: vec![],
            ..AutopilotConfig::default()
        },
        crew_roles: BTreeMap::new(),
        scoring: Default::default(),
        milestones: Vec::new(),
        density_map: AHashMap::default(),
    };
    content.constants.derive_tick_values();
    content.init_caches();
    content
}

/// Standard game state: 1 ship, 1 station, 1 scan site at `test_body`.
pub fn base_state(content: &GameContent) -> GameState {
    let ship_id = test_ship_id();
    let station_id = test_station_id();
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
            position: test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        }],
        asteroids: std::collections::BTreeMap::new(),
        ships: [(
            ship_id.clone(),
            ShipState {
                id: ship_id,
                position: test_position(),
                owner,
                inventory: vec![],
                cargo_capacity_m3: 20.0,
                task: None,
                speed_ticks_per_au: None,
                modifiers: crate::modifiers::ModifierSet::default(),
                hull_id: HullId("hull_general_purpose".to_string()),
                fitted_modules: vec![],
                propellant_kg: 0.0,
                propellant_capacity_kg: 0.0,
                crew: Default::default(),
                leaders: Vec::new(),
            },
        )]
        .into_iter()
        .collect(),
        stations: [(
            station_id.clone(),
            StationState {
                id: station_id,
                position: test_position(),
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                modules: vec![],
                modifiers: crate::modifiers::ModifierSet::default(),
                crew: Default::default(),
                leaders: Vec::new(),
                thermal_links: Vec::new(),
                power: crate::PowerState::default(),
                cached_inventory_volume_m3: None,
                module_type_index: crate::ModuleTypeIndex::default(),
                module_id_index: std::collections::HashMap::new(),
                power_budget_cache: crate::PowerBudgetCache::default(),
            },
        )]
        .into_iter()
        .collect(),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: AHashMap::default(),
            evidence: AHashMap::default(),
            action_counts: AHashMap::default(),
        },
        balance: 0.0,
        export_revenue_total: 0.0,
        export_count: 0,
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
        modifiers: crate::modifiers::ModifierSet::default(),
        events: crate::sim_events::SimEventState::default(),
        propellant_consumed_total: 0.0,
        body_cache: crate::build_body_cache(&content.solar_system.bodies),
    }
}

/// Rebuild module type indices for all stations in a GameState.
/// Call after constructing test states that include modules.
pub fn rebuild_indices(state: &mut GameState, content: &GameContent) {
    for station in state.stations.values_mut() {
        station.rebuild_module_index(content);
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

    let smelt_recipe_id = insert_recipe(&mut content, test_smelt_recipe());
    content.module_defs.insert(
        "module_basic_smelter".to_string(),
        ModuleDefBuilder::new("module_basic_smelter")
            .name("Basic Smelter")
            .mass(6000.0)
            .volume(12.0)
            .power(30.0)
            .wear(0.015)
            .behavior(crate::ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_minutes: 1,
                processing_interval_ticks: 1,
                recipes: vec![smelt_recipe_id],
            }))
            .thermal(ThermalDef {
                heat_capacity_j_per_k: 50_000.0,
                passive_cooling_coefficient: 5.0,
                max_temp_mk: 2_500_000,
                operating_min_mk: Some(1_800_000),
                operating_max_mk: Some(2_100_000),
                thermal_group: Some("default".to_string()),
                idle_heat_generation_w: None,
            })
            .build(),
    );

    content.module_defs.insert(
        "module_basic_radiator".to_string(),
        ModuleDefBuilder::new("module_basic_radiator")
            .name("Basic Radiator")
            .mass(800.0)
            .volume(15.0)
            .wear(0.001)
            .behavior(crate::ModuleBehaviorDef::Radiator(RadiatorDef {
                cooling_capacity_w: 500.0,
            }))
            .thermal(ThermalDef {
                heat_capacity_j_per_k: 10_000.0,
                passive_cooling_coefficient: 10.0,
                max_temp_mk: 3_000_000,
                operating_min_mk: None,
                operating_max_mk: None,
                thermal_group: Some("default".to_string()),
                idle_heat_generation_w: None,
            })
            .build(),
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
            selected_recipe: None,
        }),
        wear: WearState::default(),
        thermal: Some(ThermalState {
            temp_mk,
            thermal_group: Some("default".to_string()),
            ..Default::default()
        }),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
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
            ..Default::default()
        }),
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
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
        .expect("station_earth_orbit missing from base_state");
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
        .expect("station_earth_orbit missing from base_state");
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
        .expect("station_earth_orbit missing from base_state");
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
        .expect("station_earth_orbit missing from base_state");
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
            crate::tick(&mut state, &[], &content, &mut rng, None);
        }
    }

    #[test]
    fn radiator_fixture_ticks_without_panic() {
        let content = thermal_content();
        let mut state = state_with_radiator(&content);
        let mut rng = make_rng();
        for _ in 0..10 {
            crate::tick(&mut state, &[], &content, &mut rng, None);
        }
    }

    #[test]
    fn smelter_and_radiators_fixture_ticks_without_panic() {
        let content = thermal_content();
        let mut state = state_with_smelter_and_radiators(&content);
        let mut rng = make_rng();
        for _ in 0..10 {
            crate::tick(&mut state, &[], &content, &mut rng, None);
        }
    }

    #[test]
    fn cold_smelter_stalls() {
        let content = thermal_content();
        let mut state = state_with_smelter(&content);
        let mut rng = make_rng();
        let events = crate::tick(&mut state, &[], &content, &mut rng, None);

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
        let events = crate::tick(&mut state, &[], &content, &mut rng, None);

        let has_produced = events
            .iter()
            .any(|e| matches!(&e.event, crate::Event::RefineryRan { .. }));
        assert!(has_produced, "hot smelter should run and produce output");
    }
}
